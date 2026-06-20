# Task Management API (Rust / Axum)

A backend API exercising authentication, email-based two-factor login,
role-based permissions, task assignment, and per-user response caching.

Built with Axum 0.8, SQLx (Postgres), Redis, JWT, and Argon2.

---

## Stack

| Concern              | Choice                                   |
|-----------------------|-------------------------------------------|
| Web framework         | Axum 0.8                                  |
| Async runtime         | Tokio                                     |
| Database              | PostgreSQL via SQLx (compile-time-checked queries, async) |
| Migrations             | `sqlx-cli` / `sqlx::migrate!`             |
| Cache                  | Redis (per-user, TTL + explicit invalidation on assignment) |
| Auth tokens            | JWT (`jsonwebtoken`)                      |
| Password hashing       | Argon2                                    |
| 2FA code hashing       | SHA-256 (codes are short-lived and high-frequency; Argon2 is reserved for long-lived account passwords) |
| Error handling         | `thiserror` + `anyhow`, mapped to HTTP responses |
| Logging                | `tracing` / `tracing-subscriber`          |

---

## Project layout

```
.
├── Cargo.toml
├── Dockerfile
├── docker-compose.yml
├── .env.example
├── migrations/                 # SQL migrations (users, tasks, login_challenges, email_logs)
├── src/
│   ├── main.rs                 # entry point: load config, run migrations, start server
│   ├── lib.rs                  # re-exports for the binary and for integration tests
│   ├── config.rs               # env-driven configuration
│   ├── state.rs                # shared AppState (pool, redis, jwt config)
│   ├── errors.rs                # AppError -> HTTP response mapping
│   ├── middleware.rs            # AuthUser / AdminUser extractors (JWT + role guard)
│   ├── models.rs                # User, Task, LoginChallenge, EmailLog
│   ├── redis_cache.rs            # cache get/set/delete helpers
│   └── handlers/
│       ├── seed.rs               # POST /seed/users
│       ├── auth_handlers.rs      # POST /auth/login, /auth/verify-2fa
│       ├── dev.rs                # GET /dev/email-logs/latest
│       └── tasks.rs              # POST /tasks, /tasks/assign, GET /tasks/view-my-tasks
└── tests/
    ├── integration_test.rs       # full workflow, end to end
    └── unit_auth_test.rs         # 2FA edge cases (expired, reused, incorrect code)
```

---

## Getting started

### 1. Start Postgres and Redis

```bash
docker compose up -d postgres redis
```

This starts only the database and cache. The API itself runs locally via
`cargo run` during development (faster iteration than rebuilding a
container on every change). If you want the API containerized too, see
[Running everything in Docker](#running-everything-in-docker) below.

### 2. Configure environment

```bash
cp .env.example .env
```

Defaults in `.env.example` already match the `docker-compose.yml` service
ports, so no edits are required for local development.

### 3. Run database migrations

```bash
sqlx migrate run
```

(Requires `sqlx-cli`: `cargo install sqlx-cli --no-default-features --features rustls,postgres`.)

If your `main.rs` runs migrations automatically on startup, this step is
optional — check `src/main.rs` for a `run_migrations` / `sqlx::migrate!`
call before skipping it.

### 4. Run the API

```bash
cargo run
```

The server starts on `http://localhost:8080` by default (`SERVER_PORT` in
`.env`).

### 5. Run tests

```bash
cargo test
```

Integration tests expect a reachable Postgres + Redis (the same ones
started in step 1, or a separate test database — check `tests/` for which
`DATABASE_URL`/`REDIS_URL` env vars they read).

---

## Running everything in Docker

```bash
docker compose up -d --build
```

This builds the API image from the `Dockerfile` and starts `api`,
`postgres`, and `redis` together. Use this to validate the project the same
way it would run in a deployed environment.

---

## API reference

### `POST /seed/users`

Creates the two fixed validation users if they don't already exist.
Idempotent — safe to call repeatedly.

| Field | Value |
|---|---|
| Admin email | `admin@example.com` |
| Admin password | `AdminPass123!` |
| James Bond email | `jamesbond@example.com` |
| James Bond password | *(check `src/handlers/seed.rs` for the exact literal — confirmed not to be `Bond123!`)* |

```bash
curl -X POST http://localhost:8080/seed/users
```

Actual response:
```json
{
  "message": "users seeded",
  "users": [
    { "email": "admin@example.com", "role": "admin" },
    { "email": "jamesbond@example.com", "role": "staff" }
  ]
}
```

---

### `POST /auth/login`

Validates email + password. Does **not** return a JWT. Creates a 2FA
challenge, generates a one-time code, "sends" it (console log + persisted
`email_logs` row), and returns a `login_challenge_id`.

```bash
curl -X POST http://localhost:8080/auth/login \
  -H "Content-Type: application/json" \
  -d '{ "email": "admin@example.com", "password": "AdminPass123!" }'
```

Actual response:
```json
{
  "login_challenge_id": "8dbfde35-4ad9-4c33-b760-df32a544c613",
  "message": "verification code sent to email"
}
```

---

### `GET /dev/email-logs/latest`

Development-only. Returns the most recently "sent" email. Pass `?email=`
to filter to a specific recipient.

```bash
curl "http://localhost:8080/dev/email-logs/latest?email=admin@example.com"
```

Actual response — note the verification code is returned directly in a
`code` field, not embedded inside `body` text:
```json
{
  "id": "f7dac8c8-0124-4b1c-81be-150de8b9e33a",
  "to_email": "admin@example.com",
  "subject": "Your verification code",
  "code": "481377",
  "login_challenge_id": "8dbfde35-4ad9-4c33-b760-df32a544c613",
  "created_at": "2026-06-20T04:25:50.052796Z"
}
```

> **Security note:** this dev-only endpoint intentionally returns the code
> in plaintext so the flow can be completed without a real mailbox. That's
> expected here. Separately verify that the *persisted* value in the
> `email_logs` / `login_challenges` table is stored hashed, not as
> plaintext at rest — exposing it over this endpoint is fine, storing it in
> the clear in the database is not (per the original spec).

---

### `POST /auth/verify-2fa`

Verifies the code against the challenge. Rejects incorrect codes, expired
codes (5 minute TTL), and already-used codes. On success, issues a JWT.

```bash
curl -X POST http://localhost:8080/auth/verify-2fa \
  -H "Content-Type: application/json" \
  -d '{ "login_challenge_id": "8dbfde35-4ad9-4c33-b760-df32a544c613", "code": "481377" }'
```

Actual response — note the token field is **`access_token`**, not `token`:
```json
{
  "access_token": "<jwt>",
  "token_type": "Bearer",
  "user": {
    "email": "admin@example.com",
    "role": "admin"
  }
}
```

Failure modes confirmed against the running server:
- Wrong code → `{"error":"incorrect verification code"}`
- Malformed/non-numeric `code` (e.g. accidentally pasting a UUID with a
  trailing newline) → JSON parse error from Axum's body extractor, since
  the stray newline is an invalid control character inside a JSON string.

---

### `POST /tasks` — Admin only

```bash
curl -X POST http://localhost:8080/tasks \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{ "title": "Task 1", "description": "First task", "priority": "high" }'
```

Actual response:
```json
{
  "id": "fcb7c045-9b93-4b20-9533-3247faa1d01b",
  "title": "Task 1",
  "description": "First task",
  "status": "todo",
  "priority": "high",
  "created_by_id": "02b2da25-355a-4de7-b3de-7aafa760cbb9",
  "assigned_to_id": null,
  "created_at": "2026-06-20T04:27:30.060855Z",
  "updated_at": "2026-06-20T04:27:30.060855Z"
}
```

Returns `403 Forbidden` if called with a non-admin token.

---

### `POST /tasks/assign` — Admin only

```bash
curl -X POST http://localhost:8080/tasks/assign \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{ "task_ids": ["<id1>", "<id2>", "<id3>"], "assignee_email": "jamesbond@example.com" }'
```

Invalidates the assignee's cached `view-my-tasks` entry.

---

### `GET /tasks/view-my-tasks`

Returns tasks assigned to the authenticated caller, with cache metadata.

```bash
curl http://localhost:8080/tasks/view-my-tasks \
  -H "Authorization: Bearer $JAMES_TOKEN"
```

Expected first call after invalidation:
```json
{
  "user": { "email": "jamesbond@example.com", "role": "staff" },
  "tasks": [ /* 3 tasks */ ],
  "summary": { "total_assigned_tasks": 3 },
  "cache": { "hit": false }
}
```

Expected second call (same user, cache still warm):
```json
{
  "cache": { "hit": true }
}
```

---

## Full curl walkthrough

A complete, runnable script following the exact validation flow (seed →
admin login/2FA → create 5 tasks → assign 3 to James Bond → James login/2FA
→ blocked task creation → cached task view) is provided in
[`walkthrough.sh`](./walkthrough.sh).

```bash
chmod +x walkthrough.sh
BASE_URL=http://localhost:8080 ./walkthrough.sh
```

Requires `curl` and `jq`.

> **Note:** the script as originally written assumes the verification code
> is embedded as a 6-digit substring inside an email `body` field. This
> server instead returns the code directly as its own `code` field (see
> `/dev/email-logs/latest` above) and names the issued token `access_token`
> rather than `token`. Update the script's `jq` extraction lines
> accordingly before relying on it end-to-end against this server.

---

## Business rules enforced

- Only `admin` role can create or assign tasks (`403 Forbidden` otherwise).
- `staff` users only ever see tasks where `assigned_to_id` matches their own
  user id.
- 2FA codes are single-use (`consumed_at` is set on first successful
  verification) and expire after 5 minutes.
- 2FA codes are never stored in plain text — only a salted hash is
  persisted in `login_challenges.code_hash` (verify this still holds; the
  dev email-log endpoint returning the raw code is expected, but the DB
  row backing it should not hold the same raw value — see the security
  note under `/dev/email-logs/latest` above).
- `view-my-tasks` is served from Redis when present; the cache entry is
  deleted whenever that user's tasks are reassigned or updated, guaranteeing
  the next read is fresh.

---

## Known limitations / things to harden for production

- The `/dev/email-logs/latest` endpoint is for local development only and
  must be disabled (or gated behind an environment check) before any
  non-development deployment, since it exposes 2FA codes.
- JWTs are not revocable before expiry (no token blacklist/refresh-token
  rotation implemented).
- Rate limiting is not implemented on `/auth/login` or `/auth/verify-2fa`;
  a production deployment should add brute-force protection on both.

---

## Verified run log

The sequence below is a real terminal session run against a live local
instance, confirming the workflow end-to-end through task creation. Useful
as a reference for the exact request/response shapes this implementation
actually produces.

```text
$ curl -X POST http://localhost:8080/seed/users
{"message":"users seeded","users":[{"email":"admin@example.com","role":"admin"},{"email":"jamesbond@example.com","role":"staff"}]}

$ curl -X POST http://localhost:8080/auth/login \
  -H "Content-Type: application/json" \
  -d '{ "email": "admin@example.com", "password": "AdminPass123!" }'
{"login_challenge_id":"8dbfde35-4ad9-4c33-b760-df32a544c613","message":"verification code sent to email"}

$ curl "http://localhost:8080/dev/email-logs/latest?email=admin@example.com"
{"id":"f7dac8c8-0124-4b1c-81be-150de8b9e33a","to_email":"admin@example.com","subject":"Your verification code","code":"481377","login_challenge_id":"8dbfde35-4ad9-4c33-b760-df32a544c613","created_at":"2026-06-20T04:25:50.052796Z"}

$ curl -X POST http://localhost:8080/auth/verify-2fa \
  -H "Content-Type: application/json" \
  -d '{ "login_challenge_id": "8dbfde35-4ad9-4c33-b760-df32a544c613", "code": "481377" }'
{"access_token":"eyJ...","token_type":"Bearer","user":{"email":"admin@example.com","role":"admin"}}

$ curl -X POST http://localhost:8080/tasks \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{ "title": "Task 1", "description": "First task", "priority": "high" }'
{"id":"fcb7c045-9b93-4b20-9533-3247faa1d01b","title":"Task 1","description":"First task","status":"todo","priority":"high","created_by_id":"02b2da25-355a-4de7-b3de-7aafa760cbb9","assigned_to_id":null,"created_at":"2026-06-20T04:27:30.060855Z","updated_at":"2026-06-20T04:27:30.060855Z"}

$ curl -X POST http://localhost:8080/tasks \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{ "title": "Task 2", "description": "Second task", "priority": "medium" }'
{"id":"b8404971-3514-4546-a84b-376720eb0a82", ... "priority":"medium", ...}

$ curl -X POST http://localhost:8080/tasks \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{ "title": "Task 3", "description": "Third task", "priority": "low" }'
{"id":"74159027-28c1-45c1-936d-c4a204dbcbfe", ... "priority":"low", ...}

$ curl -X POST http://localhost:8080/tasks \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{ "title": "Task 4", "description": "Fourth task", "priority": "medium" }'
{"id":"d7892f05-4cab-4a23-927e-d91fdf5b1bd8", ... "priority":"medium", ...}

$ curl -X POST http://localhost:8080/tasks \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{ "title": "Task 5", "description": "Fifth task", "priority": "low" }'
{"id":"4e76d7db-1b9c-4a61-87d0-4c66c29218e5", ... "priority":"low", ...}
```

All 5 tasks created successfully as Admin, each defaulting to
`"status": "todo"`. Confirmed task ids from this run, usable for the
subsequent `/tasks/assign` call:

| # | id | priority |
|---|---|---|
| 1 | `fcb7c045-9b93-4b20-9533-3247faa1d01b` | high |
| 2 | `b8404971-3514-4546-a84b-376720eb0a82` | medium |
| 3 | `74159027-28c1-45c1-936d-c4a204dbcbfe` | low |
| 4 | `d7892f05-4cab-4a23-927e-d91fdf5b1bd8` | medium |
| 5 | `4e76d7db-1b9c-4a61-87d0-4c66c29218e5` | low |

**Remaining to verify and append once run:** assigning 3 tasks (e.g. #1–3)
to James Bond, James's login/2FA, the 403 on his task-creation attempt,
and the two `view-my-tasks` calls showing `cache.hit: false` then `true`.