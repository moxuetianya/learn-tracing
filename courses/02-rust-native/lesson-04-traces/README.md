# 第 04 课：Traces — 将 tracing Span 桥接到 OpenTelemetry

## 本课目标

- 使用 `tracing-opentelemetry` 桥接 crate，将 `tracing` crate 的 Span 自动导出为 OTel Span
- 配置 OTLP gRPC Exporter，将 Span 批量发送到 OpenTelemetry Collector
- 理解 Subscriber 组合模式：JSON fmt layer + EnvFilter + OTel layer 并存
- 在 Jaeger 中可视化完整的请求调用链
- 对比 Course 1 手写 OTel Span 的方式：此处 `#[instrument]` 一行宏即可

---

## 核心概念

| 术语 | 解释 |
|------|------|
| `tracing-opentelemetry` | 桥接 crate，将 `tracing` 的 Subscriber Layer 转换为 OpenTelemetry 的 Span Exporter |
| **Layer** | `tracing-subscriber` 中的插件化组件。每个 Layer 处理一种数据流向（如 fmt layer 输出 stdout，otel layer 输出到 Collector） |
| **Subscriber 组合** | `tracing_subscriber::registry()` 创建一个支持多层 Layer 的 Subscriber 注册表，通过 `.with(layer)` 叠加多个 Layer |
| `SdkTracerProvider` | OpenTelemetry SDK 的核心组件，管理 Span 的创建和导出 |
| **Batch Exporter** | 将 Span 批量发送到 Collector（而非每个 Span 单独发），减少网络开销 |
| **OTLP gRPC** | OpenTelemetry Protocol over gRPC，标准化的遥测数据传输协议 |
| `tracer_provider.shutdown()` | 优雅关闭，确保所有待发送的 Span 都刷新完毕后再退出进程 |

---

## 依赖说明

本课新增依赖：

```toml
tracing-opentelemetry = "0.30"
opentelemetry = "0.29"
opentelemetry_sdk = "0.29"
opentelemetry-otlp = { version = "0.29", features = ["grpc-tonic"] }
tower-http = { version = "0.6", features = ["trace"] }
```

| Crate | 用途 |
|-------|------|
| `tracing-opentelemetry` | 核心桥接：将 tracing Span → OTel Span |
| `opentelemetry` | OTel 的 Trace API trait（如 `TracerProvider`） |
| `opentelemetry_sdk` | OTel 的 Trace SDK 实现（`SdkTracerProvider`、`Resource`） |
| `opentelemetry-otlp` | OTLP 协议的 Rust 实现，通过 tonic（gRPC）传输 |
| `tower-http` | axum 中间件，自动为每个 HTTP 请求创建 span |

---

## 代码逐段讲解

### 1. 导入声明

```rust
use opentelemetry::trace::TracerProvider as _;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
```

关键 trait：
- `TracerProvider as _` — 导入 `TracerProvider::tracer()` 方法（但不导入 trait 名，避免命名冲突）
- `SubscriberExt as _` — 导入 `.with()` 方法，用于向 Registry 添加 Layer
- `SubscriberInitExt as _` — 导入 `.init()` 方法，用于安装 Subscriber
- `WithExportConfig` — 导入 `.with_endpoint()` 等构建方法

### 2. 创建 OTLP Span Exporter

```rust
let span_exporter = opentelemetry_otlp::SpanExporter::builder()
    .with_tonic()
    .with_endpoint("http://localhost:4317")
    .build()?;
```

逐行解释：
- `SpanExporter::builder()` — 创建导出器构建器
- `.with_tonic()` — 选择 gRPC 传输（tonic 是 Rust 的 gRPC 实现）
- `.with_endpoint("http://localhost:4317")` — OTLP gRPC 接收地址（Collector 的默认端口是 4317）
- `.build()?` — 完成构建，`?` 传播错误

> OTLP 有两个标准端口：`4317`（gRPC）和 `4318`（HTTP）。此处使用 gRPC。

### 3. 创建 SdkTracerProvider

```rust
let tracer_provider = SdkTracerProvider::builder()
    .with_batch_exporter(span_exporter)
    .with_resource(
        opentelemetry_sdk::Resource::builder()
            .with_service_name("learn-tracing-native")
            .build(),
    )
    .build();
```

解释：
- `with_batch_exporter(span_exporter)` — 将 Span 批量发送到 Collector。batch 模式下 Span 不会立即发送，而是积攒一定数量或时间后统一发送
- `with_resource(...)` — 设置服务属性。`service.name = "learn-tracing-native"` 是 OTel 语义约定中的**必须字段**，用于在 Jaeger 中按服务名筛选
- `.build()` — 构建 `SdkTracerProvider`

### 4. 创建 OTel Bridge Layer

```rust
let tracer = tracer_provider.tracer("learn-tracing");
let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
```

- `tracer_provider.tracer("learn-tracing")` — 创建一个名为 `"learn-tracing"` 的 Tracer 实例
- `tracing_opentelemetry::layer()` — 创建桥接 Layer（实现了 tracing 的 `Layer` trait）
- `.with_tracer(tracer)` — 将 Tracer 注入 Layer。此后，所有通过 `#[instrument]` 创建的 Span 都会被此 Tracer 转换为 OTel Span 并最终导出

### 5. Subscriber 组合（关键！）

```rust
tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer().json())
    .with(tracing_subscriber::EnvFilter::new("info"))
    .with(otel_layer)
    .init();
```

这是本课最重要的部分。与之前三课不同，这里使用了 `registry()` 而非 `fmt().init()`：

| 之前 | 本课 |
|------|------|
| `tracing_subscriber::fmt().json().with_env_filter(...).init()` | `tracing_subscriber::registry().with(fmt_layer).with(env_filter).with(otel_layer).init()` |

区别：
- **`fmt().init()`** — 创建单一功能的 Subscriber，只能做一件事（格式化输出）。不支持组合
- **`registry().with(...).init()`** — 创建支持多层 Layer 的注册表。每一层 `.with(layer)` 添加了一层处理逻辑

本课的 Layer 栈：

```
Registry (核心 Subscriber)
├── fmt::layer()   → 将日志格式化为 JSON 输出到 stdout
├── EnvFilter      → 过滤日志级别
└── otel_layer     → 将 Span 桥接到 OTel 并导出到 Collector
```

所有三个 Layer 同时工作、互不影响。

### 6. 优雅关闭

```rust
tracer_provider.shutdown()?;
Ok(())
```

关键点：
- `axum::serve(...).await.unwrap()` 正常情况下永不返回（除非出错或收到信号）
- 一旦 serve 返回（例如 Ctrl+C），`tracer_provider.shutdown()` 确保所有积攒在 Batch Exporter 缓冲区中的 Span 都发送完毕再退出
- **忘记调用 `shutdown()`** 是最常见的 bug——导致最后一批 Span 丢失

> 注意 `main()` 的返回类型改为 `Result<(), Box<dyn std::error::Error + Send + Sync>>`，以支持 `?` 操作符。

### 7. 请求处理器与 Lesson 02 完全相同

```rust
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
    // ...
}
```

**无需修改任何业务代码**。`#[tracing::instrument]` 的 Span 由 otel_layer 自动捕获并导出。这正是桥接模式的威力：

- 用 Rust 习惯的方式标注代码（`#[instrument]`、`tracing::info!()`）
- 通过组合 Subscriber Layer，将同一份 Span 数据同时发送到 stdout 和 Collector

### 8. 数据流向图

```
┌─────────────────────────────────────────────────────┐
│  Application Code                                   │
│  ┌─────────────────────────────────────────────┐    │
│  │ #[tracing::instrument]                      │    │
│  │ async fn create_task() {                    │    │
│  │     tracing::info!("creating task");        │    │
│  │ }                                          │    │
│  └────────────────┬────────────────────────────┘    │
│                   │ Span + Event                     │
│                   ▼                                  │
│  ┌─────────────────────────────────────────────┐    │
│  │ tracing_subscriber::registry()              │    │
│  │   ├── fmt::layer(json)  →  stdout (JSON)    │    │
│  │   ├── EnvFilter         →  过滤            │    │
│  │   └── otel_layer        →  OTLP gRPC        │    │
│  └─────────────────────────────────────────────┘    │
└───────────────────┬─────────────────────────────────┘
                    │ OTLP gRPC (port 4317)
                    ▼
          ┌─────────────────────┐
          │ OTel Collector      │
          │ (或直接到 Jaeger)    │
          └─────────┬───────────┘
                    ▼
          ┌─────────────────────┐
          │ Jaeger UI           │
          │ localhost:16686     │
          └─────────────────────┘
```

---

## 运行验证

### 前置条件：启动 Collector / Jaeger

方式一 —— 启动 Jaeger all-in-one（含内置 Collector）：
```bash
docker run -d --name jaeger \
  -p 16686:16686 \
  -p 4317:4317 \
  jaegertracing/all-in-one:latest
```

方式二 —— 启动 OTel Collector + Jaeger（docker-compose）：
```yaml
services:
  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686"
      - "4317:4317"
```

### 启动应用

```bash
RUST_LOG=info cargo run -p lesson-04-traces
```

### 发送请求生成 trace

```bash
curl http://localhost:3001/health
curl -X POST http://localhost:3001/tasks \
  -H "Content-Type: application/json" \
  -d '{"title":"Learn OTel bridge"}'
curl http://localhost:3001/tasks/1
```

### 查看 stdout 日志

终端输出同时包含 JSON 格式日志：

```json
{"timestamp":"...","level":"INFO","target":"lesson_04_traces","fields":{"message":"Server running on http://127.0.0.1:3001"}}
{"timestamp":"...","level":"INFO","target":"lesson_04_traces","span":{"name":"health"},"fields":{"message":"health check requested"}}
{"timestamp":"...","level":"INFO","target":"lesson_04_traces","span":{"name":"create_task"},"fields":{"message":"creating task","title":"Learn OTel bridge"}}
{"timestamp":"...","level":"INFO","target":"lesson_04_traces","span":{"name":"create_task"},"fields":{"message":"task created","task_id":1}}
```

### 在 Jaeger 中查看 trace

1. 打开 http://localhost:16686
2. 在 Service 下拉菜单中选择 `learn-tracing-native`
3. 点击 "Find Traces"
4. 看到 `create_task` 的 trace，点击查看详情：

```
Service: learn-tracing-native
Trace: ┌─────────────────────────────────────────┐
       │ create_task (21.3ms)                    │
       │ ├── creating task                        │
       │ ├── sleep(20ms)                          │
       │ └── task created                         │
       └─────────────────────────────────────────┘
```

每个 Span 包含：
- 开始时间和持续时间
- Span 标签（如 `task_id`、`title`）
- 子 Span 和日志事件

### 验证 shutdown 是否工作

按 Ctrl+C 停止应用，观察最后没有错误信息，确认 `tracer_provider.shutdown()` 执行成功。

---

## 疑难点

### 1. Subscriber 组合的注意点

使用 `registry()` 时，**不能**再调用 `.json()` 或 `.init()` 在单个 Layer 上构建。这些是 `fmt()` 专用方法。正确用法：

```rust
// ✅ 正确 — 用 registry() 组合
tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer().json()) // 注意: fmt::layer() 不是 fmt()
    .with(otel_layer)
    .init();

// ❌ 错误 — 不能用 fmt().init() 组合
tracing_subscriber::fmt().json().with(otel_layer).init(); // 编译错误
```

### 2. 忘记 shutdown 导致数据丢失

如果不在 `main()` 末尾调用 `tracer_provider.shutdown()`，Batch Exporter 缓冲区中未发送的 Span 在进程退出时直接丢弃，导致 Jaeger 中看不到最后几个 Span。这非常难调试，因为本地测试时往往能看到（退出够快），但生产环境中可能丢失。

### 3. 与 Course 1 的区别

| 方面 | Course 1 (CNCF) | 本课 (Native) |
|------|----------------|---------------|
| Span 创建 | 手动调用 `tracer.start("name")` | `#[tracing::instrument]` 宏自动创建 |
| 代码改动量 | 每个函数需要获取并传入 tracer | 一行 `#[instrument]` |
| Span 属性 | 手动 `.set_attribute()` | `tracing::info!(key = val)` 自动提取 |
| 与日志集成 | 需额外配置关联 | `tracing::info!()` 在 Span 内自动关联 |
| 样板代码 | 多 | 少 |

### 4. gRPC 连接失败的处理

如果 Collector 未启动，OTLP gRPC 连接会失败，但**应用不会 panic**。Batch Exporter 会重试连接。如果 Collector 一直不可用，Span 会被丢弃（取决于内部的缓冲区容量）。

### 5. 环境变量配置 Endpoint

生产环境中，Collector 地址不应硬编码：

```rust
let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
    .unwrap_or_else(|_| "http://localhost:4317".to_string());

let span_exporter = opentelemetry_otlp::SpanExporter::builder()
    .with_tonic()
    .with_endpoint(endpoint)
    .build()?;
```

OTel SDK 已有自动读取 `OTEL_EXPORTER_OTLP_ENDPOINT` 等环境变量的机制，可通过 `opentelemetry-otlp` 的 `TonicExporterBuilder` 启用。
