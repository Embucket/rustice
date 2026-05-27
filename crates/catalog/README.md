# catalog

Implements DataFusion's `CatalogProvider` and related traits, enabling the query engine to discover and interact with schemas, tables, and views managed by Embucket's metastore and external catalog sources like Iceberg.

## Purpose

This crate acts as a bridge between Embucket's metadata management (`catalog-metastore`) and the DataFusion query engine (`executor`), allowing DataFusion to understand the structure of data accessible via Embucket.

## Key types

- `EmbucketCatalogList` — manages multiple catalogs and registers Iceberg catalogs, eagerly
  (`register_iceberg_catalog`) or lazily (`register_iceberg_catalog_lazy`).
- `CachingCatalog` / `CachingSchema` / `CachingTable` — implement DataFusion's
  `CatalogProvider` / `SchemaProvider` / `TableProvider` over Iceberg sources.
- `CatalogType` — `Embucket` or `Memory`.
- `information_schema` — virtual `INFORMATION_SCHEMA` for Snowflake-style metadata discovery.
- `dev_catalog` — in-memory catalog used for development and tests.

## Iceberg & REST catalog support

Iceberg catalogs (including the **Iceberg REST catalog**, configured via `ICEBERG_REST_*`
env vars in `rest_catalog_config.rs`, with OAuth/bearer auth) are wrapped in a
`DataFusionIcebergCatalog` and then a `CachingCatalog`. Snowflake `database.schema.table`
names map onto Iceberg `namespace = [schema]` + `table`; column/schema names are normalized
for Snowflake-style case-insensitivity.

## Caching is advisory

Schema and table metadata are cached in `DashMap`s to avoid repeated remote lookups, but the
cache is **advisory, not the source of truth** — Iceberg tables can be mutated externally, so
lookups fall back to the underlying provider on a miss (see `schema.rs`).

Consumed by `executor` and `api-snowflake-rest`.