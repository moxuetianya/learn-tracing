# Lesson 01: 环境搭建 — 裸 axum 服务

## 本课目标

搭建一个最基础的 axum HTTP 服务，**暂不接入任何可观测性工具**，作为后续课程的起点。

## 核心概念

| 概念 | 说明 |
|------|------|
| axum | Rust 异步 HTTP 框架，基于 tokio + tower |
| Router | 路由注册器，将 HTTP 方法和路径绑定到 handler |
| State | axum 的共享状态机制，通过 `Arc` 在多线程间共享 |
| AtomicU64 | 原子计数器，无需 Mutex 即可安全更新 |

## 依赖说明

```toml
[dependencies]
axum = "0.8"                          # HTTP 框架
tokio = { version = "1", features = ["full"] }  # 异步运行时
serde = { version = "1", features = ["derive"] }  # 序列化
serde_json = "1"                      # JSON 支持
```

## 代码讲解

### 1. 数据模型

```rust
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
```

定义了两个结构体：
- `Task` — 任务的完整表示，实现 `Serialize`（输出 JSON）和 `Deserialize`
- `CreateTask` — 创建任务时的请求体，只需 `title` 字段

### 2. 共享状态

```rust
struct AppState {
    counter: AtomicU64,
}
```

`AppState` 通过 `Arc` 在所有 handler 间共享。`AtomicU64` 保证计数器在并发请求下安全递增，无需 Mutex。

### 3. 路由注册

```rust
let app = Router::new()
    .route("/health", get(health))
    .route("/tasks", post(create_task))
    .route("/tasks/{id}", get(get_task))
    .with_state(state);
```

- `.route(path, method(handler))` 绑定路由
- `{id}` 是路径参数，会被提取到 handler 的 `Path` 参数中
- `.with_state()` 将共享状态注入到 router 中

### 4. Handler 函数

```rust
async fn create_task(
    State(state): State<Arc<AppState>>,  // 提取共享状态
    Json(payload): Json<CreateTask>,       // 提取 JSON 请求体
) -> Json<Task> {                          // 返回 JSON 响应
    let id = state.counter.fetch_add(1, Ordering::SeqCst);
    Json(Task { id, title: payload.title, done: false })
}
```

- `State` extractor 获取共享状态
- `Json` extractor 自动反序列化请求体，也用于包装响应
- `fetch_add` 原子地递增并返回旧值

## 运行验证

```bash
# 启动服务
cargo run -p lesson-01-setup

# 另一个终端测试
curl http://127.0.0.1:3001/health
# → {"status":"ok"}

curl -X POST http://127.0.0.1:3001/tasks -H 'Content-Type: application/json' -d '{"title":"learn tracing"}'
# → {"id":1,"title":"learn tracing","done":false}

curl http://127.0.0.1:3001/tasks/42
# → {"id":42,"title":"example task","done":false,"note":"static demo data"}
```

## 疑难点

- **为什么用 `Arc` 而不是 `Mutex`？** 因为 `AtomicU64` 本身是线程安全的，不需要互斥锁。如果状态包含多个字段需要原子更新，才需要 `Mutex`。
- **`127.0.0.1:3001` vs `0.0.0.0:3001`？** 本课程用 `127.0.0.1` 仅本地访问。如果要从 Docker 容器访问，需要 `0.0.0.0`。
- **handler 必须是 async 吗？** axum 要求 handler 返回 Future，所以必须是 `async fn`。
