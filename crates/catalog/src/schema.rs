use crate::catalog::CatalogConfig;
use crate::df_error::CatalogSnafu;
use crate::table::{CachingTable, IcebergTableBuilder};
use crate::{block_on_with_timeout, error};
use async_trait::async_trait;
use dashmap::DashMap;
use datafusion::catalog::{SchemaProvider, TableProvider};
use datafusion_common::DataFusionError;
use datafusion_expr::TableType;
use datafusion_iceberg::DataFusionTable;
use iceberg_rust::catalog::Catalog;
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
        self.schema.table_names()
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

    fn register_table(
        &self,
        name: String,
        table: Arc<dyn TableProvider>,
    ) -> datafusion_common::Result<Option<Arc<dyn TableProvider>>> {
        let table_provider: Arc<dyn TableProvider> = if let Some(catalog) = &self.iceberg_catalog
            && let Some(iceberg_builder) = table.as_any().downcast_ref::<IcebergTableBuilder>()
        {
            let catalog = Arc::clone(catalog);
            let mut builder = iceberg_builder.builder.clone();
            let namespace = vec![self.name.clone()];
            let table_name = name.clone();

            let provider = block_on_with_timeout(
                async move {
                    let ident = Identifier::new(&namespace, &table_name);
                    let iceberg_table = builder
                        .build(ident.namespace(), catalog)
                        .await
                        .context(error::IcebergSnafu)?;
                    let tabular = IcebergTabular::Table(iceberg_table);
                    let table_provider: Arc<dyn TableProvider> =
                        Arc::new(DataFusionTable::new(tabular, None, None, None));
                    Ok(table_provider)
                },
                self.config.catalog_timeout(),
            )
            .context(CatalogSnafu)?
            .map_err(|err: error::Error| DataFusionError::External(Box::new(err)))?;

            // Update the inner mirror so reads see the new table. The provider
            // argument is required by the trait but ignored by IcebergSchema.
            self.schema
                .register_table(name.clone(), Arc::clone(&provider))?;
            provider
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

    #[allow(clippy::as_conversions)]
    fn deregister_table(
        &self,
        name: &str,
    ) -> datafusion_common::Result<Option<Arc<dyn TableProvider>>> {
        let table = self.tables_cache.remove(name);

        if let Some((_, caching_table)) = table {
            if caching_table.table_type() != TableType::View {
                if let Some(catalog) = &self.iceberg_catalog {
                    let catalog = Arc::clone(catalog);
                    let namespace = vec![self.name.clone()];
                    let table_name = name.to_string();

                    block_on_with_timeout(
                        async move {
                            let ident = Identifier::new(&namespace, &table_name);
                            catalog
                                .drop_table(&ident)
                                .await
                                .context(error::IcebergSnafu)
                        },
                        self.config.catalog_timeout(),
                    )
                    .context(CatalogSnafu)?
                    .map_err(|err| DataFusionError::External(Box::new(err)))?;

                    // Drop from the mirror so reads stop returning it.
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
