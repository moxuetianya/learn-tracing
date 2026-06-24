# Lesson 01: OTel SDK 初始化 — 纯 OpenTelemetry 的开端

## 本课目标

学习如何**手动初始化** OpenTelemetry Rust SDK，创建 `TracerProvider`、`MeterProvider`、`LoggerProvider` 三大 Provider，并配置 OTLP gRPC 导出器。这是整个"纯 OTel"课程的基础，后续每节课都将使用相同的初始化代码。

关键对比：Course 2 只需 `tracing_subscriber::fmt().init()` 一行，本课需要约 50 行样板代码。

## 核心概念

| 概念 | 说明 |
|------|------|
| `TracerProvider` | Span 工厂。负责将 Span 数据通过 Exporter 发送到 Collector |
| `MeterProvider` | Metric 仪器工厂。定时推送度量数据（periodic export） |
| `LoggerProvider` | LogRecord 工厂。管理日志导出器 |
| `Exporter` | 导出器，将 OTel 数据按特定协议发送到后端。本课使用 OTLP over gRPC |
| `Resource` | 资源的静态属性，如 `service.name`、`service.version`，会附加到所有遥测数据上 |
| OTLP endpoint | Collector 的 gRPC 接收地址 `http://localhost:4317` |
| `batch_exporter` | 批处理导出器 — 累积多个 Span 后一次性发送，减少网络开销（Tracing 默认） |
| `periodic_exporter` | 周期性导出器 — 每隔固定间隔推送度量数据（Metrics 默认） |
| `simple_exporter` | 简单导出器 — 每条数据立即发送，适合低流量开发环境（Logs 默认） |

## 依赖说明

```toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
opentelemetry = "0.29"
opentelemetry_sdk = { version = "0.29", features = ["trace", "metrics", "logs"] }
opentelemetry-otlp = { version = "0.29", features = ["grpc-tonic", "logs", "metrics"] }
```

| 依赖 | 说明 |
|------|------|
| `opentelemetry` | OTel API（抽象接口，如 `Tracer` trait） |
| `opentelemetry_sdk` | OTel Rust SDK 实现（具体的 provider、exporter builder）。需显式开启 `trace`/`metrics`/`logs` feature |
| `opentelemetry-otlp` | OTLP 协议的 Rust 实现。`grpc-tonic` 选择 tonic（gRPC）传输；`logs` 和 `metrics` 需显式开启 |

> **关键：** 本课**没有依赖 `tracing` crate**。没有 `tracing::info!()` 宏，没有 `#[instrument]` 宏，也没有 `tracing-subscriber`。日志直接用 `println!`。

## 代码逐段讲解

### 1. 初始化 Tracing Provider（第 33-48 行）

```rust
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
```

逐行解析：

1. **`SpanExporter::builder()`** — 创建 Span 导出器的 builder。OTLP 协议每种信号类型（trace/metric/log）有各自的 exporter 类型。
2. **`.with_tonic()`** — 选择 tonic（gRPC）作为传输层。OTLP 协议规定 gRPC 走端口 4317，HTTP 走端口 4318。
3. **`.with_endpoint("http://localhost:4317")`** — 指向本地运行的 OTel Collector gRPC 接收端口。
4. **`Resource::builder().with_service_name(...)`** — 为所有从此 provider 导出的数据打上 `service.name=learn-tracing-otel` 标签。Collector 和后端按此字段筛选数据。
5. **`.with_batch_exporter(exporter)`** — 使用 **batch 模式**：Span 先排队缓存，达到阈值或定时超时后批量发送。这样在 1000 QPS 的场景下不会每个 Span 都单独建立 gRPC 连接。

> **注意：** 返回的 `SdkTracerProvider` 以 `_` 前缀绑定（第 86 行），因为本课只演示初始化，不使用 Tracer。

### 2. 初始化 Metrics Provider（第 50-65 行）

```rust
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
```

与 Tracing 初始化几乎相同的模式，但有细微差别：

- **`MetricExporter`** 而不是 `SpanExporter` — OTel 设计上每种信号有独立的 exporter 类型，不能混用。
- **`.with_periodic_exporter(exporter)`** 而不是 `batch_exporter` — Metrics 按固定时间间隔（默认 30 秒）推送到 Collector。这是 Push-based metrics 的标准模式。
- `Resource` 配置完全相同 — 保证所有信号数据都带有相同的 `service.name`。

### 3. 初始化 Logs Provider（第 67-82 行）

```rust
fn init_logs() -> SdkLoggerProvider {
    let exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .expect("failed to create log exporter");

    SdkLoggerProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("learn-tracing-otel")
                .build(),
        )
        .with_simple_exporter(exporter)
        .build()
}
```

- **`LogExporter`** — 日志信号的 OTLP 导出器。
- **`.with_simple_exporter(exporter)`** — 使用 **simple 模式**：每条日志立即发送，不做批处理。适合日志量不大、需要即时可见的场景。
- 三类 exporter 的对比：

| Provider | 导出模式 | 原因 |
|----------|---------|------|
| TracerProvider | `batch_exporter` | Span 数量大，批处理减少网络开销 |
| MeterProvider | `periodic_exporter` | Metrics 按固定周期推送，不需要每条即时发送 |
| LoggerProvider | `simple_exporter` | 日志需要实时可见，延迟过大影响排查体验 |

### 4. 主函数 — 调用初始化（第 84-106 行）

```rust
#[tokio::main]
async fn main() {
    let _tracer_provider = init_tracing();
    let _meter_provider = init_metrics();
    let _logger_provider = init_logs();

    let state = Arc::new(AppState {
        counter: AtomicU64::new(1),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .with_state(state);

    println!("Server starting on http://127.0.0.1:3003");
    println!("OTel SDK initialized with OTLP gRPC exporters (traces, metrics, logs)");
    println!("Compare this ~50 lines of boilerplate with Course 2's single `tracing_subscriber::fmt().init()`");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3003").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

关键观察：

- **三个 Provider 的返回值**以 `_` 前缀绑定（`_tracer_provider` 等）— 表示"我持有这个值但不使用"。这很关键：如果不绑定到变量，Provider 会在构造后立即 drop，导致 exporter 的 gRPC 连接被关闭。在 Rust 中，`let _ = expr` 会立即 drop，只有 `let _x = expr` 才会延长生命周期。
- **所有日志都用 `println!`** — 没有 `tracing::info!()`，没有 `log::info!()`。这就是"纯 OTel"的含义：不通过任何 Rust 日志门面，直接用 stdout。
- **端口 3003** — 与 Course 1 的 3001、Course 2 的 3002 不同，便于同时运行对比。
- **`AppState` 只有 `counter`** — 本课未将 Provider 或 Logger/Tracer 放入状态中。

### 5. Handler 函数（第 108-145 行）

```rust
async fn health() -> Json<serde_json::Value> {
    println!("health check called");
    Json(serde_json::json!({ "status": "ok" }))
}

async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    println!("creating task: {}", payload.title);

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let id = state.counter.fetch_add(1, Ordering::SeqCst);
    println!("task created: id={}", id);

    Json(Task {
        id,
        title: payload.title,
        done: false,
    })
}
```

Handler 中的 `println!` 宏只会输出到 stdout，**不会**进入 OTel 管线。这意味着：

- 这些日志字符串 Collector 看不到
- 也没有 Trace/Span 数据被发送
- 没有 Metrics 数据被记录

本课重在"搭台"，后续课程逐步"唱戏"。

## 运行验证

### 1. 启动 OTel Collector

```bash
# 从项目根目录
docker-compose up -d
```

验证 Collector 是否启动：

```bash
docker-compose ps
# 应看到 otel-collector 状态为 Up
```

### 2. 编译并运行

```bash
cd courses/03-otel-otlp
cargo run -p lesson-01-otel-init
```

预期输出：

```
Server starting on http://127.0.0.1:3003
OTel SDK initialized with OTLP gRPC exporters (traces, metrics, logs)
Compare this ~50 lines of boilerplate with Course 2's single `tracing_subscriber::fmt().init()`
```

### 3. 发送请求

```bash
curl http://127.0.0.1:3003/health
# → {"status":"ok"}

curl -X POST http://127.0.0.1:3003/tasks \
  -H 'Content-Type: application/json' \
  -d '{"title":"hello otel"}'
# → {"id":1,"title":"hello otel","done":false}

curl http://127.0.0.1:3003/tasks/42
# → {"id":42,"title":"example task","done":false,"note":"pure otel demo - no tracing crate"}
```

### 4. 验证：Collector 收到数据了吗？

```bash
docker-compose logs otel-collector
```

你会发现 Collector 的日志中**没有**任何 trace/metric/log 数据。这是预期的！本课只初始化了 exporters，但 handler 中用的是 `println!`，没有实际调用 `Span`、`Counter` 或 `Logger` 的 API。数据不会凭空产生。

## 疑难点

- **为什么三个 Provider 要用 `_tracer_provider` 而不是 `_`？** 在 Rust 中 `let _ = expr` 会**立即 drop**返回值，使 Provider 在 `main()` 第一行就析构，导致 exporter 被关闭。`let _x = expr` 则会将值绑定到变量，生命周期延长到 `main()` 结束。这是 Rust 的生命周期细节，但在这个场景下影响很大。

- **OTLP endpoint 为什么是 `http://localhost:4317`？** 4317 是 OTLP over gRPC 的标准化默认端口。HTTP 方式使用 4318。OTel Collector 在 `docker-compose.yml` 中映射了这两个端口到宿主机。

- **每次初始化都要重复写 `Resource` 配置？** 是的。当前 OTel Rust SDK 没有跨信号共享 Resource 的便捷 API。这是 Course 2 的 `tracing-subscriber` 用一行 `init()` 解决掉的样板代码量。

- **`with_batch_exporter`、`with_periodic_exporter`、`with_simple_exporter` 有什么区别？**

| 导出模式 | 行为 | 适用信号 | 延迟 |
|---------|------|---------|------|
| `batch` | 累积多条后批量发送 | Traces | 毫秒级延迟 |
| `periodic` | 每 N 秒定时发送 | Metrics | 秒级延迟 |
| `simple` | 每条立即发送 | Logs | 即时 |

- **为什么 SpanExporter 用 batch 而 MetricExporter 用 periodic？** Span 是"事件驱动"的 — 请求来了就生成 Span，用 batch 可以合并多个 Span 为一次 gRPC 调用。Metrics 是"定期采样的" — 每次请求发出增量变化，periodic exporter 每 30 秒将这些变化打包成一个 MetricsData 消息推送出去，符合 Prometheus 风格的采集模型。

- **AppState 中没有 Tracer/Logger，后面的课怎么用？** 这是故意的。Lesson 01 只演示初始化开销。Lesson 02 会把 Logger 放入 `AppState`；Lesson 03 加入 Metrics 仪器；Lesson 04 最终加入 Tracer。这是一条步步叠加的学习路径。

---

**下一课：** [Lesson 02 — OTel Logs API](../lesson-02-otel-logs/README.md)，学习如何用 OTel Logger API 替代 `println!`，手动构建和发送 `LogRecord`。
