use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post},
    Router,
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
}

#[tokio::main]
async fn main() {
    let state = Arc::new(AppState {
        counter: AtomicU64::new(1),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await.unwrap();
    println!("Server running on http://127.0.0.1:3001");
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
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
    Json(serde_json::json!({
        "id": id,
        "title": "example task",
        "done": false,
        "note": "static demo data"
    }))
}
