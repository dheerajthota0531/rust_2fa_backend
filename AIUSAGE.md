# AI Usage Disclosure

This document describes how AI assistance (Claude) was used while building
this project, in the interest of transparency for the assessment review.

## Summary


AI assistance was used throughout this project for:


- Initial project scaffolding (folder structure, `Cargo.toml`, module
  layout) based on the written specification.
- Drafting boilerplate-heavy code: SQL migrations, Axum route wiring,
  request/response DTOs, and the JWT/Argon2/Redis integration glue.
- Debugging compiler errors as the project was adapted and modified
  locally, including:
  - A `Cargo.toml` version-specifier mistake (`axum = "8"` vs the required
    `axum = "0.8"`, since axum has not yet reached 1.0).
  - An `E0195` lifetime mismatch caused by combining the `#[async_trait]`
    macro with axum 0.8's native `async fn`-in-trait support for
    `FromRequestParts` (axum-core 0.5 no longer requires the macro).
  - A crate-name mismatch between `Cargo.toml`'s `[lib].name` and the
    `use` statements in `main.rs` after the package was renamed locally.
  - A `docker compose build` failure caused by a missing `Dockerfile` in
    the build context referenced by `docker-compose.yml`.
- Writing this README, the curl walkthrough script, and this disclosure
  document.

## What was generated vs. what was reviewed/changed by hand

- **AI-generated, then reviewed:** migrations, model structs, the
  Axum router, the JWT/2FA/cache services, and the integration tests.
- **Manually adapted:** the project was restructured locally (module
  names, file layout, package/binary naming) after the initial scaffold,
  and AI assistance was used incrementally afterward to fix the resulting
  compile errors rather than to regenerate the project from scratch.
- **Manually verified:** the full curl workflow (seed → login → 2FA →
  task creation → assignment → cache hit/miss behavior) was run against a
  live local instance to confirm the documented request/response shapes
  match actual server behavior before inclusion in the README.

## Why this matters for review

Two-factor authentication, role-based authorization, and cache
invalidation are easy to get subtly wrong (e.g. forgetting to invalidate a
cache key on reassignment, or accepting an already-used 2FA code). AI
assistance was used to accelerate writing the repetitive plumbing around
these features, but the business-rule correctness (admin-only task
creation, single-use codes, exactly-3-tasks cache shape, etc.) was
validated against the spec by manually running the end-to-end flow in
`walkthrough.sh` and inspecting actual responses, not assumed from
generated code alone.

## Tooling

- Model: Claude (Anthropic), used via chat for code generation, debugging
  assistance, and documentation drafting.
- No autonomous agents or unattended code execution against production or
  shared infrastructure were used; all generated code was reviewed before
  being run locally.
