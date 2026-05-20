use super::TEST_JWT_SECRET;
use crate::server::core_state::CoreState;
use crate::server::make_snowflake_router;
use crate::server::server_models::RestApiConfig;
use crate::server::state::AppState;
use executor::models::QueryContext;
use executor::service::ExecutionService;
use executor::utils::Config as UtilsConfig;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Builder;
use tokio::sync::Notify;
use tokio::sync::oneshot;
#[cfg(feature = "traces-test-log")]
use tracing_subscriber::{fmt, fmt::format::FmtSpan};

static INIT: std::sync::Once = std::sync::Once::new();

pub struct TestRestApiServer {
    addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl TestRestApiServer {
    #[must_use]
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }
}

impl Deref for TestRestApiServer {
    type Target = SocketAddr;

    fn deref(&self) -> &Self::Target {
        &self.addr
    }
}

impl Drop for TestRestApiServer {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[allow(clippy::expect_used)]
#[must_use]
pub fn rest_default_cfg(data_format: &str) -> RestApiConfig {
    RestApiConfig::new(data_format, TEST_JWT_SECRET.to_string())
        .expect("Failed to create server config")
        .with_demo_credentials("embucket".to_string(), "embucket".to_string())
}

#[allow(clippy::expect_used)]
#[must_use]
pub fn executor_default_cfg() -> UtilsConfig {
    UtilsConfig::default().with_max_concurrency_level(2)
}

#[allow(clippy::expect_used)]
pub async fn run_test_rest_api_server(
    rest_cfg: Option<RestApiConfig>,
    executor_cfg: Option<UtilsConfig>,
) -> TestRestApiServer {
    let rest_cfg = rest_cfg.unwrap_or_else(|| rest_default_cfg("json"));
    let executor_cfg = executor_cfg.unwrap_or_else(executor_default_cfg);

    let notify = Arc::new(Notify::new());
    let notify_clone = Arc::clone(&notify);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let listener = TcpListener::bind("0.0.0.0:0").expect("Failed to bind to address");
    let addr = listener.local_addr().expect("Failed to get local address");
    listener
        .set_nonblocking(true)
        .expect("Failed to set listener to non-blocking mode");

    // Start a new thread with its own runtime for the server.
    // A dedicated runtime is required because catalog code uses
    // block_in_place + handle.block_on, which deadlocks if run
    // on another runtime's worker thread via tokio::spawn.
    let thread = std::thread::spawn(move || {
        let rt = Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime");

        rt.block_on(async {
            run_test_rest_api_server_with_config(
                rest_cfg,
                executor_cfg,
                listener,
                notify_clone,
                shutdown_rx,
            )
            .await;
        });
    });

    let timeout_duration = std::time::Duration::from_secs(1);

    // Await notification without blocking tokio worker threads
    match tokio::time::timeout(timeout_duration, notify.notified()).await {
        Ok(()) => {
            tracing::info!("Test server is up and running.");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Err(_) => {
            tracing::error!("Timeout occurred while waiting for server start.");
        }
    }

    TestRestApiServer {
        addr,
        shutdown_tx: Some(shutdown_tx),
        thread: Some(thread),
    }
}

fn setup_tracing() {
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::Resource;
    use opentelemetry_sdk::trace::BatchSpanProcessor;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tracing_subscriber::filter::{LevelFilter, Targets};
    use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};

    const DISABLED_TARGETS: [&str; 1] = ["h2"];

    INIT.call_once(|| {
        // Initialize OTLP exporter using gRPC (Tonic)
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .build()
            .expect("Failed to create OTLP exporter");

        let resource = Resource::builder().with_service_name("Em").build();

        let tracing_provider = SdkTracerProvider::builder()
            .with_span_processor(BatchSpanProcessor::builder(exporter).build())
            .with_resource(resource)
            .build();

        let targets_with_level =
            |targets: &[&'static str], level: LevelFilter| -> Vec<(&str, LevelFilter)> {
                targets.iter().map(|t| ((*t), level)).collect()
            };

        let registry = tracing_subscriber::registry().with(
            tracing_opentelemetry::OpenTelemetryLayer::new(tracing_provider.tracer("embucket"))
                .with_level(true)
                .with_filter(
                    Targets::default()
                        .with_targets(targets_with_level(&DISABLED_TARGETS, LevelFilter::OFF))
                        .with_default(LevelFilter::TRACE),
                ),
        );

        #[cfg(feature = "traces-test-log")]
        let registry = registry.with(
            fmt::layer()
                .with_writer(
                    std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("traces.log")
                        .expect("Failed to open traces.log"),
                )
                .with_ansi(false)
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_file(true)
                .with_line_number(true)
                .with_span_events(FmtSpan::NONE)
                .json()
                .with_level(true)
                .with_filter(
                    Targets::default()
                        .with_targets(targets_with_level(&DISABLED_TARGETS, LevelFilter::OFF))
                        .with_default(LevelFilter::TRACE),
                ),
        );

        registry.init();
        opentelemetry::global::set_tracer_provider(tracing_provider);
    });
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
pub async fn run_test_rest_api_server_with_config(
    snowflake_rest_cfg: RestApiConfig,
    execution_cfg: UtilsConfig,
    listener: std::net::TcpListener,
    notify: Arc<Notify>,
    shutdown_rx: oneshot::Receiver<()>,
) {
    let addr = listener.local_addr().unwrap();

    setup_tracing();
    tracing::info!("Starting server at {addr}");

    let core_state = CoreState::new_dev(execution_cfg, snowflake_rest_cfg, "/dev".to_string())
        .await
        .expect("Core state creation error");
    core_state
        .executor
        .create_session("test-bootstrap")
        .await
        .expect("Failed to create REST test bootstrap session");
    core_state
        .executor
        .query(
            "test-bootstrap",
            "CREATE SCHEMA IF NOT EXISTS embucket.public",
            QueryContext::default(),
        )
        .await
        .expect("Failed to bootstrap REST test schema");

    let app = make_snowflake_router(AppState::from(&core_state))
        .into_make_service_with_connect_info::<SocketAddr>();

    // Notify the waiting task that the server is ready
    notify.notify_one();

    tracing::info!("Server ready at {addr}");

    let listener = tokio::net::TcpListener::from_std(listener)
        .expect("Failed to create Tokio listener from std listener");

    // Serve the application until the test guard is dropped.
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        })
        .await
        .unwrap();
}
