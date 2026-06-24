use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post},
    Router,
};
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

    let state = Arc::new(AppState {
        counter: AtomicU64::new(1),
    });
    let app = Router::new()
        .route("/health", get(health))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .with_state(state);

    tracing::info!("Server running on http://127.0.0.1:3001");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await.unwrap();
    axum::serve(listener, app).await.unwrap();

    tracer_provider.shutdown()?;
    Ok(())
}

#[tracing::instrument]
async fn health() -> Json<serde_json::Value> {
    tracing::info!("health check requested");
    Json(serde_json::json!({ "status": "ok" }))
}

#[tracing::instrument(skip(state))]
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    tracing::info!(title = %payload.title, "creating task");
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let id = state.counter.fetch_add(1, Ordering::SeqCst);
    let task = Task {
        id,
        title: payload.title,
        done: false,
    };
    tracing::info!(task_id = id, "task created");
    Json(task)
}

#[tracing::instrument(skip(_state))]
async fn get_task(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    tracing::info!(task_id = id, "task fetched");
    Json(serde_json::json!({"id":id,"title":"example task","done":false}))
}
