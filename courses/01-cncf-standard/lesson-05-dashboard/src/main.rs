use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post},
    Router,
};
use opentelemetry::{
    metrics::{Counter, Histogram},
    trace::TracerProvider as _,
    KeyValue,
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    metrics::{PeriodicReader, SdkMeterProvider},
    propagation::TraceContextPropagator,
    trace::SdkTracerProvider,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tower_http::trace::TraceLayer;
use tracing::{info, instrument};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

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
    request_counter: Counter<u64>,
    request_duration: Histogram<f64>,
}

fn init_observability() -> SdkTracerProvider {
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

    let log_exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .expect("failed to create log exporter");

    let logger_provider = opentelemetry_sdk::logs::SdkLoggerProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("learn-tracing-cncf")
                .build(),
        )
        .with_batch_exporter(log_exporter)
        .build();

    let otel_log_layer =
        opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider);

    let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .expect("failed to create metric exporter");

    let reader = PeriodicReader::builder(metric_exporter)
        .with_interval(std::time::Duration::from_secs(5))
        .build();

    let meter_provider = SdkMeterProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("learn-tracing-cncf")
                .build(),
        )
        .with_reader(reader)
        .build();

    opentelemetry::global::set_meter_provider(meter_provider);

    let span_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .expect("failed to create span exporter");

    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("learn-tracing-cncf")
                .build(),
        )
        .with_batch_exporter(span_exporter)
        .build();

    let tracer = tracer_provider.tracer("learn-tracing");
    let otel_trace_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(otel_log_layer)
        .with(otel_trace_layer)
        .init();

    tracer_provider
}

#[tokio::main]
async fn main() {
    let _tracer_provider = init_observability();

    let meter = opentelemetry::global::meter_provider().meter("learn-tracing");

    let state = Arc::new(AppState {
        counter: AtomicU64::new(1),
        request_counter: meter
            .u64_counter("http.requests.total")
            .with_description("Total number of HTTP requests")
            .build(),
        request_duration: meter
            .f64_histogram("http.request.duration")
            .with_description("HTTP request duration in seconds")
            .with_unit("s")
            .with_boundaries(vec![0.005, 0.01, 0.015, 0.02, 0.025, 0.03, 0.04, 0.05, 0.075, 0.1, 0.25, 0.5])
            .build(),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    info!("Server starting on http://127.0.0.1:3001");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[instrument]
async fn health() -> Json<serde_json::Value> {
    info!("health check called");
    Json(serde_json::json!({ "status": "ok" }))
}

#[instrument(skip(state), fields(task_title = %payload.title))]
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    let start = Instant::now();
    info!("creating task");

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let id = state.counter.fetch_add(1, Ordering::SeqCst);
    let elapsed = start.elapsed().as_secs_f64();

    state.request_counter.add(1, &[KeyValue::new("method", "POST"), KeyValue::new("route", "/tasks")]);
    state.request_duration.record(elapsed, &[KeyValue::new("method", "POST"), KeyValue::new("route", "/tasks")]);

    info!(task_id = id, duration_secs = elapsed, "task created");
    Json(Task { id, title: payload.title, done: false })
}

#[instrument(skip(state), fields(task_id = id))]
async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    let start = Instant::now();
    info!("fetching task");

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    let elapsed = start.elapsed().as_secs_f64();

    state.request_counter.add(1, &[KeyValue::new("method", "GET"), KeyValue::new("route", "/tasks/:id")]);
    state.request_duration.record(elapsed, &[KeyValue::new("method", "GET"), KeyValue::new("route", "/tasks/:id")]);

    Json(serde_json::json!({
        "id": id,
        "title": "example task",
        "done": false,
        "note": "demo data with traces"
    }))
}
