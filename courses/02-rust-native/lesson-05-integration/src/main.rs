use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post},
    Router,
};
use metrics::{counter, describe_counter, describe_histogram, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use opentelemetry::trace::TracerProvider as _;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Task {
    id: u64,
    title: String,
    done: bool,
}

#[derive(Debug, Deserialize)]
struct CreateTask {
    title: String,
}

struct AppState {
    counter: AtomicU64,
    prometheus_handle: PrometheusHandle,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let span_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()?;

    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter)
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("learn-tracing-native")
                .build(),
        )
        .build();

    let tracer = tracer_provider.tracer("learn-tracing");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(otel_layer)
        .init();

    let prometheus_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    describe_counter!(
        "http_requests_total",
        metrics::Unit::Count,
        "Total number of HTTP requests"
    );
    describe_counter!(
        "tasks_created_total",
        metrics::Unit::Count,
        "Total number of tasks created"
    );
    describe_histogram!(
        "http_request_duration_seconds",
        metrics::Unit::Seconds,
        "HTTP request duration in seconds"
    );

    let state = Arc::new(AppState {
        counter: AtomicU64::new(1),
        prometheus_handle,
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    tracing::info!("Server running on http://127.0.0.1:3001");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await.unwrap();
    axum::serve(listener, app).await.unwrap();

    tracer_provider.shutdown()?;
    Ok(())
}

async fn metrics_handler(State(state): State<Arc<AppState>>) -> String {
    state.prometheus_handle.render()
}

#[tracing::instrument]
async fn health() -> Json<serde_json::Value> {
    counter!("http_requests_total", "endpoint" => "health").increment(1);
    let start = std::time::Instant::now();

    let response = Json(serde_json::json!({ "status": "ok" }));

    let duration = start.elapsed().as_secs_f64();
    histogram!("http_request_duration_seconds", "endpoint" => "health").record(duration);
    tracing::info!("health check requested");
    response
}

#[tracing::instrument(skip(state))]
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    counter!("http_requests_total", "endpoint" => "create_task").increment(1);
    let start = std::time::Instant::now();
    tracing::info!(title = %payload.title, "creating task");

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let id = state.counter.fetch_add(1, Ordering::SeqCst);
    let task = Task {
        id,
        title: payload.title,
        done: false,
    };

    counter!("tasks_created_total").increment(1);
    let duration = start.elapsed().as_secs_f64();
    histogram!("http_request_duration_seconds", "endpoint" => "create_task").record(duration);
    tracing::info!(task_id = id, "task created");
    Json(task)
}

#[tracing::instrument(skip(_state))]
async fn get_task(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    counter!("http_requests_total", "endpoint" => "get_task").increment(1);
    let start = std::time::Instant::now();

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    let duration = start.elapsed().as_secs_f64();
    histogram!("http_request_duration_seconds", "endpoint" => "get_task").record(duration);
    tracing::info!(task_id = id, "task fetched");
    Json(serde_json::json!({"id":id,"title":"example task","done":false}))
}
