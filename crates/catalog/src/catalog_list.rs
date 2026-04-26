use super::catalogs::embucket::catalog::EmbucketCatalog;
use crate::catalog::{CachingCatalog, CatalogType, Properties};
#[cfg(feature = "rest-catalog")]
use crate::catalogs::rest::catalog::RestCatalog;
use crate::df_error;
use crate::error::{self as catalog_error, InvalidCacheSnafu, Result};
use crate::schema::CachingSchema;
use crate::table::CachingTable;
use crate::utils::fetch_table_providers;
#[cfg(not(feature = "rest-catalog"))]
use aws_config::{BehaviorVersion, Region, defaults, timeout::TimeoutConfigBuilder};
#[cfg(not(feature = "rest-catalog"))]
use aws_credential_types::{Credentials, provider::SharedCredentialsProvider};
use catalog_metastore::{AwsCredentials, S3TablesVolume};
use dashmap::DashMap;
use datafusion::{
    catalog::{CatalogProvider, CatalogProviderList},
    execution::object_store::ObjectStoreRegistry,
};
use datafusion_iceberg::catalog::catalog::IcebergCatalog as DataFusionIcebergCatalog;
#[cfg(feature = "rest-catalog")]
use iceberg_rest_catalog::apis::configuration::{AWSv4Key, Configuration};
#[cfg(feature = "rest-catalog")]
use iceberg_rest_catalog::catalog::RestCatalog as IcebergRestCatalog;
use iceberg_rust::catalog::Catalog;
use iceberg_rust::object_store::ObjectStoreBuilder;
#[cfg(not(feature = "rest-catalog"))]
use iceberg_s3tables_catalog::S3TablesCatalog;
use object_store::ObjectStore;
use object_store::local::LocalFileSystem;
#[cfg(feature = "rest-catalog")]
use secrecy::SecretString;
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
    #[cfg(not(feature = "rest-catalog"))]
    pub aws_sdk_timeout_config: TimeoutConfigBuilder, // using builder as it has default
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

    #[tracing::instrument(
        name = "EmbucketCatalogList::s3tables_catalog",
        level = "debug",
        skip(self),
        err
    )]
    pub async fn s3tables_iceberg_catalog(
        &self,
        volume: S3TablesVolume,
        db_name: &str,
        should_refresh: bool,
    ) -> Result<CachingCatalog> {
        let (ak, sk, token) = match volume.credentials {
            AwsCredentials::AccessKey(ref creds) => (
                Some(creds.aws_access_key_id.clone()),
                Some(creds.aws_secret_access_key.clone()),
                creds.aws_session_token.clone(),
            ),
            AwsCredentials::Token(ref t) => (None, None, Some(t.clone())),
        };
        #[cfg(not(feature = "rest-catalog"))]
        {
            let creds =
                Credentials::from_keys(ak.unwrap_or_default(), sk.unwrap_or_default(), token);
            let config = defaults(BehaviorVersion::latest())
                .timeout_config(self.config.aws_sdk_timeout_config.clone().build())
                .credentials_provider(SharedCredentialsProvider::new(creds))
                .region(Region::new(volume.region()))
                .load()
                .await;
            let iceberg_catalog: Arc<dyn Catalog> = Arc::new(
                S3TablesCatalog::new(
                    &config,
                    volume.arn.as_str(),
                    ObjectStoreBuilder::S3(Box::new(volume.s3_builder())),
                )
                .context(catalog_error::S3TablesSnafu)?,
            );
            let catalog = DataFusionIcebergCatalog::new_sync(iceberg_catalog.clone(), None);
            return Ok(CachingCatalog::new(
                Arc::new(catalog),
                db_name.to_owned(),
                Some(iceberg_catalog),
                self.config.clone().into(),
            )
            .with_refresh(should_refresh)
            .with_catalog_type(CatalogType::S3tables));
        }

        #[cfg(feature = "rest-catalog")]
        {
            let base_path = volume.endpoint.clone().unwrap_or_else(|| {
                format!("https://s3tables.{}.amazonaws.com/iceberg", volume.region())
            });
            let config = Configuration {
                base_path,
                aws_v4_key: Some(AWSv4Key {
                    access_key: ak.unwrap_or_default(),
                    secret_key: SecretString::new(sk.unwrap_or_default()),
                    session_token: token.map(SecretString::new),
                    region: volume.region(),
                    service: "s3tables".to_string(),
                }),
                ..Default::default()
            };
            let object_store_builder = ObjectStoreBuilder::S3(Box::new(volume.s3_builder()));
            let rest_catalog: Arc<dyn Catalog> = Arc::new(IcebergRestCatalog::new(
                Some(volume.arn.as_str()),
                config.clone(),
                Some(object_store_builder.clone()),
            ));
            let iceberg_catalog: Arc<dyn Catalog> = Arc::new(RestCatalog::new(
                Some(volume.arn.as_str()),
                config,
                rest_catalog,
                object_store_builder,
            ));
            let catalog = DataFusionIcebergCatalog::new_sync(iceberg_catalog.clone(), None);
            Ok(CachingCatalog::new(
                Arc::new(catalog),
                db_name.to_owned(),
                Some(iceberg_catalog),
                self.config.clone().into(),
            )
            .with_refresh(should_refresh)
            .with_catalog_type(CatalogType::S3tables))
        }
    }

    /// Register an iceberg catalog as an embucket catalog (with EmbucketCatalog as
    /// the DataFusion CatalogProvider).
    pub fn register_iceberg_catalog(
        &self,
        name: &str,
        iceberg_catalog: Arc<dyn Catalog>,
        should_refresh: bool,
    ) {
        let catalog_provider: Arc<dyn CatalogProvider> = Arc::new(EmbucketCatalog::new(
            name.to_owned(),
            iceberg_catalog.clone(),
            (&self.config).into(),
        ));
        let caching = CachingCatalog::new(
            catalog_provider,
            name.to_owned(),
            Some(iceberg_catalog),
            (&self.config).into(),
        )
        .with_refresh(should_refresh)
        .with_properties(Properties::default());

        self.catalogs.insert(name.to_owned(), Arc::new(caching));
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
