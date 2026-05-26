# AGENTS.md

## Project purpose
Rustice is a Snowflake-compatible query engine built on DataFusion, intended to run locally and in managed compute environments such as SPCS/Snowpark, with Iceberg/Parquet storage.

## Current product direction
Primary target: Snowflake-compatible execution for Snowplow-style analytical workloads.
Do not assume read-only by default. The codebase must support clear routing contracts:
- local execution
- Snowflake passthrough/fallback
- offline/local test execution without Snowflake

## Engineering rules
- Prefer small, isolated changes.
- Do not broaden Snowflake compatibility unless tied to a failing test, query corpus item, or documented contract.
- Do not add runtime dependencies without justification.
- Do not change public API behavior without updating docs and tests.

## Verification
Use the narrowest test that proves the change:
- crate-level unit tests
- sqllogictest compatibility tests
- local/offline query-engine tests
- integration tests only when protocol/runtime behavior changes

## Done means
- Changed behavior is documented.
- Tests exist or an explicit reason is given.
- Known unsupported behavior is recorded.
- No silent fallback is introduced unless the routing contract says so.
- In case of architectural changes AGENTS.md and README.md updated for coresponded module.