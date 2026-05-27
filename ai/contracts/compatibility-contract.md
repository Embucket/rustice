# Compatibility contract

## Purpose

This contract defines how Rustice should add and evaluate Snowflake compatibility without turning every difference into an unbounded implementation task.

Compatibility work must be tied to a failing local test, compatibility corpus case, documented contract, or explicit product requirement. It must preserve local/offline testing as a first-class path.

## Compatibility scope categories

Use these categories when adding or triaging compatibility work:

- `supported`: implemented locally and covered by tests
- `partially supported`: accepted but known to differ in documented ways
- `unsupported`: rejected explicitly or detected by the unimplemented-function checker
- `passthrough candidate`: should not run locally under some policy, but can be routed remotely when routing exists
- `open question`: behavior needs more design or oracle evidence before implementation

Do not call a feature compatible just because parsing succeeds. Execution, result shape, errors, and metadata must be considered.

## Function compatibility rules

Function changes belong primarily in `functions` and should follow local registration and test patterns.

For each new or changed function:

- define accepted signatures and aliases
- use local logical type/coercion patterns where available
- specify null behavior and scalar-vs-array behavior
- define return type, nullability, precision, scale, and timezone behavior when relevant
- add focused snapshot tests under `crates/functions/src/tests/`
- update unimplemented-function tracking when the support status changes
- run a relevant sqllogictest path when corpus coverage exists

If exact Snowflake behavior is unknown, document the current local expectation and mark the uncertainty as an open question.

## Type, cast, null, and timestamp behavior

Current known areas requiring care:

- `VARIANT` and semi-structured values are represented locally through JSON/text-oriented Arrow types rather than native Snowflake storage.
- Numeric coercion and Decimal precision can differ from Snowflake and must be tested for aggregates, casts, and formatting.
- Timestamp precision uses Arrow/DataFusion representations and may differ from Snowflake's variable precision and per-value timezone behavior.
- Text collation and charset behavior are not equivalent to Snowflake collation support.
- Null propagation must be asserted per function or expression class instead of assumed.

Compatibility work in these areas should include both positive and negative tests, because parser acceptance alone is not enough.

## Result comparison expectations

Result comparisons should be deterministic unless the behavior is explicitly nondeterministic.

Local snapshots should normalize only values that are inherently environment-specific, such as UUIDs, generated paths, timing metrics, or host parallelism. Do not normalize away semantic differences.

Sqllogictest comparisons are intentionally strict by default. Use the existing `<REGEX>:` validator only for known nondeterministic cells, and keep the pattern narrow.

REST compatibility tests must check wire-level response shape separately from engine results: `rowtype`, rowset format, query ID, `sqlState`, error code, and success/message fields can regress even when the engine result is correct.

## Known-difference policy

Known differences are acceptable only when they are documented and tested at the right layer.

A known difference entry should state:

- the affected SQL feature, function, type, or API behavior
- current local expectation
- Snowflake expectation, if known
- reason for accepting the difference today
- whether it should remain local-only, become passthrough-eligible, or be fixed locally
- the test that guards the current behavior

Do not hide known differences by broad snapshot filters, broad regexes, or silent fallback.

## How to add compatibility work safely

1. Identify the owning crate: `functions` for UDF/UDAF/UDTF behavior, `executor` for SQL planning/execution, `catalog` for metadata providers, `api-snowflake-rest` for wire behavior, or `sqllogictest` for harness behavior.
2. Add or update the smallest local test that fails for the desired behavior.
3. Implement the change in the owning crate without broadening unrelated compatibility.
4. Preserve explicit unsupported errors for behavior that remains out of scope.
5. Run focused crate tests and any relevant offline sqllogictest path.
6. If behavior depends on live Snowflake, record it as oracle evidence and add a local regression test for the selected Rustice behavior.
7. Update docs or AGENTS.md when the change moves a boundary, status category, or routing policy.
