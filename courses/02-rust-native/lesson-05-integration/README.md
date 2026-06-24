# 第 05 课：集成 — JSON 日志 + Prometheus 指标 + OTel 追踪

## 本课目标

- 将前三课的技术栈合为一体：JSON 日志（stdout）+ Prometheus 指标（`/metrics`）+ OTel 追踪（Jaeger）
- 理解多个 Subscriber Layer 共存的机制：每个 Layer 只处理自己关心的事情
- 掌握 `metrics` + `tracing-subscriber` 同时工作的原理：全局 Recorder 和全局 Subscriber 互不干扰
- 建立完整的可观测性闭环：发送请求 → JSON 日志线 → 指标计数器递增 → Jaeger 中查看完整 trace
- 对比 Course 1（CNCF 方式），建立"何时选择哪种方案"的判断力

---

## 核心概念

| 术语 | 解释 |
|------|------|
| **可观测性闭环** | 通过同一个请求，验证三种遥测信号（logs、metrics、traces）都正确工作 |
| **Layer 栈** | `registry().with(A).with(B).with(C)` — 多个 Layer 以栈式结构共存，每个 Layer 独立处理自己关心的数据 |
| **Registry** | `tracing_subscriber::registry()` 返回的特殊 Subscriber，支持任意数量的 Layer 叠加 |
| **全局 Recorder vs 全局 Subscriber** | `metrics` 的 Recorder 和 `tracing` 的 Subscriber 是两个完全独立的全局单例，互不干扰 |
| **Pull + Push 混合** | 指标采用 Prometheus pull（`/metrics` 端点），追踪采用 OTLP push（gRPC 到 Collector），日志采用 stdout push |
| `tower-http::trace` | axum 中间件，自动为每个 HTTP 请求创建 root span（包含 method、uri、status_code 等字段） |

---

## 依赖说明

本课是前面所有依赖的组合：

```toml
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-opentelemetry = "0.30"
opentelemetry = "0.29"
opentelemetry_sdk = "0.29"
opentelemetry-otlp = { version = "0.29", features = ["grpc-tonic"] }
metrics = "0.24"
metrics-exporter-prometheus = "0.16"
tower-http = { version = "0.6", features = ["trace"] }
```

与各分课的对应关系：

| Crate | 来源 |
|-------|------|
| `tracing` + `tracing-subscriber` | Lesson 01 (日志) |
| `metrics` + `metrics-exporter-prometheus` | Lesson 03 (指标) |
| `tracing-opentelemetry` + `opentelemetry_*` | Lesson 04 (追踪) |
| `tower-http` | Lesson 04 (HTTP 中间件) |

---

## 代码逐段讲解

### 1. 初始化 Subscriber Layer 栈（第一步）

```rust
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
```

这段代码与 Lesson 04 完全相同。关键点：

- `tracing_subscriber::registry()` 创建支持多层 Layer 的注册表
- `.with(fmt::layer().json())` — JSON 格式日志输出到 stdout
- `.with(EnvFilter::new("info"))` — 过滤日志级别
- `.with(otel_layer)` — 将 tracing Span 桥接到 OTel gRPC Exporter
- `.init()` — 安装为全局默认 Subscriber

### 2. 安装 Prometheus 指标 Recorder（第二步）

```rust
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
```

这段代码与 Lesson 03 完全相同。**关键事实**：

- `install_recorder()` 安装的是 `metrics` crate 的**全局 Recorder**
- 这与 `tracing_subscriber` 的**全局 Subscriber** 是完全独立的两个全局单例
- 二者互不干扰：Recorder 只接收 `counter!()` / `histogram!()` 调用，Subscriber 只接收 `tracing::info!()` / Span 事件

### 3. AppState 融合

```rust
struct AppState {
    counter: AtomicU64,
    prometheus_handle: PrometheusHandle,
}
```

将 `PrometheusHandle` 添加进 `AppState`，因为 `/metrics` 端点需要通过 `handle.render()` 生成 Prometheus 输出。

### 4. 路由配置

```rust
let app = Router::new()
    .route("/health", get(health))
    .route("/tasks", post(create_task))
    .route("/tasks/{id}", get(get_task))
    .route("/metrics", get(metrics_handler))
    .with_state(state);
```

比之前多了 `/metrics` 路由。

### 5. `/metrics` 端点

```rust
async fn metrics_handler(State(state): State<Arc<AppState>>) -> String {
    state.prometheus_handle.render()
}
```

返回 Prometheus text format 字符串。

### 6. 请求处理器 — 三种遥测信号合并

```rust
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
```

每个请求处理器同时产生三种遥测信号：

| 信号 | 代码 | 目标 |
|------|------|------|
| **Traces** | `#[tracing::instrument]` → 自动创建 Span | Jaeger（via OTLP gRPC） |
| **Metrics** | `counter!()` + `histogram!()` | Prometheus（via `/metrics`） |
| **Logs** | `tracing::info!("...")` | stdout（via fmt Layer） |

### 7. `create_task` 的完整遥测

```rust
#[tracing::instrument(skip(state))]
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    // ── Metrics ──
    counter!("http_requests_total", "endpoint" => "create_task").increment(1);
    let start = std::time::Instant::now();

    // ── Logs ──
    tracing::info!(title = %payload.title, "creating task");

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let id = state.counter.fetch_add(1, Ordering::SeqCst);
    let task = Task { id, title: payload.title, done: false };

    // ── Metrics ──
    counter!("tasks_created_total").increment(1);
    let duration = start.elapsed().as_secs_f64();
    histogram!("http_request_duration_seconds", "endpoint" => "create_task").record(duration);

    // ── Logs ──
    tracing::info!(task_id = id, "task created");

    // ── Traces: Span created by #[tracing::instrument] automatically ──

    Json(task)
}
```

### 8. 优雅关闭

```rust
axum::serve(listener, app).await.unwrap();

tracer_provider.shutdown()?;
Ok(())
```

`tracer_provider.shutdown()` 确保所有积攒在 Batch Exporter 中的 Span 发送到 Collector 后再退出。

---

## 运行验证

### 前置条件

确保已启动 Jaeger（或 OTel Collector），端口 4317（gRPC）和 16686（UI）可用：

```bash
docker run -d --name jaeger \
  -p 16686:16686 \
  -p 4317:4317 \
  jaegertracing/all-in-one:latest
```

### 启动应用

```bash
RUST_LOG=info cargo run -p lesson-05-integration
```

### 验证三种遥测信号

#### 1. 日志（stdout）

```bash
curl http://localhost:3001/health
```

终端输出：
```json
{"timestamp":"...","level":"INFO","span":{"name":"health"},"fields":{"message":"health check requested"}}
```

#### 2. 指标（Prometheus）

```bash
# 先产生一些请求
curl -X POST http://localhost:3001/tasks \
  -H "Content-Type: application/json" \
  -d '{"title":"Task 1"}'
curl -X POST http://localhost:3001/tasks \
  -H "Content-Type: application/json" \
  -d '{"title":"Task 2"}'
curl http://localhost:3001/tasks/1

# 查看指标
curl http://localhost:3001/metrics
```

预期输出包含：

```
# HELP http_requests_total Total number of HTTP requests
# TYPE http_requests_total counter
http_requests_total{endpoint="health"} 1
http_requests_total{endpoint="create_task"} 2
http_requests_total{endpoint="get_task"} 1

# HELP tasks_created_total Total number of tasks created
# TYPE tasks_created_total counter
tasks_created_total 2

# HELP http_request_duration_seconds HTTP request duration in seconds
# TYPE http_request_duration_seconds histogram
http_request_duration_seconds_bucket{endpoint="create_task",le="0.005"} ...
http_request_duration_seconds_sum{endpoint="create_task"} ...
http_request_duration_seconds_count{endpoint="create_task"} 2
```

#### 3. 追踪（Jaeger）

1. 打开 http://localhost:16686
2. Service 选择 `learn-tracing-native`
3. 点击 "Find Traces"
4. 看到 `create_task` 和 `health` 的 trace，点击展开查看完整调用链

### 完整的可观测性循环验证

按顺序执行，验证三种信号全部正确：

```bash
# Step 1: 发送请求
curl -X POST http://localhost:3001/tasks \
  -H "Content-Type: application/json" \
  -d '{"title":"Complete observability"}'

# Step 2: 观察 JSON 日志（终端输出）
# → 看到 "creating task" 和 "task created" 两条日志

# Step 3: 检查指标计数器递增
curl http://localhost:3001/metrics | grep http_requests_total
# → 确认 create_task 的计数 +1

# Step 4: 在 Jaeger 中查看 trace
# → http://localhost:16686
# → 看到 create_task span 及其内部的日志事件
```

---

## 与 Course 1 的完整对比

| 方面 | Course 1 (CNCF 原生 OpenTelemetry) | Course 2 (Rust Native Tracing) |
|------|-----------------------------------|-------------------------------|
| **日志** | OTLP → Collector → 后端（Loki/Elastic） | JSON stdout（可配合 log 收集器转发） |
| **指标** | OTLP Push → Collector → Prometheus | Prometheus Pull `/metrics`（应用直接暴露端点） |
| **追踪** | 手动 `tracer.start("name")`，手动 set_attribute | `#[tracing::instrument]` 宏自动创建 Span |
| **Span 嵌套** | 手动传入 parent context | 宏自动处理父子关系 |
| **初始化复杂度** | 约 60 行样板代码（LogExporter + Meter + Tracer） | 约 30 行（registry + layers） |
| **与 Rust 生态整合** | 需要了解 OTel 概念体系 | 使用 Rust 习惯的宏和 trait |
| **依赖数量** | 8+ OTel crates | 5-8 crates（tracing 生态更集中） |
| **指标分发方式** | 每个指标通过 `meter.u64_counter()` 创建并传递 | 全局 `counter!()` 宏，无需传参 |

### 何时选择 Course 1 方案？

| 场景 | 推荐 |
|------|------|
| 多语言微服务环境（Rust + Go + Python） | Course 1（统一使用 OTel 标准） |
| 需要完整 OTel 链路传播（W3C TraceContext） | Course 1 |
| 已部署 OTel Collector，所有服务统一通过 OTLP 推送 | Course 1 |
| 需要 vendor-neutral（可替换后端为 Datadog/Jaeger） | Course 1 |

### 何时选择 Course 2 方案？

| 场景 | 推荐 |
|------|------|
| 纯 Rust 项目 | Course 2 |
| 追求最小样板代码 | Course 2 |
| 指标采用 Prometheus pull 模式 | Course 2 |
| 希望日志保持简单（stdout JSON） | Course 2 |
| 只关心 traces 导出到 OTel（日志和指标不需要 OTLP） | Course 2 |

> 实际上两种方案可以混合使用——例如用 `#[tracing::instrument]` 创建 Span，用 Course 2 的方式管理日志和指标，同时用 `tracing-opentelemetry` 桥接导出到 OTel（如 Lesson 05 所示）。这结合了 Rust 习惯写法和 OTel 标准的优点。

---

## 疑难点

### 1. `install_recorder()` 和 `init()` 的调用顺序

`install_recorder()`（`metrics` crate）和 `.init()`（`tracing-subscriber`）的调用顺序**没有严格要求**。它们是两个独立的全局单例注册。本课先调用 `.init()` 再调用 `install_recorder()`，反过来也完全可以。

### 2. 多个全局单例不会冲突

这是一个常见的疑问：同时有全局 Subscriber、全局 Recorder，是否冲突？

**不会**。它们内部维护不同的全局变量（`tracing` 用 `thread_local!` + `Atomic`，`metrics` 用 `OnceLock`）。Rust 的类型系统确保它们互不干涉。

### 3. Prometheus 指标在进程重启后归零

Counter 和 Histogram 的值保存在进程内存中。进程重启后所有值归零，Prometheus 会检测到重置并正确处理。如果需要持久化指标值，需要外部存储——这不是 `metrics` crate 的职责范围。

### 4. OTLP gRPC 连接失败不影响应用

如果 Jaeger/Collector 未启动，应用仍然正常运行。Batch Exporter 会在后台重试连接。长时间连接失败会导致 Span 被丢弃（超出缓冲区容量），但不会导致应用崩溃或请求延迟增加。

### 5. JSON 日志生产环境部署

stdout JSON 日志在生产环境中通常配合以下方案使用：

- **systemd journald** — 系统级日志收集，自动采集 stdout
- **Fluentd / Fluent Bit** — 读取容器 stdout，转发到 Elasticsearch / Loki
- **Docker logging driver** — `json-file` 或 `fluentd` 驱动
- **Kubernetes** — `kubectl logs` 自动读取容器 stdout

如需将日志推送到 OTLP Collector（与 traces 一致），可以添加 `opentelemetry-appender-tracing` bridge layer。

### 6. `#[tracing::instrument]` 对性能的影响

`#[instrument]` 创建的 Span 会：
- 分配少量堆内存
- 在函数入口和出口记录时间戳

对于高吞吐量（>10k req/s）的路径，建议谨慎使用或只对关键路径添加 `#[instrument]`。可以使用 `RUST_LOG` 过滤来动态控制 Span 创建。

### 7. 架构总览图

```
                    ┌────────────────────────────────┐
                    │      Application Process        │
                    │                                │
HTTP Request ───────▶   axum Handler                 │
                    │   │                            │
                    │   ├── #[tracing::instrument]   │  → Span created
                    │   ├── tracing::info!("...")     │  → Log event
                    │   ├── counter!("...")           │  → Metric increment
                    │   └── histogram!("...")         │  → Latency record
                    │                                │
                    │   ┌─ Subscriber (global) ──┐   │
                    │   │  fmt::layer  → stdout   │   │  → JSON logs
                    │   │  EnvFilter    → filter  │   │
                    │   │  otel_layer   → gRPC ───┼───▶ OTLP → Jaeger
                    │   └─────────────────────────┘   │
                    │                                │
                    │   ┌─ Recorder (global) ─────┐   │
                    │   │  PrometheusRecorder      │   │
                    │   │  └─ render() → /metrics  │   │  → Prometheus
                    │   └─────────────────────────-┘   │
                    └────────────────────────────────┘

                    Outputs:
                    ① stdout JSON  → Fluent Bit → Loki
                    ② /metrics     → Prometheus scrape
                    ③ OTLP gRPC    → Collector → Jaeger
```
