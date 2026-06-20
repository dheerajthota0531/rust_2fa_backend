pub mod auth;
pub mod config;
pub mod errors;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod redis_cache;
pub mod state;

use axum::routing::{get, post};
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::redis_cache::Cache;
use crate::state::AppState;

pub async fn build_state(config: Config) -> anyhow::Result<AppState> {
    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&db).await?;

    let cache = Cache::new(&config.redis_url, config.cache_ttl_seconds)?;

    Ok(AppState { db, cache, config })
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/seed/users", post(handlers::seed::seed_users))
        .route("/auth/login", post(handlers::auth_handlers::login))
        .route("/auth/verify-2fa", post(handlers::auth_handlers::verify_2fa))
        .route("/dev/email-logs/latest", get(handlers::dev::latest_email_log))
        .route("/tasks", post(handlers::tasks::create_task))
        .route("/tasks/assign", post(handlers::tasks::assign_tasks))
        .route("/tasks/view-my-tasks", get(handlers::tasks::view_my_tasks))
        .route("/health", get(health))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}