# Query execution contract

## Purpose

This contract defines how Rustice should reason about executing SQL. It is a development contract, not a claim that every mode is implemented today.

Rustice must support multiple execution contracts over time:

- local execution through Rustice/DataFusion without contacting Snowflake
- Snowflake passthrough/fallback for statements explicitly classified for remote execution
- hybrid execution where supported statements run locally and unsupported or policy-controlled statements route elsewhere
- offline testing that gives useful query-engine compatibility feedback without Snowflake

Read-only operation may be a benchmark, safety, or test mode, but it is not the default product contract.

## Execution modes

### Local execution

Current expectation: local execution is the primary implemented path. Requests enter through `api-snowflake-rest` or tests, create or resolve an `executor::UserSession`, parse Snowflake-oriented SQL, rewrite supported syntax, plan with DataFusion, access catalogs through `catalog`, execute functions from `functions`, and return `executor::models::QueryResult`.

Local execution must not contact Snowflake.

### Snowflake passthrough/fallback

Current expectation: this repository does not expose a complete passthrough/fallback implementation in the inspected REST/executor flow. Future work may add one, but it must be explicit and observable.

Passthrough/fallback means a classified statement is forwarded to Snowflake under a policy chosen by the caller or deployment. It must not be hidden inside local execution after planning fails.

### Hybrid execution

Hybrid mode combines local and passthrough decisions. Supported queries can run locally, while unsupported, write-oriented, or policy-sensitive statements can be routed according to a routing policy.

Hybrid mode must define how metadata, session variables, result formats, and errors are reconciled across local and remote execution before it is treated as production behavior.

### Offline testing

Offline testing runs local execution paths and local REST harnesses without Snowflake. It is first-class because it catches parser, planner, function, catalog, result-shape, and error-mapping regressions quickly.

## Local execution responsibilities

Local execution owns:

- parsing and post-processing Snowflake-oriented SQL in `executor`
- DataFusion session setup, custom planners, analyzers, optimizers, and UDF/UDAF/UDTF registration
- catalog and table resolution through `catalog` and `catalog-metastore`
- implemented DDL, DML, COPY, MERGE, SHOW, EXPLAIN, function, and table-function behavior
- query IDs, request IDs, cancellation, timeouts, running-query history, and query stats
- conversion from `QueryResult` to REST JSON or Arrow formats in `api-snowflake-rest`

Local execution must fail explicitly for unsupported statements or unsupported functions.

## Inputs and outputs

Inputs include SQL text, session ID, request ID, query submission time, session metadata, current database/schema, client IP, REST serialization format, and execution configuration.

Outputs include:

- `QueryResult` with Arrow record batches and column metadata
- REST `JsonResponse` with Snowflake-shaped row metadata, rowset or rowsetBase64, query ID, success flag, SQL state, and error code when applicable
- cancellation and timeout state for running queries

Result-shape changes must be verified at the narrowest layer: executor snapshots for engine behavior, REST snapshots for wire behavior, and sqllogictest for compatibility corpus behavior.

## Error behavior

Current expectation: local execution maps many executor, catalog, function, and DataFusion errors into Snowflake-shaped error objects through `executor::snowflake_error`, `executor::error_code`, and `api-snowflake-rest::server::error`.

Local errors must preserve enough context for debugging while returning stable client-facing error envelopes where tests assert them.

Passthrough errors, when implemented, must be identified as remote-origin errors in logs/metrics. A local planning or execution error must not silently turn into a remote execution attempt unless the routing contract classifies that fallback and the request metadata records it.

## Observability expectations

Execution should expose enough information to reconstruct routing and outcome:

- execution mode: local, passthrough, fallback, hybrid, or rejected
- query ID and request ID
- session ID and safe session metadata
- statement classification result
- elapsed time, timeout, cancellation, and retry behavior
- whether results were JSON or Arrow
- sanitized error class and origin

Sensitive headers, JWTs, cookies, access keys, and caller tokens must be redacted.

## Non-goals

- This contract does not require all Snowflake SQL to run locally.
- This contract does not require Snowflake passthrough/fallback to exist before local execution can improve.
- This contract does not make read-only behavior the default.
- This contract does not define a distributed execution model.
- This contract does not replace crate-local ownership: REST belongs in `api-snowflake-rest`, sessions in `api-snowflake-rest-sessions`, execution in `executor`, functions in `functions`, and catalog behavior in `catalog`/`catalog-metastore`.
