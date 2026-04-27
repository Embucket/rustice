use crate::catalog::{CachingCatalog, Properties};
use crate::df_error;
use crate::error::{self as catalog_error, InvalidCacheSnafu, Result};
use crate::schema::CachingSchema;
use crate::table::CachingTable;
use crate::utils::fetch_table_providers;
use dashmap::DashMap;
use datafusion::{
    catalog::{CatalogProvider, CatalogProviderList},
    execution::object_store::ObjectStoreRegistry,
};
use datafusion_iceberg::catalog::catalog::IcebergCatalog as DataFusionIcebergCatalog;
use iceberg_rust::catalog::Catalog;
use object_store::ObjectStore;
use object_store::local::LocalFileSystem;
use snafu::ResultExt;
use std::any::Any;
use std::sync::Arc;
use url::Url;

pub const DEFAULT_CATALOG: &str = "embucket";

pub struct EmbucketCatalogList {
    pub table_object_store: Arc<DashMap<String, Arc<dyn ObjectStore>>>,
    pub catalogs: DashMap<String, Arc<CachingCatalog>>,
    pub config: CatalogListConfig,
}

#[derive(Default, Clone)]
pub struct CatalogListConfig {
    pub max_concurrent_table_fetches: usize,
    pub iceberg_table_timeout_secs: u64,
    pub iceberg_catalog_timeout_secs: u64,
}

impl EmbucketCatalogList {
    pub fn new(config: CatalogListConfig) -> Self {
        let table_object_store: DashMap<String, Arc<dyn ObjectStore>> = DashMap::new();
        table_object_store.insert("file://".to_string(), Arc::new(LocalFileSystem::new()));
        Self {
            table_object_store: Arc::new(table_object_store),
            catalogs: DashMap::default(),
            config,
        }
    }

    #[tracing::instrument(
        name = "EmbucketCatalogList::drop_catalog",
        level = "debug",
        skip(self),
        err
    )]
    pub async fn drop_catalog(&self, name: &str, _cascade: bool) -> Result<()> {
        let Some(_) = self.catalogs.remove(name) else {
            return InvalidCacheSnafu {
                entity: "catalog",
                name,
            }
            .fail();
        };
        Ok(())
    }

    /// Register an iceberg catalog wrapped in a `CachingCatalog`. The underlying
    /// `IcebergCatalog` mirror cache is prefilled here so that read paths
    /// (`schema_names`, `schema`, `table_names`, `table_exist`) can be answered
    /// synchronously without `block_on`.
    pub async fn register_iceberg_catalog(
        &self,
        name: &str,
        iceberg_catalog: Arc<dyn Catalog>,
        should_refresh: bool,
    ) -> Result<()> {
        let catalog_provider: Arc<dyn CatalogProvider> = Arc::new(
            DataFusionIcebergCatalog::new(iceberg_catalog.clone(), None)
                .await
                .context(catalog_error::DataFusionSnafu)?,
        );
        let caching = CachingCatalog::new(
            catalog_provider,
            name.to_owned(),
            Some(iceberg_catalog),
            (&self.config).into(),
        )
        .with_refresh(should_refresh)
        .with_properties(Properties::default());

        self.catalogs.insert(name.to_owned(), Arc::new(caching));
        Ok(())
    }

    #[allow(clippy::as_conversions)]
    #[tracing::instrument(
        name = "EmbucketCatalogList::refresh",
        level = "debug",
        skip(self),
        fields(catalogs_to_refresh),
        err
    )]
    pub async fn refresh(&self) -> Result<()> {
        // Record the result as part of the current span.
        tracing::Span::current().record(
            "catalogs_to_refresh",
            format!(
                "{:?}",
                self.catalogs
                    .iter()
                    .filter(|cat| cat.should_refresh)
                    .map(|cat| cat.name.clone())
                    .collect::<Vec<_>>()
            ),
        );

        for catalog in self.catalogs.iter_mut() {
            if catalog.should_refresh {
                let schemas = catalog.schema_names();
                for schema in schemas.clone() {
                    if let Some(schema_provider) = catalog.catalog.schema(&schema) {
                        let schema = CachingSchema {
                            schema: schema_provider,
                            tables_cache: DashMap::default(),
                            name: schema.clone(),
                            iceberg_catalog: catalog.iceberg_catalog.clone(),
                            config: catalog.config.clone(),
                        };
                        let table_providers = fetch_table_providers(
                            Arc::clone(&schema.schema),
                            self.config.max_concurrent_table_fetches,
                        )
                        .await
                        .context(catalog_error::DataFusionSnafu)?;

                        for (table_name, table_provider) in table_providers {
                            schema.tables_cache.insert(
                                table_name.clone(),
                                Arc::new(CachingTable::new_with_schema(
                                    table_name,
                                    table_provider.schema(),
                                    Arc::clone(&table_provider),
                                )),
                            );
                        }
                        catalog
                            .schemas_cache
                            .insert(schema.name.clone(), Arc::new(schema));
                    }
                }
                // Cleanup removed schemas from the cache
                for schema in &catalog.schemas_cache {
                    if !schemas.contains(&schema.key().clone()) {
                        catalog.schemas_cache.remove(schema.key());
                    }
                }
            }
        }
        Ok(())
    }
}

impl std::fmt::Debug for EmbucketCatalogList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbucketCatalogList").finish()
    }
}

/// Get the key of a url for object store registration.
/// The credential info will be removed
#[must_use]
fn get_url_key(url: &Url) -> String {
    format!(
        "{}://{}",
        url.scheme(),
        &url[url::Position::BeforeHost..url::Position::AfterPort],
    )
}

impl ObjectStoreRegistry for EmbucketCatalogList {
    #[tracing::instrument(
        name = "ObjectStoreRegistry::register_store",
        level = "debug",
        skip(self, store)
    )]
    fn register_store(
        &self,
        url: &Url,
        store: Arc<dyn ObjectStore>,
    ) -> Option<Arc<dyn ObjectStore>> {
        let url = get_url_key(url);
        self.table_object_store.insert(url, store)
    }

    #[tracing::instrument(
        name = "ObjectStoreRegistry::get_store",
        level = "debug",
        skip(self),
        err
    )]
    fn get_store(&self, url: &Url) -> datafusion_common::Result<Arc<dyn ObjectStore>> {
        let url = get_url_key(url);
        if let Some(object_store) = self.table_object_store.get(&url) {
            Ok(object_store.clone())
        } else {
            df_error::ObjectStoreNotFoundSnafu { url }.fail()?
        }
    }
}

impl CatalogProviderList for EmbucketCatalogList {
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[tracing::instrument(
        name = "EmbucketCatalogList::register_catalog",
        level = "debug",
        skip(self, catalog)
    )]
    fn register_catalog(
        &self,
        name: String,
        catalog: Arc<dyn CatalogProvider>,
    ) -> Option<Arc<dyn CatalogProvider>> {
        let catalog = CachingCatalog::new(catalog, name, None, self.config.clone().into());
        self.catalogs
            .insert(catalog.name.clone(), Arc::new(catalog))
            .map(|arc| {
                let catalog: Arc<dyn CatalogProvider> = arc;
                catalog
            })
    }

    #[tracing::instrument(
        name = "EmbucketCatalogList::catalog_names",
        level = "debug",
        skip(self)
    )]
    fn catalog_names(&self) -> Vec<String> {
        self.catalogs.iter().map(|c| c.key().clone()).collect()
    }

    #[allow(clippy::as_conversions)]
    #[tracing::instrument(name = "EmbucketCatalogList::catalog", level = "debug", skip(self))]
    fn catalog(&self, name: &str) -> Option<Arc<dyn CatalogProvider>> {
        self.catalogs
            .get(name)
            .map(|c| Arc::clone(c.value()) as Arc<dyn CatalogProvider>)
    }
}
