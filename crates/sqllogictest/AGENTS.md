# embucket-sqllogictest agent guide

## What this crate owns
- The in-process sqllogictest compatibility harness for the Embucket SQL engine.
- Discovery and execution of `.slt` files under `tests/slt/`, CLI filters, parallelism, soft/strict failure mode, and summary output.
- The `EmbucketSession` adapter from `sqllogictest::AsyncDB` to `executor::UserSession`.
- RecordBatch-to-sqllogictest normalization, custom directive stripping, and `<REGEX>:` expected-value validation.
- The vendored `.slt` corpus and local sync script.

## What this crate must not own
- SQL engine fixes; implement those in `executor`, `functions`, `catalog`, or `catalog-metastore`.
- Snowflake REST/auth/session behavior; this harness intentionally bypasses the server and runs locally/offline.
- Upstream sqllogictest parser behavior beyond the small local preprocessor for known Embucket directives.
- Snowflake passthrough/fallback routing or contract checks.

## Important files and modules
- `tests/sqllogictests.rs` is the harness binary registered with `harness = false`.
- `src/engine.rs` runs SQL through a fresh `UserSession` and returns `DBOutput`.
- `src/preprocessor.rs` strips `exclude-from-coverage`, `skip-if`, and `only-if` directives.
- `src/normalize.rs`, `conversion.rs`, and `output.rs` convert Arrow results into sqllogictest rows and type codes.
- `src/lib.rs` exposes modules and the `<REGEX>:` validator.
- `tests/slt/bronze_scope/` is the main compatibility corpus; `tests/slt/databend/` is opt-in.
- `dev/sync-slt.sh` bulk-syncs the vendored corpus and should be reviewed carefully after use.

## Local verification
- List matching files without running: `cargo test -p embucket-sqllogictest -- --list`
- Run one path offline: `cargo test -p embucket-sqllogictest -- sql-reference-functions/Aggregate/listagg.slt --test-threads 1`
- Fail the process on any failing file: `cargo test -p embucket-sqllogictest -- --strict`
- Include Databend corpus only when needed: `cargo test -p embucket-sqllogictest -- --include-databend`

## Common failure modes
- Forgetting the harness exits 0 by default even when files fail; use `--strict` for gating checks.
- Sharing session state across files; each file is expected to get a fresh local `/dev` catalog session.
- Stripping upstream `onlyif`/`skipif` directives accidentally; only the hyphenated Embucket directives are removed.
- Changing normalization and causing broad corpus churn unrelated to the engine change.
- Bulk-syncing `tests/slt/` without reviewing deletions and expected-output changes.
