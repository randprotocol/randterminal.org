use terminal_api::{routes::create_router, state::AppState, ApiConfig};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();
    let cfg = ApiConfig::from_env();
    tracing::info!(
        "terminal-api {} (circuits {}) on {}",
        env!("CARGO_PKG_VERSION"),
        terminal_api::CIRCUITS_REV,
        cfg.listen_addr
    );
    if std::env::var_os("TERMINAL_SANDBOX_IMAGE").is_none() {
        tracing::warn!(
            "TERMINAL_SANDBOX_IMAGE is not set; using the development default {} (production sets the tag sandbox/tag.sh prints)",
            cfg.sandbox_image
        );
    }
    let runner: std::sync::Arc<dyn terminal_sandbox::Runner> = std::sync::Arc::new(
        terminal_sandbox::DockerRunner::new(cfg.sandbox_image.clone()),
    );
    let state = AppState::new(cfg.clone(), runner);
    state.store.clone().spawn_sweeper();
    state.limiter.clone().spawn_sweeper();
    let listener = tokio::net::TcpListener::bind(cfg.listen_addr).await?;
    axum::serve(
        listener,
        create_router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
