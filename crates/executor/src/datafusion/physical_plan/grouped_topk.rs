use std::cmp::Ordering;
use std::mem::size_of;
use std::sync::Arc;

use async_stream::try_stream;
use datafusion::arrow::array::{ArrayRef, UInt32Array, UInt64Array};
use datafusion::arrow::compute::{SortOptions, concat_batches, take_record_batch};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::arrow::row::{OwnedRow, RowConverter, Rows, SortField};
use datafusion::execution::TaskContext;
use datafusion::execution::memory_pool::{MemoryConsumer, MemoryReservation};
use datafusion::physical_expr::expressions::Column;
use datafusion::physical_expr::{
    Distribution, EquivalenceProperties, LexOrdering, PhysicalExpr, PhysicalSortExpr,
};
use datafusion_common::{DataFusionError, Result};
use datafusion_physical_plan::execution_plan::EmissionType;
use datafusion_physical_plan::metrics::{
    BaselineMetrics, ExecutionPlanMetricsSet, MetricsSet, SpillMetrics,
};
use datafusion_physical_plan::sorts::sort::sort_batch_chunked;
use datafusion_physical_plan::sorts::streaming_merge::{SortedSpillFile, StreamingMergeBuilder};
use datafusion_physical_plan::spill::{SpillManager, get_record_batch_memory_size};
use datafusion_physical_plan::stream::RecordBatchStreamAdapter;
use datafusion_physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, ExecutionPlanProperties, PlanProperties,
    SendableRecordBatchStream,
};
use futures::{StreamExt, stream};

const HIDDEN_SEQUENCE_COLUMN: &str = "__grouped_topk_seq";
const ESTIMATED_TOPK_STATE_BYTES_PER_ROW: usize =
    size_of::<OwnedRow>() + size_of::<(usize, usize)>() + 64;

#[derive(Debug)]
pub struct GroupedTopKExec {
    input: Arc<dyn ExecutionPlan>,
    partition_by: Vec<Arc<dyn PhysicalExpr>>,
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

struct GroupedTopKState {
    partition_by: Vec<Arc<dyn PhysicalExpr>>,
    order_by: Vec<PhysicalSortExpr>,
    limit: usize,
    input_schema: SchemaRef,
    output_schema: SchemaRef,
    candidate_schema: SchemaRef,
    candidate_sort_expr: LexOrdering,
    partition_converter: RowConverter,
    order_converter: RowConverter,
    input_batches: Vec<RecordBatch>,
    batch_base_sequences: Vec<u64>,
    partition_rows: Vec<Rows>,
    order_rows: Vec<Rows>,
    segment_rows: usize,
    next_sequence: u64,
    reservation: MemoryReservation,
    merge_reservation: MemoryReservation,
    spill_manager: SpillManager,
    spill_files: Vec<SortedSpillFile>,
    metrics: BaselineMetrics,
    batch_size: usize,
}

impl GroupedTopKExec {
    pub fn try_new(
        input: Arc<dyn ExecutionPlan>,
        partition_by: Vec<Arc<dyn PhysicalExpr>>,
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
        let input_stream = input.execute(partition, Arc::clone(&context))?;
        let metrics_set = self.metrics.clone();

        let stream = try_stream! {
            let mut output_stream = {
                let _timer = metrics.elapsed_compute().timer();
                let mut state = GroupedTopKState::try_new(
                    partition,
                    partition_by,
                    order_by,
                    limit,
                    input.schema(),
                    Arc::clone(&schema),
                    context.as_ref(),
                    &metrics_set,
                    metrics.clone(),
                )?;
                state.consume(input_stream).await?;
                state.finish()?
            };

            while let Some(batch) = output_stream.next().await {
                let batch = batch?;
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

impl GroupedTopKState {
    #[expect(clippy::too_many_arguments)]
    fn try_new(
        partition: usize,
        partition_by: Vec<Arc<dyn PhysicalExpr>>,
        order_by: Vec<PhysicalSortExpr>,
        limit: usize,
        input_schema: SchemaRef,
        output_schema: SchemaRef,
        context: &TaskContext,
        metrics_set: &ExecutionPlanMetricsSet,
        metrics: BaselineMetrics,
    ) -> Result<Self> {
        let candidate_schema = candidate_schema(&input_schema);
        let candidate_sort_expr =
            candidate_sort_expr(&partition_by, &order_by, input_schema.fields().len())?;
        let partition_converter = build_partition_converter(&partition_by, &input_schema)?;
        let order_converter = build_order_converter(&order_by, &input_schema)?;
        let runtime = context.runtime_env();
        let reservation = MemoryConsumer::new(format!("GroupedTopKExec[{partition}]"))
            .with_can_spill(true)
            .register(&runtime.memory_pool);
        let merge_reservation = MemoryConsumer::new(format!("GroupedTopKMerge[{partition}]"))
            .register(&runtime.memory_pool);
        let spill_manager = SpillManager::new(
            Arc::clone(&runtime),
            SpillMetrics::new(metrics_set, partition),
            Arc::clone(&candidate_schema),
        )
        .with_compression_type(context.session_config().spill_compression());

        Ok(Self {
            partition_by,
            order_by,
            limit,
            input_schema,
            output_schema,
            candidate_schema,
            candidate_sort_expr,
            partition_converter,
            order_converter,
            input_batches: Vec::new(),
            batch_base_sequences: Vec::new(),
            partition_rows: Vec::new(),
            order_rows: Vec::new(),
            segment_rows: 0,
            next_sequence: 0,
            reservation,
            merge_reservation,
            spill_manager,
            spill_files: Vec::new(),
            metrics,
            batch_size: context.session_config().batch_size(),
        })
    }

    async fn consume(&mut self, mut input: SendableRecordBatchStream) -> Result<()> {
        while let Some(batch) = input.next().await {
            self.insert_batch(batch?)?;
        }
        Ok(())
    }

    fn insert_batch(&mut self, batch: RecordBatch) -> Result<()> {
        if batch.num_rows() == 0 {
            return Ok(());
        }

        let base_sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(u64::try_from(batch.num_rows()).map_err(|_| {
                DataFusionError::Execution(format!(
                    "GroupedTopKExec input batch row count {} exceeds UInt64 range",
                    batch.num_rows()
                ))
            })?)
            .ok_or_else(|| {
                DataFusionError::Execution(
                    "GroupedTopKExec input row sequence exceeds UInt64 range".to_string(),
                )
            })?;

        let (partition_rows, order_rows) = encode_batch_rows(
            &batch,
            &self.partition_by,
            &self.order_by,
            &self.partition_converter,
            &self.order_converter,
        )?;

        self.segment_rows += batch.num_rows();
        self.batch_base_sequences.push(base_sequence);
        self.input_batches.push(batch);
        self.partition_rows.push(partition_rows);
        self.order_rows.push(order_rows);

        if self
            .reservation
            .try_resize(self.estimated_segment_memory())
            .is_err()
        {
            self.spill_current_segment()?;
        }

        Ok(())
    }

    fn finish(&mut self) -> Result<SendableRecordBatchStream> {
        if self.spill_files.is_empty() {
            if self.segment_rows == 0 {
                return Ok(batches_stream(Arc::clone(&self.output_schema), Vec::new()));
            }

            if self
                .reservation
                .try_grow(self.estimated_selection_memory())
                .is_ok()
            {
                let selected_by_batch =
                    select_topk_rows(&self.partition_rows, &self.order_rows, self.limit)?;
                let output_batches = build_output_batches(
                    &self.input_batches,
                    selected_by_batch,
                    &self.output_schema,
                )?;
                return Ok(batches_stream(
                    Arc::clone(&self.output_schema),
                    output_batches,
                ));
            }
        }

        self.spill_current_segment()?;
        self.finish_spilled()
    }

    fn finish_spilled(&mut self) -> Result<SendableRecordBatchStream> {
        if self.spill_files.is_empty() {
            return Ok(batches_stream(Arc::clone(&self.output_schema), Vec::new()));
        }

        let sort_expr = self.candidate_sort_expr.clone();
        let mut merged = StreamingMergeBuilder::new()
            .with_sorted_spill_files(std::mem::take(&mut self.spill_files))
            .with_spill_manager(self.spill_manager.clone())
            .with_schema(Arc::clone(&self.candidate_schema))
            .with_expressions(&sort_expr)
            .with_metrics(self.metrics.clone())
            .with_batch_size(self.batch_size)
            .with_fetch(None)
            .with_reservation(self.merge_reservation.new_empty())
            .build()?;

        let output_schema = Arc::clone(&self.output_schema);
        let stream_schema = Arc::clone(&output_schema);
        let input_column_count = self.input_schema.fields().len();
        let limit = self.limit;
        let partition_by = self.partition_by.clone();
        let partition_converter = build_partition_converter(&partition_by, &self.candidate_schema)?;

        let stream = try_stream! {
            let mut current_group: Option<OwnedRow> = None;
            let mut current_row_number = 0usize;

            while let Some(batch) = merged.next().await {
                let batch = batch?;
                let partition_columns = evaluate_exprs(&partition_by, &batch)?;
                let partition_rows = partition_converter.convert_columns(&partition_columns)?;
                let mut selected_rows = Vec::new();

                for row_index in 0..batch.num_rows() {
                    let row = partition_rows.row(row_index);
                    let is_new_group = current_group
                        .as_ref()
                        .is_none_or(|current| current.row() != row);

                    if is_new_group {
                        current_group = Some(row.owned());
                        current_row_number = 0;
                    }

                    if current_row_number < limit {
                        current_row_number += 1;
                        selected_rows.push(SelectedRow {
                            row_index,
                            row_number: row_number_from_offset(current_row_number)?,
                        });
                    }
                }

                if !selected_rows.is_empty() {
                    yield build_output_batch_from_candidate(
                        &batch,
                        selected_rows,
                        input_column_count,
                        &output_schema,
                    )?;
                }
            }
        };

        Ok(Box::pin(RecordBatchStreamAdapter::new(
            stream_schema,
            stream,
        )))
    }

    fn spill_current_segment(&mut self) -> Result<()> {
        if self.segment_rows == 0 {
            return Ok(());
        }

        let candidate_batch = self.materialize_candidate_batch()?;
        self.clear_segment();

        let Some(candidate_batch) = candidate_batch else {
            return Ok(());
        };

        self.sort_and_spill_candidate_batch(&candidate_batch)
    }

    fn materialize_candidate_batch(&self) -> Result<Option<RecordBatch>> {
        let selected_by_batch =
            select_topk_rows(&self.partition_rows, &self.order_rows, self.limit)?;
        let candidate_batches = build_candidate_batches(
            &self.input_batches,
            selected_by_batch,
            &self.batch_base_sequences,
            &self.candidate_schema,
        )?;

        if candidate_batches.is_empty() {
            return Ok(None);
        }

        if candidate_batches.len() == 1 {
            return Ok(candidate_batches.into_iter().next());
        }

        Ok(Some(concat_batches(
            &self.candidate_schema,
            &candidate_batches,
        )?))
    }

    fn sort_and_spill_candidate_batch(&mut self, candidate_batch: &RecordBatch) -> Result<()> {
        if candidate_batch.num_rows() == 0 {
            return Ok(());
        }

        let candidate_memory = get_record_batch_memory_size(candidate_batch);
        let sort_memory = candidate_memory.saturating_mul(2).max(1);
        self.reservation.try_grow(sort_memory).map_err(|err| {
            DataFusionError::ResourcesExhausted(format!(
                "Failed to reserve memory for GroupedTopKExec spill sort: {err}"
            ))
        })?;

        let result: Result<Option<SortedSpillFile>> = (|| {
            let sorted_batches =
                sort_batch_chunked(candidate_batch, &self.candidate_sort_expr, self.batch_size)?;
            let max_record_batch_memory = sorted_batches
                .iter()
                .map(get_record_batch_memory_size)
                .max()
                .unwrap_or(0);
            let spill_file = self
                .spill_manager
                .spill_record_batch_and_finish(&sorted_batches, "GroupedTopKExec spill")?;
            Ok(spill_file.map(|file| SortedSpillFile {
                file,
                max_record_batch_memory,
            }))
        })();
        self.reservation.free();

        if let Some(spill_file) = result? {
            self.spill_files.push(spill_file);
        }

        Ok(())
    }

    fn clear_segment(&mut self) {
        self.input_batches.clear();
        self.batch_base_sequences.clear();
        self.partition_rows.clear();
        self.order_rows.clear();
        self.segment_rows = 0;
        self.reservation.free();
    }

    fn estimated_segment_memory(&self) -> usize {
        let batch_memory = self
            .input_batches
            .iter()
            .map(get_record_batch_memory_size)
            .sum::<usize>();
        let partition_rows_memory = self.partition_rows.iter().map(Rows::size).sum::<usize>();
        let order_rows_memory = self.order_rows.iter().map(Rows::size).sum::<usize>();
        let state_memory = self
            .segment_rows
            .saturating_mul(ESTIMATED_TOPK_STATE_BYTES_PER_ROW);

        batch_memory
            .saturating_add(partition_rows_memory)
            .saturating_add(order_rows_memory)
            .saturating_add(state_memory)
    }

    const fn estimated_selection_memory(&self) -> usize {
        self.segment_rows
            .saturating_mul(ESTIMATED_TOPK_STATE_BYTES_PER_ROW)
            .saturating_add(self.limit.saturating_mul(size_of::<(usize, usize)>()))
    }
}

fn select_topk_rows(
    all_partition_rows: &[Rows],
    all_order_rows: &[Rows],
    limit: usize,
) -> Result<Vec<Vec<SelectedRow>>> {
    if limit == 1 {
        return Ok(select_top1_rows(all_partition_rows, all_order_rows));
    }

    // Each group keeps up to `limit` row coordinates; comparisons read sort-encoded bytes
    // from the retained rows, with the (batch, row) tiebreak standing in for input order.
    let mut winners: datafusion_common::HashMap<OwnedRow, Vec<(usize, usize)>> =
        datafusion_common::HashMap::default();

    for (batch_index, partition_rows) in all_partition_rows.iter().enumerate() {
        for row_index in 0..partition_rows.num_rows() {
            let group = winners
                .entry(partition_rows.row(row_index).owned())
                .or_insert_with(|| Vec::with_capacity(limit.min(8)));
            insert_topk_coord(group, (batch_index, row_index), all_order_rows, limit);
        }
    }

    let mut selected_by_batch = all_partition_rows
        .iter()
        .map(|rows| Vec::with_capacity(rows.num_rows().min(limit)))
        .collect::<Vec<_>>();
    for mut coords in winners.into_values() {
        coords.sort_unstable_by(|&a, &b| cmp_coord_order(a, b, all_order_rows));
        for (offset, (batch_index, row_index)) in coords.into_iter().enumerate() {
            selected_by_batch[batch_index].push(SelectedRow {
                row_index,
                row_number: row_number_from_offset(offset + 1)?,
            });
        }
    }

    Ok(selected_by_batch)
}

fn select_top1_rows(all_partition_rows: &[Rows], all_order_rows: &[Rows]) -> Vec<Vec<SelectedRow>> {
    use datafusion_common::hash_map::Entry;

    struct Top1Winner {
        batch_index: usize,
        row_index: usize,
    }

    let mut winners: datafusion_common::HashMap<OwnedRow, Top1Winner> =
        datafusion_common::HashMap::default();

    for (batch_index, partition_rows) in all_partition_rows.iter().enumerate() {
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

    let mut selected_by_batch = all_partition_rows
        .iter()
        .map(|rows| Vec::with_capacity(rows.num_rows().min(1)))
        .collect::<Vec<_>>();
    for winner in winners.into_values() {
        selected_by_batch[winner.batch_index].push(SelectedRow {
            row_index: winner.row_index,
            row_number: 1,
        });
    }

    selected_by_batch
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
        let indices = row_indices(&selected_rows)?;
        let selected = take_record_batch(batch, &indices)?;

        let mut columns: Vec<ArrayRef> = selected.columns().to_vec();
        columns.push(Arc::new(UInt64Array::from(row_numbers)));
        output_batches.push(RecordBatch::try_new(Arc::clone(output_schema), columns)?);
    }

    Ok(output_batches)
}

fn build_candidate_batches(
    input_batches: &[RecordBatch],
    selected_by_batch: Vec<Vec<SelectedRow>>,
    batch_base_sequences: &[u64],
    candidate_schema: &SchemaRef,
) -> Result<Vec<RecordBatch>> {
    let mut output_batches = Vec::new();
    for ((batch, mut selected_rows), base_sequence) in input_batches
        .iter()
        .zip(selected_by_batch)
        .zip(batch_base_sequences)
    {
        if selected_rows.is_empty() {
            continue;
        }
        selected_rows.sort_unstable_by_key(|selected| selected.row_index);
        let sequences = selected_rows
            .iter()
            .map(|selected| {
                base_sequence
                    .checked_add(u64::try_from(selected.row_index).map_err(|_| {
                        DataFusionError::Execution(format!(
                            "GroupedTopKExec row index {} exceeds UInt64 sequence range",
                            selected.row_index
                        ))
                    })?)
                    .ok_or_else(|| {
                        DataFusionError::Execution(
                            "GroupedTopKExec row sequence exceeds UInt64 range".to_string(),
                        )
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        let indices = row_indices(&selected_rows)?;
        let selected = take_record_batch(batch, &indices)?;

        let mut columns: Vec<ArrayRef> = selected.columns().to_vec();
        columns.push(Arc::new(UInt64Array::from(sequences)));
        output_batches.push(RecordBatch::try_new(Arc::clone(candidate_schema), columns)?);
    }

    Ok(output_batches)
}

fn build_output_batch_from_candidate(
    candidate_batch: &RecordBatch,
    mut selected_rows: Vec<SelectedRow>,
    input_column_count: usize,
    output_schema: &SchemaRef,
) -> Result<RecordBatch> {
    selected_rows.sort_unstable_by_key(|selected| selected.row_index);
    let row_numbers = selected_rows
        .iter()
        .map(|selected| selected.row_number)
        .collect::<Vec<_>>();
    let indices = row_indices(&selected_rows)?;
    let selected = take_record_batch(candidate_batch, &indices)?;

    let mut columns: Vec<ArrayRef> = selected
        .columns()
        .iter()
        .take(input_column_count)
        .cloned()
        .collect();
    columns.push(Arc::new(UInt64Array::from(row_numbers)));
    RecordBatch::try_new(Arc::clone(output_schema), columns).map_err(Into::into)
}

fn row_indices(selected_rows: &[SelectedRow]) -> Result<UInt32Array> {
    let indices = selected_rows
        .iter()
        .map(|selected| {
            u32::try_from(selected.row_index).map_err(|_| {
                DataFusionError::Execution(format!(
                    "GroupedTopKExec row index {} exceeds UInt32 take index range",
                    selected.row_index
                ))
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(UInt32Array::from(indices))
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

fn evaluate_exprs(exprs: &[Arc<dyn PhysicalExpr>], batch: &RecordBatch) -> Result<Vec<ArrayRef>> {
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

fn encode_batch_rows(
    batch: &RecordBatch,
    partition_by: &[Arc<dyn PhysicalExpr>],
    order_by: &[PhysicalSortExpr],
    partition_converter: &RowConverter,
    order_converter: &RowConverter,
) -> Result<(Rows, Rows)> {
    let partition_columns = evaluate_exprs(partition_by, batch)?;
    let order_columns = evaluate_sort_exprs(order_by, batch)?;
    Ok((
        partition_converter.convert_columns(&partition_columns)?,
        order_converter.convert_columns(&order_columns)?,
    ))
}

fn build_partition_converter(
    partition_by: &[Arc<dyn PhysicalExpr>],
    schema: &SchemaRef,
) -> Result<RowConverter> {
    RowConverter::new(
        partition_by
            .iter()
            .map(|expr| Ok(SortField::new(expr.data_type(schema.as_ref())?)))
            .collect::<Result<Vec<_>>>()?,
    )
    .map_err(Into::into)
}

fn build_order_converter(
    order_by: &[PhysicalSortExpr],
    schema: &SchemaRef,
) -> Result<RowConverter> {
    RowConverter::new(
        order_by
            .iter()
            .map(|sort_expr| {
                Ok(SortField::new_with_options(
                    sort_expr.expr.data_type(schema.as_ref())?,
                    sort_expr.options,
                ))
            })
            .collect::<Result<Vec<_>>>()?,
    )
    .map_err(Into::into)
}

fn candidate_schema(input_schema: &SchemaRef) -> SchemaRef {
    let mut fields = input_schema.fields().to_vec();
    fields.push(Arc::new(Field::new(
        HIDDEN_SEQUENCE_COLUMN,
        DataType::UInt64,
        false,
    )));
    Arc::new(Schema::new(fields))
}

fn candidate_sort_expr(
    partition_by: &[Arc<dyn PhysicalExpr>],
    order_by: &[PhysicalSortExpr],
    input_column_count: usize,
) -> Result<LexOrdering> {
    let mut sort_exprs = Vec::with_capacity(partition_by.len() + order_by.len() + 1);
    sort_exprs.extend(partition_by.iter().map(|expr| {
        PhysicalSortExpr::new(
            Arc::clone(expr),
            SortOptions {
                descending: false,
                nulls_first: true,
            },
        )
    }));
    sort_exprs.extend(order_by.iter().cloned());
    sort_exprs.push(PhysicalSortExpr::new(
        Arc::new(Column::new(HIDDEN_SEQUENCE_COLUMN, input_column_count)),
        SortOptions::default(),
    ));

    LexOrdering::new(sort_exprs).ok_or_else(|| {
        DataFusionError::Internal(
            "GroupedTopKExec candidate spill sort expression cannot be empty".to_string(),
        )
    })
}

fn row_number_from_offset(offset: usize) -> Result<u64> {
    u64::try_from(offset).map_err(|_| {
        DataFusionError::Execution(format!(
            "GroupedTopKExec row number {offset} exceeds UInt64 range"
        ))
    })
}

fn batches_stream(schema: SchemaRef, batches: Vec<RecordBatch>) -> SendableRecordBatchStream {
    Box::pin(RecordBatchStreamAdapter::new(
        schema,
        stream::iter(batches.into_iter().map(Ok)),
    ))
}
