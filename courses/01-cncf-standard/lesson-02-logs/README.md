# Lesson 02: 结构化日志 — OpenTelemetry Logs

## 本课目标

在 `lesson-01-setup` 的基础上，接入 **OpenTelemetry Logs**，将 `tracing` 宏产生的结构化日志通过 **OTLP/gRPC** 协议导出到 OpenTelemetry Collector，最终在 Collector 的 debug 输出中查看。

## 核心概念

| 概念 | 说明 |
|------|------|
| `LogExporter` | 日志导出器，负责将日志数据发送到后端。本课使用 OTLP via gRPC |
| `SdkLoggerProvider` | 日志 Provider 工厂，配置 Resource（如 `service.name`）并持有 exporter |
| `OpenTelemetryTracingBridge` | 将 tracing crate 的事件（`info!`/`warn!`/`error!`）桥接到 OpenTelemetry Logs 管线 |
| `tracing_subscriber::registry()` | Layer 组合模式：注册中心负责在一个 `Subscriber` 上叠加多层行为 |
| JSON fmt layer | `tracing-subscriber` 的格式化层，将日志以 JSON 格式输出到 stdout |
| `EnvFilter` | 基于环境变量 `RUST_LOG` 控制日志级别，默认 `info` |
| Structured fields | `tracing` 的 `%field_name`（Display）和 `?field_name`（Debug）语法，内联字段自动变成日志属性 |

## 依赖说明

本课相比于 `lesson-01-setup` 新增了以下依赖：

```toml
[dependencies]
tracing = "0.1"                                                  # Rust 日志门面：提供 info!/warn!/error! 宏
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
    # env-filter: 通过 RUST_LOG 环境变量控制日志级别
    # json: 将日志输出为 JSON 格式
opentelemetry = "0.29"                                           # OTel API
opentelemetry_sdk = { version = "0.29", features = ["logs"] }
    # logs: 启用 SdkLoggerProvider
opentelemetry-otlp = { version = "0.29", features = ["logs", "grpc-tonic"] }
    # logs: 启用 LogExporter
    # grpc-tonic: 通过 tonic 实现 OTLP over gRPC（即 HTTP/2 + Protobuf）
opentelemetry-appender-tracing = "0.29"
    # 桥接层：把 tracing crate 的事件 → OpenTelemetry LogRecord
```

## 代码逐段讲解

### 1. 初始化日志管线

```rust
fn init_logs() {
    let log_exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .expect("failed to create log exporter");
```

`LogExporter::builder()` 使用 builder 模式构建日志导出器：

- `.with_tonic()` — 选择 gRPC 传输层（基于 tonic crate，HTTP/2 + Protobuf）
- `.with_endpoint("http://localhost:4317")` — 指向本地 OTel Collector 的 gRPC 端口（4317）
- `.build()` — 完成构建

### 2. 构建 LoggerProvider

```rust
    let logger_provider = opentelemetry_sdk::logs::SdkLoggerProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("learn-tracing-cncf")
                .build(),
        )
        .with_simple_exporter(log_exporter)
        .build();
```

- `Resource::builder().with_service_name("learn-tracing-cncf")` — 将服务名写入所有日志条目的 `resource` 字段，下游可按服务名筛选
- `.with_simple_exporter(log_exporter)` — 使用 **简单导出器**（每条日志立即发送，不批处理）
- `SdkLoggerProvider` 是整个日志管线的顶层入口

> **注意：** `with_simple_exporter` 适合开发和低流量场景。生产环境应用 `with_batch_exporter` 实现批量导出以降低网络开销。

### 3. 桥接层

```rust
    let otel_layer =
        opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider);
```

`OpenTelemetryTracingBridge` 是一个 `tracing-subscriber::Layer` 的实现：

- 它订阅 tracing 事件（`info!`、`warn!`、`error!`），将每条事件转换为 OTel 的 `LogRecord`
- `LogRecord` 的属性自动从 `tracing` 的结构化字段映射而来
- `logger_provider` 拥有 `logger_provider` 的所有权，不会被桥接层 clone

### 4. Layer 组合模式

```rust
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(otel_layer)
        .init();
}
```

`tracing_subscriber::registry()` 创建了一个注册中心，然后 `.with()` 叠加三层：

| 层顺序 | Layer | 作用 |
|--------|-------|------|
| 1 | `fmt::layer().json()` | 将所有 span/event 以 JSON 格式输出到 stdout |
| 2 | `EnvFilter::new("info")` | 过滤低于 `info` 的事件。可通过 `RUST_LOG=debug` 覆盖 |
| 3 | `OpenTelemetryTracingBridge` | 将事件桥接到 OTel 管线，通过 OTLP 导出 |

> **Layer 执行顺序：** 注册中心从外到内（或从先加到后加）执行各 layer。filter 在最内层，先过滤再格式化/导出。

### 5. 结构化字段

```rust
info!(title = %payload.title, "creating task");
```

- `title = %payload.title` — 使用 `%` 前缀调用 `Display` trait 来格式化该值
- 这个字段会被 **双向输出**：
  1. `fmt::layer()` 将其写入 JSON 日志行的 `fields` 中
  2. `OpenTelemetryTracingBridge` 将其转换为 LogRecord 的 `Attributes`

另一个示例：

```rust
info!(task_id = id, "fetching task");
```

- `task_id = id` — 没有前缀修饰符时，整数直接以数值形式记录

如果使用 `?` 前缀（如 `?payload`），会调用 `Debug` trait 输出完整的调试表示。

### 6. 路由与 handler（与前课相同）

```rust
let app = Router::new()
    .route("/health", get(health))
    .route("/tasks", post(create_task))
    .route("/tasks/{id}", get(get_task))
    .with_state(state);
```

`AtomicU64` 作为计数器在 `AppState` 中共享。每个 `create_task` 调用递增 ID。

## 运行验证

### 1. 启动后端基础设施

```bash
# 从项目根目录启动 OTel Collector
cd ../.. && podman-compose up -d
```

### 2. 运行本课代码

```bash
cargo run -p lesson-02-logs
```

终端会输出 JSON 格式日志：

```json
{"timestamp":"...","level":"INFO","fields":{"message":"Server starting on http://127.0.0.1:3001"},"target":"lesson_02_logs"}
```

### 3. 发送请求

```bash
# Health check
curl http://127.0.0.1:3001/health
# → {"status":"ok"}

# 创建任务（触发结构化日志）
curl -X POST http://127.0.0.1:3001/tasks \
  -H 'Content-Type: application/json' \
  -d '{"title":"learn tracing"}'
# → {"id":1,"title":"learn tracing","done":false}

# 获取任务
curl http://127.0.0.1:3001/tasks/42
# → {"id":42,"title":"example task","done":false}
```

### 4. 查看 Collector 中的日志

```bash
podman-compose logs otel-collector
```

在 Collector 日志中可以看到 `LogRecord` 的详细信息，包括：

```
LogRecord #0
ObservedTimestamp: ...
Body: Str(creating task)
Attributes:
    -> title: Str(learn tracing)
Resource:
    -> service.name: Str(learn-tracing-cncf)
```

## 疑难点

- **日志去哪了？** 两条路径：
  1. stdout — `fmt::layer().json()` 印到控制台
  2. OTel Collector — `OpenTelemetryTracingBridge` 通过 OTLP 发送到 Collector 的 `debug` exporter 打印
  Collector 当前配置中 **日志只做 debug 输出，不转发到外部后端**。

- **`with_tonic()` 的作用？** 指定 gRPC 传输。OTLP 协议支持 gRPC（端口 4317）和 HTTP（端口 4318）两种方式。`with_tonic()` 走 gRPC。

- **为什么用 `with_simple_exporter` 而不是 `with_batch_exporter`？** 本课是教学示例，简单导出器确保每条日志立刻发送，便于调试观察。生产环境应该用 batch exporter 减少网络往返。

- **`EnvFilter::new("info")` 如何覆盖？** 在运行命令前设置环境变量：
  ```bash
  RUST_LOG=debug cargo run -p lesson-02-logs
  ```

- **为什么要用 `%` 前缀？** `tracing` 要求字段值实现某种格式化 trait：
  - 无前缀 — 仅数字类型可直接使用
  - `%` — 调用 `std::fmt::Display`（适合 String、&str 等）
  - `?` — 调用 `std::fmt::Debug`（适合所有实现了 Debug 的类型）
  - `tracing` 2.x 需要给字段提供一种格式化方式
