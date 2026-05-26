# catalog agent guide

## What this crate owns
- DataFusion catalog integration for Embucket: `CatalogProviderList`, `CatalogProvider`, `SchemaProvider`, `TableProvider`, and `ObjectStoreRegistry` wrappers.
- Iceberg catalog registration, lazy/eager REST catalog loading, local dev catalog construction, and object-store registration for table access.
- Cache wrappers for catalogs, schemas, and tables, including Snowflake-style case normalization and information schema views.
- Local tests for `INFORMATION_SCHEMA` output and catalog-facing utilities.

## What this crate must not own
- Metastore source-of-truth data models and CRUD traits; those belong in `catalog-metastore`.
- SQL statement routing and DDL/DML behavior; those belong in `executor`.
- REST protocol, auth/session handling, or function implementation.
- Snowflake passthrough/fallback routing.

## Important files and modules
- `src/catalog_list.rs` defines `EmbucketCatalogList`, catalog registration, refresh, and object-store lookup.
- `src/catalog.rs` defines `CachingCatalog`, namespace create/drop helpers, catalog properties, and catalog type.
- `src/schema.rs` defines `CachingSchema`, table cache lookup, and async Iceberg table create/drop helpers.
- `src/table.rs` wraps `TableProvider`, normalizes case, rewrites filters for case-sensitive schemas, and refreshes view sources.
- `src/dev_catalog.rs` builds local/offline file, S3, REST, or in-memory dev catalogs.
- `src/rest_catalog_config.rs` reads Iceberg REST catalog env config and auth tokens.
- `src/information_schema/` implements Snowflake-style metadata tables.
- `src/tests/information_schema.rs` and snapshots verify virtual metadata output.

## Local verification
- `cargo test -p catalog`
- Focused metadata snapshots: `cargo test -p catalog -- information_schema`
- For query-facing catalog changes, also run a narrow executor test such as `cargo test -p executor -- query_show`

## Common failure modes
- Treating the cache as the source of truth; remote Iceberg metadata may need provider lookup on cache misses.
- Calling sync DataFusion `register_table`/`deregister_table` for Iceberg operations that require async catalog writes.
- Breaking deterministic ordering in `schema_names`, `table_names`, or information-schema snapshots.
- Missing case-normalization behavior for Snowflake-style identifiers.
- Changing REST catalog env handling without checking lazy bootstrap schemas/tables.
