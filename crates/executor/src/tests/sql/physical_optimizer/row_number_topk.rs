use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::models::QueryContext;
use crate::session::UserSession;
use crate::tests::query::create_df_session_with_catalog_url;
use datafusion::arrow::array::{ArrayRef, UInt64Array};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::arrow::util::pretty::pretty_format_batches;
use datafusion::datasource::memory::MemTable;
use datafusion::execution::memory_pool::FairSpillPool;
use datafusion::execution::runtime_env::RuntimeEnvBuilder;
use datafusion::execution::{SessionStateBuilder, TaskContext};
use datafusion::physical_optimizer::optimizer::{PhysicalOptimizer, PhysicalOptimizerRule};
use datafusion::physical_plan::metrics::MetricsSet;
use datafusion::physical_plan::{ExecutionPlan, collect as collect_physical_plan};
use datafusion::prelude::{SessionConfig, SessionContext};

const SETUP_QUERY: &str = "
CREATE OR REPLACE TABLE embucket.public.row_number_topk_input (
    event_id VARCHAR,
    collector_tstamp TIMESTAMP,
    dvce_created_tstamp TIMESTAMP,
    payload VARCHAR,
    seq INT,
    score DOUBLE
);
INSERT INTO embucket.public.row_number_topk_input VALUES
    ('a', '2022-01-01 00:00:02', '2022-01-01 00:00:00', 'late', 2, 2.0),
    ('a', '2022-01-01 00:00:01', '2022-01-01 00:00:00', 'early', 1, 1.0),
    ('b', '2022-01-01 00:00:01', '2022-01-01 00:00:01', 'tie_a', 1, 1.0),
    ('b', '2022-01-01 00:00:01', '2022-01-01 00:00:01', 'tie_b', 2, 1.0);
";

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[tokio::test]
async fn qualify_row_number_eq_one_returns_one_best_row_per_key() {
    let ctx = setup().await;

    let formatted = run_query(
        &ctx,
        "
        SELECT event_id, payload
        FROM embucket.public.row_number_topk_input
        WHERE event_id = 'a'
        QUALIFY ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) = 1
        ",
    )
    .await;
    assert!(
        formatted.contains("| a        | early   |"),
        "expected earliest row for event a, got:\n{formatted}"
    );

    let formatted = run_query(
        &ctx,
        "
        SELECT event_id, COUNT(*) AS selected_rows
        FROM (
            SELECT event_id
            FROM embucket.public.row_number_topk_input
            QUALIFY ROW_NUMBER() OVER (
                PARTITION BY event_id
                ORDER BY collector_tstamp, dvce_created_tstamp
            ) = 1
        )
        GROUP BY event_id
        ORDER BY event_id
        ",
    )
    .await;
    assert!(
        formatted.contains("| a        | 1             |")
            && formatted.contains("| b        | 1             |"),
        "expected one row per event_id, got:\n{formatted}"
    );
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[tokio::test]
async fn qualify_row_number_topk_returns_real_row_numbers() {
    let ctx = setup().await;

    let formatted = run_query(
        &ctx,
        "
        SELECT event_id, payload, ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) AS rn
        FROM embucket.public.row_number_topk_input
        WHERE event_id = 'a'
        QUALIFY rn <= 2
        ORDER BY rn
        ",
    )
    .await;

    assert!(
        formatted.contains("| a        | early   | 1  |")
            && formatted.contains("| a        | late    | 2  |"),
        "expected top-2 rows with row numbers, got:\n{formatted}"
    );

    let formatted = run_query(
        &ctx,
        "
        SELECT event_id, payload
        FROM embucket.public.row_number_topk_input
        WHERE event_id = 'a'
        QUALIFY ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) = 2
        ",
    )
    .await;

    assert!(
        formatted.contains("| a        | late    |")
            && !formatted.contains("| a        | early   |"),
        "expected exactly the second ordered row, got:\n{formatted}"
    );
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[tokio::test]
async fn explain_uses_grouped_topk_without_sort_or_window() {
    let ctx = setup().await;

    for (sql, limit) in [
        (
            "
        SELECT event_id, payload
        FROM embucket.public.row_number_topk_input
        QUALIFY ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) = 1
        ",
            1,
        ),
        (
            "
        SELECT event_id, payload
        FROM embucket.public.row_number_topk_input
        QUALIFY ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) = 2
        ",
            2,
        ),
        (
            "
        SELECT event_id, payload
        FROM embucket.public.row_number_topk_input
        QUALIFY ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) <= 2
        ",
            2,
        ),
        (
            "
        SELECT event_id, payload
        FROM embucket.public.row_number_topk_input
        QUALIFY ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) < 3
        ",
            2,
        ),
        (
            "
        SELECT event_id, payload
        FROM embucket.public.row_number_topk_input
        QUALIFY 2 >= ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        )
        ",
            2,
        ),
        (
            "
        SELECT event_id, payload, ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) AS rn
        FROM embucket.public.row_number_topk_input
        QUALIFY rn <= 2
        ",
            2,
        ),
    ] {
        assert_rewritten(&ctx, sql, limit).await;
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[tokio::test]
async fn explain_does_not_rewrite_unbounded_predicates_or_other_rank_functions() {
    let ctx = setup().await;

    for sql in [
        "
        SELECT event_id, payload
        FROM embucket.public.row_number_topk_input
        QUALIFY ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) > 1
        ",
        "
        SELECT event_id, payload
        FROM embucket.public.row_number_topk_input
        QUALIFY ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) >= 2
        ",
        "
        SELECT event_id, payload
        FROM embucket.public.row_number_topk_input
        QUALIFY ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) <= 0
        ",
        "
        SELECT event_id, payload
        FROM embucket.public.row_number_topk_input
        QUALIFY RANK() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) = 1
        ",
        "
        SELECT event_id, payload
        FROM embucket.public.row_number_topk_input
        QUALIFY DENSE_RANK() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) = 1
        ",
    ] {
        assert_not_rewritten(&ctx, sql).await;
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[tokio::test]
async fn explain_does_not_rewrite_unsupported_types() {
    let ctx = setup().await;

    assert_not_rewritten(
        &ctx,
        "
        WITH with_array AS (
            SELECT [event_id]::ARRAY AS event_key, seq
            FROM embucket.public.row_number_topk_input
        )
        SELECT seq
        FROM with_array
        QUALIFY ROW_NUMBER() OVER (
            PARTITION BY event_key
            ORDER BY seq
        ) = 1
        ",
    )
    .await;

    assert_not_rewritten(
        &ctx,
        "
        SELECT seq
        FROM embucket.public.row_number_topk_input
        QUALIFY ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY score
        ) = 1
        ",
    )
    .await;
}

async fn setup() -> Arc<UserSession> {
    let ctx = create_df_session_with_catalog_url("/dev").await;
    for query in SETUP_QUERY
        .split(';')
        .filter(|query| !query.trim().is_empty())
    {
        let mut q = ctx.query(query, QueryContext::default());
        q.execute().await.expect("setup query");
    }
    ctx
}

async fn assert_not_rewritten(ctx: &Arc<UserSession>, sql: &str) {
    let plan = explain(ctx, sql).await;
    assert!(
        !plan.contains("GroupedTopKExec"),
        "query should not use grouped top-K rewrite:\n{plan}"
    );
    assert!(
        plan.contains("WindowAggExec"),
        "query should retain a window exec:\n{plan}"
    );
}

async fn assert_rewritten(ctx: &Arc<UserSession>, sql: &str, limit: usize) {
    let plan = explain(ctx, sql).await;
    assert!(
        plan.contains("GroupedTopKExec"),
        "expected grouped top-K rewrite, got:\n{plan}"
    );
    assert!(
        plan.contains(&format!("limit={limit}")),
        "expected grouped top-K limit {limit}, got:\n{plan}"
    );
    assert!(
        !plan.contains("BoundedWindowAggExec") && !plan.contains("WindowAggExec"),
        "optimized plan should not contain a window exec:\n{plan}"
    );
    assert!(
        !plan.contains("SortExec"),
        "optimized plan should not contain a sort exec:\n{plan}"
    );
}

async fn explain(ctx: &Arc<UserSession>, sql: &str) -> String {
    run_query(ctx, &format!("EXPLAIN {sql}")).await
}

async fn run_query(ctx: &Arc<UserSession>, sql: &str) -> String {
    let mut query = ctx.query(sql, QueryContext::default());
    let result = query.execute().await.expect("query execution");
    pretty_format_batches(&result.records)
        .expect("format batches")
        .to_string()
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[ignore = "synthetic performance check; run manually with --ignored --nocapture"]
#[tokio::test]
async fn synthetic_row_number_topk_perf() {
    let groups = env_usize("ROW_NUMBER_TOPK_PERF_GROUPS", 25_000);
    let duplicates = env_usize("ROW_NUMBER_TOPK_PERF_DUPLICATES", 20);
    let payload_columns = env_usize("ROW_NUMBER_TOPK_PERF_PAYLOAD_COLUMNS", 16);
    let limit = env_usize("ROW_NUMBER_TOPK_PERF_LIMIT", 3);
    assert!(duplicates >= limit, "duplicates must be >= limit");

    let table = synthetic_topk_table(groups, duplicates, payload_columns);
    let baseline_ctx = synthetic_perf_context(Arc::clone(&table), false);
    let optimized_ctx = synthetic_perf_context(table, true);
    let payload_projection = (0..payload_columns)
        .map(|index| format!("payload_{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "
        SELECT event_id, {payload_projection}
        FROM events
        QUALIFY ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) <= {limit}
        "
    );

    let baseline_plan = explain_datafusion(&baseline_ctx, &sql).await;
    assert!(
        !baseline_plan.contains("GroupedTopKExec"),
        "baseline plan unexpectedly used custom TopK rule:\n{baseline_plan}"
    );
    assert!(
        baseline_plan.contains("WindowAggExec"),
        "baseline plan should use DataFusion's window path:\n{baseline_plan}"
    );

    let optimized_plan = explain_datafusion(&optimized_ctx, &sql).await;
    assert!(
        optimized_plan.contains("GroupedTopKExec"),
        "optimized plan should use grouped TopK:\n{optimized_plan}"
    );
    assert!(
        !optimized_plan.contains("WindowAggExec") && !optimized_plan.contains("SortExec"),
        "optimized plan should avoid window/sort path:\n{optimized_plan}"
    );

    let expected_rows = groups * limit;
    assert_eq!(
        run_datafusion_query(&baseline_ctx, &sql).await,
        expected_rows,
        "baseline row count"
    );
    assert_eq!(
        run_datafusion_query(&optimized_ctx, &sql).await,
        expected_rows,
        "optimized row count"
    );

    let baseline = time_datafusion_query(&baseline_ctx, &sql).await;
    let optimized = time_datafusion_query(&optimized_ctx, &sql).await;
    let speedup = baseline.as_secs_f64() / optimized.as_secs_f64();

    eprintln!(
        "synthetic row_number top-k perf: rows={}, groups={}, duplicates={}, limit={}, payload_columns={}, baseline={baseline:?}, optimized={optimized:?}, speedup={speedup:.2}x",
        groups * duplicates,
        groups,
        duplicates,
        limit,
        payload_columns
    );
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[ignore = "perf benchmark harness; emits OPTIMIZED_RATIO for autoresearch verify. Run with --ignored --nocapture"]
#[tokio::test]
async fn snowplow_dedup_topk_bench() {
    let groups = env_usize("SNOWPLOW_BENCH_GROUPS", 200_000);
    let duplicates = env_usize("SNOWPLOW_BENCH_DUPLICATES", 4);
    let payload_columns = env_usize("SNOWPLOW_BENCH_PAYLOAD_COLUMNS", 12);
    let limit = env_usize("SNOWPLOW_BENCH_LIMIT", 1);
    let repeats = env_usize("SNOWPLOW_BENCH_REPEATS", 7);
    assert!(duplicates >= limit, "duplicates must be >= limit");
    assert!(repeats > 0, "repeats must be greater than zero");

    let table = synthetic_topk_table(groups, duplicates, payload_columns);
    // The reference context uses DataFusion's native window path (no custom rewrite). It
    // never executes in-scope code, so its timing stays constant under optimization and
    // serves to normalize out large cross-run machine noise (CPU frequency scaling,
    // co-tenancy). The optimized context exercises the GroupedTopKExec fast path.
    let baseline_ctx = synthetic_perf_context(Arc::clone(&table), false);
    let optimized_ctx = synthetic_perf_context(table, true);
    let payload_projection = (0..payload_columns)
        .map(|index| format!("payload_{index}"))
        .collect::<Vec<_>>()
        .join(", ");

    // Canonical Snowplow event deduplication: keep the earliest row per event_id.
    let sql = format!(
        "
        SELECT event_id, collector_tstamp, dvce_created_tstamp, {payload_projection}
        FROM events
        QUALIFY ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) <= {limit}
        "
    );

    // Anti-gaming: the optimized plan must use the grouped top-K rewrite and avoid the
    // window/sort path; the reference plan must use the native window path.
    let optimized_plan = explain_datafusion(&optimized_ctx, &sql).await;
    assert!(
        optimized_plan.contains("GroupedTopKExec"),
        "bench requires the grouped top-K rewrite to fire:\n{optimized_plan}"
    );
    assert!(
        !optimized_plan.contains("WindowAggExec") && !optimized_plan.contains("SortExec"),
        "bench plan must avoid the window/sort path:\n{optimized_plan}"
    );
    let baseline_plan = explain_datafusion(&baseline_ctx, &sql).await;
    assert!(
        baseline_plan.contains("WindowAggExec"),
        "reference plan must use the native window path:\n{baseline_plan}"
    );

    let expected_rows = groups * limit;
    assert_eq!(
        run_datafusion_query(&baseline_ctx, &sql).await,
        expected_rows,
        "reference row count must match dedup expectation"
    );
    assert_eq!(
        run_datafusion_query(&optimized_ctx, &sql).await,
        expected_rows,
        "optimized row count must match dedup expectation"
    );

    // Warm up both paths (allocator first-touch, plan/state caches).
    let _ = time_datafusion_query(&baseline_ctx, &sql).await;
    let _ = time_datafusion_query(&optimized_ctx, &sql).await;

    // Paired differential timing: measure reference and optimized back-to-back so both
    // observe the same instantaneous machine state, then report the MEDIAN of the
    // optimized/reference ratios. This cancels the large cross-run noise that makes raw
    // milliseconds unusable for keep/discard decisions. Lower ratio = faster fast-path.
    let mut ratios = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let reference = time_datafusion_query(&baseline_ctx, &sql)
            .await
            .as_secs_f64();
        let optimized = time_datafusion_query(&optimized_ctx, &sql)
            .await
            .as_secs_f64();
        ratios.push(optimized / reference);
    }
    ratios.sort_by(|a, b| a.partial_cmp(b).expect("ratios are finite"));
    let median = ratios[ratios.len() / 2];

    eprintln!("OPTIMIZED_RATIO={median:.4}");
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[ignore = "spilling proof; run manually with --ignored --nocapture"]
#[tokio::test]
async fn grouped_topk_spills_under_memory_pressure() {
    let groups = env_usize("ROW_NUMBER_TOPK_SPILL_GROUPS", 30_000);
    let duplicates = env_usize("ROW_NUMBER_TOPK_SPILL_DUPLICATES", 8);
    let payload_columns = env_usize("ROW_NUMBER_TOPK_SPILL_PAYLOAD_COLUMNS", 8);
    let limit = env_usize("ROW_NUMBER_TOPK_SPILL_LIMIT", 3);
    let memory_mb = env_usize("ROW_NUMBER_TOPK_SPILL_MEMORY_MB", 48);
    assert!(duplicates >= limit, "duplicates must be >= limit");

    let table = synthetic_topk_table(groups, duplicates, payload_columns);
    let baseline_ctx = synthetic_perf_context(Arc::clone(&table), false);
    let spilling_ctx = synthetic_spill_context(table, memory_mb);
    let sql = format!(
        "
        SELECT event_id, collector_tstamp, dvce_created_tstamp, payload_0
        FROM events
        QUALIFY ROW_NUMBER() OVER (
            PARTITION BY event_id
            ORDER BY collector_tstamp, dvce_created_tstamp
        ) <= {limit}
        "
    );
    let ordered_sql = format!(
        "
        SELECT *
        FROM ({sql})
        ORDER BY event_id, collector_tstamp, dvce_created_tstamp, payload_0
        "
    );

    assert_eq!(
        collect_datafusion_formatted(&baseline_ctx, &ordered_sql).await,
        collect_datafusion_formatted(&spilling_ctx, &ordered_sql).await,
        "spilled grouped TopK results must match native window results"
    );

    let optimized_plan = explain_datafusion(&spilling_ctx, &sql).await;
    assert!(
        optimized_plan.contains("GroupedTopKExec"),
        "spill proof requires the grouped TopK rewrite:\n{optimized_plan}"
    );

    let dataframe = spilling_ctx.sql(&sql).await.expect("dataframe");
    let physical_plan = dataframe
        .create_physical_plan()
        .await
        .expect("physical plan");
    let task_ctx = Arc::new(TaskContext::from(&spilling_ctx.state()));
    let result = collect_physical_plan(Arc::clone(&physical_plan), task_ctx)
        .await
        .expect("collect physical plan");
    assert_eq!(
        result.iter().map(RecordBatch::num_rows).sum::<usize>(),
        groups * limit,
        "spilled grouped TopK row count"
    );

    let metrics = grouped_topk_metrics(&physical_plan).expect("GroupedTopKExec metrics");
    assert!(
        metrics.spill_count().unwrap_or_default() > 0,
        "expected GroupedTopKExec to create spill files, got {metrics:?}"
    );
    assert!(
        metrics.spilled_bytes().unwrap_or_default() > 0,
        "expected GroupedTopKExec to spill bytes, got {metrics:?}"
    );
    assert!(
        metrics.spilled_rows().unwrap_or_default() > 0,
        "expected GroupedTopKExec to spill rows, got {metrics:?}"
    );
}

fn synthetic_perf_context(table: Arc<MemTable>, use_topk: bool) -> SessionContext {
    let mut rules: Vec<Arc<dyn PhysicalOptimizerRule + Send + Sync>> =
        PhysicalOptimizer::default().rules;
    if use_topk {
        rules.insert(
            0,
            Arc::new(crate::datafusion::physical_optimizer::row_number_topk::RowNumberTopK::new()),
        );
    }

    let state = SessionStateBuilder::new()
        .with_config(
            SessionConfig::new()
                .set_usize("datafusion.execution.target_partitions", 8)
                .set_str("datafusion.sql_parser.dialect", "Generic"),
        )
        .with_default_features()
        .with_physical_optimizer_rules(rules)
        .build();
    let ctx = SessionContext::new_with_state(state);
    ctx.register_table("events", table)
        .expect("register synthetic table");
    ctx
}

fn synthetic_spill_context(table: Arc<MemTable>, memory_mb: usize) -> SessionContext {
    let mut rules: Vec<Arc<dyn PhysicalOptimizerRule + Send + Sync>> =
        PhysicalOptimizer::default().rules;
    rules.insert(
        0,
        Arc::new(crate::datafusion::physical_optimizer::row_number_topk::RowNumberTopK::new()),
    );

    let runtime = RuntimeEnvBuilder::new()
        .with_memory_pool(Arc::new(FairSpillPool::new(
            memory_mb.saturating_mul(1024 * 1024),
        )))
        .build_arc()
        .expect("runtime env");
    let state = SessionStateBuilder::new()
        .with_config(
            SessionConfig::new()
                .with_sort_spill_reservation_bytes(0)
                .set_usize("datafusion.execution.target_partitions", 4)
                .set_usize("datafusion.execution.batch_size", 8192)
                .set_str("datafusion.sql_parser.dialect", "Generic"),
        )
        .with_default_features()
        .with_runtime_env(runtime)
        .with_physical_optimizer_rules(rules)
        .build();
    let ctx = SessionContext::new_with_state(state);
    ctx.register_table("events", table)
        .expect("register synthetic table");
    ctx
}

fn synthetic_topk_table(groups: usize, duplicates: usize, payload_columns: usize) -> Arc<MemTable> {
    let rows = groups * duplicates;
    let schema = synthetic_topk_schema(payload_columns);
    let mut event_id = Vec::with_capacity(rows);
    let mut collector_tstamp = Vec::with_capacity(rows);
    let mut dvce_created_tstamp = Vec::with_capacity(rows);
    let mut payloads = (0..payload_columns)
        .map(|_| Vec::with_capacity(rows))
        .collect::<Vec<_>>();

    for group in 0..groups {
        for duplicate in (0..duplicates).rev() {
            event_id.push(usize_to_u64(group));
            collector_tstamp.push(usize_to_u64(duplicate));
            dvce_created_tstamp.push(usize_to_u64(duplicates - duplicate));
            for (payload_index, payload) in payloads.iter_mut().enumerate() {
                payload.push(usize_to_u64(
                    (group * duplicates + duplicate) * (payload_index + 1),
                ));
            }
        }
    }

    let mut columns: Vec<ArrayRef> = vec![
        Arc::new(UInt64Array::from(event_id)),
        Arc::new(UInt64Array::from(collector_tstamp)),
        Arc::new(UInt64Array::from(dvce_created_tstamp)),
    ];
    columns.extend(
        payloads
            .into_iter()
            .map(|payload| -> ArrayRef { Arc::new(UInt64Array::from(payload)) }),
    );

    let batch = RecordBatch::try_new(Arc::clone(&schema), columns).expect("record batch");
    Arc::new(MemTable::try_new(schema, vec![vec![batch]]).expect("mem table"))
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).expect("synthetic TopK test value fits in u64")
}

fn synthetic_topk_schema(payload_columns: usize) -> SchemaRef {
    let mut fields = vec![
        Field::new("event_id", DataType::UInt64, false),
        Field::new("collector_tstamp", DataType::UInt64, false),
        Field::new("dvce_created_tstamp", DataType::UInt64, false),
    ];
    fields.extend(
        (0..payload_columns)
            .map(|index| Field::new(format!("payload_{index}"), DataType::UInt64, false)),
    );
    Arc::new(Schema::new(fields))
}

async fn explain_datafusion(ctx: &SessionContext, sql: &str) -> String {
    let batches = ctx
        .sql(&format!("EXPLAIN {sql}"))
        .await
        .expect("explain dataframe")
        .collect()
        .await
        .expect("explain collect");
    pretty_format_batches(&batches)
        .expect("format explain batches")
        .to_string()
}

async fn collect_datafusion_formatted(ctx: &SessionContext, sql: &str) -> String {
    let batches = ctx
        .sql(sql)
        .await
        .expect("dataframe")
        .collect()
        .await
        .expect("collect");
    pretty_format_batches(&batches)
        .expect("format batches")
        .to_string()
}

async fn run_datafusion_query(ctx: &SessionContext, sql: &str) -> usize {
    ctx.sql(sql)
        .await
        .expect("dataframe")
        .collect()
        .await
        .expect("collect")
        .iter()
        .map(RecordBatch::num_rows)
        .sum()
}

fn grouped_topk_metrics(plan: &Arc<dyn ExecutionPlan>) -> Option<MetricsSet> {
    if plan.name() == "GroupedTopKExec" {
        return plan.metrics();
    }

    plan.children().into_iter().find_map(grouped_topk_metrics)
}

async fn time_datafusion_query(ctx: &SessionContext, sql: &str) -> Duration {
    let start = Instant::now();
    let rows = run_datafusion_query(ctx, sql).await;
    assert!(rows > 0, "timed query returned no rows");
    start.elapsed()
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}
