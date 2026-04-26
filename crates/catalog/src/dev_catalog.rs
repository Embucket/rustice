use crate::catalog_list::{CatalogListConfig, DEFAULT_CATALOG, EmbucketCatalogList};
use crate::error::{IcebergSqlCatalogSnafu, Result};
use async_trait::async_trait;
use iceberg_rust::catalog::commit::{CommitTable, CommitView};
use iceberg_rust::catalog::create::{CreateMaterializedView, CreateTable, CreateView};
use iceberg_rust::catalog::identifier::Identifier;
use iceberg_rust::catalog::namespace::Namespace;
use iceberg_rust::catalog::tabular::Tabular;
use iceberg_rust::catalog::{Catalog, CatalogList};
use iceberg_rust::error::Error as IcebergError;
use iceberg_rust::materialized_view::MaterializedView;
use iceberg_rust::object_store::ObjectStoreBuilder;
use iceberg_rust::table::Table;
use iceberg_rust::view::View;
use iceberg_sql_catalog::SqlCatalogList;
use snafu::ResultExt;
use std::collections::HashMap;
use std::sync::Arc;

/// Build an in-memory dev-mode catalog list backed by an in-memory SQLite
/// Iceberg SQL catalog and an in-memory object store.
///
/// Registers a single default catalog under the name [`DEFAULT_CATALOG`].
pub async fn build_dev_catalog_list(
    config: CatalogListConfig,
) -> Result<Arc<EmbucketCatalogList>> {
    let object_store = ObjectStoreBuilder::memory();
    let sql_catalog_list = Arc::new(
        SqlCatalogList::new("sqlite://", object_store)
            .await
            .context(IcebergSqlCatalogSnafu)?,
    );

    let embucket = Arc::new(EmbucketCatalogList::new(config));

    if let Some(catalog) = sql_catalog_list.catalog(DEFAULT_CATALOG) {
        let wrapped: Arc<dyn Catalog> =
            Arc::new(DevCatalog::new(catalog, "/dev".to_string()));
        embucket
            .register_iceberg_catalog(DEFAULT_CATALOG, wrapped, false)
            .await?;
    }

    Ok(embucket)
}

/// Dev-only wrapper around an iceberg `Catalog` that injects a default
/// table location on `create_table` / `create_view` / `create_materialized_view`
/// when the caller has not supplied one. The upstream `SqlCatalog` requires
/// `CreateTable.location` to be set; production catalogs (s3tables, REST) set
/// it themselves and so don't need this.
///
/// The default location is derived from a fixed warehouse root and the table's
/// fully-qualified identifier, mirroring `iceberg-file-catalog::tabular_path`.
#[derive(Debug)]
struct DevCatalog {
    inner: Arc<dyn Catalog>,
    warehouse_root: String,
}

impl DevCatalog {
    fn new(inner: Arc<dyn Catalog>, warehouse_root: String) -> Self {
        Self {
            inner,
            warehouse_root,
        }
    }

    fn tabular_path(&self, identifier: &Identifier) -> String {
        let mut path = self.warehouse_root.trim_end_matches('/').to_owned();
        for part in identifier.namespace().iter() {
            path.push('/');
            path.push_str(part);
        }
        path.push('/');
        path.push_str(identifier.name());
        path
    }
}

#[async_trait]
impl Catalog for DevCatalog {
    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn create_namespace(
        &self,
        namespace: &Namespace,
        properties: Option<HashMap<String, String>>,
    ) -> std::result::Result<HashMap<String, String>, IcebergError> {
        self.inner.create_namespace(namespace, properties).await
    }

    async fn drop_namespace(
        &self,
        namespace: &Namespace,
    ) -> std::result::Result<(), IcebergError> {
        self.inner.drop_namespace(namespace).await
    }

    async fn load_namespace(
        &self,
        namespace: &Namespace,
    ) -> std::result::Result<HashMap<String, String>, IcebergError> {
        self.inner.load_namespace(namespace).await
    }

    async fn update_namespace(
        &self,
        namespace: &Namespace,
        updates: Option<HashMap<String, String>>,
        removals: Option<Vec<String>>,
    ) -> std::result::Result<(), IcebergError> {
        self.inner
            .update_namespace(namespace, updates, removals)
            .await
    }

    async fn namespace_exists(
        &self,
        namespace: &Namespace,
    ) -> std::result::Result<bool, IcebergError> {
        self.inner.namespace_exists(namespace).await
    }

    async fn list_tabulars(
        &self,
        namespace: &Namespace,
    ) -> std::result::Result<Vec<Identifier>, IcebergError> {
        self.inner.list_tabulars(namespace).await
    }

    async fn list_namespaces(
        &self,
        parent: Option<&str>,
    ) -> std::result::Result<Vec<Namespace>, IcebergError> {
        self.inner.list_namespaces(parent).await
    }

    async fn tabular_exists(
        &self,
        identifier: &Identifier,
    ) -> std::result::Result<bool, IcebergError> {
        self.inner.tabular_exists(identifier).await
    }

    async fn drop_table(&self, identifier: &Identifier) -> std::result::Result<(), IcebergError> {
        self.inner.drop_table(identifier).await
    }

    async fn drop_view(&self, identifier: &Identifier) -> std::result::Result<(), IcebergError> {
        self.inner.drop_view(identifier).await
    }

    async fn drop_materialized_view(
        &self,
        identifier: &Identifier,
    ) -> std::result::Result<(), IcebergError> {
        self.inner.drop_materialized_view(identifier).await
    }

    async fn load_tabular(
        self: Arc<Self>,
        identifier: &Identifier,
    ) -> std::result::Result<Tabular, IcebergError> {
        self.inner.clone().load_tabular(identifier).await
    }

    async fn create_table(
        self: Arc<Self>,
        identifier: Identifier,
        mut create_table: CreateTable,
    ) -> std::result::Result<Table, IcebergError> {
        if create_table.location.is_none() {
            create_table.location = Some(self.tabular_path(&identifier));
        }
        self.inner.clone().create_table(identifier, create_table).await
    }

    async fn create_view(
        self: Arc<Self>,
        identifier: Identifier,
        mut create_view: CreateView<Option<()>>,
    ) -> std::result::Result<View, IcebergError> {
        if create_view.location.is_none() {
            create_view.location = Some(self.tabular_path(&identifier));
        }
        self.inner.clone().create_view(identifier, create_view).await
    }

    async fn create_materialized_view(
        self: Arc<Self>,
        identifier: Identifier,
        mut create_view: CreateMaterializedView,
    ) -> std::result::Result<MaterializedView, IcebergError> {
        if create_view.location.is_none() {
            create_view.location = Some(self.tabular_path(&identifier));
        }
        self.inner
            .clone()
            .create_materialized_view(identifier, create_view)
            .await
    }

    async fn update_table(
        self: Arc<Self>,
        commit: CommitTable,
    ) -> std::result::Result<Table, IcebergError> {
        self.inner.clone().update_table(commit).await
    }

    async fn update_view(
        self: Arc<Self>,
        commit: CommitView<Option<()>>,
    ) -> std::result::Result<View, IcebergError> {
        self.inner.clone().update_view(commit).await
    }

    async fn update_materialized_view(
        self: Arc<Self>,
        commit: CommitView<Identifier>,
    ) -> std::result::Result<MaterializedView, IcebergError> {
        self.inner.clone().update_materialized_view(commit).await
    }

    async fn register_table(
        self: Arc<Self>,
        identifier: Identifier,
        metadata_location: &str,
    ) -> std::result::Result<Table, IcebergError> {
        self.inner
            .clone()
            .register_table(identifier, metadata_location)
            .await
    }
}
