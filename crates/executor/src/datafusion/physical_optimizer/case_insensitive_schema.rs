use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::physical_plan::{
    FileScanConfig, FileScanConfigBuilder, ParquetSource,
};
use datafusion::datasource::source::DataSourceExec;
use datafusion::error::Result as DFResult;
use datafusion::physical_optimizer::PhysicalOptimizerRule;
use datafusion_common::config::ConfigOptions;
use datafusion_common::tree_node::{Transformed, TransformedResult, TreeNode};
use datafusion::physical_expr::expressions::Column;
use datafusion::physical_expr_adapter::{
    DefaultPhysicalExprAdapterFactory, PhysicalExprAdapter, PhysicalExprAdapterFactory,
};
use datafusion::physical_expr_common::physical_expr::PhysicalExpr;
use datafusion_physical_plan::ExecutionPlan;
use std::sync::Arc;

#[derive(Default, Debug)]
pub struct CaseInsensitiveSchemaDataSourceExec;

impl CaseInsensitiveSchemaDataSourceExec {
    pub const fn new() -> Self {
        Self
    }
}

/// The rule which uses a physical expression adapter factory that normalizes file field names
/// to lowercase before delegating to the default adapter, ensuring case-insensitive mapping
/// between table schema and physical Parquet files.
impl PhysicalOptimizerRule for CaseInsensitiveSchemaDataSourceExec {
    fn optimize(
        &self,
        plan: Arc<dyn ExecutionPlan>,
        _config: &ConfigOptions,
    ) -> DFResult<Arc<dyn ExecutionPlan>> {
        plan.transform_up(|plan| {
            if let Some(source_exec) = plan.as_any().downcast_ref::<DataSourceExec>()
                && let Some(config) = source_exec
                    .data_source()
                    .as_any()
                    .downcast_ref::<FileScanConfig>()
                && config
                    .file_source
                    .as_any()
                    .downcast_ref::<ParquetSource>()
                    .is_some()
                && !config
                    .file_schema()
                    .fields()
                    .iter()
                    .any(|field| field.name().eq(&field.name().to_ascii_uppercase()))
            {
                let expr_adapter: Arc<dyn PhysicalExprAdapterFactory> =
                    Arc::new(CaseInsensitiveExprAdapterFactory);

                let data_source = Arc::new(
                    FileScanConfigBuilder::from(config.clone())
                        .with_expr_adapter(Some(expr_adapter))
                        .build(),
                );

                let plan = Arc::new(source_exec.clone().with_data_source(data_source));
                return Ok(Transformed::yes(plan));
            }

            Ok(Transformed::no(plan))
        })
        .data()
    }

    fn name(&self) -> &'static str {
        "CaseInsensitiveSchemaDataSourceExec"
    }

    fn schema_check(&self) -> bool {
        true
    }
}

/// A physical expression adapter factory that normalizes file field names to lowercase
/// before delegating to the default adapter, ensuring case-insensitive mapping
/// between table schema and physical Parquet files.
#[derive(Debug, Default)]
struct CaseInsensitiveExprAdapterFactory;

impl PhysicalExprAdapterFactory for CaseInsensitiveExprAdapterFactory {
    fn create(
        &self,
        logical_file_schema: SchemaRef,
        physical_file_schema: SchemaRef,
    ) -> DFResult<Arc<dyn PhysicalExprAdapter>> {
        // Create a normalized (lowercased) version of the physical schema
        // so that the default adapter can match columns case-insensitively
        let normalized_physical = normalize_schema_ref(&physical_file_schema);
        let inner = DefaultPhysicalExprAdapterFactory
            .create(logical_file_schema, normalized_physical)?;
        Ok(Arc::new(CaseInsensitiveExprAdapter {
            inner,
            physical_file_schema,
        }))
    }
}

#[derive(Debug)]
struct CaseInsensitiveExprAdapter {
    inner: Arc<dyn PhysicalExprAdapter>,
    physical_file_schema: SchemaRef,
}

impl PhysicalExprAdapter for CaseInsensitiveExprAdapter {
    fn rewrite(&self, expr: Arc<dyn PhysicalExpr>) -> DFResult<Arc<dyn PhysicalExpr>> {
        // First let the default adapter rewrite using normalized schema
        let rewritten = self.inner.rewrite(expr)?;
        // Then fix up column indices to reference the actual physical schema
        rewritten
            .transform(|expr| {
                if let Some(col) = expr.as_any().downcast_ref::<Column>() {
                    // Try to find the column in the actual physical schema by case-insensitive match
                    let col_name_lower = col.name().to_ascii_lowercase();
                    for (i, field) in self.physical_file_schema.fields().iter().enumerate() {
                        if field.name().to_ascii_lowercase() == col_name_lower {
                            return Ok(Transformed::yes(Arc::new(Column::new(
                                field.name(),
                                i,
                            ))
                                as Arc<dyn PhysicalExpr>));
                        }
                    }
                }
                Ok(Transformed::no(expr))
            })
            .data()
    }
}

fn normalize_schema_ref(schema: &SchemaRef) -> SchemaRef {
    let fields = schema
        .fields()
        .iter()
        .map(|field| {
            let mut cloned = field.as_ref().clone();
            cloned.set_name(field.name().to_ascii_lowercase());
            cloned
        })
        .collect::<Vec<_>>();
    Arc::new(datafusion::arrow::datatypes::Schema::new(fields))
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use datafusion::datasource::listing::PartitionedFile;
    use datafusion::datasource::object_store::ObjectStoreUrl;
    use datafusion::datasource::physical_plan::{FileGroup, FileScanConfigBuilder};

    fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![Field::new("Id", DataType::Int32, false)]))
    }

    #[tokio::test]
    async fn test_sets_expr_adapter_on_file_scan_config() -> DFResult<()> {
        let object_store_url = ObjectStoreUrl::parse("s3://bucket")?;
        let file_source = Arc::new(ParquetSource::new(schema()));
        let file_scan_config = Arc::new(
            FileScanConfigBuilder::new(object_store_url, file_source)
                .with_file_groups(vec![FileGroup::new(vec![PartitionedFile::new("path", 1)])])
                .build(),
        );

        let data_source_exec = Arc::new(DataSourceExec::new(file_scan_config));
        let rule = CaseInsensitiveSchemaDataSourceExec::new();
        let optimized = rule.optimize(data_source_exec, &ConfigOptions::default())?;

        let data_source_exec = optimized
            .as_any()
            .downcast_ref::<DataSourceExec>()
            .expect("expected DataSourceExec");

        let file_scan_config = data_source_exec
            .data_source()
            .as_any()
            .downcast_ref::<FileScanConfig>()
            .expect("expected FileScanConfig");

        assert!(file_scan_config.expr_adapter_factory.is_some());

        Ok(())
    }
}
