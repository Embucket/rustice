# catalog-metastore agent guide

## What this crate owns
- Metastore data models for volumes, databases, schemas, and tables.
- The async `Metastore` trait and in-memory implementation used by local runtime and tests.
- Volume-to-object-store creation for memory, file, and S3-backed storage.
- Iceberg table metadata creation, registration, update application, metadata JSON writes, and YAML/env bootstrap config.

## What this crate must not own
- DataFusion `CatalogProvider` wrappers and information schema views; those belong in `catalog`.
- SQL parsing, DDL/DML statement behavior, and query execution; those belong in `executor`.
- REST API routes, sessions, or response models.
- Durable external state-store behavior outside this crate's visible metastore abstraction.

## Important files and modules
- `src/metastore.rs` defines the `Metastore` trait, `InMemoryMetastore`, normalized keys, object-store cache, table metadata writes, and Iceberg update application.
- `src/models/table.rs` defines `TableIdent`, `Table`, `TableCreateRequest`, `TableFormat`, and Iceberg requirement checks.
- `src/models/volumes.rs` defines volume types, credential validation, object-store builders, and URL-based object-store creation.
- `src/models/database.rs`, `schema.rs`, and `mod.rs` define database/schema identifiers and `RwObject` timestamps.
- `src/metastore_bootstrap_config.rs` loads YAML, JSON, and env bootstrap data for volumes/databases/schemas/tables.
- `src/metastore_settings_config.rs` carries object-store timeout settings.
- `src/error.rs` is the shared error surface for metastore callers.

## Local verification
- `cargo test -p catalog-metastore`
- Model-focused filters: `cargo test -p catalog-metastore -- models`
- Use memory or file volumes for local/offline checks; S3 paths require explicit credentials/env.

## Common failure modes
- Forgetting that map keys are normalized to lowercase while display identifiers preserve input strings.
- Updating table metadata without writing a new metadata JSON file and touching the `RwObject`.
- Leaking access keys or tokens through `Debug`, `Display`, or tracing output.
- Changing cascade behavior without checking dependent databases, schemas, and tables.
- Invalidating `object_store_cache` incorrectly when volumes are updated or deleted.
