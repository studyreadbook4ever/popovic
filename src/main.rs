mod agent;
mod bootstrap;
mod deploy;
mod mcp;
mod metrics;
mod models;
mod secret;
mod static_http;
mod store;
mod ui;
mod web;

use agent::AgentEngine;
use axum::Router;
use std::{net::SocketAddr, process};
use store::Store;
use web::WebState;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("popovic failed: {error}");
        process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let store = Store::open()?;
    bootstrap::run(&store)?;
    let config = store.read();
    let dashboard_addr: SocketAddr = config.dashboard_addr.parse()?;
    let static_addr: SocketAddr = config.static_addr.parse()?;

    let agent = AgentEngine::new();
    let web_state = WebState {
        store: store.clone(),
        agent,
    };

    tokio::spawn(metrics::run_collector(store.clone()));

    let dashboard = web::dashboard_router(web_state);
    let static_apps = Router::new()
        .fallback(static_http::serve_static_app)
        .with_state(store);

    let dashboard_listener = tokio::net::TcpListener::bind(dashboard_addr).await?;
    let static_listener = tokio::net::TcpListener::bind(static_addr).await?;

    println!("Popovic dashboard: http://{dashboard_addr}");
    println!("Popovic static origin: http://{static_addr}");

    let dashboard_task =
        tokio::spawn(async move { axum::serve(dashboard_listener, dashboard).await });
    let static_task = tokio::spawn(async move { axum::serve(static_listener, static_apps).await });

    tokio::select! {
        result = dashboard_task => result??,
        result = static_task => result??,
        _ = tokio::signal::ctrl_c() => {}
    }
    Ok(())
}
