# 第 02 课：Span — 自动创建调用链的 Span

## 本课目标

- 使用 `#[tracing::instrument]` 宏为异步函数自动创建 Span
- 理解 Span 的层级关系：父 Span 和子 Span
- 掌握 `skip` 参数排除大型/敏感字段，以及 `fields` 参数添加自定义字段
- 理解 JSON 日志输出中的 `span` 字段——每个日志行自动继承当前 Span 的 ID

---

## 核心概念

| 术语 | 解释 |
|------|------|
| **Span** | 代表一次操作的生命周期（如一个 HTTP 请求处理），有开始和结束时间，可以嵌套形成父子关系 |
| `#[tracing::instrument]` | 过程宏，自动为被标注的函数创建 Span。函数名作为 Span 名，参数自动作为 Span 字段 |
| `skip` | `#[instrument]` 的参数，指定哪些函数参数**不**被记录到 Span 字段中。用于跳过大型结构体（如数据库连接池）或敏感数据 |
| `fields` | `#[instrument]` 的参数，手动添加额外的键值对到 Span 中 |
| Span ID | 每个 Span 都有唯一 ID，日志行中的 `span` 字段包含当前 Span 的名称和 ID |
| **父 Span / 子 Span** | 当函数 A 调用函数 B，且两者都有 `#[instrument]`，B 的 Span 自动成为 A 的 Span 的子节点 |

---

## 依赖说明

本课未新增依赖，沿用 Lesson 01 的 `tracing` + `tracing-subscriber`。

`#[tracing::instrument]` 宏由 `tracing` crate 直接提供，无需额外的 feature flag。

---

## 代码逐段讲解

### 1. 无参数的 `#[tracing::instrument]`

```rust
#[tracing::instrument]
async fn health() -> Json<serde_json::Value> {
    tracing::info!("health check requested");
    Json(serde_json::json!({ "status": "ok" }))
}
```

这是最简单的用法：
- `health` 函数被调用时，自动创建一个名为 `health` 的 Span
- 函数返回时，Span 自动结束
- 函数内部的所有 `tracing::info!()` 调用都会**自动继承**这个 Span 的上下文

对应的 JSON 日志输出：

```json
{
  "timestamp": "...",
  "level": "INFO",
  "target": "lesson_02_span",
  "span": { "name": "health" },
  "fields": { "message": "health check requested" }
}
```

注意多了 `"span"` 字段：
- `name` — Span 的名称（即函数名）
- 实际输出中还包含 `trace_id` 和嵌套的 span 信息

### 2. `skip` — 跳过不需要记录的参数

```rust
#[tracing::instrument(skip(state))]
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    // ...
}
```

`skip(state)` 告诉宏：**不要**将 `state` 参数记录到 Span 的字段中。原因：

- `state` 是 `Arc<AppState>`，包含 `AtomicU64`，序列化它没有意义
- 状态对象通常很大，记录到 Span 只会产生噪音
- `payload` 没有被 skip，因此它的字段（如 `title`）会自动出现在 Span 中

> **最佳实践**：始终 skip 数据库连接池、共享状态、大型结构体等参数。

### 3. `skip` 带下划线前缀的参数

```rust
#[tracing::instrument(skip(_state))]
async fn get_task(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    // ...
}
```

注意这里的 `_state` 带有下划线前缀。Rust 约定：带 `_` 前缀的变量名表示"有意不使用"。在 `#[instrument]` 中，即使参数名以 `_` 开头，仍需要在 `skip` 中显式声明，否则宏会尝试记录它。

### 4. Span 层级关系

当请求到达时，Span 的层级结构如下：

```
root (隐式)
└── create_task [span]          ← #[tracing::instrument] 自动创建
    ├── tracing::info!("creating task")
    ├── sleep(20ms)             ← tokio::time::sleep
    └── tracing::info!("task created")
```

如果 `create_task` 内部调用了另一个也有 `#[instrument]` 的函数（如 `save_to_db`），层级会是：

```
root
└── create_task
    └── save_to_db             ← 子 Span
        └── tracing::info!("...")
```

JSON 日志中每条子 Span 内部的日志行会同时包含**父 Span 和当前 Span** 的信息：

```json
{
  "timestamp": "...",
  "level": "INFO",
  "span": {
    "name": "save_to_db",
    "parent": { "name": "create_task" }
  },
  "fields": { "message": "row inserted" }
}
```

### 5. 对比 Lesson 01 的代码差异

**Lesson 01 的 `create_task`**：
```rust
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    // ...
    tracing::info!(task_id = id, title = %task.title, "task created");
    Json(task)
}
```

**Lesson 02 的 `create_task`**：
```rust
#[tracing::instrument(skip(state))]
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    tracing::info!(title = %payload.title, "creating task");
    // ...
    tracing::info!(task_id = id, "task created");
    Json(task)
}
```

区别：
- 添加了 `#[tracing::instrument(skip(state))]` — 自动创建 Span
- 增加了一条 `tracing::info!("creating task")` — 记录操作开始
- `payload.title` 字段自动进入 Span，无需在每条日志中手动添加

### 6. 完整的 Span 输出示例

运行 `RUST_LOG=info` 并发送一个 POST 请求后，终端输出大致如下：

```json
{"timestamp":"...","level":"INFO","target":"lesson_02_span","span":{"name":"create_task"},"fields":{"message":"creating task","title":"Buy milk"}}
{"timestamp":"...","level":"INFO","target":"lesson_02_span","span":{"name":"create_task"},"fields":{"message":"task created","task_id":1}}
```

两条日志都携带了 `"span":{"name":"create_task"}`，表明它们属于同一个 Span。

---

## 运行验证

### 启动服务

```bash
RUST_LOG=info cargo run -p lesson-02-span
```

### 测试创建任务

```bash
curl -X POST http://localhost:3001/tasks \
  -H "Content-Type: application/json" \
  -d '{"title":"Learn spans"}'
```

观察终端输出中的 `"span"` 字段：

```json
{"timestamp":"...","level":"INFO","target":"lesson_02_span","span":{"name":"create_task"},"fields":{"message":"creating task","title":"Learn spans"}}
{"timestamp":"...","level":"INFO","target":"lesson_02_span","span":{"name":"create_task"},"fields":{"message":"task created","task_id":1}}
```

### 对比 Lesson 01

切换到 Lesson 01 运行，观察缺少 `"span"` 字段：
```bash
RUST_LOG=info cargo run -p lesson-01-tracing-subscriber
```

日志输出中**没有** `"span"` 顶层字段，因为未使用 `#[instrument]`。

---

## 疑难点

### 1. `#[tracing::instrument]` 对异步函数的要求

`#[instrument]` 要求函数返回类型实现 `Future`（即必须是 `async fn`）。如果用在同步函数上，需要确保函数名不与 `tracing` 的内部生成冲突。

### 2. `skip` 中字段名写错不会编译报错

`skip(arg_name)` 中的 `arg_name` 对应的是**函数参数名**。如果参数名写错（比如写成 `skip(sttae)`），宏会静默忽略——它不会报编译错误，只是不会跳过实际参数。需要仔细核对参数名。

### 3. Span 不会自动包含 `task.id`

`#[instrument]` 会自动记录**函数参数**到 Span 字段，但不会记录函数内部创建的变量。例如 `let task = Task { id, ... }` 中的 `task.id` 不会自动进入 Span——必须在 `tracing::info!()` 中手动添加。

### 4. Span 的生命周期

`#[instrument]` 创建的 Span 在函数**返回时**自动结束。如果函数是 `async`，Span 在 Future 完成时结束（而非函数被调用时）。这意味着 Span 的持续时间精确覆盖了包含 `.await` 点的整个异步操作。

### 5. 如何添加自定义字段到 Span

除了 `#[instrument]` 自动记录参数外，还可以手动添加字段：

```rust
#[tracing::instrument(fields(task_title = %payload.title))]
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    // payload.title 现在作为 Span 级别的字段存在
}
```

这对于**在每个日志行中不需要重复记录**、但希望作为 Span 属性的字段非常有用。

### 6. 访问当前 Span

```rust
use tracing::Span;

let current_span = Span::current();
current_span.record("custom_field", "some_value");
```

在函数内部可以通过 `Span::current()` 获取当前活跃的 Span 并动态添加字段。
