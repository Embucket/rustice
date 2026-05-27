# Offline test contract

## Purpose

This contract defines what local/offline tests are expected to prove for Rustice. Offline tests must remain first-class: useful compatibility work should be possible without Snowflake credentials, network access, or a running external service.

Current expectation: this repository already has local executor tests, local REST tests, and an in-process sqllogictest harness that runs through `executor::test_helpers`.

## Why offline tests matter

Offline tests provide fast feedback for:

- SQL parsing and AST rewrites
- DataFusion planning, optimization, and execution
- function signatures, coercion, null behavior, and result formatting
- catalog and information-schema behavior
- REST response shape, authentication/session extraction, and local server behavior
- compatibility corpus progress through `embucket-sqllogictest`

They also protect development in environments where Snowflake is unavailable.

## What offline tests can prove

Offline tests can prove that Rustice:

- accepts or rejects a SQL statement as expected
- produces stable local results for supported behavior
- preserves expected Arrow schema and Snowflake-shaped column metadata
- maps selected errors into expected local or REST error envelopes
- keeps local sessions, query IDs, cancellation, timeout, and retry behavior working
- handles local file, memory, and configured dev catalog paths
- avoids contacting Snowflake in local execution mode

Offline tests are the default verification loop for local execution changes.

## What offline tests cannot prove

Offline tests cannot prove:

- exact Snowflake server behavior for every SQL edge case
- remote Snowflake authorization, governance, account, or warehouse semantics
- network behavior, remote latency, remote retry semantics, or remote error wording
- correctness of a passthrough/fallback integration that is not exercised
- that local metadata matches a live Snowflake account

When a behavior depends on Snowflake itself, mark that as an oracle-test requirement or open question instead of weakening offline tests.

## Fixture strategy

Use fixtures that are visible in this repository:

- `executor::test_helpers::create_df_session*` for isolated local sessions
- in-memory `/dev` catalogs for fast query-engine tests
- file-backed dev catalogs when persistence or object-store behavior matters
- crate-local snapshots for executor, functions, catalog, and REST behavior
- `.slt` files under `crates/sqllogictest/tests/slt/` for compatibility coverage

Fixture data should be small, deterministic, and scoped to the feature being tested. Avoid shared mutable state between files or tests unless the test is specifically about shared state.

## Required local verification loops

Choose the narrowest loop that proves the change:

- Function implementation: `cargo test -p functions -- <function_or_module_filter>`
- Executor SQL behavior: `cargo test -p executor -- <query_or_module_filter>`
- Catalog metadata behavior: `cargo test -p catalog -- information_schema` or a narrower catalog filter
- Metastore model behavior: `cargo test -p catalog-metastore`
- REST wire behavior: `cargo test -p api-snowflake-rest -- test_rest_api`
- Session/auth extraction: `cargo test -p api-snowflake-rest-sessions`
- Compatibility corpus check: `cargo test -p embucket-sqllogictest -- <path-substring> --test-threads 1`

Full workspace tests are useful before broad changes, but they do not replace focused evidence.

## Relationship to Snowflake oracle tests

Snowflake oracle tests are allowed as a separate verification layer when behavior cannot be proven locally. They should compare documented local expectations against live Snowflake behavior, but they must not become required inputs for ordinary local development.

Oracle tests should record:

- the statement class under test
- local expected behavior
- observed Snowflake behavior
- whether the difference is accepted, planned, or a bug
- whether the finding affects routing policy

If a Snowflake oracle result changes the expected local behavior, add or update an offline regression test whenever possible.
