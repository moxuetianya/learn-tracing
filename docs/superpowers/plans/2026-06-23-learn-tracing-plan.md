# Learn Tracing 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建一个渐进式可观测性学习项目，包含 3 个课程 × 5 课时 = 15 课时的 Rust 项目 + 教程文档

**Architecture:** Monorepo 结构，共享 docker-compose 后端服务。每个课程是独立 Cargo workspace，每课时是独立 crate。3 个课程分别覆盖 CNCF 标准生态、Rust 原生 tracing 生态、纯 OpenTelemetry OTLP 三种方案。

**Tech Stack:** Rust 1.96, axum 0.8, tokio 1, opentelemetry 0.29, tracing 0.1, Docker Compose (Collector, Jaeger, Prometheus, Grafana)

---

### Task 1: 共享基础设施 — Docker Compose + Collector/Prometheus/Grafana 配置

**Files:**
- Create: `docker-compose.yml`
- Create: `configs/otel-collector-config.yaml`
- Create: `configs/prometheus.yml`
- Create: `configs/grafana-datasources.yml`
- Create: `configs/grafana-dashboards.yml`
- Create: `configs/dashboards/app-dashboard.json`

- [ ] **Step 1: 创建 docker-compose.yml**

```yaml
version: "3.8"
services:
  otel-collector:
    image: otel/opentelemetry-collector-contrib:latest
    container_name: otel-collector
    command: ["--config=/etc/otel-collector-config.yaml"]
    volumes:
      - ./configs/otel-collector-config.yaml:/etc/otel-collector-config.yaml
    ports:
      - "4317:4317"   # OTLP gRPC
      - "4318:4318"   # OTLP HTTP

  jaeger:
    image: jaegertracing/all-in-one:1
    container_name: jaeger
    environment:
      - COLLECTOR_OTLP_ENABLED=true
    ports:
      - "16686:16686"  # UI

  prometheus:
    image: prom/prometheus:latest
    container_name: prometheus
    extra_hosts:
      - "host.docker.internal:host-gateway"
    volumes:
      - ./configs/prometheus.yml:/etc/prometheus/prometheus.yml
    ports:
      - "9090:9090"

  grafana:
    image: grafana/grafana:latest
    container_name: grafana
    environment:
      - GF_AUTH_ANONYMOUS_ENABLED=true
      - GF_AUTH_ANONYMOUS_ORG_ROLE=Admin
    volumes:
      - ./configs/grafana-datasources.yml:/etc/grafana/provisioning/datasources/datasources.yml
      - ./configs/grafana-dashboards.yml:/etc/grafana/provisioning/dashboards/dashboards.yml
      - ./configs/dashboards:/etc/grafana/provisioning/dashboards
    ports:
      - "3000:3000"
```

- [ ] **Step 2: 创建 otel-collector-config.yaml**

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318

processors:
  batch:
    timeout: 1s
    send_batch_size: 1024

exporters:
  debug:
    verbosity: detailed
  otlp/jaeger:
    endpoint: jaeger:4317
    tls:
      insecure: true

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug, otlp/jaeger]
    metrics:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug]
    logs:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug]
```

- [ ] **Step 3: 创建 prometheus.yml**

```yaml
global:
  scrape_interval: 5s

scrape_configs:
  - job_name: "app"
    static_configs:
      - targets: ["host.docker.internal:3001"]
        labels:
          service: "learn-tracing-app"
```

- [ ] **Step 4: 创建 grafana-datasources.yml**

```yaml
apiVersion: 1
datasources:
  - name: Prometheus
    type: prometheus
    url: http://prometheus:9090
    access: proxy
    isDefault: true
  - name: Jaeger
    type: jaeger
    url: http://jaeger:16686
    access: proxy
```

- [ ] **Step 5: 创建 grafana-dashboards.yml**

```yaml
apiVersion: 1
providers:
  - name: "default"
    folder: "Learn Tracing"
    type: file
    options:
      path: /etc/grafana/provisioning/dashboards
```

- [ ] **Step 6: 创建 dashboards/app-dashboard.json**

写入一个基础 Grafana Dashboard JSON，包含：
- HTTP 请求速率面板 (Prometheus `rate(http_requests_total[1m])`)
- P50/P95/P99 延迟面板 (Prometheus `histogram_quantile`)
- Error rate 面板
- Jaeger trace 链接

(完整 JSON 见下方，由于篇幅较长，此处为简略版，实际创建时写入完整 JSON)

- [ ] **Step 7: 验证 docker-compose**

```bash
docker compose up -d
docker compose ps  # 确认 4 个服务都在运行
curl http://localhost:16686  # Jaeger UI
curl http://localhost:9090   # Prometheus
curl http://localhost:3000   # Grafana
docker compose down
```

---

### Task 2: 课程 1 环境搭建 — Lesson 01（CNCF 标准生态）

**Files:**
- Create: `courses/01-cncf-standard/Cargo.toml`
- Create: `courses/01-cncf-standard/README.md`
- Create: `courses/01-cncf-standard/lesson-01-setup/Cargo.toml`
- Create: `courses/01-cncf-standard/lesson-01-setup/src/main.rs`
- Create: `courses/01-cncf-standard/lesson-01-setup/README.md`

- [ ] **Step 1: 创建 workspace Cargo.toml**

```toml
[workspace]
resolver = "2"
members = [
    "lesson-01-setup",
    "lesson-02-logs",
    "lesson-03-metrics",
    "lesson-04-traces",
    "lesson-05-dashboard",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
```

- [ ] **Step 2: 创建 Lesson 01 Cargo.toml**

```toml
[package]
name = "lesson-01-setup"
version.workspace = true
edition.workspace = true

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 3: 创建 lesson-01-setup/src/main.rs**

```rust
use axum::{
    extract::Path,
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
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
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
    axum::extract::State(_state): axum::extract::State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "id": id,
        "title": "example task",
        "done": false,
        "note": "static demo data"
    }))
}
```

- [ ] **Step 4: 创建 README.md 教程文档**

内容：介绍课程目标、本课内容（裸 axum 服务，无可观测性）、运行方式、验证方法。

- [ ] **Step 5: 编译验证**

```bash
cd courses/01-cncf-standard && cargo build -p lesson-01-setup
```

---

### Task 3: 课程 1 Logs — Lesson 02（CNCF 标准生态）

**Files:**
- Create: `courses/01-cncf-standard/lesson-02-logs/Cargo.toml`
- Create: `courses/01-cncf-standard/lesson-02-logs/src/main.rs`
- Create: `courses/01-cncf-standard/lesson-02-logs/README.md`

- [ ] **Step 1: 创建 Cargo.toml**

```toml
[package]
name = "lesson-02-logs"
version.workspace = true
edition.workspace = true

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
opentelemetry = "0.29"
opentelemetry_sdk = "0.29"
opentelemetry-otlp = { version = "0.29", features = ["grpc-tonic"] }
opentelemetry-appender-tracing = "0.29"
```

- [ ] **Step 2: 创建 main.rs — 初始化 Logs pipeline**

```rust
use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post},
    Router,
};
use opentelemetry::KeyValue;
use opentelemetry_otlp::{LogExporter, WithExportConfig};
use opentelemetry_sdk::logs::LoggerProvider;
use opentelemetry_sdk::Resource;
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
    let exporter = LogExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .unwrap();

    let logger_provider = LoggerProvider::builder()
        .with_resource(Resource::new(vec![KeyValue::new(
            "service.name",
            "learn-tracing-cncf",
        )]))
        .with_simple_exporter(exporter)
        .build();

    let otel_layer =
        opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider);

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().json()) // 本地 JSON 输出
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(otel_layer) // OTel 导出到 Collector
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
```

- [ ] **Step 3: 创建 README.md 教程文档**

详细讲解：LoggerProvider 初始化、OTLP LogExporter、opentelemetry-appender-tracing bridge、结构化日志字段、Collector 中查看日志。

- [ ] **Step 4: 编译验证**

```bash
cd courses/01-cncf-standard && cargo build -p lesson-02-logs
```

---

### Task 4: 课程 1 Metrics — Lesson 03（CNCF 标准生态）

**Files:**
- Create: `courses/01-cncf-standard/lesson-03-metrics/Cargo.toml`
- Create: `courses/01-cncf-standard/lesson-03-metrics/src/main.rs`
- Create: `courses/01-cncf-standard/lesson-03-metrics/README.md`

- [ ] **Step 1: 创建 Cargo.toml**

在 lesson-02 的基础上增加 metrics 相关依赖：
```toml
[package]
name = "lesson-03-metrics"
version.workspace = true
edition.workspace = true

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
opentelemetry = "0.29"
opentelemetry_sdk = "0.29"
opentelemetry-otlp = { version = "0.29", features = ["grpc-tonic", "metrics"] }
opentelemetry-appender-tracing = "0.29"
```

- [ ] **Step 2: 创建 main.rs — 增加 Metrics**

关键代码变化：
1. 初始化 MeterProvider 和 OTLP MetricsExporter
2. 创建 `http.request.duration` Histogram 和 `http.requests.total` Counter
3. 在每个 handler 中手动记录 metrics
4. 使用 axum 中间件自动记录

```rust
use axum::{
    extract::{Path, State},
    middleware,
    response::Json,
    routing::{get, post},
    Router,
};
use opentelemetry::{
    metrics::{Counter, Histogram},
    KeyValue,
};
use opentelemetry_otlp::{LogExporter, MetricExporter, WithExportConfig};
use opentelemetry_sdk::{
    logs::LoggerProvider,
    metrics::{Aggregation, Instrument, MeterProviderBuilder, PeriodicReader, SdkMeterProvider},
    runtime::TokioCurrentThread,
    Resource,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod metrics;

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

fn init_observability() -> SdkMeterProvider {
    let log_exporter = LogExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .unwrap();

    let logger_provider = LoggerProvider::builder()
        .with_resource(Resource::new(vec![KeyValue::new(
            "service.name",
            "learn-tracing-cncf",
        )]))
        .with_simple_exporter(log_exporter)
        .build();

    let otel_layer =
        opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider);

    let metric_exporter = MetricExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .unwrap();

    let meter_provider = SdkMeterProvider::builder()
        .with_resource(Resource::new(vec![KeyValue::new(
            "service.name",
            "learn-tracing-cncf",
        )]))
        .with_reader(
            PeriodicReader::builder(metric_exporter, TokioCurrentThread)
                .with_interval(std::time::Duration::from_secs(5))
                .build(),
        )
        .build();

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(otel_layer)
        .init();

    meter_provider
}

#[tokio::main]
async fn main() {
    let meter_provider = init_observability();
    let meter = meter_provider.meter("learn-tracing");

    let request_counter = meter
        .u64_counter("http.requests.total")
        .with_description("Total HTTP requests")
        .build();

    let request_duration = meter
        .f64_histogram("http.request.duration")
        .with_description("HTTP request duration in seconds")
        .build();

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
```

- [ ] **Step 3: 创建 README.md 教程文档**

讲解：MeterProvider 初始化、PeriodicReader、Counter vs Histogram、OTLP MetricsExporter、在 handler 中记录 metrics 的最佳实践。

---

### Task 5: 课程 1 Traces — Lesson 04（CNCF 标准生态）

**Files:**
- Create: `courses/01-cncf-standard/lesson-04-traces/Cargo.toml`
- Create: `courses/01-cncf-standard/lesson-04-traces/src/main.rs`
- Create: `courses/01-cncf-standard/lesson-04-traces/README.md`

- [ ] **Step 1: 创建 Cargo.toml**

在 lesson-03 基础上增加 trace 导出：
```toml
[package]
name = "lesson-04-traces"
version.workspace = true
edition.workspace = true

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
opentelemetry = "0.29"
opentelemetry_sdk = "0.29"
opentelemetry-otlp = { version = "0.29", features = ["grpc-tonic", "metrics"] }
opentelemetry-appender-tracing = "0.29"
tower = "0.5"
tower-http = { version = "0.6", features = ["trace"] }
```

- [ ] **Step 2: 创建 main.rs — 增加 Traces**

关键新增：
1. 初始化 TracerProvider + OTLP SpanExporter
2. 使用 `#[tracing::instrument]` 宏创建 span
3. axum 自动 trace 中间件 (tower-http trace layer)
4. W3C TraceContext 传播

```rust
use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post},
    Router,
};
use opentelemetry::{
    global,
    metrics::{Counter, Histogram},
    trace::{Span, Tracer},
    KeyValue,
};
use opentelemetry_otlp::{LogExporter, MetricExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::{
    logs::LoggerProvider,
    metrics::SdkMeterProvider,
    propagation::TraceContextPropagator,
    Resource,
    runtime::TokioCurrentThread,
    trace::SdkTracerProvider,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
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
}

fn init_observability() -> SdkTracerProvider {
    global::set_text_map_propagator(TraceContextPropagator::new());

    let log_exporter = LogExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .unwrap();

    let logger_provider = LoggerProvider::builder()
        .with_resource(Resource::new(vec![KeyValue::new(
            "service.name",
            "learn-tracing-cncf",
        )]))
        .with_simple_exporter(log_exporter)
        .build();

    let otel_layer =
        opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider);

    let span_exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .unwrap();

    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(Resource::new(vec![KeyValue::new(
            "service.name",
            "learn-tracing-cncf",
        )]))
        .with_batch_exporter(span_exporter, TokioCurrentThread)
        .build();

    let tracer_layer = tracing_opentelemetry::layer()
        .with_tracer(tracer_provider.tracer("learn-tracing"));

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(otel_layer)
        .with(tracer_layer)
        .init();

    tracer_provider
}

#[tokio::main]
async fn main() {
    let tracer_provider = init_observability();

    let state = Arc::new(AppState {
        counter: AtomicU64::new(1),
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
    info!("creating task");
    // Simulate database write
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let id = state.counter.fetch_add(1, Ordering::SeqCst);
    Json(Task {
        id,
        title: payload.title,
        done: false,
    })
}

#[instrument(skip(state), fields(task_id = id))]
async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    info!("fetching task");
    // Simulate cache lookup
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    Json(serde_json::json!({
        "id": id,
        "title": "example task",
        "done": false,
        "note": "demo data from lesson-04"
    }))
}
```

- [ ] **Step 3: 创建 README.md 教程文档**

讲解：TracerProvider、Span 与 Event 的区别、`#[instrument]` 宏、tower-http TraceLayer、W3C TraceContext 传播、Jaeger UI 中查看 trace timeline。

---

### Task 6: 课程 1 Dashboard — Lesson 05（CNCF 标准生态）

**Files:**
- Create: `courses/01-cncf-standard/lesson-05-dashboard/Cargo.toml`
- Create: `courses/01-cncf-standard/lesson-05-dashboard/src/main.rs`
- Create: `courses/01-cncf-standard/lesson-05-dashboard/README.md`
- Modify: `configs/dashboards/app-dashboard.json` (完善 Dashboard JSON)

- [ ] **Step 1: 创建 Cargo.toml** — 同 Lesson 04，增加完整的 metrics 记录

- [ ] **Step 2: 创建 main.rs** — 整合前 4 课全部能力

代码整合 logs + metrics + traces，增加以下改进：
1. 使用 `Middleware` 统一记录 request metrics（避免每个 handler 重复代码）
2. 在 handler 中添加 `Span::current().add_event()` 记录业务事件
3. 模拟有意义的延迟（DB 20ms，缓存 5ms）

- [ ] **Step 3: 完善 configs/dashboards/app-dashboard.json**

写入完整的 Grafana Dashboard JSON，包含：
- **Row 1: Overview** — 服务健康状态
- **Row 2: HTTP Metrics** — QPS 折线图、P50/P95/P99 延迟热力图
- **Row 3: Error Tracking** — 错误率面板
- **Row 4: Trace Explorer** — 指向 Jaeger 的链接

- [ ] **Step 4: 创建 README.md 教程文档**

讲解：Grafana Dashboard 的设计思路、PromQL 基础查询、Jaeger trace 链接配置、Grafana 数据源 provision。

---

### Task 7: 课程 2 Rust 原生 tracing — Lesson 01 ~ 05

**Files:**
- Create: `courses/02-rust-native/Cargo.toml` (workspace)
- Create: `courses/02-rust-native/README.md`
- Create: `courses/02-rust-native/lesson-01-tracing-subscriber/{Cargo.toml,src/main.rs,README.md}`
- Create: `courses/02-rust-native/lesson-02-span/{Cargo.toml,src/main.rs,README.md}`
- Create: `courses/02-rust-native/lesson-03-metrics/{Cargo.toml,src/main.rs,README.md}`
- Create: `courses/02-rust-native/lesson-04-traces/{Cargo.toml,src/main.rs,README.md}`
- Create: `courses/02-rust-native/lesson-05-integration/{Cargo.toml,src/main.rs,README.md}`

课程 2 的核心区别：
- **无 opentelemetry_sdk 直接使用**，通过 `tracing-opentelemetry` bridge 间接输出到 OTel
- 使用 `tracing-subscriber` 的 Layer 组合模式：fmt layer (JSON stdout) + opentelemetry layer (Jaeger)
- Metrics 使用 Rust `metrics` crate + `metrics-exporter-prometheus`，在 `/metrics` 端点暴露

**Lesson 01 (`lesson-01-tracing-subscriber`):** 初始化 tracing-subscriber Registry，fmt layer 输出 JSON。展示 subscriber 的 Layer 组合模式。

**Lesson 02 (`lesson-02-span`):** `#[instrument]` 宏详细用法，`Span::current()`、span 字段与生命周期、axum 自动 span、`in_scope()`。

**Lesson 03 (`lesson-03-metrics`):** `metrics` crate 的 `counter!`/`histogram!` 宏、`metrics-exporter-prometheus` 暴露 `/metrics` 端点、Prometheus 抓取验证。

**Lesson 04 (`lesson-04-traces`):** `tracing-opentelemetry` layer 初始化、将 tracing span 桥接到 OTLP、Collector → Jaeger 验证。

**Lesson 05 (`lesson-05-integration`):** 多层 subscriber 组合（JSON + OTel + Prometheus 同时工作）、对比课程 1 的差异、选型建议。

每个 lesson 的 main.rs 结构与课程 1 对应 lesson 相似，但依赖和初始化代码不同。核心区别代码示例：

*Lesson 04 traces 初始化（区别于课程 1）:*
```rust
fn init_tracing() {
    let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(
            opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint("http://localhost:4317")
                .build()
                .unwrap(),
            opentelemetry_sdk::runtime::TokioCurrentThread,
        )
        .with_resource(Resource::new(vec![KeyValue::new("service.name", "learn-tracing-native")]))
        .build();

    let tracer = tracer_provider.tracer("learn-tracing");

    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(otel_layer)
        .init();
}
```

---

### Task 8: 课程 3 纯 OTel OTLP — Lesson 01 ~ 05

**Files:**
- Create: `courses/03-otel-otlp/Cargo.toml` (workspace)
- Create: `courses/03-otel-otlp/README.md`
- Create: `courses/03-otel-otlp/lesson-01-otel-init/{Cargo.toml,src/main.rs,README.md}`
- Create: `courses/03-otel-otlp/lesson-02-otel-logs/{Cargo.toml,src/main.rs,README.md}`
- Create: `courses/03-otel-otlp/lesson-03-otel-metrics/{Cargo.toml,src/main.rs,README.md}`
- Create: `courses/03-otel-otlp/lesson-04-otel-traces/{Cargo.toml,src/main.rs,README.md}`
- Create: `courses/03-otel-otlp/lesson-05-collector-pipeline/{Cargo.toml,src/main.rs,README.md}`

课程 3 的核心区别：
- **完全不依赖 tracing crate**，直接使用 opentelemetry Rust SDK API
- 手动管理 Span 的 start/end 生命周期
- 手动处理 Context 传播

**Lesson 01 (`lesson-01-otel-init`):** 纯 OTel SDK 初始化，无 tracing crate。TracerProvider + MeterProvider + LoggerProvider 三合一配置。

**Lesson 02 (`lesson-02-otel-logs`):** OTel Logs API，Logger 创建，LogRecord 构建，与 span context 关联。

**Lesson 03 (`lesson-03-otel-metrics`):** OTel Metrics API，Counter/Histogram 的创建和记录，Attributes 使用。

**Lesson 04 (`lesson-04-otel-traces`):** OTel Traces API，Span start/end 生命周期，Context 传播，SpanKind 区分。

**Lesson 05 (`lesson-05-collector-pipeline`):** 深入 Collector 配置：receivers、processors (batch, memory_limiter, attributes)、exporters、pipelines。展示完整数据流转。

核心区别代码示例（Lesson 04 手动 span 管理）：
```rust
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    let tracer = global::tracer("learn-tracing");
    let mut span = tracer
        .span_builder("POST /tasks")
        .with_kind(opentelemetry::trace::SpanKind::Server)
        .start(&tracer);
    
    span.set_attribute(KeyValue::new("task.title", payload.title.clone()));
    
    let id = state.counter.fetch_add(1, Ordering::SeqCst);
    
    let task = Task {
        id,
        title: payload.title,
        done: false,
    };
    
    span.set_attribute(KeyValue::new("task.id", id as i64));
    span.end();
    
    Json(task)
}
```

---

### Task 9: 项目根 README 和清理

**Files:**
- Create: `README.md` (项目根目录)
- Create: `.gitignore`

- [ ] **Step 1: 创建根 README.md**

```markdown
# Learn Tracing — 可观测性学习项目

通过 Rust + Docker Compose 渐进式学习可观测性三大支柱：Logs、Metrics、Traces。

## 课程结构

| 课程 | 方案 | 核心库 | 课时 |
|---|---|---|---|
| [01-cncf-standard](./courses/01-cncf-standard/) | CNCF 标准生态 | opentelemetry SDK + OTLP | 5 |
| [02-rust-native](./courses/02-rust-native/) | Rust 原生 tracing 生态 | tracing crate 家族 | 5 |
| [03-otel-otlp](./courses/03-otel-otlp/) | 纯 OpenTelemetry OTLP | opentelemetry SDK (无 tracing) | 5 |

## 快速开始

### 1. 启动可观测性后端

\`\`\`bash
docker compose up -d
\`\`\`

### 2. 运行课程代码

\`\`\`bash
# 以课程 1 第 4 课为例
cd courses/01-cncf-standard
cargo run -p lesson-04-traces
\`\`\`

### 3. 查看结果

- **Jaeger UI**: http://localhost:16686
- **Prometheus**: http://localhost:9090
- **Grafana**: http://localhost:3000

### 4. 停止服务

\`\`\`bash
docker compose down
\`\`\`

## 学习路径推荐

1. 新手 → 先学 [课程 2 (Rust 原生)](./courses/02-rust-native/) 理解 tracing 概念
2. 进阶 → 学 [课程 1 (CNCF 标准)](./courses/01-cncf-standard/) 掌握行业标准
3. 深入 → 学 [课程 3 (纯 OTel)](./courses/03-otel-otlp/) 理解底层 API

## 要求

- Rust 1.85+
- Docker & Docker Compose
```

- [ ] **Step 2: 创建 .gitignore**

```
target/
**/target/
```

- [ ] **Step 3: 全局编译验证**

```bash
cd courses/01-cncf-standard && cargo check --workspace
cd ../02-rust-native && cargo check --workspace
cd ../03-otel-otlp && cargo check --workspace
```
