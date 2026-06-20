use sqlx::PgPool;

use crate::config::Config;
use crate::redis_cache::Cache;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cache: Cache,
    pub config: Config,
}