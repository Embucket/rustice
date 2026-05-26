# embucketd

The main executable (daemon) for the Embucket application, responsible for initializing and orchestrating the Snowflake-compatible REST API and core components (executor, catalog/metastore, sessions).

## Overview

This crate contains the `main` function for the Embucket server. It parses configuration
(`cli.rs`, clap + env), sets up tracing/OpenTelemetry, builds `CoreState`/`AppState`, and
serves the Snowflake REST router (`api-snowflake-rest::make_snowflake_router`) over Axum
(default `localhost:3000`) with middleware from `layers.rs` (auth, compression, timeout, trace).

## Routes

- `POST /session/v1/login-request` — authenticate, create a session, return a JWT.
- `POST /queries/v1/query-request` — execute SQL (JSON or base64-Arrow results).
- `POST /queries/v1/abort-request` — cancel a running query.
- `POST /session`, `POST /session/heartbeat` — session delete / keep-alive.
- `GET /health` and Swagger UI at `/`.
