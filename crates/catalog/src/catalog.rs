use crate::catalog_list::CatalogListConfig;
use crate::error;
use crate::error::Result as CatalogResult;
use crate::schema::CachingSchema;
use chrono::NaiveDateTime;
use dashmap::DashMap;
use datafusion::catalog::{CatalogProvider, SchemaProvider};
use datafusion_common::DataFusionError;
use iceberg_rust::catalog::Catalog;
use iceberg_rust_spec::namespace::Namespace;
use snafu::ResultExt;
use std::fmt::{Display, Formatter};
use std::time::Duration;
use std::{any::Any, sync::Arc};

#[derive(Clone)]
pub struct CachingCatalog {
    pub catalog: Arc<dyn CatalogProvider>,
    pub iceberg_catalog: Option<Arc<dyn Catalog>>,
    pub catalog_type: CatalogType,
    pub schemas_cache: DashMap<String, Arc<CachingSchema>>,
    pub should_refresh: bool,
    pub name: String,
    pub enable_information_schema: bool,
    pub properties: Option<Properties>,
    pub config: CatalogConfig,
}

#[derive(Clone)]
pub struct Properties {
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl Default for Properties {
    fn default() -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Clone, Debug)]
pub enum CatalogType {
    Embucket,
    Memory,
}

impl Display for CatalogType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Embucket => write!(f, "embucket"),
            Self::Memory => write!(f, "memory"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CatalogConfig {
    pub iceberg_catalog_timeout_secs: u64,
    pub iceberg_table_timeout_secs: u64,
}

impl From<&CatalogListConfig> for CatalogConfig {
    fn from(config: &CatalogListConfig) -> Self {
        Self {
            iceberg_catalog_timeout_secs: config.iceberg_catalog_timeout_secs,
            iceberg_table_timeout_secs: config.iceberg_table_timeout_secs,
        }
    }
}

impl From<CatalogListConfig> for CatalogConfig {
    fn from(config: CatalogListConfig) -> Self {
        (&config).into()
    }
}

impl CatalogConfig {
    #[must_use]
    pub const fn table_timeout(&self) -> Duration {
        Duration::from_secs(self.iceberg_table_timeout_secs)
    }

    #[must_use]
    pub const fn catalog_timeout(&self) -> Duration {
        Duration::from_secs(self.iceberg_catalog_timeout_secs)
    }
}

impl CachingCatalog {
    pub fn new(
        catalog_provider: Arc<dyn CatalogProvider>,
        name: String,
        iceberg_catalog: Option<Arc<dyn Catalog>>,
        config: CatalogConfig,
    ) -> Self {
        Self {
            catalog: catalog_provider,
            iceberg_catalog,
            schemas_cache: DashMap::new(),
            should_refresh: false,
            enable_information_schema: true,
            name,
            catalog_type: CatalogType::Embucket,
            properties: None,
            config,
        }
    }
    #[must_use]
    pub const fn with_refresh(mut self, refresh: bool) -> Self {
        self.should_refresh = refresh;
        self
    }
    #[must_use]
    pub const fn with_information_schema(mut self, enable_information_schema: bool) -> Self {
        self.enable_information_schema = enable_information_schema;
        self
    }

    #[must_use]
    pub const fn with_catalog_type(mut self, catalog_type: CatalogType) -> Self {
        self.catalog_type = catalog_type;
        self
    }

    #[must_use]
    pub const fn with_properties(mut self, properties: Properties) -> Self {
        self.properties = Some(properties);
        self
    }

    /// Asynchronously create a namespace in the underlying iceberg catalog and
    /// reflect it in the in-memory mirror. Use this from async SQL handlers
    /// instead of calling the sync `register_schema` trait method, which only
    /// updates the mirror.
    ///
    /// Returns the resulting `CachingSchema` wrapper.
    #[tracing::instrument(
        name = "CachingCatalog::create_namespace_async",
        level = "debug",
        skip(self),
        err
    )]
    pub async fn create_namespace_async(&self, name: &str) -> CatalogResult<Arc<CachingSchema>> {
        if let Some(catalog) = &self.iceberg_catalog {
            let namespace = Namespace::try_new(std::slice::from_ref(&name.to_string()))
                .map_err(|err| iceberg_rust::error::Error::External(Box::new(err)))
                .context(error::IcebergSnafu)?;
            catalog
                .create_namespace(&namespace, None)
                .await
                .context(error::IcebergSnafu)?;
        }

        // Update the mirror so subsequent reads see the namespace.
        // The schema argument is required by the trait but ignored by IcebergCatalog.
        let dummy = empty_mirror_schema();
        let _ = self
            .catalog
            .register_schema(name, dummy)
            .context(error::DataFusionSnafu)?;

        let schema_provider = self
            .catalog
            .schema(name)
            .ok_or_else(|| {
                DataFusionError::Internal(format!(
                    "Failed to look up newly registered schema {name} in catalog {}",
                    self.name
                ))
            })
            .context(error::DataFusionSnafu)?;

        let caching_schema = Arc::new(CachingSchema {
            name: name.to_string(),
            schema: schema_provider,
            tables_cache: DashMap::new(),
            iceberg_catalog: self.iceberg_catalog.clone(),
            config: self.config.clone(),
        });
        self.schemas_cache
            .insert(name.to_string(), Arc::clone(&caching_schema));
        Ok(caching_schema)
    }

    /// Asynchronously drop a namespace from the underlying iceberg catalog and
    /// remove it from the in-memory mirror.
    #[tracing::instrument(
        name = "CachingCatalog::drop_namespace_async",
        level = "debug",
        skip(self),
        err
    )]
    pub async fn drop_namespace_async(
        &self,
        name: &str,
        cascade: bool,
    ) -> CatalogResult<Option<Arc<CachingSchema>>> {
        if let Some(catalog) = &self.iceberg_catalog {
            let namespace = Namespace::try_new(std::slice::from_ref(&name.to_string()))
                .map_err(|err| iceberg_rust::error::Error::External(Box::new(err)))
                .context(error::IcebergSnafu)?;
            catalog
                .drop_namespace(&namespace)
                .await
                .context(error::IcebergSnafu)?;
        }

        let _ = self
            .catalog
            .deregister_schema(name, cascade)
            .context(error::DataFusionSnafu)?;
        Ok(self.schemas_cache.remove(name).map(|(_, s)| s))
    }
}

fn empty_mirror_schema() -> Arc<dyn SchemaProvider> {
    use datafusion::catalog::MemorySchemaProvider;
    Arc::new(MemorySchemaProvider::new())
}

#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for CachingCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Catalog")
            .field("name", &self.name)
            .field("should_refresh", &self.should_refresh)
            .field("schemas_cache", &self.schemas_cache)
            .field("catalog", &"")
            .finish()
    }
}

impl CatalogProvider for CachingCatalog {
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[tracing::instrument(
        name = "CachingCatalog::schema_names",
        level = "debug",
        skip(self),
        fields(schemas_names_count, catalog_name=format!("{:?}", self.name)),
    )]
    fn schema_names(&self) -> Vec<String> {
        let mut schema_names = self.catalog.schema_names();
        // The underlying iceberg mirror uses a DashMap whose iteration order is
        // non-deterministic. Sort here so callers (information_schema, SHOW
        // SCHEMAS, snapshot tests) see a stable ordering.
        schema_names.sort();

        // Remove outdated records
        let schema_names_set: std::collections::HashSet<_> = schema_names.iter().cloned().collect();
        self.schemas_cache
            .retain(|name, _| schema_names_set.contains(name));

        // Record the result as part of the current span.
        tracing::Span::current().record("schemas_names_count", schema_names.len());

        schema_names
    }

    #[tracing::instrument(name = "CachingCatalog::schema", level = "debug", skip(self))]
    #[allow(clippy::as_conversions)]
    fn schema(&self, name: &str) -> Option<Arc<dyn SchemaProvider>> {
        if let Some(schema) = self.schemas_cache.get(name) {
            Some(Arc::clone(schema.value()) as Arc<dyn SchemaProvider>)
        } else if let Some(schema) = self.catalog.schema(name) {
            let caching_schema = Arc::new(CachingSchema {
                name: name.to_string(),
                schema: Arc::clone(&schema),
                tables_cache: DashMap::new(),
                iceberg_catalog: self.iceberg_catalog.clone(),
                config: self.config.clone(),
            });

            self.schemas_cache
                .insert(name.to_string(), Arc::clone(&caching_schema));
            Some(caching_schema as Arc<dyn SchemaProvider>)
        } else {
            None
        }
    }

    #[tracing::instrument(
        name = "CachingCatalog::register_schema",
        level = "debug",
        skip(self),
        fields(schemas_names_count, catalog_name=format!("{:?}", self.name)),
    )]
    /// Register a schema. For non-iceberg catalogs this delegates to the inner
    /// provider as usual. For iceberg-backed catalogs this only updates the
    /// in-memory mirror — the namespace must already have been created in the
    /// iceberg catalog by an earlier `create_namespace_async` call.
    fn register_schema(
        &self,
        name: &str,
        schema: Arc<dyn SchemaProvider>,
    ) -> datafusion_common::Result<Option<Arc<dyn SchemaProvider>>> {
        if self.iceberg_catalog.is_none() {
            return self.catalog.register_schema(name, schema);
        }

        let _ = self.catalog.register_schema(name, Arc::clone(&schema))?;
        let schema_provider = self.catalog.schema(name).ok_or_else(|| {
            DataFusionError::Internal(format!(
                "Failed to look up newly registered schema {name} in catalog {}",
                self.name
            ))
        })?;

        let caching_schema = Arc::new(CachingSchema {
            name: name.to_string(),
            schema: schema_provider,
            tables_cache: DashMap::new(),
            iceberg_catalog: self.iceberg_catalog.clone(),
            config: self.config.clone(),
        });
        self.schemas_cache
            .insert(name.to_string(), Arc::clone(&caching_schema));
        Ok(Some(caching_schema))
    }

    /// Deregister a schema. For non-iceberg catalogs this delegates to the inner
    /// provider. For iceberg-backed catalogs this only updates the mirror — the
    /// underlying namespace must be dropped via `drop_namespace_async`.
    #[tracing::instrument(
        name = "CachingCatalog::deregister_schema",
        level = "debug",
        skip(self),
        fields(schemas_names_count, catalog_name=format!("{:?}", self.name)),
    )]
    fn deregister_schema(
        &self,
        name: &str,
        cascade: bool,
    ) -> datafusion_common::Result<Option<Arc<dyn SchemaProvider>>> {
        if self.iceberg_catalog.is_none() {
            return self.catalog.deregister_schema(name, cascade);
        }
        let _ = self.catalog.deregister_schema(name, cascade)?;
        Ok(self
            .schemas_cache
            .remove(name)
            .map(|(_, s)| -> Arc<dyn SchemaProvider> { s }))
    }
}
