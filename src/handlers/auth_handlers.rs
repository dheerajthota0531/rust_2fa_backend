use axum::extract::State;
use axum::Json;
use chrono::{Duration, Utc};

use crate::auth::{generate_otp_code, hash_code, issue_jwt, verify_password};
use crate::errors::{ApiError, ApiResult};
use crate::models::{
    LoginChallenge, LoginRequest, LoginResponse, PublicUser, User, Verify2faRequest,
    Verify2faResponse,
};
use crate::state::AppState;

/// POST /auth/login
/// Validates email/password, creates a 2FA challenge and "sends" an email
/// (recorded in email_logs) containing a one-time code. Does NOT return a JWT.
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> ApiResult<Json<LoginResponse>> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
        .bind(&body.email)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| ApiError::Unauthorized("invalid email or password".into()))?;

    let valid = verify_password(&body.password, &user.hashed_password)?;
    if !valid {
        return Err(ApiError::Unauthorized("invalid email or password".into()));
    }

    let code = generate_otp_code();
    let code_hash = hash_code(&code);
    let expires_at = Utc::now() + Duration::seconds(state.config.two_factor_code_ttl_seconds);

    let challenge = sqlx::query_as::<_, LoginChallenge>(
        r#"
        INSERT INTO login_challenges (user_id, code_hash, expires_at)
        VALUES ($1, $2, $3)
        RETURNING *
        "#,
    )
    .bind(user.id)
    .bind(&code_hash)
    .bind(expires_at)
    .fetch_one(&state.db)
    .await?;

    // Simulate sending the email by recording it. In a real deployment this
    // would call out to SMTP / a transactional email provider instead.
    sqlx::query(
        r#"
        INSERT INTO email_logs (to_email, subject, code, login_challenge_id)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(&user.email)
    .bind("Your verification code")
    .bind(&code)
    .bind(challenge.id)
    .execute(&state.db)
    .await?;

    tracing::info!(email = %user.email, "2FA code generated (dev mode, see email_logs)");

    Ok(Json(LoginResponse {
        login_challenge_id: challenge.id,
        message: "verification code sent to email".to_string(),
    }))
}

/// POST /auth/verify-2fa
/// Verifies the one-time code against the stored hash, enforces expiry and
/// single-use, then issues a JWT access token.
pub async fn verify_2fa(
    State(state): State<AppState>,
    Json(body): Json<Verify2faRequest>,
) -> ApiResult<Json<Verify2faResponse>> {
    let challenge =
        sqlx::query_as::<_, LoginChallenge>("SELECT * FROM login_challenges WHERE id = $1")
            .bind(body.login_challenge_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| ApiError::BadRequest("invalid login challenge".into()))?;

    if challenge.used {
        return Err(ApiError::BadRequest("this code has already been used".into()));
    }

    if Utc::now() > challenge.expires_at {
        return Err(ApiError::BadRequest("verification code has expired".into()));
    }

    let provided_hash = hash_code(&body.code);
    if provided_hash != challenge.code_hash {
        return Err(ApiError::Unauthorized("incorrect verification code".into()));
    }

    // Mark used atomically with a WHERE used = false guard to prevent replay
    // under concurrent requests.
    let updated = sqlx::query(
        "UPDATE login_challenges SET used = true WHERE id = $1 AND used = false",
    )
    .bind(challenge.id)
    .execute(&state.db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(ApiError::BadRequest("this code has already been used".into()));
    }

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(challenge.user_id)
        .fetch_one(&state.db)
        .await?;

    let token = issue_jwt(
        user.id,
        &user.email,
        &user.role,
        &state.config.jwt_secret,
        state.config.jwt_expiry_seconds,
    )?;

    Ok(Json(Verify2faResponse {
        access_token: token,
        token_type: "Bearer".to_string(),
        user: PublicUser::from(&user),
    }))
}