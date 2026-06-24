use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const RETENTION_DAYS: i64 = 7;
pub const DEFAULT_DASHBOARD_ADDR: &str = "127.0.0.1:7626";
pub const DEFAULT_STATIC_ADDR: &str = "127.0.0.1:7627";
pub const CLOUDFLARE_MCP_URL: &str = "https://mcp.cloudflare.com/mcp";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub dashboard_addr: String,
    pub static_addr: String,
    pub apps: Vec<StaticApp>,
    pub settings: Settings,
    pub agent_tasks: Vec<AgentTask>,
    pub metric_buckets: Vec<MetricBucket>,
    pub alerts: Vec<Alert>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dashboard_addr: DEFAULT_DASHBOARD_ADDR.to_string(),
            static_addr: DEFAULT_STATIC_ADDR.to_string(),
            apps: Vec::new(),
            settings: Settings::default(),
            agent_tasks: Vec::new(),
            metric_buckets: Vec::new(),
            alerts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticApp {
    pub id: Uuid,
    pub name: String,
    pub repo_url: String,
    pub repo_subdir: String,
    pub hostnames: Vec<String>,
    pub current_release: Option<String>,
    pub previous_release: Option<String>,
    pub last_deploy_at: Option<DateTime<Utc>>,
    pub last_deploy_status: DeployStatus,
    pub health: HealthState,
    pub container_mode: ContainerMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeployStatus {
    NeverDeployed,
    Succeeded,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthState {
    Unknown,
    Healthy,
    Unhealthy(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContainerMode {
    Disabled,
    ExternalOrigin { local_url: String },
}

impl StaticApp {
    pub fn new(
        name: String,
        repo_url: String,
        repo_subdir: String,
        hostnames: Vec<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            repo_url,
            repo_subdir,
            hostnames,
            current_release: None,
            previous_release: None,
            last_deploy_at: None,
            last_deploy_status: DeployStatus::NeverDeployed,
            health: HealthState::Unknown,
            container_mode: ContainerMode::Disabled,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub ai_provider: AiProvider,
    pub ai_api_key_sealed: String,
    pub github_oauth_token_sealed: String,
    pub github_mcp_url: String,
    pub github_default_repo: String,
    pub github_write_policy: WriteBackPolicy,
    pub cloudflare_api_token_sealed: String,
    pub cloudflare_account_id: String,
    pub cloudflare_zone_id: String,
    pub cloudflare_tunnel_id: String,
    pub cloudflare_mcp_url: String,
    pub cloudflare_allow_dns_changes: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ai_provider: AiProvider::OpenAi,
            ai_api_key_sealed: String::new(),
            github_oauth_token_sealed: String::new(),
            github_mcp_url: String::new(),
            github_default_repo: String::new(),
            github_write_policy: WriteBackPolicy::AskEveryTime,
            cloudflare_api_token_sealed: String::new(),
            cloudflare_account_id: String::new(),
            cloudflare_zone_id: String::new(),
            cloudflare_tunnel_id: String::new(),
            cloudflare_mcp_url: CLOUDFLARE_MCP_URL.to_string(),
            cloudflare_allow_dns_changes: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AiProvider {
    OpenAi,
    Anthropic,
    Antigravity,
    Cursor,
}

impl AiProvider {
    pub fn from_form_value(value: &str) -> Self {
        match value {
            "anthropic" => Self::Anthropic,
            "antigravity" => Self::Antigravity,
            "cursor" => Self::Cursor,
            _ => Self::OpenAi,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
            Self::Antigravity => "antigravity",
            Self::Cursor => "cursor",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WriteBackPolicy {
    AskEveryTime,
    SkipAskFor24Hours { until: DateTime<Utc> },
    LocalOnly,
}

impl Default for WriteBackPolicy {
    fn default() -> Self {
        Self::AskEveryTime
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricBucket {
    pub start: DateTime<Utc>,
    pub host: HostMetrics,
    pub red: RedMetrics,
    #[serde(default)]
    pub app_red: BTreeMap<Uuid, RedMetrics>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostMetrics {
    pub cpu_percent: f32,
    pub ram_percent: f32,
    pub load1: f32,
    pub disk_percent: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RedMetrics {
    pub requests: u64,
    pub errors: u64,
    pub p95_ms: f32,
    pub p99_ms: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub severity: AlertSeverity,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

impl Alert {
    pub fn warning(source: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            severity: AlertSeverity::Warning,
            source: source.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub app_id: Uuid,
    pub prompt: String,
    pub status: AgentTaskStatus,
    pub plan: String,
    pub diff: String,
    pub changes: Vec<FileChange>,
    pub github_action: ExternalAction,
    pub cloudflare_action: ExternalAction,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentTaskStatus {
    Proposed,
    Approved,
    Rejected,
    Applied,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalAction {
    pub summary: String,
    pub requires_approval: bool,
}

impl Default for ExternalAction {
    fn default() -> Self {
        Self {
            summary: "No external action proposed.".to_string(),
            requires_approval: false,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DashboardSnapshot {
    pub running_apps: Vec<AppSnapshot>,
    pub tunnel_status: TunnelStatus,
    pub cpu_percent: f32,
    pub ram_percent: f32,
    pub alerts: Vec<Alert>,
    pub use_red: Vec<MetricBucket>,
}

#[derive(Debug, Serialize)]
pub struct AppSnapshot {
    pub id: Uuid,
    pub name: String,
    pub domains: Vec<String>,
    pub status: String,
    pub last_deploy: String,
    pub requests_5m: u64,
    pub errors_5m: u64,
}

#[derive(Debug, Serialize)]
pub struct TunnelStatus {
    pub status: String,
    pub tunnel_id: String,
    pub route_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct AiProposalResponse {
    pub summary: String,
    pub changes: Vec<FileChange>,
    #[serde(default)]
    pub github_action: Option<ExternalAction>,
    #[serde(default)]
    pub cloudflare_action: Option<ExternalAction>,
}
