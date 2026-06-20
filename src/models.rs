use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct User {
    pub id: Uuid,
    pub full_name: String,
    pub email: String,
    #[serde(skip_serializing)]
    pub hashed_password: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicUser {
    pub email: String,
    pub role: String,
}

impl From<&User> for PublicUser {
    fn from(u: &User) -> Self {
        Self {
            email: u.email.clone(),
            role: u.role.clone(),
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub created_by_id: Uuid,
    pub assigned_to_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskView {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub assigned_to: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LoginChallenge {
    pub id: Uuid,
    pub user_id: Uuid,
    pub code_hash: String,
    pub expires_at: DateTime<Utc>,
    pub used: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct EmailLog {
    pub id: Uuid,
    pub to_email: String,
    pub subject: String,
    pub code: String,
    pub login_challenge_id: Uuid,
    pub created_at: DateTime<Utc>,
}

// ---- Request / response DTOs ----

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub login_challenge_id: Uuid,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct Verify2faRequest {
    pub login_challenge_id: Uuid,
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct Verify2faResponse {
    pub access_token: String,
    pub token_type: String,
    pub user: PublicUser,
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_priority")]
    pub priority: String,
}

fn default_priority() -> String {
    "medium".to_string()
}

#[derive(Debug, Deserialize)]
pub struct AssignTasksRequest {
    pub task_ids: Vec<Uuid>,
    pub assignee_email: String,
}

#[derive(Debug, Serialize)]
pub struct AssignTasksResponse {
    pub assigned_count: usize,
    pub assignee_email: String,
}

#[derive(Debug, Serialize)]
pub struct CacheMeta {
    pub hit: bool,
}

#[derive(Debug, Serialize)]
pub struct TaskSummary {
    pub total_assigned_tasks: usize,
}

#[derive(Debug, Serialize)]
pub struct ViewMyTasksResponse {
    pub user: PublicUser,
    pub tasks: Vec<TaskView>,
    pub summary: TaskSummary,
    pub cache: CacheMeta,
}