use super::schema::EmbucketSchema;
use crate::catalog::CatalogConfig;
use crate::{block_on_with_timeout, error};
use datafusion::catalog::{CatalogProvider, SchemaProvider};
use iceberg_rust::catalog::Catalog as IcebergCatalog;
use iceberg_rust::error::Error as IcebergError;
use iceberg_rust_spec::namespace::Namespace;
use snafu::ResultExt;
use std::{any::Any, sync::Arc};
use tracing::error;

fn make_namespace(schema: &str) -> Result<Namespace, IcebergError> {
    Namespace::try_new(std::slice::from_ref(&schema.to_string()))
        .map_err(|e| IcebergError::External(Box::new(e)))
}

pub struct EmbucketCatalog {
    pub database: String,
    pub iceberg_catalog: Arc<dyn IcebergCatalog>,
    pub config: CatalogConfig,
}

impl EmbucketCatalog {
    pub fn new(
        database: String,
        iceberg_catalog: Arc<dyn IcebergCatalog>,
        config: CatalogConfig,
    ) -> Self {
        Self {
            database,
            iceberg_catalog,
            config,
        }
    }

    #[must_use]
    pub fn catalog(&self) -> Arc<dyn IcebergCatalog> {
        self.iceberg_catalog.clone()
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for EmbucketCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DFCatalog")
            .field("database", &self.database)
            .field("iceberg_catalog", &"")
            .finish()
    }
}

impl CatalogProvider for EmbucketCatalog {
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[tracing::instrument(name = "EmbucketCatalog::schema_names", level = "debug", skip(self))]
    fn schema_names(&self) -> Vec<String> {
        let iceberg_catalog = self.iceberg_catalog.clone();

        #[allow(clippy::expect_used)]
        block_on_with_timeout(
            async move {
                iceberg_catalog
                    .list_namespaces(None)
                    .await
                    .context(error::IcebergSnafu)
                    .map(|namespaces| {
                        namespaces
                            .into_iter()
                            .map(|ns| ns.to_string())
                            .collect()
                    })
            },
            self.config.catalog_timeout(),
        )
        .expect("Catalog timeout on: list_namespaces")
        .unwrap_or_else(|error| {
            error!(
                ?error,
                "Failed to list Iceberg namespaces; returning empty list"
            );
            vec![]
        })
    }

    #[tracing::instrument(name = "EmbucketCatalog::schema", level = "debug", skip(self))]
    fn schema(&self, name: &str) -> Option<Arc<dyn SchemaProvider>> {
        let iceberg_catalog = self.iceberg_catalog.clone();
        let database = self.database.clone();
        let schema_name = name.to_string();
        let config = self.config.clone();

        #[allow(clippy::expect_used)]
        block_on_with_timeout(
            async move {
                let namespace = make_namespace(&schema_name).context(error::IcebergSnafu)?;
                let exists = iceberg_catalog
                    .namespace_exists(&namespace)
                    .await
                    .context(error::IcebergSnafu)?;

                if exists {
                    let provider: Arc<dyn SchemaProvider> = Arc::new(EmbucketSchema {
                        database,
                        schema: schema_name,
                        iceberg_catalog,
                        config,
                    });
                    Ok(Some(provider))
                } else {
                    Ok(None)
                }
            },
            self.config.catalog_timeout(),
        )
        .expect("Catalog timeout on: namespace_exists")
        .unwrap_or_else(|error: error::Error| {
            error!(?error, "Failed to get schema; assuming missing");
            None
        })
    }
}
