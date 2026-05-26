# api-snowflake-rest-sessions

Manages user/API sessions and authentication for the Snowflake REST API: JWT issuing and
validation, session storage, SPCS ingress, and the Axum extractor/middleware used to attach
a session to each request.

## Purpose

This crate makes Snowflake REST interactions stateful and authenticated, on top of the
session registry exposed by `executor`'s `ExecutionService`.

## Mechanics

- `TokenizedSession` — `(session_id, SessionMetadata)`; implements `FromRequestParts`, so
  handlers receive a resolved session. Resolution order: trusted **SPCS ingress** headers →
  **JWT** (`Authorization: Bearer`) → request extensions/cookie.
- **JWT** (`helpers.rs`) — `sub` = username, `aud` = request `Host`, `exp` = issued + 3 days;
  validated with an audience check and a 5-second leeway.
- **Sessions** — keyed by `session_id` (cookie `session_id`), 4-hour TTL
  (`SESSION_EXPIRATION_SECONDS = 14400`), refreshed on each request; a background task purges
  expired sessions.
- **SPCS ingress** — `sf-context-current-user` / `-account` / `-user-token` headers, trusted
  only when configured.
