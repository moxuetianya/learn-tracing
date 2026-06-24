# Lesson 04: 链路追踪 — Span + Jaeger

## 本课目标

在 `lesson-03-metrics` 的基础上，新增 **OpenTelemetry Traces** 管线。使用 `#[tracing::instrument]` 宏和 `tower_http::TraceLayer` 创建 **Span**，通过 OTLP/gRPC 导出到 Collector，再由 Collector 转发到 **Jaeger** 进行可视化分析。

## 核心概念

| 概念 | 说明 |
|------|------|
| Span | 链路追踪的最小单位，代表一个操作的时间跨度。有 start/end 时间、属性、父子关系 |
| `SdkTracerProvider` | Tracer Provider，创建和管理 Tracer，持有 exporter |
| `with_batch_exporter` | 批量导出器，将 Span 缓冲后批量发送（相比 simple exporter 更高效） |
| `TraceContextPropagator` | W3C Trace Context 传播器，负责在服务间传递 `traceparent` header |
| `set_text_map_propagator` | 全局注册传播器，使 HTTP 客户端/服务端框架自动注入和提取 trace context |
| `tracing_opentelemetry::layer()` | 桥接层，将 tracing crate 的 span/event 映射为 OTel Span |
| `#[tracing::instrument]` | 过程宏，自动为 async fn 创建父 span |
| `TraceLayer::new_for_http()` | tower-http 的中间件层，为每个 HTTP 请求自动创建 root span |

## 依赖说明

本课相比 `lesson-03-metrics` 新增了以下依赖：

```toml
[dependencies]
opentelemetry_sdk = { version = "0.29", features = ["logs", "metrics", "trace"] }
    # trace: 启用 SdkTracerProvider 和 propagation 模块

tracing-opentelemetry = "0.30"
    # 桥接层：将 tracing crate 的 Span → OpenTelemetry Span

tower-http = { version = "0.6", features = ["trace"] }
    # tower-http TraceLayer: HTTP 层自动创建 span（含 method、uri、status_code 属性）
```

`opentelemetry-otlp` 依赖中不再需要新增 feature——traces 的 SpanExporter 内置在 `opentelemetry-otlp` crate 中，无需显式 feature flag。

## 代码逐段讲解

### 1. W3C Trace Context 传播器

```rust
fn init_observability() -> SdkTracerProvider {
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
```

- `TraceContextPropagator::new()` — 实现 W3C Trace Context 标准（`traceparent` header）
- `set_text_map_propagator(...)` — 注册到全局。此后：
  - `TraceLayer` 从 HTTP 请求头中**提取** `traceparent`，如果上游已有 trace context 则加入现有 trace
  - 你的 HTTP 客户端在发请求时自动**注入** `traceparent` header，实现跨服务链路串联

> **如果某请求不带 `traceparent`？** `TraceLayer` 会生成一个新的 trace ID（root span），因此**不依赖上游**也能独立工作。

### 2. SpanExporter

```rust
    let span_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .expect("failed to create span exporter");
```

与前面的 `LogExporter`、`MetricExporter` 模式一致——构建器 + tonic + endpoint。

### 3. SdkTracerProvider + Batch Exporter

```rust
    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("learn-tracing-cncf")
                .build(),
        )
        .with_batch_exporter(span_exporter)
        .build();
```

- `.with_batch_exporter(span_exporter)` — **批量导出器**，与 Logs 的 `with_simple_exporter` 不同：
  - Span 会先累积在内存缓冲区中
  - 达到一定数量（默认 512）或一定时间间隔（默认 5s）后批量发送
  - 对性能友好的原因：高 QPS 下每秒可能有数千个 span，逐个发送开销太大
- `SdkTracerProvider` 返回后需要保留引用（见下文 `_tracer_provider`）

### 4. 全局 MeterProvider（前课迁移）

```rust
    opentelemetry::global::set_meter_provider(meter_provider);
```

与 lesson-03 不同，本课将 `meter_provider` 注册到全局：
- 前课通过 `init_observability` 返回 `meter_provider` 然后在 main 中引用
- 本课改为全局注册后，可通过 `opentelemetry::global::meter_provider()` 在任意位置获取 meter，无需显式传递引用
- 这使代码更简洁，尤其是 handler 之外的位置也可能需要获取 meter

### 5. tracing-opentelemetry 桥接层

```rust
    let tracer = tracer_provider.tracer("learn-tracing");
    let otel_trace_layer = tracing_opentelemetry::layer().with_tracer(tracer);
```

- `tracer_provider.tracer("learn-tracing")` — 创建一个名为 `learn-tracing` 的 tracer 实例
- `tracing_opentelemetry::layer()` — 创建一个 tracing subscriber layer
- `.with_tracer(tracer)` — 将 OTel tracer 绑定到该 layer

这个 layer 的工作机制：
1. 监听 tracing crate 的所有 span 创建/进入/退出事件
2. 将每个 tracing span 转换为一个 OTel Span
3. Span 名称、属性、父子关系自动映射

### 6. 最终 Layer 组合（4 层）

```rust
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(otel_log_layer)
        .with(otel_trace_layer)
        .init();

    tracer_provider
}
```

| 层 | 类型 | 作用 |
|----|------|------|
| 1. `fmt::layer().json()` | formatting | stdout JSON 日志 |
| 2. `EnvFilter::new("info")` | filtering | 日志级别过滤 |
| 3. `otel_log_layer` | log bridge | tracing event → OTel LogRecord |
| 4. `otel_trace_layer` | trace bridge | tracing span → OTel Span |

> **注意：** `tracer_provider` 被返回给 main 并绑定到 `_tracer_provider`。由于使用了 `with_batch_exporter`，tracer_provider 在 main 结束时必须显式 shutdown，否则缓冲区中尚未发送的 span 会丢失。在 drop 时 `SdkTracerProvider` 会自动调用 `shutdown()`。

### 7. 全局获取 Meter

```rust
#[tokio::main]
async fn main() {
    let _tracer_provider = init_observability();

    let meter = opentelemetry::global::meter_provider().meter("learn-tracing");
```

因为 `meter_provider` 已注册到全局，此处通过 `opentelemetry::global::meter_provider()` 直接获取，不再需要从函数返回值传递。

### 8. TraceLayer 中间件

```rust
    let app = Router::new()
        .route("/health", get(health))
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .layer(TraceLayer::new_for_http())
        .with_state(state);
```

`TraceLayer::new_for_http()` 是 tower-http 提供的中间件：

- 为每个 HTTP 请求创建一个 **root span**
- span 名称格式为 `HTTP {method} {route}`（如 `HTTP POST /tasks`）
- 自动包含属性：`http.method`、`http.url`、`http.status_code`、`http.route`
- 自动从请求头提取 W3C trace context（通过全局 propagator）
- 自动将当前 span context 注入到响应头中

> **`TraceLayer` 在 middleware 管线中的位置：** `.layer()` 在 `.with_state()` 之前调用，确保 TraceLayer 包裹所有 handler，即使 handler 内部 panic 也会捕获 status_code。

### 9. `#[instrument]` 宏

```rust
#[instrument]
async fn health() -> Json<serde_json::Value> {
    info!("health check called");
    Json(serde_json::json!({ "status": "ok" }))
}
```

- `#[instrument]` 将整个 async fn 包裹在一个 span 中
- span 名称默认是函数名：`health`
- 函数的参数自动作为 span 属性（如上例无参数无额外属性）

```rust
#[instrument(skip(state), fields(task_title = %payload.title))]
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
```

- `.fields(task_title = %payload.title)` — 自定义字段，将 `payload.title` 作为 span 属性 `task_title`
- `skip(state)` — **跳过** `state` 参数，不将其序列化为 span 属性
  - `Arc<AppState>` 可能很大或包含不可序列化的内容（如 `AtomicU64`），记录它没意义
  - 跳过后的字段不会出现在 span 上

```rust
#[instrument(skip(state), fields(task_id = id))]
async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
```

- `.fields(task_id = id)` — 将路径参数 `id` 作为 span 属性
- `skip(state)` 理由同上

### 10. 完整 handler 流程

```rust
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
```

Jaeger 中的 Span 层级结构：

```
HTTP POST /tasks                ← TraceLayer 创建 (root span)
├── health                      ← 无，create_task 是 POST 的 handler
├── create_task                 ← #[instrument] 创建 (child span)
│   ├── info!("creating task")  ← event 附着在 create_task span 上
│   ├── sleep(20ms)             ← 等待时间包含在 span 时长中
│   ├── counter.add             ← 指标记录
│   ├── request_duration.record ← 指标记录
│   └── info!("task created")  ← event 附着在 create_task span 上
```

对于 `GET /tasks/{id}` 请求：

```
HTTP GET /tasks/{id}            ← TraceLayer 创建 (root span)
└── get_task                    ← #[instrument] 创建 (child span)
    ├── info!("fetching task")  ← event
    └── ...                     ← metrics + response
```

## 运行验证

### 1. 启动后端

```bash
cd ../.. && podman-compose up -d
```

### 2. 运行本课

```bash
cargo run -p lesson-04-traces
```

### 3. 发送请求

```bash
# 一个 POST 请求
curl -s -X POST http://127.0.0.1:3001/tasks \
  -H 'Content-Type: application/json' \
  -d '{"title":"learn tracing"}'

# 几个 GET 请求
curl -s http://127.0.0.1:3001/tasks/1
curl -s http://127.0.0.1:3001/tasks/2
```

### 4. 查看 Jaeger

打开浏览器访问 **http://localhost:16686**：

1. 在 Service 下拉菜单选择 `learn-tracing-cncf`
2. 点击 **Find Traces**
3. 可以看到每个请求对应一条 trace
4. 点击某条 trace 展开详情：
   - **Timeline** 视图：显示每个 span 的时间条（`HTTP POST /tasks` 包裹 `create_task`）
   - **Span Details**：显示 span 的属性（`task_title`、`task_id` 等）
   - **Logs**：显示 span 内的事件（`info!` 产生的 event + 属性）

### 5. 理解 Trace Timeline

```
Timeline:
│  HTTP POST /tasks     ████████████████████████████  ~21ms
│  └─ create_task       ██████████████████████████    ~21ms
│     └─ info event     ▌
│     └─ sleep          ██████████████████████        20ms
│     └─ info event                                ▌

│  HTTP GET /tasks/{id} ████████████████████████████  ~6ms
│  └─ get_task          ██████████████████████████    ~6ms
│     └─ info event     ▌
│     └─ sleep          ██████████████████████        5ms
```

## 疑难点

- **Batch exporter 的时效性：** Span 不会立即出现在 Jaeger 中。等待 5 秒（默认 flush 间隔）后或在服务关闭（tracer_provider 被 drop）时才推送。如果需要立即看到，可以在代码末尾 sleep 几秒再退出。

- **`#[instrument]` 和 `TraceLayer` 的关系：**
  - `TraceLayer` 创建 **HTTP 层 root span**
  - `#[instrument]` 创建 **handler 函数 child span**
  - 两者不冲突——tracing-opentelemetry 桥接层自动识别父 span（通过 tracing 的 span context），构建正确的父子关系
  - 离开 `#[instrument]` 仍然能工作，但 handler 内部逻辑在 Jaeger 中只是一个扁平的事件列表，无 span 层级

- **`skip(state)` 的必要性：** 如果省略，`state` 会被序列化为 span 属性。`Arc<AppState>` 可能包含不可序列化的字段（如 `Counter<u64>`），导致编译错误或运行时警告。

- **W3C trace context header：** 当请求进入时，TraceLayer 检查是否存在 `traceparent` header。如果存在（如从另一个服务转发而来），当前服务成为该 trace 的一个中间 span。如果不存在，生成新 trace。

- **Jaeger 中看不到日志？** 本课中 `info!` 语句产生的日志会同时作为 LogRecord（通过 `otel_log_layer`）和 Span Event（通过 `otel_trace_layer`）。在 Jaeger UI 中，Span 详情页的 **Logs** 标签页显示的是附着在 span 上的 events，而非独立的 LogRecord。

- **为什么 `set_meter_provider` 是全局的？** 全局注册简化了从多个位置获取 Meter 的方式。`tracer_provider` 没有全局注册是因为需要确保 shutdown 时的精确控制。
