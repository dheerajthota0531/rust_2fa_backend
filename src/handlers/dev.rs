use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::errors::{ApiError, ApiResult};
use crate::models::EmailLog;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct EmailLogQuery {
    pub email: String,
}

/// GET /dev/email-logs/latest?email=...
/// Development-only endpoint. Returns the most recently sent verification
/// email for the given address, including the plain-text one-time code, so
/// that the 2FA flow can be exercised end-to-end without real email delivery.
/// This route must be disabled / removed before production deployment.
pub async fn latest_email_log(
    State(state): State<AppState>,
    Query(params): Query<EmailLogQuery>,
) -> ApiResult<Json<EmailLog>> {
    let log = sqlx::query_as::<_, EmailLog>(
        r#"
        SELECT * FROM email_logs
        WHERE to_email = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(&params.email)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::NotFound("no email logs found for this address".into()))?;

    Ok(Json(log))
}