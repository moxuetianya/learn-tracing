# Lesson 02: OTel Logs API — 手动构建 LogRecord

## 本课目标

学习如何直接使用 OpenTelemetry Logs API 构建和发送结构化日志记录。用 `SdkLogger` 的 `create_log_record()` / `set_body()` / `set_severity_number()` / `add_attribute()` / `emit()` 方法链替代 Course 2 的一行 `tracing::info!()` 宏。

你将理解 OTel LogRecord 的内部结构，以及 logger 是如何将日志与 trace context 关联的。

## 核心概念

| 概念 | 说明 |
|------|------|
| `SdkLogger` | OTel SDK 的具体 Logger 类型，持有对 `LoggerProvider` 的引用和 instrumentation scope |
| `LogRecord` | 日志记录的 OTel 数据结构，包含 body、severity、attributes、trace_id、span_id 等字段 |
| `Severity` | OTel 定义的日志严重级别枚举（`Trace`~`Fatal`），比 Rust log crate 的级别更丰富 |
| `Logger::create_log_record()` | 创建一个全新的 LogRecord 构造器，用于逐步填充字段 |
| LogRecord builder 的 API | `set_body()`、`set_severity_number()`、`set_severity_text()`、`add_attribute()` |
| `Logger::emit()` | 将构建完成的 LogRecord 实例通过 Logger 发送（委托给内部的 LoggerProvider → Exporter） |
| Trait imports | `use opentelemetry::logs::{Logger as _, LoggerProvider as _, LogRecord as _}` — 每个 trait 方法都需要对应的 `use` 声明 |

## 依赖说明

与 Lesson 01 相同，无需新增依赖。只是代码中新增了日志相关的 trait imports。

关键 import 新增：

```rust
use opentelemetry::logs::{Logger as _, LoggerProvider as _, LogRecord as _, Severity};
use opentelemetry_sdk::logs::{SdkLogger, SdkLoggerProvider};
```

| import | 用途 |
|--------|------|
| `Logger as _` | 引入 `Logger` trait 的方法（如 `create_log_record()`、`emit()`） |
| `LoggerProvider as _` | 引入 `LoggerProvider` trait 的方法（如 `logger()`） |
| `LogRecord as _` | 引入 LogRecord builder 的方法（如 `set_body()`、`add_attribute()`） |
| `Severity` | OTel 的严重级别枚举 |

> `as _` 语法表示"只引入 trait 方法，不 import 类型名称到当前作用域"。这是 Rust 中常见的 trait-only import 写法。

## 代码逐段讲解

### 1. `AppState` 新增 `logger` 字段（第 30-33 行）

```rust
struct AppState {
    counter: AtomicU64,
    logger: SdkLogger,
}
```

与 Lesson 01 相比，`AppState` 增加了 `logger: SdkLogger` 字段。`SdkLogger` 是具体的 OTel Logger 类型，实现了 `Clone` trait（内部是 `Arc` 包装），可以安全地在 handler 间共享。

### 2. `init_logs()` 返回 Logger（第 69-88 行）

```rust
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
```

关键变化：函数现在返回 `(SdkLoggerProvider, SdkLogger)` 元组，而不仅是 Provider。

- **`provider.logger("learn-tracing")`** — 从 Provider 创建一个**有名字的 Logger**。参数 `"learn-tracing"` 是 instrumentation scope name（插桩作用域名称），会在 Collector 中显示为每个 LogRecord 的 `InstrumentationScope.Name`。同一个 Provider 可以创建多个不同名字的 Logger（例如按模块划分）。

- `SdkLogger` 是对 Provider 内部的 `Arc` 克隆，因此可被安全地传递到 `AppState` 中。

### 3. `main()` 接收并储存 Logger（第 90-116 行）

```rust
#[tokio::main]
async fn main() {
    let _tracer_provider = init_tracing();
    let _meter_provider = init_metrics();
    let (logger_provider, logger) = init_logs();

    let state = Arc::new(AppState {
        counter: AtomicU64::new(1),
        logger,
    });

    // ...
}
```

`logger` 被移动到 `AppState` 中。`logger_provider` 仍用 `_` 前缀绑定，确保其生命周期持续（防止 Logger 内部的弱引用失效）。

### 4. 启动日志 — 第一个手工 LogRecord（第 107-112 行）

```rust
let mut startup = logger_provider.logger("learn-tracing").create_log_record();
startup.set_body("Server starting on http://127.0.0.1:3003".into());
startup.set_severity_number(Severity::Info);
startup.add_attribute("course", "03-otel-otlp");
startup.add_attribute("lesson", "02-logs");
logger_provider.logger("learn-tracing").emit(startup);
```

这是 OTel 中最原生的日志发送方式。每一步都是手动的：

1. **`provider.logger("learn-tracing")`** — 获取一个 Logger 实例（这里直接从 provider 获取，不使用 state 中的 logger）
2. **`.create_log_record()`** — 创建一个空的 LogRecord builder
3. **`.set_body("...")`** — 设置日志正文。`.into()` 将 `&str` 转换为 `opentelemetry::logs::AnyValue`
4. **`.set_severity_number(Severity::Info)`** — 设置严重级别为 Info
5. **`.add_attribute("course", "03-otel-otlp")`** — 添加结构化属性（key-value 对）
6. **`.emit(startup)`** — 实际发送 LogRecord

对比 Course 2 的等价代码：

```rust
// Course 2 — tracing crate
info!(course = "03-otel-otlp", lesson = "02-logs", "Server starting on http://127.0.0.1:3000");
```

一行 vs 六行。但 OTel 的方式给了你精细控制：你可以动态决定 body 的类型（String、整型、浮点数）、设置自定义 trace_id/span_id 进行日志-链路关联、或者条件性地决定是否发送（不调用 `emit()`）。

### 5. Handler 中的日志记录（第 122-162 行）

**`create_task` handler（第 122-142 行）：**

```rust
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let id = state.counter.fetch_add(1, Ordering::SeqCst);

    let mut record = state.logger.create_log_record();
    record.set_body(format!("task created: id={}", id).into());
    record.set_severity_number(Severity::Info);
    record.add_attribute("task.id", id as i64);
    record.add_attribute("task.title", payload.title.clone());
    state.logger.emit(record);

    Json(Task {
        id,
        title: payload.title,
        done: false,
    })
}
```

逐行分析：

- **`state.logger.create_log_record()`** — 从 AppState 中获取 Logger，创建空白日志记录
- **`record.set_body(format!("task created: id={}", id).into())`** — body 可以是任何 `Into<AnyValue>` 的类型。这里用了格式化字符串
- **`record.set_severity_number(Severity::Info)`** — 与启动日志相同的安全级别
- **`record.add_attribute("task.id", id as i64)`** — 添加**结构化属性** `task.id`。注意 `id` 是 `u64`，需要转为 `i64`（因为 OTel 的 `AnyValue` 接受 `i64` 而非 `u64`）
- **`record.add_attribute("task.title", payload.title.clone())`** — 添加字符串属性 OTel 属性是强类型的：String 和 i64 在 Collector 中是不同的数据类型
- **`state.logger.emit(record)`** — 发射日志记录

与 Course 2 的对比：

```rust
// Course 2 — 一行搞定
info!(task_id = id, title = %payload.title, "task created");
```

**`get_task` handler（第 144-162 行）：**

```rust
async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    let mut record = state.logger.create_log_record();
    record.set_body(format!("task fetched: id={}", id).into());
    record.set_severity_number(Severity::Info);
    record.add_attribute("task.id", id as i64);
    state.logger.emit(record);

    Json(serde_json::json!({
        "id": id,
        "title": "example task",
        "done": false,
        "note": "otel logger api - no tracing crate macros"
    }))
}
```

相同的 LogRecord 构建模式，只记录了 `task.id` 属性。

### 6. OTel 的 Severity 级别

```rust
pub enum Severity {
    Trace = 1,
    Trace2 = 2,
    Trace3 = 3,
    Trace4 = 4,
    Debug = 5,
    Debug2 = 6,
    Debug3 = 7,
    Debug4 = 8,
    Info = 9,
    Info2 = 10,
    Info3 = 11,
    Info4 = 12,
    Warn = 13,
    Warn2 = 14,
    Warn3 = 15,
    Warn4 = 16,
    Error = 17,
    Error2 = 18,
    Error3 = 19,
    Error4 = 20,
    Fatal = 21,
    Fatal2 = 22,
    Fatal3 = 23,
    Fatal4 = 24,
}
```

OTel 的严重级别粒度极细（24 级），兼容各种日志系统的级别映射。实践中多数场景只用 `Info`、`Warn`、`Error`。

## LogRecord 的完整数据结构

一个 OTel LogRecord 包含以下字段（本课使用了其中部分）：

| 字段 | 本课设置方式 | 说明 |
|------|-------------|------|
| `body` | `.set_body(...)` | 日志正文，可以是任意 `AnyValue` |
| `severity_number` | `.set_severity_number(...)` | 严重级别（数字枚举） |
| `severity_text` | `.set_severity_text(...)` | 可为空的级别文本描述 |
| `attributes` | `.add_attribute(...)` | 结构化键值对数组 |
| `trace_id` | 未设置（默认空） | 关联的 trace id |
| `span_id` | 未设置（默认空） | 关联的 span id |
| `timestamp` | 自动填充 | LogRecord 创建的时间戳 |
| `observed_timestamp` | 自动填充 | LogRecord 被 Collector 观测的时间戳 |

> **trace_id/span_id 的重要性：** OTel 的 LogRecord 内建了对 Trace Context 的支持。当你同时发送 Trace 和 Log 时，日志中自动携带当前 Span 的 trace_id 和 span_id，这让 Collector 和后端（如 Loki）能够将日志与链路关联起来。这是 `println!` 和简单日志系统无法做到的。本课尚未使用 Trace，这两个字段为空。

## 运行验证

### 1. 启动 OTel Collector

```bash
docker-compose up -d
```

### 2. 运行本课代码

```bash
cd courses/03-otel-otlp
cargo run -p lesson-02-otel-logs
```

### 3. 发送请求

```bash
# Health check
curl http://127.0.0.1:3003/health
# → {"status":"ok"}

# 创建任务（触发日志）
curl -X POST http://127.0.0.1:3003/tasks \
  -H 'Content-Type: application/json' \
  -d '{"title":"learn otel logs"}'
# → {"id":1,"title":"learn otel logs","done":false}

# 查询任务
curl http://127.0.0.1:3003/tasks/1
# → {"id":1,"title":"example task","done":false,...}
```

### 4. 查看 Collector 中的日志

```bash
docker-compose logs otel-collector
```

预期在 Collector 的 debug 输出中看到 LogRecord：

```
LogRecord #0
ObservedTimestamp: 2026-06-24T...
Body: Str(task created: id=1)
SeverityNumber: Info(9)
Attributes:
    -> task.id: Int(1)
    -> task.title: Str(learn otel logs)
Resource:
    -> service.name: Str(learn-tracing-otel)
InstrumentationScope learn-tracing v
```

你还会看到启动时的日志 `Body: Str(Server starting on http://127.0.0.1:3003)`，其 Attributes 包含 `course` 和 `lesson` 字段。

## 疑难点

- **为什么要区分 `SdkLogger` 和 `SdkLoggerProvider`？** `SdkLoggerProvider` 是整个日志管线的顶层控制器，负责管理 exporter、Resource 配置。`SdkLogger` 是具体的日志记录器，绑定到一个 instrumentation scope name。这种分层允许同一个 Provider 下创建多个 Logger（例如 `db` Logger 和 `http` Logger）。

- **`add_attribute` 的值类型有限制吗？** 是的。`AnyValue` 接受以下 Rust 类型：`i64`、`f64`、`String`、`bool`、`Vec<AnyValue>`（数组）、以及 `HashMap<String, AnyValue>`（嵌套 map）。注意 `u64` 不在列表中，必须转为 `i64`。

- **Logger 的 `name`（`"learn-tracing"`）在 Collector 中如何体现？** 它显示为 LogRecord 的 `InstrumentationScope.Name` 字段。多个 Logger 按 name 区分，便于按模块筛选日志。

- **是不是每条日志都要写 `create_log_record()` + 5 个 setter？** 本课为了教学故意展示最原始的方式。生产代码通常会封装一个辅助函数：

```rust
fn info(logger: &SdkLogger, body: &str, attrs: &[KeyValue]) {
    let mut record = logger.create_log_record();
    record.set_body(body.into());
    record.set_severity_number(Severity::Info);
    for attr in attrs {
        record.add_attribute(attr.0.clone(), attr.1.clone());
    }
    logger.emit(record);
}
```

但更推荐的方式是用 `tracing` crate + `opentelemetry-appender-tracing` 桥接，如 Course 1 和 2 所做的。这也是为什么"纯 OTel"主要用于需要精确控制 SDK 行为的场景。

- **`println!` 还在用吗？** 本课的 handler 中已经**完全移除**了 `println!`，所有日志都走 OTel Logger API。但 `println!` 也可以和 Logger API 共存 — Logger API 发送到 Collector（远程可观测），`println!` 输出到 stdout（本地调试）。

---

**下一课：** [Lesson 03 — OTel Metrics API](../lesson-03-otel-metrics/README.md)，学习创建 Counter 和 Histogram 仪器，记录度量数据。
