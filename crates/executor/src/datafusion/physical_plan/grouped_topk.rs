use std::cmp::Ordering;
use std::sync::Arc;

use async_stream::try_stream;
use datafusion::arrow::array::{ArrayRef, UInt32Array, UInt64Array};
use datafusion::arrow::compute::take_record_batch;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::arrow::row::{OwnedRow, RowConverter, Rows, SortField};
use datafusion::execution::TaskContext;
use datafusion::physical_expr::{Distribution, EquivalenceProperties, PhysicalSortExpr};
use datafusion_common::{DataFusionError, Result};
use datafusion_physical_plan::execution_plan::EmissionType;
use datafusion_physical_plan::metrics::{BaselineMetrics, ExecutionPlanMetricsSet, MetricsSet};
use datafusion_physical_plan::stream::RecordBatchStreamAdapter;
use datafusion_physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, ExecutionPlanProperties, PlanProperties,
    SendableRecordBatchStream,
};
use futures::StreamExt;

#[derive(Debug)]
pub struct GroupedTopKExec {
    input: Arc<dyn ExecutionPlan>,
    partition_by: Vec<Arc<dyn datafusion::physical_expr::PhysicalExpr>>,
    order_by: Vec<PhysicalSortExpr>,
    limit: usize,
    schema: SchemaRef,
    properties: Arc<PlanProperties>,
    metrics: ExecutionPlanMetricsSet,
}

#[derive(Clone)]
struct SelectedRow {
    row_index: usize,
    row_number: u64,
}

impl GroupedTopKExec {
    pub fn try_new(
        input: Arc<dyn ExecutionPlan>,
        partition_by: Vec<Arc<dyn datafusion::physical_expr::PhysicalExpr>>,
        order_by: Vec<PhysicalSortExpr>,
        limit: usize,
        schema: SchemaRef,
    ) -> Result<Self> {
        if limit == 0 {
            return Err(DataFusionError::Internal(
                "GroupedTopKExec limit must be greater than zero".to_string(),
            ));
        }

        if schema.fields().len() != input.schema().fields().len() + 1 {
            return Err(DataFusionError::Internal(format!(
                "GroupedTopKExec output schema must contain input columns plus row_number, got input={} output={}",
                input.schema().fields().len(),
                schema.fields().len()
            )));
        }

        let properties = Arc::new(PlanProperties::new(
            EquivalenceProperties::new(Arc::clone(&schema)),
            input.output_partitioning().clone(),
            EmissionType::Final,
            input.boundedness(),
        ));

        Ok(Self {
            input,
            partition_by,
            order_by,
            limit,
            schema,
            properties,
            metrics: ExecutionPlanMetricsSet::new(),
        })
    }
}

impl DisplayAs for GroupedTopKExec {
    fn fmt_as(&self, t: DisplayFormatType, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match t {
            DisplayFormatType::Default
            | DisplayFormatType::Verbose
            | DisplayFormatType::TreeRender => {
                write!(
                    f,
                    "GroupedTopKExec: partition_by=[{}], order_by=[{}], limit={}",
                    self.partition_by
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    self.order_by
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    self.limit
                )
            }
        }
    }
}

impl ExecutionPlan for GroupedTopKExec {
    fn name(&self) -> &'static str {
        "GroupedTopKExec"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }

    fn required_input_distribution(&self) -> Vec<Distribution> {
        vec![Distribution::HashPartitioned(self.partition_by.clone())]
    }

    fn maintains_input_order(&self) -> Vec<bool> {
        vec![false]
    }

    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![&self.input]
    }

    fn metrics(&self) -> Option<MetricsSet> {
        Some(self.metrics.clone_inner())
    }

    fn with_new_children(
        self: Arc<Self>,
        mut children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if children.len() != 1 {
            return Err(DataFusionError::Internal(format!(
                "GroupedTopKExec expected one child, got {}",
                children.len()
            )));
        }

        Ok(Arc::new(Self::try_new(
            children.swap_remove(0),
            self.partition_by.clone(),
            self.order_by.clone(),
            self.limit,
            Arc::clone(&self.schema),
        )?))
    }

    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream> {
        let input = Arc::clone(&self.input);
        let partition_by = self.partition_by.clone();
        let order_by = self.order_by.clone();
        let limit = self.limit;
        let schema = Arc::clone(&self.schema);
        let stream_schema = Arc::clone(&schema);
        let metrics = BaselineMetrics::new(&self.metrics, partition);
        let input_stream = input.execute(partition, context)?;

        let stream = try_stream! {
            let output_batches = {
                let _timer = metrics.elapsed_compute().timer();
                let input_batches = collect_input(input_stream).await?;
                select_topk_batches(&input_batches, &partition_by, &order_by, limit, Arc::clone(&schema))?
            };

            for batch in output_batches {
                metrics.record_output(batch.num_rows());
                yield batch;
            }
        };

        Ok(Box::pin(RecordBatchStreamAdapter::new(
            stream_schema,
            stream,
        )))
    }
}

async fn collect_input(mut input: SendableRecordBatchStream) -> Result<Vec<RecordBatch>> {
    let mut batches = Vec::new();
    while let Some(batch) = input.next().await {
        batches.push(batch?);
    }
    Ok(batches)
}

fn select_topk_batches(
    input_batches: &[RecordBatch],
    partition_by: &[Arc<dyn datafusion::physical_expr::PhysicalExpr>],
    order_by: &[PhysicalSortExpr],
    limit: usize,
    output_schema: SchemaRef,
) -> Result<Vec<RecordBatch>> {
    if limit == 1 {
        return select_top1_batches(input_batches, partition_by, order_by, output_schema);
    }

    let (all_partition_rows, all_order_rows) = encode_rows(input_batches, partition_by, order_by)?;

    // Each group keeps up to `limit` row coordinates; comparisons read sort-encoded bytes
    // from the retained rows, with the (batch, row) tiebreak standing in for input order.
    let mut winners: datafusion_common::HashMap<OwnedRow, Vec<(usize, usize)>> =
        datafusion_common::HashMap::default();

    for batch_index in 0..input_batches.len() {
        let partition_rows = &all_partition_rows[batch_index];
        for row_index in 0..partition_rows.num_rows() {
            let group = winners
                .entry(partition_rows.row(row_index).owned())
                .or_insert_with(|| Vec::with_capacity(limit));
            insert_topk_coord(group, (batch_index, row_index), &all_order_rows, limit);
        }
    }

    let mut selected_by_batch = vec![Vec::new(); input_batches.len()];
    for mut coords in winners.into_values() {
        coords.sort_unstable_by(|&a, &b| cmp_coord_order(a, b, &all_order_rows));
        for (offset, (batch_index, row_index)) in coords.into_iter().enumerate() {
            selected_by_batch[batch_index].push(SelectedRow {
                row_index,
                row_number: u64::try_from(offset + 1).map_err(|_| {
                    DataFusionError::Execution(format!(
                        "GroupedTopKExec row number {} exceeds UInt64 range",
                        offset + 1
                    ))
                })?,
            });
        }
    }

    build_output_batches(input_batches, selected_by_batch, &output_schema)
}

fn select_top1_batches(
    input_batches: &[RecordBatch],
    partition_by: &[Arc<dyn datafusion::physical_expr::PhysicalExpr>],
    order_by: &[PhysicalSortExpr],
    output_schema: SchemaRef,
) -> Result<Vec<RecordBatch>> {
    use datafusion_common::hash_map::Entry;

    struct Top1Winner {
        batch_index: usize,
        row_index: usize,
    }

    // Encode every batch once and keep the rows alive; a winner is referenced solely by its
    // (batch, row) coordinates, and comparisons read sort-encoded bytes from the retained
    // buffers — no per-row ScalarValue materialization or per-winner order allocation.
    let (all_partition_rows, all_order_rows) = encode_rows(input_batches, partition_by, order_by)?;

    let mut winners: datafusion_common::HashMap<OwnedRow, Top1Winner> =
        datafusion_common::HashMap::default();

    for batch_index in 0..input_batches.len() {
        let partition_rows = &all_partition_rows[batch_index];
        let order_rows = &all_order_rows[batch_index];

        for row_index in 0..order_rows.num_rows() {
            match winners.entry(partition_rows.row(row_index).owned()) {
                Entry::Occupied(mut slot) => {
                    // Sort-encoded rows compare bytewise in the configured sort order, so a
                    // strictly smaller row replaces the winner; ties keep the earliest seen.
                    let winner = slot.get();
                    if order_rows.row(row_index)
                        < all_order_rows[winner.batch_index].row(winner.row_index)
                    {
                        slot.insert(Top1Winner {
                            batch_index,
                            row_index,
                        });
                    }
                }
                Entry::Vacant(slot) => {
                    slot.insert(Top1Winner {
                        batch_index,
                        row_index,
                    });
                }
            }
        }
    }

    let mut selected_by_batch = vec![Vec::new(); input_batches.len()];
    for winner in winners.into_values() {
        selected_by_batch[winner.batch_index].push(SelectedRow {
            row_index: winner.row_index,
            row_number: 1,
        });
    }

    build_output_batches(input_batches, selected_by_batch, &output_schema)
}

fn build_output_batches(
    input_batches: &[RecordBatch],
    selected_by_batch: Vec<Vec<SelectedRow>>,
    output_schema: &SchemaRef,
) -> Result<Vec<RecordBatch>> {
    let mut output_batches = Vec::new();
    for (batch, mut selected_rows) in input_batches.iter().zip(selected_by_batch) {
        if selected_rows.is_empty() {
            continue;
        }
        selected_rows.sort_unstable_by_key(|selected| selected.row_index);
        let row_numbers = selected_rows
            .iter()
            .map(|selected| selected.row_number)
            .collect::<Vec<_>>();
        let indices = selected_rows
            .into_iter()
            .map(|selected| {
                u32::try_from(selected.row_index).map_err(|_| {
                    DataFusionError::Execution(format!(
                        "GroupedTopKExec row index {} exceeds UInt32 take index range",
                        selected.row_index
                    ))
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let indices = UInt32Array::from(indices);
        let selected = take_record_batch(batch, &indices)?;

        let mut columns: Vec<ArrayRef> = selected.columns().to_vec();
        columns.push(Arc::new(UInt64Array::from(row_numbers)));
        output_batches.push(RecordBatch::try_new(Arc::clone(output_schema), columns)?);
    }

    Ok(output_batches)
}

/// Orders two row coordinates by their sort-encoded order bytes, breaking ties by the
/// (batch, row) coordinate, which equals input order since batches and rows are visited in
/// order. This is the single ordering used for both top-K selection and final ranking.
fn cmp_coord_order(a: (usize, usize), b: (usize, usize), all_order_rows: &[Rows]) -> Ordering {
    all_order_rows[a.0]
        .row(a.1)
        .cmp(&all_order_rows[b.0].row(b.1))
        .then(a.cmp(&b))
}

fn insert_topk_coord(
    group: &mut Vec<(usize, usize)>,
    coord: (usize, usize),
    all_order_rows: &[Rows],
    limit: usize,
) {
    if group.len() < limit {
        group.push(coord);
        return;
    }

    let mut worst = 0;
    for index in 1..group.len() {
        if cmp_coord_order(group[index], group[worst], all_order_rows).is_gt() {
            worst = index;
        }
    }

    if cmp_coord_order(coord, group[worst], all_order_rows).is_lt() {
        group[worst] = coord;
    }
}

fn evaluate_exprs(
    exprs: &[Arc<dyn datafusion::physical_expr::PhysicalExpr>],
    batch: &RecordBatch,
) -> Result<Vec<ArrayRef>> {
    exprs
        .iter()
        .map(|expr| expr.evaluate(batch)?.into_array_of_size(batch.num_rows()))
        .collect()
}

fn evaluate_sort_exprs(exprs: &[PhysicalSortExpr], batch: &RecordBatch) -> Result<Vec<ArrayRef>> {
    exprs
        .iter()
        .map(|expr| {
            expr.expr
                .evaluate(batch)?
                .into_array_of_size(batch.num_rows())
        })
        .collect()
}

/// Encodes each batch's partition and order columns into Arrow row format once. Partition
/// rows are encoded canonically (for grouping equality/hash) and order rows in the
/// configured sort order (so they compare bytewise, honoring ASC/DESC and null placement),
/// letting the grouping loops work on contiguous bytes instead of per-row ScalarValue vecs.
fn encode_rows(
    input_batches: &[RecordBatch],
    partition_by: &[Arc<dyn datafusion::physical_expr::PhysicalExpr>],
    order_by: &[PhysicalSortExpr],
) -> Result<(Vec<Rows>, Vec<Rows>)> {
    let Some(first_batch) = input_batches.first() else {
        return Ok((Vec::new(), Vec::new()));
    };
    let schema = first_batch.schema();

    let partition_converter = RowConverter::new(
        partition_by
            .iter()
            .map(|expr| Ok(SortField::new(expr.data_type(schema.as_ref())?)))
            .collect::<Result<Vec<_>>>()?,
    )?;
    let order_converter = RowConverter::new(
        order_by
            .iter()
            .map(|sort_expr| {
                Ok(SortField::new_with_options(
                    sort_expr.expr.data_type(schema.as_ref())?,
                    sort_expr.options,
                ))
            })
            .collect::<Result<Vec<_>>>()?,
    )?;

    let mut all_partition_rows = Vec::with_capacity(input_batches.len());
    let mut all_order_rows = Vec::with_capacity(input_batches.len());
    for batch in input_batches {
        let partition_columns = evaluate_exprs(partition_by, batch)?;
        let order_columns = evaluate_sort_exprs(order_by, batch)?;
        all_partition_rows.push(partition_converter.convert_columns(&partition_columns)?);
        all_order_rows.push(order_converter.convert_columns(&order_columns)?);
    }

    Ok((all_partition_rows, all_order_rows))
}
