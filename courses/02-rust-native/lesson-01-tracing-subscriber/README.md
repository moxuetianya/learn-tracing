# 第 01 课：tracing-subscriber — 最简单的日志订阅器

## 本课目标

- 用一行代码初始化 `tracing-subscriber`，获得 JSON 格式的结构化日志
- 理解 `tracing::info!()` 宏的用法，包括结构化的键值字段
- 对比 Course 1（CNCF/OpenTelemetry）的日志方案：此处无需 Collector 即可输出本地日志
- 掌握 `RUST_LOG` 环境变量控制日志级别的标准方式

---

## 核心概念

| 术语 | 解释 |
|------|------|
| `tracing-subscriber` | Rust `tracing` 生态系统中最常用的订阅器（Subscriber）实现，负责收集、格式化并输出 span 和 event 数据 |
| `Subscriber` | tracing 框架中的"收集器"角色——所有 `tracing::info!()`、`#[instrument]` 等宏产生的数据最终流入 Subscriber |
| `fmt().json()` | 将日志格式化为 JSON 输出，每条日志包含 `timestamp`、`level`、`target`、`fields`、`span` 等结构化字段 |
| `EnvFilter` | 从环境变量 `RUST_LOG` 读取过滤规则，控制哪些日志级别/target 的事件被记录 |
| 结构化字段 | `tracing::info!(task_id = id, title = %task.title)` 中 `=` 和 `%` 语法添加的键值对，它们被序列化到 JSON 的 `fields` 中 |

---

## 依赖说明

本课新增的依赖（相对于空白 axum 项目）：

```toml
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
```

feature 说明：
- `env-filter` — 启用基于环境变量的日志级别过滤
- `json` — 启用 JSON 格式输出

完整 `Cargo.toml` 见 `courses/02-rust-native/lesson-01-tracing-subscriber/Cargo.toml`。

---

## 代码逐段讲解

### 1. 初始化 tracing-subscriber（一行搞定！）

```rust
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    // ...
}
```

这是 Rust 式的初始化——没有 OpenTelemetry 的样板代码。三行链式调用完成一切：

1. **`tracing_subscriber::fmt()`** — 创建一个基于 `fmt` 格式化的订阅器构建器
2. **`.json()`** — 将输出格式设为 JSON（而非默认的终端友好文本格式）
3. **`.with_env_filter(EnvFilter::from_default_env())`** — 从 `RUST_LOG` 环境变量读取过滤规则，例如 `RUST_LOG=info` 表示只输出 `info` 及以上级别的日志
4. **`.init()`** — 将此订阅器安装为全局默认订阅器

> 对比 Course 1 Lesson 02：那里需要配置 `opentelemetry_sdk::LogExporter`、`LoggerProvider` 才能输出日志到 Collector。而此处，日志直接在 stdout 以 JSON 形式输出。

### 2. 普通日志宏

```rust
tracing::info!("Server running on http://127.0.0.1:3001");
```

这是最简单的用法：字符串字面量作为消息，自动带上 `level = INFO`、`timestamp` 和 `target`（模块路径）。

### 3. 带结构化字段的日志宏

```rust
tracing::info!(task_id = id, title = %task.title, "task created");
```

`tracing` 宏的强大之处在于**结构化字段**。每个日志事件可以携带任意键值对：

| 字段 | 语法 | 含义 |
|------|------|------|
| `task_id = id` | 直接用 `=` 赋值 | `id` 的值被记录到 JSON 的 `fields.task_id`，类型为数字 |
| `title = %task.title` | 用 `%=` 显示（Display） | `task.title` 调用 `.to_string()`，字段值为字符串 |
| `"task created"` | 最后的字符串 | 这是日志**消息**，写入 `fields.message` |

最终输出的 JSON 大致为：

```json
{
  "timestamp": "2025-01-01T00:00:00.000000Z",
  "level": "INFO",
  "target": "lesson_01_tracing_subscriber",
  "fields": {
    "message": "task created",
    "task_id": 2,
    "title": "Buy groceries"
  }
}
```

### 4. 字段值的显示方式

`tracing` 宏中字段值的格式化方式：

| 语法 | 等价于 | 适用场景 |
|------|--------|----------|
| `field = val` | 直接记录原始值 | 数字、bool、实现了 `tracing::Value` trait 的类型 |
| `field = %val` | `format!("{}", val)` — Display | String、实现了 `Display` 的类型 |
| `field = ?val` | `format!("{:?}", val)` — Debug | 调试输出，如 Option、Result |
| `field = %val` | 同上（`%` 语法糖） | 字符串、整型等 |

### 5. 完整的请求处理器

```rust
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let id = state.counter.fetch_add(1, Ordering::SeqCst);
    let task = Task {
        id,
        title: payload.title,
        done: false,
    };
    tracing::info!(task_id = id, title = %task.title, "task created");
    Json(task)
}

async fn get_task(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    tracing::info!(task_id = id, "task fetched");
    Json(serde_json::json!({"id":id,"title":"example task","done":false}))
}
```

每个请求处理器中都有 `tracing::info!()` 调用，记录请求的关键参数（如 `task_id`）和操作结果。

---

## 运行验证

### 启动服务

```bash
# 设置日志级别为 info 并运行
RUST_LOG=info cargo run -p lesson-01-tracing-subscriber
```

### 测试健康检查

```bash
curl http://localhost:3001/health
```

预期响应：
```json
{"status":"ok"}
```

同时终端出 JSON 日志，类似：
```json
{"timestamp":"...","level":"INFO","target":"lesson_01_tracing_subscriber","fields":{"message":"health check requested"}}
```

### 测试创建任务

```bash
curl -X POST http://localhost:3001/tasks \
  -H "Content-Type: application/json" \
  -d '{"title":"Learn tracing"}'
```

预期响应：
```json
{"id":1,"title":"Learn tracing","done":false}
```

终端输出包含结构化字段：
```json
{"timestamp":"...","level":"INFO","target":"lesson_01_tracing_subscriber","fields":{"message":"task created","task_id":1,"title":"Learn tracing"}}
```

### 测试日志级别过滤

```bash
# 只显示 warn 及以上级别的日志
RUST_LOG=warn cargo run -p lesson-01-tracing-subscriber
```

此时 `tracing::info!()` 的输出将被**完全抑制**，只有 `warn` 和 `error` 级别才会出现。

---

## 疑难点

### 1. `init()` 只能调用一次

`tracing_subscriber` 的全局初始化器（通过 `.init()` 设置）在进程生命周期中**只能调用一次**。如果尝试第二次调用 `.init()`，程序会 panic：`"attempted to set a default subscriber when one was already set"`。

### 2. `RUST_LOG` 未设置时没有输出

如果忘记设置 `RUST_LOG` 环境变量，`EnvFilter::from_default_env()` 默认返回一个**空过滤器**（不匹配任何日志），导致所有日志被丢弃。解决方案：

- 始终设置 `RUST_LOG=info` 再启动
- 或在代码中显式设置默认值：`EnvFilter::new("info")`

### 3. JSON 格式中 `fields` 的顺序

JSON 对象的字段顺序在 Rust 中由 `serde_json` 决定，不保证与代码中 `tracing::info!()` 的字段书写顺序一致。不要依赖字段顺序进行日志解析。

### 4. 与 `log` crate 的关系

`tracing` 和标准 `log` crate 是不同的生态。`tracing` 是 async-aware 的，支持 span 概念。如果项目中已有使用 `log` crate 的依赖（如 `reqwest`），可以添加 `tracing-log` 桥接 crate 将 `log` 事件转发到 `tracing`。

### 5. Course 1 vs 本课对比

| 方面 | Course 1 Lesson 02 (CNCF) | 本课 (Native) |
|------|--------------------------|--------------|
| 日志初始化 | `OtlpLogExporter` + `LoggerProvider` + `opentelemetry-appender-tracing` | `tracing_subscriber::fmt().json().init()` |
| 日志输出目标 | OTLP → Collector → 后端 | stdout（JSON 格式） |
| 需要 Collector | 是 | 否 |
| 代码行数 | ~30+ 行样板代码 | 3 行 |
| 适用场景 | 生产环境，集中式日志收集 | 开发调试、简单部署 |
