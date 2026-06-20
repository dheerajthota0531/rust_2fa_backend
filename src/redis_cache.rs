use redis::AsyncCommands;
use uuid::Uuid;

use crate::errors::ApiError;

#[derive(Clone)]
pub struct Cache {
    client: redis::Client,
    ttl_seconds: u64,
}

fn task_cache_key(user_id: Uuid) -> String {
    format!("tasks:user:{user_id}")
}

impl Cache {
    pub fn new(redis_url: &str, ttl_seconds: u64) -> Result<Self, ApiError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| ApiError::Internal(format!("redis client error: {e}")))?;
        Ok(Self { client, ttl_seconds })
    }

    async fn conn(&self) -> Result<redis::aio::MultiplexedConnection, ApiError> {
        self.client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| ApiError::Internal(format!("redis connection error: {e}")))
    }

    /// Returns Some(json_string) on cache hit, None on miss.
    pub async fn get_tasks(&self, user_id: Uuid) -> Result<Option<String>, ApiError> {
        let mut conn = self.conn().await?;
        let val: Option<String> = conn
            .get(task_cache_key(user_id))
            .await
            .map_err(|e| ApiError::Internal(format!("redis get error: {e}")))?;
        Ok(val)
    }

    pub async fn set_tasks(&self, user_id: Uuid, payload: &str) -> Result<(), ApiError> {
        let mut conn = self.conn().await?;
        conn.set_ex::<_, _, ()>(task_cache_key(user_id), payload, self.ttl_seconds)
            .await
            .map_err(|e| ApiError::Internal(format!("redis set error: {e}")))?;
        Ok(())
    }

    /// Invalidates the cached task list for a given user. Call this whenever
    /// that user's assigned tasks change (assignment, status update, etc).
    pub async fn invalidate_tasks(&self, user_id: Uuid) -> Result<(), ApiError> {
        let mut conn = self.conn().await?;
        conn.del::<_, ()>(task_cache_key(user_id))
            .await
            .map_err(|e| ApiError::Internal(format!("redis del error: {e}")))?;
        Ok(())
    }
}