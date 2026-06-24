# Lesson 03: 指标采集 — Counter + Histogram

## 本课目标

在 `lesson-02-logs` 的基础上，新增 **OpenTelemetry Metrics** 管线。采集 HTTP 请求的 **计数（Counter）** 和 **延迟分布（Histogram）**，通过 OTLP/gRPC 周期推送到 Collector，最终由 Prometheus scrape 后供后续 Grafana 可视化。

## 核心概念

| 概念 | 说明 |
|------|------|
| `MetricExporter` | 指标导出器，定期将采集的指标推送到后端 |
| `SdkMeterProvider` | 指标 Provider，持有 Resource 和导出器，负责管理所有 Meter |
| `with_periodic_exporter` | 周期导出模式，SDK 每 ~5s 自动将内存中的指标快照通过 exporter 发送一次 |
| Counter | **单调递增**指标，只增不减。适合请求计数、错误计数等累计值 |
| Histogram | 分布型指标，记录值的分布范围。适合延迟、响应体大小等需要分位数（P50/P95/P99）的场景 |
| `KeyValue` | 指标的标签（label/dimension），如 `method=POST`，每条唯一标签组合产生一条独立时间序列 |
| `meter` | Meter 是创建具体 instruments（Counter/Histogram）的工厂，命名空间隔离 |

## 依赖说明

本课相比 `lesson-02-logs`，两个核心依赖新增了 `metrics` feature：

```toml
[dependencies]
opentelemetry_sdk = { version = "0.29", features = ["logs", "metrics"] }
    # metrics: 启用 SdkMeterProvider
opentelemetry-otlp = { version = "0.29", features = ["logs", "metrics", "grpc-tonic"] }
    # metrics: 启用 MetricExporter
```

其余依赖与 `lesson-02-logs` 相同。

## 代码逐段讲解

### 1. AppState 新增指标字段

```rust
struct AppState {
    counter: AtomicU64,
    request_counter: Counter<u64>,
    request_duration: Histogram<f64>,
}
```

- `AtomicU64` — 任务 ID 的自增序号（前课遗留）
- `Counter<u64>` — `u64` 类型的计数器，用于统计每种 HTTP 请求的**总次数**
- `Histogram<f64>` — `f64` 类型的直方图，用于记录每次请求的**耗时（秒）**

> **为什么放在 AppState？** 指标对象需要在整个服务生命周期内有效。将 Counter 和 Histogram 放在共享状态中，所有 handler 都可以通过 `State` extractor 获取引用并写入。

### 2. MetricExporter 创建

```rust
fn init_observability() -> SdkMeterProvider {
    // ... log_exporter 和 logger_provider 与前课相同 ...

    let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .expect("failed to create metric exporter");
```

`MetricExporter::builder()` 与 `LogExporter` 类似：

- `.with_tonic()` — gRPC 传输
- `.with_endpoint("http://localhost:4317")` — 同上，指向 Collector 的 gRPC 端口

所有的 OTLP 信号（logs、metrics、traces）都流向 Collector 的同一端口 4317，Collector 内部根据 `Signal` 类型路由到不同管线。

### 3. SdkMeterProvider + 周期导出

```rust
    let meter_provider = SdkMeterProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("learn-tracing-cncf")
                .build(),
        )
        .with_periodic_exporter(metric_exporter)
        .build();
```

- `with_periodic_exporter(metric_exporter)` — **周期推模式**
  - SDK 内部维护一个 Delta Temporality 的指标存储
  - 约每 5 秒（默认间隔）将当前快照推送到 Collector
  - 与 Logs/Traces 的 push 模式不同，Metrics 是定时推送，不是事件驱动
- 函数返回 `meter_provider`，main 中需要保留其引用，否则会被 drop 并停止导出

### 4. 创建 Counter

```rust
let state = Arc::new(AppState {
    counter: AtomicU64::new(1),
    request_counter: meter
        .u64_counter("http.requests.total")
        .with_description("Total number of HTTP requests")
        .build(),
```

- `meter.u64_counter("http.requests.total")` — 创建一个名为 `http.requests.total` 的 `u64` 计数器
- `.with_description(...)` — 添加描述信息，在 Prometheus/Grafana 中可见
- `.build()` — 完成构建，返回 `Counter<u64>` 对象

> **命名规范：** `http.requests.total` 遵循 OpenTelemetry 语义约定。Counter 通常包含 `.total` 后缀以示累加性质。本课简化了命名，不使用 `.` 分隔的完整语义约定路径。

### 5. 创建 Histogram

```rust
    request_duration: meter
        .f64_histogram("http.request.duration")
        .with_description("HTTP request duration in seconds")
        .build(),
```

- `meter.f64_histogram("http.request.duration")` — 创建一个 `f64` 直方图
- Histogram 的值类型是 `f64`（浮点秒），相比 `u64` 计数器能表达更精确的耗时
- 默认 bucket（桶边界）由 SDK 自动决定，标准 Prometheus bucket 包括 `[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10]`

### 6. 记录 Counter

```rust
state.request_counter.add(
    1,
    &[
        KeyValue::new("method", "POST"),
        KeyValue::new("route", "/tasks"),
    ],
);
```

- `.add(1, &[kvs])` — 将计数 +1，同时附带标签（dimensions）
- `KeyValue::new("method", "POST")` 和 `KeyValue::new("route", "/tasks")` 构成标签键值对
- **维度弹性：** 每个唯一的 `(method, route)` 组合都会生成一条**独立的时间序列**
  - `{method=POST, route=/tasks}` → 一条序列
  - `{method=GET, route=/tasks/:id}` → 另一条序列

### 7. 记录 Histogram

```rust
let start = Instant::now();
// ... handler 逻辑 ...
let elapsed = start.elapsed().as_secs_f64();

state.request_duration.record(
    elapsed,
    &[
        KeyValue::new("method", "POST"),
        KeyValue::new("route", "/tasks"),
    ],
);
```

- 用 `Instant::now()` 在 handler 入口记录起始时间
- `start.elapsed().as_secs_f64()` 得到浮点秒数
- `.record(value, &[kvs])` 向直方图记录一个样本值
- 累积足够多样本后，Prometheus 可以用 `histogram_quantile()` 计算 P50/P95/P99

### 8. handler 完整流程

以 `create_task` 为例：

```rust
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    let start = Instant::now();
    info!(title = %payload.title, "creating task");

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let id = state.counter.fetch_add(1, Ordering::SeqCst);
    let elapsed = start.elapsed().as_secs_f64();

    state.request_counter.add(1, &[KeyValue::new("method", "POST"), KeyValue::new("route", "/tasks")]);
    state.request_duration.record(elapsed, &[KeyValue::new("method", "POST"), KeyValue::new("route", "/tasks")]);

    info!(task_id = id, duration_secs = elapsed, "task created");

    Json(Task {
        id,
        title: payload.title,
        done: false,
    })
}
```

步骤：
1. `start = Instant::now()` — 计时开始
2. `info!(...)` — 结构化日志
3. `sleep(20ms)` — 模拟业务处理延迟
4. `fetch_add` — 自增 ID
5. `elapsed` — 计算真实耗时
6. `request_counter.add(1, ...)` — 记录指标（POST 请求计数 +1）
7. `request_duration.record(elapsed, ...)` — 记录延迟分布样本
8. `info!(task_id, duration_secs, ...)` — 日志中也带上耗时

`get_task` handler 逻辑完全对称，只改变了标签值（`method=GET, route=/tasks/:id`）和模拟延迟（5ms）。

## 运行验证

### 1. 启动后端

```bash
cd ../.. && podman-compose up -d
```

### 2. 运行本课

```bash
cargo run -p lesson-03-metrics
```

### 3. 发送请求

```bash
# 多发几次以便积累指标
for i in {1..10}; do
  curl -s -X POST http://127.0.0.1:3001/tasks \
    -H 'Content-Type: application/json' \
    -d '{"title":"task-$i"}'
  echo
done

for i in {1..5}; do
  curl -s http://127.0.0.1:3001/tasks/$i
  echo
done
```

### 4. 查看 Prometheus

Prometheus 端口映射为 `9091`（避免与本机其他服务冲突）：

```bash
# 查看所有 http_requests_total 指标
curl http://127.0.0.1:9091/api/v1/query?query=http_requests_total

# 查看延迟直方图
curl http://127.0.0.1:9091/api/v1/query?query=http_request_duration_seconds_bucket
```

也可以在浏览器打开 `http://localhost:9091` 进入 Prometheus UI，输入 `http_requests_total` 查询。

### 5. 查看 Collector

```bash
podman-compose logs otel-collector
```

可以看到 metrics 的 debug 输出中 `Metric #0` 包含 `Sum`（Counter）和 `Histogram` 数据点。

## 疑难点

- **Counter vs Histogram 选型：**
  - 需要统计「总数」→ Counter
  - 需要统计「分布/分位数」→ Histogram
  - 不要用 Counter 记录耗时——只能加不能减
  - 不要用 Histogram 记录请求计数——无法还原总数

- **维度爆炸：** 标签（KeyValue）的每个**唯一组合**都是一条独立的时间序列。如果你的服务有 100 个路由 × 5 种 method = 500 个时间序列，每个序列占几 KB 内存。**避免将用户 ID、请求 ID 等高基数字段作为标签**——应该放到日志或 trace 中。

- **为什么用 `f64` 不是 `u64`？** 延迟单位是秒，`as_secs_f64()` 返回浮点数（如 `0.02001`），需要 `f64` 类型。Counter 使用 `u64` 是因为请求计数始终是整数。

- **Periodic exporter 的间隔：** 默认约 5 秒。这意味着指标不会实时出现——需要等待第一个导出周期后才能在 Collector/Prometheus 中看到。

- **为什么 `init_observability()` 返回 `SdkMeterProvider`？** main 函数必须持有 `meter_provider` 的引用。如果 `meter_provider` 被 drop，内部的导出循环也会停止，后续指标将不会被发送。`let meter_provider = init_observability();` 让 `meter_provider` 在 main 执行期间保持存活。

- **Prometheus scrape 方向：** 注意数据流向——
  ```
  App → (OTLP push) → Collector → (OTLP to debug only) ─≠→ Prometheus
  ```
  本课中 Collector 的 metrics 管线**只输出 debug 日志**，Prometheus **直接从 App 的 `/metrics` 端点 scrape**。
  
  实际上本课使用的是 `SdkMeterProvider` 直接推送到 Collector，Prometheus 配置 `targets: ["host.docker.internal:3001"]` 是 scrape 更标准配置中 App 的 metrics 端点。在本学习项目中，以 Collector debug 输出为最终验证目标。
