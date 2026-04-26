use crate::catalog::CatalogConfig;
use crate::{block_on_with_timeout, error};
use async_trait::async_trait;
use datafusion::catalog::{SchemaProvider, TableProvider};
use datafusion_common::DataFusionError;
use datafusion_iceberg::DataFusionTable as IcebergDataFusionTable;
use iceberg_rust::catalog::Catalog as IcebergCatalog;
use iceberg_rust::catalog::identifier::Identifier as IcebergIdentifier;
use iceberg_rust::error::Error as IcebergError;
use iceberg_rust_spec::namespace::Namespace;
use snafu::ResultExt;
use std::any::Any;
use std::sync::Arc;
use tracing::error;

fn make_namespace(schema: &str) -> Result<Namespace, IcebergError> {
    Namespace::try_new(std::slice::from_ref(&schema.to_string()))
        .map_err(|e| IcebergError::External(Box::new(e)))
}

pub struct EmbucketSchema {
    pub database: String,
    pub schema: String,
    pub iceberg_catalog: Arc<dyn IcebergCatalog>,
    pub config: CatalogConfig,
}

#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for EmbucketSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DFSchema")
            .field("database", &self.database)
            .field("schema", &self.schema)
            .field("iceberg_catalog", &"")
            .finish()
    }
}

#[async_trait]
impl SchemaProvider for EmbucketSchema {
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[tracing::instrument(
        name = "EmbucketSchema::table_names",
        level = "debug",
        skip(self),
        fields(tables_names_count, schema_name=format!("{}.{}", self.database, self.schema))
    )]
    fn table_names(&self) -> Vec<String> {
        let iceberg_catalog = self.iceberg_catalog.clone();
        let schema = self.schema.clone();

        #[allow(clippy::expect_used)]
        let table_names = block_on_with_timeout(
            async move {
                let namespace = make_namespace(&schema).context(error::IcebergSnafu)?;
                iceberg_catalog
                    .list_tabulars(&namespace)
                    .await
                    .context(error::IcebergSnafu)
                    .map(|tabulars| {
                        tabulars
                            .into_iter()
                            .map(|ident| ident.name().to_string())
                            .collect()
                    })
            },
            self.config.catalog_timeout(),
        )
        .expect("Catalog timeout on: list_tabulars")
        .unwrap_or_else(|error| {
            error!(?error, "Failed to list tables; returning empty list");
            vec![]
        });
        // Record the result as part of the current span.
        tracing::Span::current().record("tables_names_count", table_names.len());

        table_names
    }

    #[tracing::instrument(name = "EmbucketSchema::table", level = "debug", skip(self), err)]
    async fn table(&self, name: &str) -> Result<Option<Arc<dyn TableProvider>>, DataFusionError> {
        let namespace = make_namespace(&self.schema)
            .map_err(|e| DataFusionError::External(Box::new(e)))?;
        let ident = IcebergIdentifier::new(&namespace, name);

        let tabular = self
            .iceberg_catalog
            .clone()
            .load_tabular(&ident)
            .await
            .map_err(|e| DataFusionError::External(Box::new(e)))?;

        let table_provider: Arc<dyn TableProvider> =
            Arc::new(IcebergDataFusionTable::new(tabular, None, None, None));
        Ok(Some(table_provider))
    }

    #[tracing::instrument(name = "EmbucketSchema::table_exist", level = "debug", skip(self))]
    fn table_exist(&self, name: &str) -> bool {
        let iceberg_catalog = self.iceberg_catalog.clone();
        let schema = self.schema.clone();
        let table = name.to_string();

        #[allow(clippy::expect_used)]
        block_on_with_timeout(
            async move {
                let namespace = make_namespace(&schema).context(error::IcebergSnafu)?;
                let ident = IcebergIdentifier::new(&namespace, &table);
                iceberg_catalog
                    .tabular_exists(&ident)
                    .await
                    .context(error::IcebergSnafu)
            },
            self.config.catalog_timeout(),
        )
        .expect("Catalog timeout on: tabular_exists")
        .unwrap_or_else(|error| {
            error!(
                ?error,
                table_name = %name,
                "Failed to check table existence; assuming missing",
            );
            false
        })
    }
}
