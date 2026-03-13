use ama::config::AmaConfig;
use ama::server::{AppState, build_router, shutdown_signal};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let config = AmaConfig::load(Path::new("config"))?;
    tracing::info!(hashes = ?config.boot_hashes, "Boot integrity verified");

    let cleaned = ama::actuator::file::cleanup_orphan_temps(&config.workspace_root);
    if cleaned > 0 {
        tracing::warn!(count = cleaned, "Cleaned up orphan temp files from previous session");
    }

    let bind_addr = format!("{}:{}", config.bind_host, config.bind_port);
    let state = AppState::new(config);

    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(addr = %bind_addr, "AMA P0 listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("AMA P0 shut down cleanly");
    Ok(())
}
