use axum::extract::State;
use axum::Json;

use crate::errors::{ApiError, ApiResult};
use crate::middleware::{require_admin, AuthUser};
use crate::models::{
    AssignTasksRequest, AssignTasksResponse, CacheMeta, CreateTaskRequest, PublicUser, Task,
    TaskSummary, TaskView, User, ViewMyTasksResponse,
};
use crate::state::AppState;

/// POST /tasks (Admin only)
pub async fn create_task(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<CreateTaskRequest>,
) -> ApiResult<Json<Task>> {
    require_admin(&user)?;

    if body.title.trim().is_empty() {
        return Err(ApiError::BadRequest("title is required".into()));
    }

    let allowed_priorities = ["low", "medium", "high"];
    if !allowed_priorities.contains(&body.priority.as_str()) {
        return Err(ApiError::BadRequest(
            "priority must be one of: low, medium, high".into(),
        ));
    }

    let task = sqlx::query_as::<_, Task>(
        r#"
        INSERT INTO tasks (title, description, priority, created_by_id, status)
        VALUES ($1, $2, $3, $4, 'todo')
        RETURNING *
        "#,
    )
    .bind(&body.title)
    .bind(&body.description)
    .bind(&body.priority)
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(task))
}

/// POST /tasks/assign (Admin only)
/// Assigns the given task_ids to the user identified by assignee_email and
/// invalidates that user's cached task list so the next read reflects the
/// new assignment.
pub async fn assign_tasks(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<AssignTasksRequest>,
) -> ApiResult<Json<AssignTasksResponse>> {
    require_admin(&user)?;

    if body.task_ids.is_empty() {
        return Err(ApiError::BadRequest("task_ids must not be empty".into()));
    }

    let assignee = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
        .bind(&body.assignee_email)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| ApiError::NotFound("assignee not found".into()))?;

    let updated = sqlx::query(
        r#"
        UPDATE tasks
        SET assigned_to_id = $1, updated_at = now()
        WHERE id = ANY($2)
        "#,
    )
    .bind(assignee.id)
    .bind(&body.task_ids)
    .execute(&state.db)
    .await?;

    // Cache invalidation: the assignee's task list has changed.
    state.cache.invalidate_tasks(assignee.id).await?;

    Ok(Json(AssignTasksResponse {
        assigned_count: updated.rows_affected() as usize,
        assignee_email: assignee.email,
    }))
}

/// GET /tasks/view-my-tasks
/// Returns the tasks assigned to the authenticated user. Backed by a
/// per-user Redis cache: the first call is a DB read (cache.hit = false),
/// subsequent calls within the TTL are served from cache (cache.hit = true)
/// until the cache is invalidated by an assignment/update.
pub async fn view_my_tasks(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<ViewMyTasksResponse>> {
    let public_user = PublicUser {
        email: user.email.clone(),
        role: user.role.clone(),
    };

    if let Some(cached_json) = state.cache.get_tasks(user.user_id).await? {
        let tasks: Vec<TaskView> = serde_json::from_str(&cached_json)
            .map_err(|e| ApiError::Internal(format!("failed to deserialize cached tasks: {e}")))?;
        let summary = TaskSummary {
            total_assigned_tasks: tasks.len(),
        };
        return Ok(Json(ViewMyTasksResponse {
            user: public_user,
            tasks,
            summary,
            cache: CacheMeta { hit: true },
        }));
    }

    let rows = sqlx::query_as::<_, Task>(
        r#"
        SELECT * FROM tasks
        WHERE assigned_to_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(user.user_id)
    .fetch_all(&state.db)
    .await?;

    let tasks: Vec<TaskView> = rows
        .into_iter()
        .map(|t| TaskView {
            id: t.id,
            title: t.title,
            status: t.status,
            priority: t.priority,
            assigned_to: user.email.clone(),
        })
        .collect();

    let serialized = serde_json::to_string(&tasks)
        .map_err(|e| ApiError::Internal(format!("failed to serialize tasks for cache: {e}")))?;
    state.cache.set_tasks(user.user_id, &serialized).await?;

    let summary = TaskSummary {
        total_assigned_tasks: tasks.len(),
    };

    Ok(Json(ViewMyTasksResponse {
        user: public_user,
        tasks,
        summary,
        cache: CacheMeta { hit: false },
    }))
}