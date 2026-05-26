# Routing contract

## Purpose

This contract defines how Rustice should decide whether a statement runs locally, passes through to Snowflake, falls back to Snowflake, or is rejected.

Current expectation: local execution is implemented through `executor`; a complete Snowflake passthrough/fallback router is not visible in the inspected code. Until one exists, unsupported behavior should fail explicitly rather than pretending a fallback occurred.

## Statement classification

Every routed statement should be classified before execution. Classification should be based on parsed statement shape, session policy, catalog scope, function support, and operation risk.

Recommended classification result:

- `local`: execute through Rustice/DataFusion
- `passthrough`: send directly to Snowflake because policy says Rustice should not handle it
- `fallback`: try local first only when policy explicitly permits remote fallback for this class
- `reject`: fail before execution with a clear unsupported or disallowed error
- `needs_review`: no production routing until the statement class has a documented policy

Classification should be observable and testable. Do not infer routing from late failures alone.

## Local execution candidates

Current local candidates include statements already handled by the executor and covered by tests:

- SELECT queries over local/catalog-backed tables
- implemented Snowflake-oriented syntax rewrites such as TOP, FETCH, LIKE/ILIKE ANY, table functions, and timestamp rewrites
- implemented scalar, aggregate, window, and table functions from `functions`
- implemented DDL/DML such as schemas, tables, views, inserts, COPY paths, and MERGE paths where tests exist
- SHOW, EXPLAIN, USE, SET, and session/context behavior where implemented
- local information schema and catalog metadata reads

Local writes are allowed when the local engine and catalog support them. Do not encode read-only as the default classification.

## Passthrough/fallback candidates

Passthrough/fallback may be appropriate for statement classes that are intentionally outside the current local engine contract or that must remain authoritative in Snowflake for a deployment.

Future/open question candidates include:

- unsupported Snowflake SQL features that the parser can identify safely
- administrative or account-level operations outside Rustice catalog ownership
- security, grants, governance, warehouse, task, stream, stage, or integration operations that are not modeled locally
- write-oriented operations when deployment policy says Snowflake remains the write authority
- compatibility probes that intentionally compare local behavior to a Snowflake oracle

These candidates require explicit policy. A statement being unsupported locally is not enough by itself to permit fallback.

## Reject candidates

Reject before execution when:

- the parser cannot classify the statement safely
- the statement mixes local-only and remote-only side effects in one operation
- session or metadata state needed for routing is ambiguous
- credentials or remote target configuration are missing
- fallback would hide a local correctness bug
- policy forbids remote execution for the request
- policy forbids local execution for the request

Rejected statements should return an actionable error and record the classification reason.

## No-silent-fallback rule

No local execution failure may silently become Snowflake execution.

Fallback is allowed only when all of these are true:

- the request or deployment policy enables fallback for the statement class
- the statement has been classified before local execution
- the local failure is one of the allowed fallback triggers for that class
- the response, logs, or metrics record that fallback occurred
- tests cover the routing decision

If any condition is missing, return the local error or a routing error.

## Metadata/session behavior

Routing must preserve session meaning:

- current database, schema, warehouse, role-like values, and session parameters must have a defined source of truth
- `TokenizedSession` metadata from the REST/session layer must be forwarded or translated deliberately
- query IDs and request IDs must remain traceable across local and remote legs
- local catalog metadata must not be assumed to match Snowflake metadata
- remote writes must not be reflected in local catalogs unless there is an explicit synchronization path

Hybrid mode must document how it prevents stale metadata, duplicate side effects, and inconsistent session variables.

## Open questions

- Where should the central statement classifier live: REST boundary, executor boundary, or a new routing crate/module?
- What policy format should select local, passthrough, fallback, hybrid, or reject behavior?
- Which statement classes are safe for fallback after a local failure?
- How should remote credentials be configured, redacted, and tested?
- How should local and Snowflake query IDs be represented in one response?
- Which metadata mutations should synchronize between local Iceberg catalogs and Snowflake, if any?
- What is the minimum oracle-test coverage before enabling a passthrough/fallback class?
