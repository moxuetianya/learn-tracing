# Lesson 04: OTel Traces API — 手动 Span 生命周期管理

> **这是 Course 3 最重要的一课。** 本课揭示了 `#[instrument]` 宏背后真正发生的事情：Span 的创建、属性设置、状态标记和手动关闭。理解这些底层的 Span 生命周期，你才能真正掌握分布式追踪。

## 本课目标

学习使用 OpenTelemetry Traces API 手动管理 Span 的完整生命周期：

1. 通过 `span_builder()` 创建 Span
2. 设置 `SpanKind` 标识请求角色（Server/Client/Internal）
3. 在业务逻辑前后添加属性和状态
4. 必须手动调用 `span.end()` 结束 Span
5. 在 Jaeger 中查看手动创建的 Span

## 核心概念

| 概念 | 说明 |
|------|------|
| `SdkTracer` | OTel SDK 的具体 Tracer 类型。通过 `provider.tracer("scope-name")` 创建 |
| `SpanBuilder` | Span 的构造器模式。设置名称、kind、属性等 → `.start(tracer)` 触发实际创建 |
| `SpanKind` | Span 的角色枚举：`Server`（接收请求）、`Client`（发出请求）、`Internal`（内部操作）、`Producer`、`Consumer` |
| `Status` | Span 的结果状态：`Status::Ok` 或 `Status::error(description)` |
| `Span` trait | OTel API 中 Span 的抽象特征。`set_attribute()`、`set_status()`、`end()` 等方法均来自此 trait |
| `Tracer` trait | 提供 `span_builder()` 和 `start()` 方法 |
| `TracerProvider` trait | 提供 `tracer()` 方法创建 Tracer 实例 |
| Span 生命周期 | `span_builder()` → `.start()` → 业务逻辑 → `.set_attribute()` → `.set_status()` → `.end()` |
| `KeyValue` | 属性键值对，将业务上下文附加到 Span 上 |

### SpanKind 枚举

```rust
pub enum SpanKind {
    Server,   // "我是服务端，收到了外部请求"
    Client,   // "我是客户端，向外发起了请求"
    Internal, // "这是内部操作，不涉及 RPC"
    Producer, // "我向消息队列发送了一条消息"
    Consumer, // "我从消息队列消费了一条消息"
}
```

`SpanKind` 不改变 Span 的功能，但会改变后端 UI 的显示方式和延迟计算逻辑。例如在 Jaeger 中，`Server` Span 的耗时是从接收到发送响应的时间，而 `Client` Span 的耗时是从发出请求到收到响应的往返时间。

## 依赖说明

与 Lesson 03 完全相同，无需新增依赖。新增的 trait import：

```rust
use opentelemetry::trace::{
    Span as _, SpanKind, Status, Tracer as _, TracerProvider as _,
};
use opentelemetry_sdk::trace::{SdkTracer, SdkTracerProvider};
```

| import | 用途 |
|--------|------|
| `Span as _` | 引入 `Span` trait 方法：`set_attribute()`、`set_status()`、`end()` |
| `SpanKind` | Span 角色枚举 |
| `Status` | `Status::Ok` 和 `Status::error()` |
| `Tracer as _` | 引入 `Tracer` trait 方法：`span_builder()`、`start()` |
| `TracerProvider as _` | 引入 `TracerProvider` trait 方法：`tracer()` |
| `SdkTracer` | 具体的 Tracer 类型，存入 `AppState` |

## 代码逐段讲解

### 1. `AppState` 新增 `tracer` 字段（第 38-44 行）

```rust
struct AppState {
    counter: AtomicU64,
    logger: SdkLogger,
    tracer: SdkTracer,
    request_counter: Counter<u64>,
    request_duration: Histogram<f64>,
}
```

`AppState` 现在同时包含全部三种信号的工具：`logger`（Logs）、`tracer`（Traces）、`request_counter` + `request_duration`（Metrics）。

### 2. `init_tracing()` 创建 Tracer（第 46-65 行）

```rust
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
```

与 Lesson 01 的关键区别：函数现在**返回 Tracer 实例**。

- **`provider.tracer("learn-tracing")`** — 从 Provider 创建一个命名 Tracer。参数 `"learn-tracing"` 是 instrumentation scope name，在 Jaeger 中显示为 Span 的组件标识。同一 Provider 下可创建多个 Tracer（如 `db` 和 `http`），方便按模块组织 Span。

- `Tracer` 存储 Tracer 的**内部克隆**（通过 `Arc`），所以放入 `AppState` 是安全的。

### 3. `main()` 接收 Tracer（第 119-147 行）

```rust
let (_tracer_provider, tracer) = init_tracing();
let state = Arc::new(AppState {
    counter: AtomicU64::new(1),
    logger,
    tracer,
    request_counter,
    request_duration,
});
```

`tracer` 被移入 `AppState`，供所有 handler 使用。`_tracer_provider` 保持生命周期。

### 4. Span 完整生命周期 — `create_task`（第 153-204 行）

```rust
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

    // ... metrics and logs ...

    span.set_status(Status::Ok);
    span.end();

    Json(Task {
        id,
        title: payload.title,
        done: false,
    })
}
```

这是本课的核心代码。让我们逐步分解 Span 的完整生命周期：

#### 第 4.1 步：创建 Span — `span_builder()...start()`

```rust
let mut span = state
    .tracer
    .span_builder("POST /tasks")
    .with_kind(SpanKind::Server)
    .start(&state.tracer);
```

这是**三阶段链式调用**：

**阶段 1：`state.tracer.span_builder("POST /tasks")`**

创建一个 `SpanBuilder`。参数 `"POST /tasks"` 是 Span 的名称，会在 Jaeger 的 Span 列表中显示为操作名。命名约定是 `HTTP_METHOD /路径`。

**阶段 2：`.with_kind(SpanKind::Server)`**

设置 Span 的 kind 为 `Server`。这告诉 Jaeger"这个 Span 代表服务端处理一个入站请求"。Jaeger 会据此：
- 在 Span 详情中显示 `server` badge
- 以入站请求的视角组织跟踪树（server span 通常是根 span）

**阶段 3：`.start(&state.tracer)`**

将 builder 转换为**实际的 Span**。`start()` 方法：
- 在内部创建一个新的 Span ID 和（如果是根 Span）Trace ID
- 将 Span 推入当前线程的隐式 Context 中（如果使用 Context API）
- 记录 `start_time`（Span 的开始时间戳）
- 返回一个 `SdkSpan`，绑定为 `Span` trait 对象

**可变性：** `span` 声明为 `mut`，因为后续的 `set_attribute()`、`set_status()` 需要可变引用。

#### 第 4.2 步：设置初始属性

```rust
span.set_attribute(KeyValue::new("task.title", payload.title.clone()));
```

在 Span 创建后、业务逻辑执行前，立即添加**请求上下文属性**。这里将请求中的 `title` 参数记录到 Span 上，这样在 Jaeger 中就能看到"这个请求要创建什么任务"。

#### 第 4.3 步：业务逻辑执行

```rust
tokio::time::sleep(std::time::Duration::from_millis(20)).await;
let id = state.counter.fetch_add(1, Ordering::SeqCst);
```

与之前课程相同的业务逻辑。`sleep` 模拟数据库操作。

#### 第 4.4 步：设置业务结果属性

```rust
span.set_attribute(KeyValue::new("task.id", id as i64));
```

在业务逻辑完成后，将**结果上下文**（task ID）记录到 Span 上。这样如果这个请求后续出错了，你能在 Span 属性中看到涉及的是哪个 task。

#### 第 4.5 步：记录 Metrics 和 Logs（第 173-194 行）

```rust
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
```

Metrics 和 Logs 与 Span 共存于同一个 handler 中。注意一个微妙之处：

- **LogRecord 目前没有关联到 Span** — 当前代码中，LogRecord 的 `trace_id` 和 `span_id` 字段为默认值（空）。在 Lesson 05 和实际生产中，需要显式将当前 Span 的 context 注入到 LogRecord 中才能实现日志-链路关联。本课专注于 Span 本身的操作。

#### 第 4.6 步：标记 Span 状态和结束 Span

```rust
span.set_status(Status::Ok);
span.end();
```

这两行是 Span 生命周期中**最关键的部分**：

**`span.set_status(Status::Ok)`：**

将 Span 标记为成功完成。`Status` 有两个变体：

```rust
Status::Ok                                    // 成功
Status::error("database connection timeout".into())  // 失败 + 错误描述
```

`Status::error` 包含一个描述字符串，会在 Jaeger 的 Span 详情中以红色标签显示。在没有异常的系统中（如在 catch 块中，或像 Rust 的 `Result` 分支），显式标记 `Status::error` 能提供错误分类。

如果既不调用 `set_status(Ok)` 也不调用 `set_status(error(...))`，Span 的状态默认为 `Unset`，在 Jaeger 中显示为灰色无需状态标记的 Span。

**`span.end()` 是最关键的调用：**

`end()` 方法做以下几件事：
1. 记录 Span 的结束时间戳
2. 将 Span 标记为已完成
3. 触发 Span 的导出（传递给 `SpanProcessor`，最终进入 `batch_exporter`）

**如果忘记调用 `span.end()`：**
- Span 在 drop 时会被**强制关闭**，但不会正常导出
- Span 的 `end_time` 是 drop 的时间点，不是业务逻辑完成的时间点
- 在 Jaeger 中，Span 可能标记为"未完成"或缺失
- 这是初学手动 Span 管理时最常见的 bug

### 5. `get_task` Handler — 同样的生命周期（第 206-256 行）

```rust
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

    // ... metrics and logs ...

    span.set_status(Status::Ok);
    span.end();

    Json(serde_json::json!({
        "id": id,
        "title": "example task",
        "done": false,
        "note": "manual spans - pure otel traces"
    }))
}
```

与 `create_task` 完全对称的生命周期，区别在于：

- Span 名称为 `"GET /tasks/:id"` — 含路径参数模式
- 属性包含 `task.id`（从 URL 提取）和 `duration_secs`（业务操作耗时）

### 6. Span 生命周期全景图

```
  ┌─────────────────────────────────────────────────────────┐
  │                  SPAN LIFECYCLE                         │
  │                                                         │
  │  ① span_builder("name")                                 │
  │     .with_kind(Server/Client/...)                       │
  │     .start(&tracer)                                     │
  │     │                                                   │
  │     ▼                                                   │
  │  ② set_attribute(...)  ← 请求上下文                      │
  │     │                                                   │
  │     ▼                                                   │
  │  ③ [业务逻辑执行]                                        │
  │     │                                                   │
  │     ▼                                                   │
  │  ④ set_attribute(...)  ← 结果上下文                      │
  │     │                                                   │
  │     ▼                                                   │
  │  ⑤ set_status(Ok/error) ← 标记成功/失败                  │
  │     │                                                   │
  │     ▼                                                   │
  │  ⑥ end()  ← 关闭 Span，触发导出                          │
  │                                                         │
  │  ⚠ 忘记 end() = Span 被 drop 时强制关闭，导出可能缺失     │
  └─────────────────────────────────────────────────────────┘
```

### 7. 与 Course 1/2 的 `#[instrument]` 对比

Course 1 和 2 中 Span 管理只需一行宏：

```rust
// Course 1/2 — 一行自动 Span
#[instrument(name = "POST /tasks", skip(state))]
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    tracing::Span::current().record("task.title", &payload.title);
    // ...
    // Span 自动 start 和 end
}
```

本课的等价代码（Lesson 04 — 手动 Span）：

```rust
// Course 3 — 8 行手动 Span
let mut span = state.tracer
    .span_builder("POST /tasks")
    .with_kind(SpanKind::Server)
    .start(&state.tracer);
span.set_attribute(KeyValue::new("task.title", payload.title.clone()));
// ...
span.set_attribute(KeyValue::new("task.id", id as i64));
span.set_status(Status::Ok);
span.end();
```

两者的区别：

| 维度 | `#[instrument]` 宏（Course 1/2） | 手动 Span（Course 3） |
|------|----------------------------------|----------------------|
| 代码量 | 1 行 | 8+ 行（创建+属性+状态+end） |
| 函数参数自动记录 | ✅ 自动 | ❌ 手动逐个 set_attribute |
| Span 开关 | 自动 — 函数进入时 start，离开时 end | 手动 — 必须在合适的位置 start 和 end |
| 错误状态 | 自动 — 函数返回 `Err` 时 Span 标为 error | 手动 — 必须在 catch/error 分支设置 Status::error |
| SpanKind 设置 | 无（默认 Internal） | 可设置为 Server、Client 等 |
| 灵活控制 | 有限 | 完全控制 — 可条件性记录、添加中间属性、嵌套子 Span |
| async 友好 | ✅ （自动处理 .await 点） | ✅ 需要手动管理 Span 在 .await 点的存在 |

### 8. 何时使用手动 Span？

手动 Span 适用于以下场景：

1. **非函数边界的操作** — 例如，一个循环中的每次迭代、一个流式处理中的每个批次
2. **条件性 Span** — 只在特定条件下（如 `if expensive_operation`）才记录 Span
3. **自定义 SpanKind** — 设置 Client Span 以追踪外部调用
4. **嵌套 Span 的精细控制** — 在父 Span 下创建名为"数据库查询"、"缓存访问"的子 Span
5. **非 tracing 生态集成** — 使用纯 OTel SDK 而不依赖 `tracing` crate 的宏系统

## 运行验证

### 1. 启动基础设施

```bash
docker-compose up -d
```

确认 Jaeger 已启动：

```bash
docker-compose ps
# 应看到 otel-collector 和 jaeger 状态为 Up
```

### 2. 运行本课代码

```bash
cd courses/03-otel-otlp
cargo run -p lesson-04-otel-traces
```

### 3. 发送请求

```bash
# 创建多个任务
curl -X POST http://127.0.0.1:3003/tasks \
  -H 'Content-Type: application/json' \
  -d '{"title":"learn manual spans"}'

curl -X POST http://127.0.0.1:3003/tasks \
  -H 'Content-Type: application/json' \
  -d '{"title":"another task"}'

# 查询任务
curl http://127.0.0.1:3003/tasks/1
curl http://127.0.0.1:3003/tasks/2
```

### 4. 在 Jaeger 中查看 Traces

打开浏览器访问 **http://localhost:16686**

操作步骤：

1. **Service** 下拉菜单选择 `learn-tracing-otel`
2. 点击 **Find Traces** 按钮
3. 你应该看到 4 条 Trace（每个请求一条）

点击一条 Trace 查看详情：

```
learn-tracing-otel: POST /tasks  (Server)  ─  ~20ms  ✓ OK
    └─ Tags:
        ├─ service.name = learn-tracing-otel
        ├─ task.title = learn manual spans
        └─ task.id = 1
```

```
learn-tracing-otel: GET /tasks/:id  (Server)  ─  ~5ms  ✓ OK
    └─ Tags:
        ├─ service.name = learn-tracing-otel
        ├─ task.id = 1
        └─ duration_secs = 0.005
```

关键观察：

- 每条 Span 显示为 `Server` 类型（badge）
- `Span name` 对应 `span_builder()` 的第一个参数（`"POST /tasks"` 或 `"GET /tasks/:id"`）
- `Tags` 下显示通过 `set_attribute()` 添加的属性
- `Duration` 显示 Span 的实际耗时（从 `start()` 到 `end()` 的时间）
- `Status` 显示 `Ok`（绿色图标）

### 5. 查看 Collector 中的 Span 数据

```bash
docker-compose logs otel-collector | grep -E "Span #|Name:"
```

Collector 的 debug exporter 会以文本格式显示 Span 详情，包括 Name、Kind、Status 和 Attributes。

## 疑难点

- **忘记调用 `span.end()` 会怎样？**

  最常见的后果是 Span 在 Jaeger 中不出现。原因：
  1. `batch_exporter` 只在 Span 结束后才会处理它
  2. Span 在 drop 时可能会处理，但不能依赖这种行为
  3. 如果 Span drop 时 Provider 已经关闭，Span 就丢失了

  **调试技巧：** 如果 Jaeger 中看不到 Span，先在 handler 返回前加一条 `println!("span ended")`，确认 `end()` 确实被执行。

- **`span_builder().start()` vs 直接 `tracer.start()`：**
  
  `span_builder()` 是推荐方式，因为：
  - 可以在 start 前配置 SpanKind、attributes、links
  - 代码可读性更好
  - `tracer.start("name")` 是简写，但不常用

- **Span 的名称应该包含动态值吗？**

  OTel 语义约定建议 Span 名称应是**低基数的**。例如：
  - ✅ `POST /tasks`（路由模式）
  - ✅ `GET /tasks/:id`（路径参数模式）
  - ❌ `POST /tasks/12345`（包含具体 ID — 高基数）

  高基数 Span 名称会让 Jaeger 的 Span 列表混乱，且无法按名称聚合。

- **为什么属性中有 `task.id`，Span 名称却是 `POST /tasks` 而不是 `POST /tasks/:id`？**
  
  `task.id` 是操作的具体对象，应作为属性（tag）而非名称的一部分。名称标识的是操作类型（创建），而非操作对象。

- **本课 Span 是根 Span 还是子 Span？**

  当前代码中，每个 handler 创建的 Span 都是**根 Span**（无父 Span）。这是因为我们没有从请求头中提取 trace context。在生产应用中，你需要：
  1. 从 HTTP 请求头中提取 `traceparent` 和 `tracestate`
  2. 用提取的 context 作为 `span_builder()` 的父 context
  3. 这样创建出的才是子 Span，能串联起整体调用链

  这是分布式追踪中最关键的部分 — **Context Propagation**，本课为了聚焦 Span 生命周期而简化了。

- **`span.set_attribute()` 是不是可以多次为同一个 key 设置值？**

  可以多次调用，后设置的值会覆盖先前的值。但不能在 `end()` 之后设置 — `end()` 后的 Span 是只读的。

- **为什么 `span` 需要是 `mut`？**

  OTel 的 Span 内部修改需要 `&mut self`。`set_attribute()`、`set_status()`、`end()` 都需要可变引用。这是与 `tracing` crate 中 `Span::record()` 在 `&self` 上调用的一个重要区别。

- **`end()` 之后 Span 还能被读取吗？**

  不能通过 API 读取。`end()` 会将 Span 标记为只读并触发导出。之后任何写入操作都会是 no-op 或被忽略。

---

**下一课：** [Lesson 05 — Collector Pipeline](../lesson-05-collector-pipeline/README.md)，深入理解 OTel Collector 的内部管线：Receivers → Processors → Exporters。
