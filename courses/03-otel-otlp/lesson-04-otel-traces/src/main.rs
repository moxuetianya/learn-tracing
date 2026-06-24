use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post},
    Router,
};
use opentelemetry::{
    KeyValue,
    logs::{Logger as _, LoggerProvider as _, LogRecord as _, Severity},
    metrics::{Counter, Histogram, MeterProvider as _},
    trace::{
        Span as _, SpanKind, Status, Tracer as _, TracerProvider as _,
    },
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    logs::{SdkLogger, SdkLoggerProvider},
    metrics::SdkMeterProvider,
    trace::{SdkTracer, SdkTracerProvider},
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

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
    tracer: SdkTracer,
    request_counter: Counter<u64>,
    request_duration: Histogram<f64>,
}

fn init_tracing() -> (SdkTracerProvider, SdkTracer) {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .expect("failed to create span exporter");

    let provider = SdkTracerProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("learn-tracing-otel")
                .build(),
        )
        .with_batch_exporter(exporter)
        .build();

    let tracer = provider.tracer("learn-tracing");

    (provider, tracer)
}

fn init_metrics() -> (SdkMeterProvider, Counter<u64>, Histogram<f64>) {
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .expect("failed to create metric exporter");

    let provider = SdkMeterProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("learn-tracing-otel")
                .build(),
        )
        .with_periodic_exporter(exporter)
        .build();

    let meter = provider.meter("learn-tracing");

    let request_counter = meter
        .u64_counter("http.requests.total")
        .with_description("Total number of HTTP requests")
        .build();

    let request_duration = meter
        .f64_histogram("http.request.duration")
        .with_description("HTTP request duration in seconds")
        .build();

    (provider, request_counter, request_duration)
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
    let (_tracer_provider, tracer) = init_tracing();
    let (_meter_provider, request_counter, request_duration) = init_metrics();
    let (_logger_provider, logger) = init_logs();

    let state = Arc::new(AppState {
        counter: AtomicU64::new(1),
        logger,
        tracer,
        request_counter,
        request_duration,
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .with_state(state);

    let mut startup = _logger_provider.logger("learn-tracing").create_log_record();
    startup.set_body("Server starting on http://127.0.0.1:3003".into());
    startup.set_severity_number(Severity::Info);
    startup.add_attribute("lesson", "04-traces");
    _logger_provider.logger("learn-tracing").emit(startup);

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
    let start = Instant::now();

    let mut span = state
        .tracer
        .span_builder("POST /tasks")
        .with_kind(SpanKind::Server)
        .start(&state.tracer);
    span.set_attribute(KeyValue::new("task.title", payload.title.clone()));

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let id = state.counter.fetch_add(1, Ordering::SeqCst);
    let elapsed = start.elapsed().as_secs_f64();

    span.set_attribute(KeyValue::new("task.id", id as i64));

    state.request_counter.add(
        1,
        &[
            KeyValue::new("method", "POST"),
            KeyValue::new("route", "/tasks"),
        ],
    );
    state.request_duration.record(
        elapsed,
        &[
            KeyValue::new("method", "POST"),
            KeyValue::new("route", "/tasks"),
        ],
    );

    let mut record = state.logger.create_log_record();
    record.set_body(format!("task created: id={}", id).into());
    record.set_severity_number(Severity::Info);
    record.add_attribute("task.id", id as i64);
    record.add_attribute("task.title", payload.title.clone());
    record.add_attribute("duration_secs", elapsed);
    state.logger.emit(record);

    span.set_status(Status::Ok);
    span.end();

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
    let start = Instant::now();

    let mut span = state
        .tracer
        .span_builder("GET /tasks/:id")
        .with_kind(SpanKind::Server)
        .start(&state.tracer);
    span.set_attribute(KeyValue::new("task.id", id as i64));

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    let elapsed = start.elapsed().as_secs_f64();

    span.set_attribute(KeyValue::new("duration_secs", elapsed));

    state.request_counter.add(
        1,
        &[
            KeyValue::new("method", "GET"),
            KeyValue::new("route", "/tasks/:id"),
        ],
    );
    state.request_duration.record(
        elapsed,
        &[
            KeyValue::new("method", "GET"),
            KeyValue::new("route", "/tasks/:id"),
        ],
    );

    let mut record = state.logger.create_log_record();
    record.set_body(format!("task fetched: id={}", id).into());
    record.set_severity_number(Severity::Info);
    record.add_attribute("task.id", id as i64);
    record.add_attribute("duration_secs", elapsed);
    state.logger.emit(record);

    span.set_status(Status::Ok);
    span.end();

    Json(serde_json::json!({
        "id": id,
        "title": "example task",
        "done": false,
        "note": "manual spans - pure otel traces"
    }))
}
