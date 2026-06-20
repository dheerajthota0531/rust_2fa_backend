use axum::extract::State;
use axum::Json;
use serde_json::json;

use crate::auth::hash_password;
use crate::errors::ApiResult;
use crate::models::User;
use crate::state::AppState;

/// POST /seed/users
/// Idempotent: if Admin / James Bond already exist, they are left untouched.
pub async fn seed_users(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let admin = upsert_user(
        &state,
        "Admin",
        "admin@example.com",
        "AdminPass123!",
        "admin",
    )
    .await?;

    let james = upsert_user(
        &state,
        "James Bond",
        "jamesbond@example.com",
        "JamesPass123!",
        "staff",
    )
    .await?;

    Ok(Json(json!({
        "message": "users seeded",
        "users": [
            { "email": admin.email, "role": admin.role },
            { "email": james.email, "role": james.role },
        ]
    })))
}

async fn upsert_user(
    state: &AppState,
    full_name: &str,
    email: &str,
    plain_password: &str,
    role: &str,
) -> ApiResult<User> {
    if let Some(existing) = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
        .bind(email)
        .fetch_optional(&state.db)
        .await?
    {
        return Ok(existing);
    }

    let hashed = hash_password(plain_password)?;

    let user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (full_name, email, hashed_password, role)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(full_name)
    .bind(email)
    .bind(hashed)
    .bind(role)
    .fetch_one(&state.db)
    .await?;

    Ok(user)
}