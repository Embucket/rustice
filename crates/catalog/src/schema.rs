use crate::catalog::CatalogConfig;
use crate::error;
use crate::error::Result as CatalogResult;
use crate::table::{CachingTable, IcebergTableBuilder};
use async_trait::async_trait;
use dashmap::DashMap;
use datafusion::catalog::{SchemaProvider, TableProvider};
use datafusion_common::DataFusionError;
use datafusion_expr::TableType;
use datafusion_iceberg::DataFusionTable;
use iceberg_rust::catalog::Catalog;
use iceberg_rust::catalog::create::CreateTableBuilder;
use iceberg_rust::catalog::tabular::Tabular as IcebergTabular;
use iceberg_rust_spec::identifier::Identifier;
use snafu::ResultExt;
use std::any::Any;
use std::sync::Arc;

pub struct CachingSchema {
    pub schema: Arc<dyn SchemaProvider>,
    pub iceberg_catalog: Option<Arc<dyn Catalog>>,
    pub name: String,
    pub tables_cache: DashMap<String, Arc<CachingTable>>,
    pub config: CatalogConfig,
}

#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for CachingSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Schema")
            .field("schema", &"")
            .field("name", &self.name)
            .field("tables_cache", &self.tables_cache)
            .finish()
    }
}

#[async_trait]
impl SchemaProvider for CachingSchema {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn table_names(&self) -> Vec<String> {
        // Sorted for the same reason as `CachingCatalog::schema_names`: the
        // underlying iceberg mirror is a DashMap with non-deterministic order.
        let mut names = self.schema.table_names();
        names.sort();
        names
    }

    #[allow(clippy::as_conversions)]
    #[tracing::instrument(name = "CachingSchema::table", level = "debug", skip(self), err)]
    async fn table(&self, name: &str) -> Result<Option<Arc<dyn TableProvider>>, DataFusionError> {
        // NOTE: We should always rely on the original schema provider instead of the cache,
        // because the underlying Iceberg catalog may have updated the table metadata outside
        // of SQL (e.g., via direct catalog API calls). In such cases, our cache could contain
        // stale metadata and ignore the latest snapshot updates.
        //
        // However, since we assume that users will interact with the Iceberg catalog
        // exclusively through Embucket, we can safely enable caching — in this case,
        // the data will remain consistent across all queries.
        if let Some(table) = self.tables_cache.get(name) {
            return Ok(Some(Arc::clone(table.value()) as Arc<dyn TableProvider>));
        }

        if let Some(table) = self.schema.table(name).await? {
            let caching_table = Arc::new(CachingTable::new(name.to_string(), Arc::clone(&table)));

            // Optionally update the cache for reuse (not as source of truth)
            self.tables_cache
                .insert(name.to_string(), Arc::clone(&caching_table));

            Ok(Some(caching_table as Arc<dyn TableProvider>))
        } else {
            Ok(None)
        }
    }

    /// Register a table. For iceberg-backed schemas, the iceberg table must
    /// already have been created via `create_iceberg_table_async` (or another
    /// async path) before invoking this method — the sync trait method only
    /// updates the in-memory mirror and the local cache so reads see the new
    /// table. Passing an `IcebergTableBuilder` here is a programming error.
    fn register_table(
        &self,
        name: String,
        table: Arc<dyn TableProvider>,
    ) -> datafusion_common::Result<Option<Arc<dyn TableProvider>>> {
        if self.iceberg_catalog.is_some()
            && table
                .as_any()
                .downcast_ref::<IcebergTableBuilder>()
                .is_some()
        {
            return Err(DataFusionError::Internal(
                "register_table called with IcebergTableBuilder; \
                 callers must use CachingSchema::create_iceberg_table_async instead"
                    .to_string(),
            ));
        }

        let table_provider: Arc<dyn TableProvider> = if self.iceberg_catalog.is_some() {
            self.schema
                .register_table(name.clone(), Arc::clone(&table))?;
            table
        } else if table.table_type() == TableType::View {
            table
        } else {
            self.schema
                .register_table(name.clone(), Arc::clone(&table))?;
            table
        };

        let caching_table = Arc::new(CachingTable::new(name.clone(), Arc::clone(&table_provider)));
        self.tables_cache.insert(name, caching_table);
        Ok(Some(table_provider))
    }

    /// Deregister a table. For iceberg-backed schemas, the iceberg table must
    /// already have been dropped via `drop_table_async`. The sync trait method
    /// only updates the mirror and local cache.
    #[allow(clippy::as_conversions)]
    fn deregister_table(
        &self,
        name: &str,
    ) -> datafusion_common::Result<Option<Arc<dyn TableProvider>>> {
        let table = self.tables_cache.remove(name);

        if let Some((_, caching_table)) = table {
            if caching_table.table_type() != TableType::View {
                if self.iceberg_catalog.is_some() {
                    // Mirror-only update; iceberg drop is the caller's responsibility.
                    let _ = self.schema.deregister_table(name)?;
                } else {
                    return self.schema.deregister_table(name);
                }
            }
            return Ok(Some(caching_table as Arc<dyn TableProvider>));
        }
        Ok(None)
    }

    fn table_exist(&self, name: &str) -> bool {
        if self.tables_cache.contains_key(name) {
            return true;
        }
        self.schema.table_exist(name)
    }
}

impl CachingSchema {
    /// Asynchronously create an iceberg table from a `CreateTableBuilder` and
    /// reflect it in the in-memory mirror and local cache. Use this from async
    /// SQL handlers instead of the sync `register_table` trait method.
    #[tracing::instrument(
        name = "CachingSchema::create_iceberg_table_async",
        level = "debug",
        skip(self, builder),
        err
    )]
    pub async fn create_iceberg_table_async(
        &self,
        name: String,
        mut builder: CreateTableBuilder,
    ) -> CatalogResult<Arc<dyn TableProvider>> {
        let catalog = self
            .iceberg_catalog
            .clone()
            .ok_or_else(|| iceberg_rust::error::Error::NotFound("iceberg catalog".to_string()))
            .context(error::IcebergSnafu)?;
        let namespace = vec![self.name.clone()];
        let ident = Identifier::new(&namespace, &name);
        let iceberg_table = builder
            .build(ident.namespace(), catalog)
            .await
            .context(error::IcebergSnafu)?;

        let provider: Arc<dyn TableProvider> = Arc::new(DataFusionTable::new(
            IcebergTabular::Table(iceberg_table),
            None,
            None,
            None,
        ));

        // Update the inner mirror. Argument is ignored by IcebergSchema.
        self.schema
            .register_table(name.clone(), Arc::clone(&provider))
            .context(error::DataFusionSnafu)?;

        let caching_table = Arc::new(CachingTable::new(name.clone(), Arc::clone(&provider)));
        self.tables_cache.insert(name, caching_table);
        Ok(provider)
    }

    /// Asynchronously drop an iceberg table and remove it from the mirror and
    /// local cache.
    #[tracing::instrument(
        name = "CachingSchema::drop_table_async",
        level = "debug",
        skip(self),
        err
    )]
    pub async fn drop_table_async(&self, name: &str) -> CatalogResult<Option<Arc<CachingTable>>> {
        let removed = self.tables_cache.remove(name).map(|(_, t)| t);

        if let Some(catalog) = &self.iceberg_catalog {
            let namespace = vec![self.name.clone()];
            let ident = Identifier::new(&namespace, name);
            // Best-effort: ignore errors when dropping a view through this path —
            // views go through deregister via the catalog provider, not here.
            if removed
                .as_ref()
                .is_some_and(|t| t.table_type() != TableType::View)
            {
                catalog
                    .drop_table(&ident)
                    .await
                    .context(error::IcebergSnafu)?;
            }
            // Mirror update.
            let _ = self
                .schema
                .deregister_table(name)
                .context(error::DataFusionSnafu)?;
        } else {
            let _ = self
                .schema
                .deregister_table(name)
                .context(error::DataFusionSnafu)?;
        }
        Ok(removed)
    }
}
