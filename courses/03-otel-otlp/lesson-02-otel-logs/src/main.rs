use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post},
    Router,
};
use opentelemetry::logs::{Logger as _, LoggerProvider as _, LogRecord as _, Severity};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    logs::{SdkLogger, SdkLoggerProvider},
    metrics::SdkMeterProvider,
    trace::SdkTracerProvider,
};
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
    logger: SdkLogger,
}

fn init_tracing() -> SdkTracerProvider {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .expect("failed to create span exporter");

    SdkTracerProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("learn-tracing-otel")
                .build(),
        )
        .with_batch_exporter(exporter)
        .build()
}

fn init_metrics() -> SdkMeterProvider {
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .expect("failed to create metric exporter");

    SdkMeterProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("learn-tracing-otel")
                .build(),
        )
        .with_periodic_exporter(exporter)
        .build()
}

fn init_logs() -> (SdkLoggerProvider, SdkLogger) {
    let exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .expect("failed to create log exporter");

    let provider = SdkLoggerProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("learn-tracing-otel")
                .build(),
        )
        .with_simple_exporter(exporter)
        .build();

    let logger = provider.logger("learn-tracing");

    (provider, logger)
}

#[tokio::main]
async fn main() {
    let _tracer_provider = init_tracing();
    let _meter_provider = init_metrics();
    let (logger_provider, logger) = init_logs();

    let state = Arc::new(AppState {
        counter: AtomicU64::new(1),
        logger,
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .with_state(state);

    let mut startup = logger_provider.logger("learn-tracing").create_log_record();
    startup.set_body("Server starting on http://127.0.0.1:3003".into());
    startup.set_severity_number(Severity::Info);
    startup.add_attribute("course", "03-otel-otlp");
    startup.add_attribute("lesson", "02-logs");
    logger_provider.logger("learn-tracing").emit(startup);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3003").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let id = state.counter.fetch_add(1, Ordering::SeqCst);

    let mut record = state.logger.create_log_record();
    record.set_body(format!("task created: id={}", id).into());
    record.set_severity_number(Severity::Info);
    record.add_attribute("task.id", id as i64);
    record.add_attribute("task.title", payload.title.clone());
    state.logger.emit(record);

    Json(Task {
        id,
        title: payload.title,
        done: false,
    })
}

async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    let mut record = state.logger.create_log_record();
    record.set_body(format!("task fetched: id={}", id).into());
    record.set_severity_number(Severity::Info);
    record.add_attribute("task.id", id as i64);
    state.logger.emit(record);

    Json(serde_json::json!({
        "id": id,
        "title": "example task",
        "done": false,
        "note": "otel logger api - no tracing crate macros"
    }))
}
