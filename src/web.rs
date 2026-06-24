use crate::{
    agent::AgentEngine,
    deploy,
    models::{AiProvider, WriteBackPolicy},
    secret,
    store::Store,
    ui,
};
use axum::{
    Json, Router,
    extract::{Form, Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone)]
pub struct WebState {
    pub store: Store,
    pub agent: AgentEngine,
}

pub fn dashboard_router(state: WebState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/status", get(status))
        .route("/api/settings", get(settings_status))
        .route("/api/settings", post(save_settings))
        .route("/api/apps", post(register_app))
        .route("/api/apps/{id}/deploy", post(deploy_app))
        .route("/api/apps/{id}/rollback", post(rollback_app))
        .route("/api/tasks", get(tasks))
        .route("/api/tasks", post(propose_task))
        .route("/api/tasks/{id}/approve", post(approve_task))
        .route("/api/tasks/{id}/reject", post(reject_task))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(ui::INDEX)
}

async fn status(State(state): State<WebState>) -> Json<crate::models::DashboardSnapshot> {
    Json(state.store.dashboard_snapshot())
}

#[derive(Serialize)]
struct SettingsStatus {
    ai_provider: String,
    ai_api_key_configured: bool,
    github_token_configured: bool,
    github_mcp_url: String,
    github_default_repo: String,
    cloudflare_token_configured: bool,
    cloudflare_account_id: String,
    cloudflare_zone_id: String,
    cloudflare_tunnel_id: String,
    cloudflare_mcp_url: String,
}

async fn settings_status(State(state): State<WebState>) -> Json<SettingsStatus> {
    let settings = state.store.read().settings;
    Json(SettingsStatus {
        ai_provider: settings.ai_provider.as_str().to_string(),
        ai_api_key_configured: secret::has_secret(&settings.ai_api_key_sealed),
        github_token_configured: secret::has_secret(&settings.github_oauth_token_sealed),
        github_mcp_url: settings.github_mcp_url,
        github_default_repo: settings.github_default_repo,
        cloudflare_token_configured: secret::has_secret(&settings.cloudflare_api_token_sealed),
        cloudflare_account_id: settings.cloudflare_account_id,
        cloudflare_zone_id: settings.cloudflare_zone_id,
        cloudflare_tunnel_id: settings.cloudflare_tunnel_id,
        cloudflare_mcp_url: settings.cloudflare_mcp_url,
    })
}

#[derive(Deserialize)]
struct SettingsForm {
    ai_provider: String,
    ai_api_key: String,
    github_oauth_token: String,
    github_mcp_url: String,
    github_default_repo: String,
    cloudflare_api_token: String,
    cloudflare_account_id: String,
    cloudflare_zone_id: String,
    cloudflare_tunnel_id: String,
    cloudflare_mcp_url: String,
}

async fn save_settings(
    State(state): State<WebState>,
    Form(form): Form<SettingsForm>,
) -> Result<Json<SettingsStatus>, AppError> {
    state.store.update(|config| {
        config.settings.ai_provider = AiProvider::from_form_value(&form.ai_provider);
        if !form.ai_api_key.trim().is_empty() {
            config.settings.ai_api_key_sealed = secret::seal(form.ai_api_key.trim());
        }
        if !form.github_oauth_token.trim().is_empty() {
            config.settings.github_oauth_token_sealed =
                secret::seal(form.github_oauth_token.trim());
        }
        config.settings.github_mcp_url = form.github_mcp_url.trim().to_string();
        config.settings.github_default_repo = form.github_default_repo.trim().to_string();
        config.settings.github_write_policy = WriteBackPolicy::AskEveryTime;
        if !form.cloudflare_api_token.trim().is_empty() {
            config.settings.cloudflare_api_token_sealed =
                secret::seal(form.cloudflare_api_token.trim());
        }
        config.settings.cloudflare_account_id = form.cloudflare_account_id.trim().to_string();
        config.settings.cloudflare_zone_id = form.cloudflare_zone_id.trim().to_string();
        config.settings.cloudflare_tunnel_id = form.cloudflare_tunnel_id.trim().to_string();
        config.settings.cloudflare_mcp_url = form.cloudflare_mcp_url.trim().to_string();
    })?;
    Ok(settings_status(State(state)).await)
}

#[derive(Deserialize)]
struct RegisterAppForm {
    name: String,
    repo_url: String,
    repo_subdir: String,
    hostnames: String,
}

async fn register_app(
    State(state): State<WebState>,
    Form(form): Form<RegisterAppForm>,
) -> Result<Json<crate::models::DashboardSnapshot>, AppError> {
    let hostnames = form
        .hostnames
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    let app_id = deploy::register_app(
        &state.store,
        form.name.trim().to_string(),
        form.repo_url.trim().to_string(),
        form.repo_subdir.trim().to_string(),
        hostnames,
    )?;
    if let Err(error) = deploy::deploy_app(&state.store, app_id) {
        deploy::deploy_failed(&state.store, app_id, error.to_string());
        return Err(AppError::bad_request(error.to_string()));
    }
    Ok(Json(state.store.dashboard_snapshot()))
}

async fn deploy_app(
    State(state): State<WebState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::models::DashboardSnapshot>, AppError> {
    if let Err(error) = deploy::deploy_app(&state.store, id) {
        deploy::deploy_failed(&state.store, id, error.to_string());
        return Err(AppError::bad_request(error.to_string()));
    }
    Ok(Json(state.store.dashboard_snapshot()))
}

async fn rollback_app(
    State(state): State<WebState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::models::DashboardSnapshot>, AppError> {
    deploy::rollback_app(&state.store, id)?;
    Ok(Json(state.store.dashboard_snapshot()))
}

async fn tasks(State(state): State<WebState>) -> Json<Vec<crate::models::AgentTask>> {
    Json(state.store.read().agent_tasks)
}

#[derive(Deserialize)]
struct TaskForm {
    app_id: Uuid,
    prompt: String,
}

async fn propose_task(
    State(state): State<WebState>,
    Form(form): Form<TaskForm>,
) -> Result<Json<crate::models::AgentTask>, AppError> {
    let task = state
        .agent
        .propose(&state.store, form.app_id, form.prompt)
        .await?;
    Ok(Json(task))
}

async fn approve_task(
    State(state): State<WebState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::models::AgentTask>, AppError> {
    let task = state.agent.approve(&state.store, id)?;
    Ok(Json(task))
}

async fn reject_task(
    State(state): State<WebState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::models::AgentTask>, AppError> {
    let task = state.agent.reject(&state.store, id)?;
    Ok(Json(task))
}

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    message: String,
}

impl AppError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::bad_request(error.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.status, self.message).into_response()
    }
}
