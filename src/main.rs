use task_api::config::Config;
use task_api::{build_router, build_state};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();
    let port = config.server_port;

    let state = build_state(config).await?;
    let app = build_router(state);

    let addr = format!("0.0.0.0:{port}");
    tracing::info!("rust_backend_assessment_af2 listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}