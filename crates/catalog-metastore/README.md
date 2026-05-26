# catalog-metastore

Core library responsible for the abstraction and interaction with the underlying metadata storage system. Defines data models and traits for metastore operations.

## Purpose

This crate provides a consistent way for other Embucket components to access and manipulate metadata about catalogs, schemas, tables, and other entities, abstracting the specific storage backend.

## Data model

Metadata is organized as a Snowflake-style 3-level hierarchy. Identifiers:

- `DatabaseIdent` — database name.
- `SchemaIdent` — `{ database, schema }`.
- `TableIdent` — `{ database, schema, table }`; `to_iceberg_ident()` maps it to a 2-level
  Iceberg identifier (`namespace = [schema]`, `table`). `Display` renders `database.schema.table`.
- `VolumeIdent` — the storage backend a database lives on.

Entities (`Database`, `Schema`, `Table`, `Volume`) are wrapped in `RwObject<T>`, which adds
`created_at` / `updated_at` timestamps and derefs to the inner value. `Table` carries the
Iceberg `TableMetadata`, its `metadata_location`, and a `TableFormat` (`Parquet` or `Iceberg`).
A `Volume` is one of `S3` / `GCS` / `Azure` / `Local` / `Memory` and produces an
`object_store` handle via `get_object_store()`.

## Metastore trait and flow

The async `Metastore` trait is the primary interface (CRUD for volumes, databases, schemas,
tables, plus `table_object_store`, `url_for_table`, etc.). `InMemoryMetastore` is the default
implementation (HashMap + `RwLock`, with an `object_store` cache).

```
create_volume → object store handle
  → create_database (references a volume)
    → create_schema
      → create_table  ── stage-create: build Iceberg metadata, write {db}/{schema}/{table}/metadata/{uuid}.metadata.json
                      └─ register:     attach an existing metadata location
update_table → apply Iceberg TableUpdates atomically, commit to object store
```

`metastore_bootstrap_config.rs` can seed volumes/databases/schemas/tables from YAML.

Consumed by `catalog`, `executor`, `api-snowflake-rest`, `embucketd`, and `embucket-lambda`.

## Timeouts related Environment Variables

These tune AWS object-storage and Iceberg access (S3 via `object_store` and the Iceberg
catalog); they are unrelated to the DynamoDB `state-store` crate.


|Variable Name  |Default Value    |
|:--------------|:----------------|
|AWS_SDK_CONNECT_TIMEOUT_SECS|3|
|AWS_SDK_OPERATION_TIMEOUT_SECS|30|
|AWS_SDK_OPERATION_ATTEMPT_TIMEOUT_SECS|10|
|ICEBERG_CREATE_TABLE_TIMEOUT_SECS|30|
|ICEBERG_CATALOG_TIMEOUT_SECS|10|
|OBJECT_STORE_TIMEOUT_SECS|30|
|OBJECT_STORE_CONNECT_TIMEOUT_SECS|3|
