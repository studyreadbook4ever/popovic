use crate::{
    deploy,
    mcp::{McpClient, McpEndpoint},
    models::{
        AgentTask, AgentTaskStatus, AiProposalResponse, AiProvider, ExternalAction, FileChange,
    },
    secret,
    store::Store,
};
use chrono::Utc;
use reqwest::Client;
use serde_json::{Value, json};
use std::{fs, io, path::Path};
use uuid::Uuid;

const MAX_CONTEXT_BYTES: usize = 96 * 1024;

#[derive(Clone)]
pub struct AgentEngine {
    http: Client,
    mcp: McpClient,
}

impl AgentEngine {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
            mcp: McpClient::new(),
        }
    }

    pub async fn propose(
        &self,
        store: &Store,
        app_id: Uuid,
        prompt: String,
    ) -> io::Result<AgentTask> {
        let app = store
            .find_app(app_id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "app not found"))?;
        let release_dir = store.current_release_dir(&app).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "app must be deployed before AI edits",
            )
        })?;
        let context = collect_site_context(&release_dir)?;
        let config = store.read();
        let settings = config.settings;
        let api_key = secret::open(&settings.ai_api_key_sealed);

        let proposal = if api_key.is_empty() {
            Err("AI API key is not configured. Add one in Settings first.".to_string())
        } else {
            self.call_provider(&settings.ai_provider, &api_key, &prompt, &context)
                .await
        };

        let mut task = match proposal {
            Ok(proposal) => {
                let changes = sanitize_changes(&release_dir, proposal.changes)?;
                let diff = build_diff(&release_dir, &changes);
                AgentTask {
                    id: Uuid::new_v4(),
                    created_at: Utc::now(),
                    app_id,
                    prompt,
                    status: AgentTaskStatus::Proposed,
                    plan: proposal.summary,
                    diff,
                    changes,
                    github_action: proposal.github_action.unwrap_or_default(),
                    cloudflare_action: proposal.cloudflare_action.unwrap_or_default(),
                    error: None,
                }
            }
            Err(error) => AgentTask {
                id: Uuid::new_v4(),
                created_at: Utc::now(),
                app_id,
                prompt,
                status: AgentTaskStatus::Failed,
                plan: "AI proposal failed before any file was changed.".to_string(),
                diff: String::new(),
                changes: Vec::new(),
                github_action: ExternalAction::default(),
                cloudflare_action: ExternalAction::default(),
                error: Some(error),
            },
        };

        self.attach_mcp_context(store, &mut task).await;
        store.update(|config| config.agent_tasks.push(task.clone()))?;
        Ok(task)
    }

    pub fn approve(&self, store: &Store, task_id: Uuid) -> io::Result<AgentTask> {
        let task = store
            .read()
            .agent_tasks
            .into_iter()
            .find(|task| task.id == task_id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "task not found"))?;
        if !matches!(task.status, AgentTaskStatus::Proposed) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "only proposed tasks can be approved",
            ));
        }
        let changes: Vec<(String, String)> = task
            .changes
            .iter()
            .map(|change| (change.path.clone(), change.content.clone()))
            .collect();
        deploy::apply_file_changes(store, task.app_id, &changes)?;
        let mut updated = task;
        updated.status = AgentTaskStatus::Applied;
        store.update(|config| {
            if let Some(existing) = config
                .agent_tasks
                .iter_mut()
                .find(|task| task.id == task_id)
            {
                *existing = updated.clone();
            }
        })?;
        Ok(updated)
    }

    pub fn reject(&self, store: &Store, task_id: Uuid) -> io::Result<AgentTask> {
        let mut rejected = None;
        store.update(|config| {
            if let Some(task) = config
                .agent_tasks
                .iter_mut()
                .find(|task| task.id == task_id)
            {
                task.status = AgentTaskStatus::Rejected;
                rejected = Some(task.clone());
            }
        })?;
        rejected.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "task not found"))
    }

    async fn call_provider(
        &self,
        provider: &AiProvider,
        api_key: &str,
        user_prompt: &str,
        site_context: &str,
    ) -> Result<AiProposalResponse, String> {
        match provider {
            AiProvider::OpenAi => self.call_openai(api_key, user_prompt, site_context).await,
            AiProvider::Anthropic => self.call_anthropic(api_key, user_prompt, site_context).await,
            AiProvider::Antigravity | AiProvider::Cursor => Err(
                "This provider is configured as an account-backed external agent connector in v0; use OpenAI or Anthropic for direct API proposals."
                    .to_string(),
            ),
        }
    }

    async fn call_openai(
        &self,
        api_key: &str,
        user_prompt: &str,
        site_context: &str,
    ) -> Result<AiProposalResponse, String> {
        let body = json!({
            "model": "gpt-4.1-mini",
            "messages": [
                {"role": "system", "content": system_prompt()},
                {"role": "user", "content": format!("User request:\n{user_prompt}\n\nCurrent static site files:\n{site_context}")}
            ],
            "temperature": 0.2
        });
        let response = self
            .http
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status();
        let value: Value = response.json().await.map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!("OpenAI request failed: {value}"));
        }
        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| "OpenAI response did not contain message content".to_string())?;
        parse_proposal(content)
    }

    async fn call_anthropic(
        &self,
        api_key: &str,
        user_prompt: &str,
        site_context: &str,
    ) -> Result<AiProposalResponse, String> {
        let body = json!({
            "model": "claude-3-5-haiku-latest",
            "max_tokens": 8192,
            "system": system_prompt(),
            "messages": [
                {"role": "user", "content": format!("User request:\n{user_prompt}\n\nCurrent static site files:\n{site_context}")}
            ]
        });
        let response = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status();
        let value: Value = response.json().await.map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!("Anthropic request failed: {value}"));
        }
        let content = value["content"][0]["text"]
            .as_str()
            .ok_or_else(|| "Anthropic response did not contain text content".to_string())?;
        parse_proposal(content)
    }

    async fn attach_mcp_context(&self, store: &Store, task: &mut AgentTask) {
        let config = store.read();
        let github_token = secret::open(&config.settings.github_oauth_token_sealed);
        let cloudflare_token = secret::open(&config.settings.cloudflare_api_token_sealed);
        let endpoints = [
            McpEndpoint {
                name: "github".to_string(),
                url: config.settings.github_mcp_url,
                bearer_token: github_token,
            },
            McpEndpoint {
                name: "cloudflare".to_string(),
                url: config.settings.cloudflare_mcp_url,
                bearer_token: cloudflare_token,
            },
        ];
        for endpoint in endpoints {
            if endpoint.url.trim().is_empty() {
                continue;
            }
            if let Err(error) = self.mcp.list_tools(&endpoint).await {
                let line = format!("\n\n{} MCP check: {}", endpoint.name, error);
                task.plan.push_str(&line);
            }
        }
    }
}

fn system_prompt() -> &'static str {
    r#"You are Popovic's static HTTP deployment assistant.
Only work on static HTML, CSS, JS, robots.txt, agents.txt, sitemap.xml, and assets.
Never propose a server framework, build step, backend route, database, or runtime dependency.
Respect semantic HTML: preserve headings, landmarks, links, alt text, and accessible names.
Return JSON only:
{
  "summary": "short human-readable plan",
  "changes": [{"path": "relative/path.html", "content": "complete replacement content"}],
  "github_action": {"summary": "optional GitHub write-back or PR recommendation", "requires_approval": true},
  "cloudflare_action": {"summary": "optional DNS/tunnel/cache recommendation", "requires_approval": true}
}
Every changed file must be returned as complete replacement content."#
}

fn collect_site_context(root: &Path) -> io::Result<String> {
    let mut out = String::new();
    collect_site_context_inner(root, root, &mut out)?;
    if out.len() > MAX_CONTEXT_BYTES {
        out.truncate(MAX_CONTEXT_BYTES);
        out.push_str("\n\n[context truncated by Popovic]\n");
    }
    Ok(out)
}

fn collect_site_context_inner(root: &Path, dir: &Path, out: &mut String) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        if out.len() >= MAX_CONTEXT_BYTES {
            return Ok(());
        }
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_site_context_inner(root, &path, out)?;
            continue;
        }
        if !deploy::allowed_static_file(&path) || !text_like(&path) {
            continue;
        }
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let content = fs::read_to_string(&path).unwrap_or_default();
        out.push_str(&format!(
            "\n---FILE:{}---\n{}\n---END:{}---\n",
            relative.display(),
            content,
            relative.display()
        ));
    }
    Ok(())
}

fn text_like(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if matches!(name, "robots.txt" | "agents.txt") {
        return true;
    }
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or(""),
        "html" | "htm" | "css" | "js" | "mjs" | "json" | "txt" | "xml" | "svg"
    )
}

fn parse_proposal(content: &str) -> Result<AiProposalResponse, String> {
    let json_text = extract_json(content);
    serde_json::from_str(json_text)
        .map_err(|error| format!("AI did not return valid proposal JSON: {error}"))
}

fn extract_json(content: &str) -> &str {
    let trimmed = content.trim();
    if let Some(without_prefix) = trimmed.strip_prefix("```json") {
        return without_prefix.trim_end_matches("```").trim();
    }
    if let Some(without_prefix) = trimmed.strip_prefix("```") {
        return without_prefix.trim_end_matches("```").trim();
    }
    trimmed
}

fn sanitize_changes(root: &Path, changes: Vec<FileChange>) -> io::Result<Vec<FileChange>> {
    let mut sanitized = Vec::new();
    for change in changes {
        if change.path.contains("..") || change.path.starts_with('/') || change.path.contains('\\')
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unsafe path rejected: {}", change.path),
            ));
        }
        let path = root.join(&change.path);
        if let Some(parent) = path.parent() {
            if !parent.starts_with(root) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "unsafe path rejected",
                ));
            }
        }
        sanitized.push(change);
    }
    Ok(sanitized)
}

fn build_diff(root: &Path, changes: &[FileChange]) -> String {
    let mut diff = String::new();
    for change in changes {
        let old = fs::read_to_string(root.join(&change.path)).unwrap_or_default();
        diff.push_str(&format!("--- {}\n+++ {}\n", change.path, change.path));
        diff.push_str(&line_diff(&old, &change.content));
        diff.push('\n');
    }
    diff
}

fn line_diff(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    if old == new {
        return " no changes\n".to_string();
    }
    let mut out = String::new();
    let max = old_lines.len().max(new_lines.len());
    for index in 0..max {
        match (old_lines.get(index), new_lines.get(index)) {
            (Some(left), Some(right)) if left == right => {
                out.push_str(&format!(" {}\n", left));
            }
            (Some(left), Some(right)) => {
                out.push_str(&format!("-{}\n+{}\n", left, right));
            }
            (Some(left), None) => out.push_str(&format!("-{}\n", left)),
            (None, Some(right)) => out.push_str(&format!("+{}\n", right)),
            (None, None) => {}
        }
    }
    out
}
