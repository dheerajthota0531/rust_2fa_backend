//! Integration tests for the full workflow described in the spec:
//! seed users -> admin login+2FA -> create 5 tasks -> assign 3 to James Bond
//! -> James Bond login+2FA -> 403 on task creation -> view-my-tasks (miss then hit).
//!
//! These tests require a running Postgres + Redis instance, matching
//! `docker-compose.yml`. Run with:
//!
//!   docker compose up -d postgres redis
//!   DATABASE_URL=postgres://postgres:postgres@localhost:5432/task_api \
//!   REDIS_URL=redis://localhost:6379 \
//!   cargo test --test integration_test -- --test-threads=1
//!
//! Each test wipes relevant tables at startup so the suite is repeatable.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt; // for `oneshot`

use rust_backend_assessment_af2::config::Config;
use rust_backend_assessment_af2::{build_router, build_state};

async fn test_app() -> axum::Router {
    dotenvy::dotenv().ok();
    let config = Config::from_env();
    let state = build_state(config).await.expect("failed to build app state");

    // Reset state so the workflow is deterministic across test runs.
    sqlx::query("TRUNCATE TABLE email_logs, login_challenges, tasks, users CASCADE")
        .execute(&state.db)
        .await
        .expect("failed to truncate tables");

    build_router(state)
}

async fn send_json(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    builder = builder.header("content-type", "application/json");
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }

    let body_bytes = match body {
        Some(v) => serde_json::to_vec(&v).unwrap(),
        None => Vec::new(),
    };

    let request = builder.body(Body::from(body_bytes)).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json_body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, json_body)
}

async fn login_and_verify(app: &axum::Router, email: &str, password: &str) -> String {
    let (status, login_body) = send_json(
        app,
        "POST",
        "/auth/login",
        Some(json!({ "email": email, "password": password })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login should succeed: {login_body:?}");
    assert!(
        login_body.get("login_challenge_id").is_some(),
        "login must not return a JWT directly: {login_body:?}"
    );
    let challenge_id = login_body["login_challenge_id"].as_str().unwrap();

    let (status, email_log) = send_json(
        app,
        "GET",
        &format!("/dev/email-logs/latest?email={email}"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "email log fetch should succeed: {email_log:?}");
    let code = email_log["code"].as_str().unwrap().to_string();

    let (status, verify_body) = send_json(
        app,
        "POST",
        "/auth/verify-2fa",
        Some(json!({ "login_challenge_id": challenge_id, "code": code })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "2FA verification should succeed: {verify_body:?}");
    verify_body["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn full_workflow_admin_and_james_bond() {
    let app = test_app().await;

    // 1. Seed users
    let (status, _) = send_json(&app, "POST", "/seed/users", None, None).await;
    assert_eq!(status, StatusCode::OK);

    // 2-4. Admin login + 2FA
    let admin_token = login_and_verify(&app, "admin@example.com", "AdminPass123!").await;

    // 5. Create exactly 5 tasks as Admin
    let priorities = ["high", "medium", "low", "medium", "high"];
    let mut task_ids = Vec::new();
    for (i, priority) in priorities.iter().enumerate() {
        let (status, body) = send_json(
            &app,
            "POST",
            "/tasks",
            Some(json!({
                "title": format!("Task {}", i + 1),
                "description": "integration test task",
                "priority": priority
            })),
            Some(&admin_token),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "task creation should succeed: {body:?}");
        task_ids.push(body["id"].as_str().unwrap().to_string());
    }
    assert_eq!(task_ids.len(), 5);

    // 6. Assign exactly 3 tasks to James Bond
    let assigned_ids: Vec<&String> = task_ids.iter().take(3).collect();
    let (status, assign_body) = send_json(
        &app,
        "POST",
        "/tasks/assign",
        Some(json!({
            "task_ids": assigned_ids,
            "assignee_email": "jamesbond@example.com"
        })),
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "assignment should succeed: {assign_body:?}");
    assert_eq!(assign_body["assigned_count"], 3);

    // 7-8. James Bond login + 2FA
    let james_token = login_and_verify(&app, "jamesbond@example.com", "JamesPass123!").await;

    // 9. James Bond cannot create a task -> 403
    let (status, _) = send_json(
        &app,
        "POST",
        "/tasks",
        Some(json!({ "title": "Unauthorized task", "priority": "low" })),
        Some(&james_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // 10. James Bond views his tasks -> cache miss, exactly 3 tasks
    let (status, view_body) = send_json(
        &app,
        "GET",
        "/tasks/view-my-tasks",
        None,
        Some(&james_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view_body["cache"]["hit"], false);
    assert_eq!(view_body["summary"]["total_assigned_tasks"], 3);
    let tasks = view_body["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 3);
    for t in tasks {
        assert_eq!(t["assigned_to"], "jamesbond@example.com");
    }

    // 11. Calling it again -> cache hit
    let (status, view_body_2) = send_json(
        &app,
        "GET",
        "/tasks/view-my-tasks",
        None,
        Some(&james_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view_body_2["cache"]["hit"], true);
    assert_eq!(view_body_2["summary"]["total_assigned_tasks"], 3);
}

#[tokio::test]
async fn rejects_incorrect_and_reused_codes() {
    let app = test_app().await;
    send_json(&app, "POST", "/seed/users", None, None).await;

    let (_, login_body) = send_json(
        &app,
        "POST",
        "/auth/login",
        Some(json!({ "email": "admin@example.com", "password": "AdminPass123!" })),
        None,
    )
    .await;
    let challenge_id = login_body["login_challenge_id"].as_str().unwrap();

    // Wrong code rejected
    let (status, _) = send_json(
        &app,
        "POST",
        "/auth/verify-2fa",
        Some(json!({ "login_challenge_id": challenge_id, "code": "000000" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Correct code works
    let (_, email_log) = send_json(
        &app,
        "GET",
        "/dev/email-logs/latest?email=admin@example.com",
        None,
        None,
    )
    .await;
    let code = email_log["code"].as_str().unwrap();

    let (status, _) = send_json(
        &app,
        "POST",
        "/auth/verify-2fa",
        Some(json!({ "login_challenge_id": challenge_id, "code": code })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Reusing the same code again must fail
    let (status, _) = send_json(
        &app,
        "POST",
        "/auth/verify-2fa",
        Some(json!({ "login_challenge_id": challenge_id, "code": code })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn login_does_not_return_jwt_directly() {
    let app = test_app().await;
    send_json(&app, "POST", "/seed/users", None, None).await;

    let (status, body) = send_json(
        &app,
        "POST",
        "/auth/login",
        Some(json!({ "email": "admin@example.com", "password": "AdminPass123!" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("access_token").is_none());
    assert!(body.get("login_challenge_id").is_some());
}