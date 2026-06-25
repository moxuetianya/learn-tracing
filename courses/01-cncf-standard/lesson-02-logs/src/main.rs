use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post},
    Router,
};
use opentelemetry_otlp::WithExportConfig;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::info;
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
}

fn init_logs() {
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

    let otel_layer =
        opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider);

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_subscriber::EnvFilter::new("warn"))
        .with(otel_layer)
        .init();
}

#[tokio::main]
async fn main() {
    init_logs();

    let state = Arc::new(AppState {
        counter: AtomicU64::new(1),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .with_state(state);

    info!("Server starting on http://127.0.0.1:3001");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> Json<serde_json::Value> {
    info!("health check called");
    Json(serde_json::json!({ "status": "ok" }))
}

async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    info!(title = %payload.title, "creating task");
    let id = state.counter.fetch_add(1, Ordering::SeqCst);
    Json(Task {
        id,
        title: payload.title,
        done: false,
    })
}

async fn get_task(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    info!(task_id = id, "fetching task");
    Json(serde_json::json!({
        "id": id,
        "title": "example task",
        "done": false
    }))
}
