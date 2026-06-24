# Lesson 03: OTel Metrics API — Counter 与 Histogram

## 本课目标

学习直接使用 OpenTelemetry Metrics API 创建和记录度量数据。创建 `Counter<u64>`（请求计数）和 `Histogram<f64>`（请求耗时分布）两种仪器，将度量数据通过 `PeriodicReader` 定时推送到 OTel Collector。

与 Course 2 的 `metrics` crate 对比：OTel 的仪器是**强类型**的（`Counter<u64>`），属性使用 `KeyValue` 结构而非宏字符串。

## 核心概念

| 概念 | 说明 |
|------|------|
| `Meter` | 仪器工厂。通过 `provider.meter("scope-name")` 创建，所有仪器绑定到同一个 Meter scope |
| `Counter<u64>` | 单调递增计数器。`.add(n, &attributes)` 增加计数值 |
| `Histogram<f64>` | 分布统计器。`.record(value, &attributes)` 记录一个观测值，适用于请求耗时、响应大小等 |
| `KeyValue` | OTel 的键值对结构：`KeyValue::new("key", value)`。与 `metrics` crate 的 `[("key", "value")]` 形式不同 |
| `PeriodicReader` | 周期性导出器（`with_periodic_exporter`）的实现核心，每隔固定间隔将累积的度量数据推送到 Collector |
| `Instant` | `std::time::Instant` — 不是 OTel 的概念，是 Rust 标准库的高精度时钟，用于测量耗时 |

### 仪器类型对比

| 仪器 | Course 3（纯 OTel） | Course 2（metrics crate） |
|------|---------------------|--------------------------|
| Counter | `meter.u64_counter("name").build()` → `Counter<u64>` | `register_counter!("name")` → 宏 |
| Histogram | `meter.f64_histogram("name").build()` → `Histogram<f64>` | `register_histogram!("name")` → 宏 |
| Recorder | `counter.add(1, &[...])` | `counter.absolute(1, [...])` |
| Attributes/Labels | `&[KeyValue::new("k", val)]` | `&[("k", "v")]` |

## 依赖说明

与 Lesson 02 相同，无需新增依赖。新增的 trait import：

```rust
use opentelemetry::{
    KeyValue,
    metrics::{Counter, Histogram, MeterProvider as _},
};
use std::time::Instant;
```

`KeyValue` 从 `opentelemetry` crate 顶层导入（它是所有信号共用的基础类型）。

## 代码逐段讲解

### 1. `AppState` 新增 metrics 仪器（第 35-40 行）

```rust
struct AppState {
    counter: AtomicU64,
    logger: SdkLogger,
    request_counter: Counter<u64>,
    request_duration: Histogram<f64>,
}
```

新增两个仪器字段：

- **`Counter<u64>`** — `u64` 泛型参数表示计数值的数据类型。OTel API 提供 `u64_counter` 和 `f64_counter` 两种 builder。
- **`Histogram<f64>`** — `f64` 泛型参数表示记录值的类型。`Histogram` 记录一个浮点数分布，后端（如 Prometheus）可据此计算 P50、P95、P99 等分位数。

> 与 Course 1/2 不同：OTel 的仪器是**类型化的泛型结构体**，而不是类型擦除的通用接口。编译器能确保你不会错误地将 `Histogram<f64>` 传给 `Counter<u64>` 的 `add()` 方法。

### 2. `init_metrics()` 创建仪器（第 59-88 行）

```rust
fn init_metrics() -> (SdkMeterProvider, Counter<u64>, Histogram<f64>) {
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint("http://localhost:4317")
        .build()
        .expect("failed to create metric exporter");

    let provider = SdkMeterProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("learn-tracing-otel")
                .build(),
        )
        .with_periodic_exporter(exporter)
        .build();

    let meter = provider.meter("learn-tracing");

    let request_counter = meter
        .u64_counter("http.requests.total")
        .with_description("Total number of HTTP requests")
        .build();

    let request_duration = meter
        .f64_histogram("http.request.duration")
        .with_description("HTTP request duration in seconds")
        .build();

    (provider, request_counter, request_duration)
}
```

关键步骤：

**a) `provider.meter("learn-tracing")` — 创建 Meter**

`Meter` 是仪器的作用域容器，类似于 Logger 的 scope name。所有通过同一个 Meter 创建的仪器都会被同一个 `InstrumentationScope` 标识。Collector 按 Meter name 分组显示度量数据。

**b) `u64_counter("http.requests.total")` — 创建 Counter 仪器**

```
meter.u64_counter("http.requests.total")   // 指定计数器名称
    .with_description("Total number...")    // 添加描述（可选，存入 metadata）
    .build()                                // 完成构建 → Counter<u64>
```

- 仪器名称 `http.requests.total` 遵循 OTel 命名规范：小写字母、点号分隔。`total` 后缀是 Counter 的约定后缀。
- `.with_description()` 是可选但推荐的方法 — 它会在 Prometheus 的 `# HELP` 注释和 Grafana 的指标说明中显示。
- `.build()` 返回具体的 `Counter<u64>` 实例。

**c) `f64_histogram("http.request.duration")` — 创建 Histogram 仪器**

```rust
meter.f64_histogram("http.request.duration")
    .with_description("HTTP request duration in seconds")
    .build()
```

- `f64_histogram` 接受 `f64` 类型的观测值。如果你想记录整数桶分布，用 `u64_histogram`。
- 仪器名称中 `duration` 暗示了单位（秒） — OTel 建议在名称中暗示单位，但最佳实践是配合 Unit 属性使用。
- 返回 `Histogram<f64>` — 其 `.record(value, attributes)` 方法将观测值放入内部聚合器。

**d) 返回值从函数中提取出来**

`init_metrics()` 返回 `(SdkMeterProvider, Counter<u64>, Histogram<f64>)`。Provider 仍用 `_` 前缀绑定保持生命周期；两个仪器移入 `AppState`。

> **与 Logger 的对比：** Logger 是通过 `provider.logger("name")` 创建并直接返回实例；而 Metrics 的仪器需要通过 builder 链 `.u64_counter("name").with_description("...").build()` 创建。这是因为 Metrics 仪器有更多可配置选项（boundary、unit 等）。

### 3. `main()` 接收并储存仪器（第 111-128 行）

```rust
let (_meter_provider, request_counter, request_duration) = init_metrics();
let (_logger_provider, logger) = init_logs();

let state = Arc::new(AppState {
    counter: AtomicU64::new(1),
    logger,
    request_counter,
    request_duration,
});
```

两个仪器和 logger 一起放入 `AppState`，通过 `Arc` 共享给所有 handler。

### 4. Handler 中的度量记录

**`create_task` handler（第 144-183 行）：**

```rust
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    let start = Instant::now();

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let id = state.counter.fetch_add(1, Ordering::SeqCst);
    let elapsed = start.elapsed().as_secs_f64();

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

    Json(Task { /* ... */ })
}
```

**度量的三个步骤：**

**第 1 步：计时开始 — `Instant::now()`**

```rust
let start = Instant::now();
```

`Instant::now()` 是 Rust 标准库的**单调时钟**（monotonic clock）。与 `SystemTime` 不同，`Instant` 永远不会倒退（不受 NTP 调整影响），适合测量时间段。

**第 2 步：记录 Counter — `counter.add(1, &[...])`**

```rust
state.request_counter.add(
    1,
    &[
        KeyValue::new("method", "POST"),
        KeyValue::new("route", "/tasks"),
    ],
);
```

- **第一个参数** `1` — 增量值。每次请求计数+1。
- **第二个参数** `&[KeyValue]` — 该度量的**属性集**（attributes）。每个唯一的属性组合会在后端形成独立的时序。例如 `{method=POST, route=/tasks}` 和 `{method=GET, route=/tasks/:id}` 是两个不同的计数器。

> `KeyValue::new("method", "POST")` 中第二个参数可以是多种 Rust 类型：`&str`、`i64`、`f64`、`bool` 等。

**第 3 步：记录 Histogram — `histogram.record(elapsed, &[...])`**

```rust
let elapsed = start.elapsed().as_secs_f64();

state.request_duration.record(
    elapsed,
    &[
        KeyValue::new("method", "POST"),
        KeyValue::new("route", "/tasks"),
    ],
);
```

- **第一个参数** `elapsed` — 观测值（请求耗时，单位秒）
- **第二个参数** 与 Counter 相同 — 按 HTTP 方法和路由维度拆分分布

**`get_task` handler（第 185-223 行）：**

```rust
async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<serde_json::Value> {
    let start = Instant::now();

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    let elapsed = start.elapsed().as_secs_f64();

    state.request_counter.add(
        1,
        &[
            KeyValue::new("method", "GET"),
            KeyValue::new("route", "/tasks/:id"),
        ],
    );
    state.request_duration.record(
        elapsed,
        &[
            KeyValue::new("method", "GET"),
            KeyValue::new("route", "/tasks/:id"),
        ],
    );

    let mut record = state.logger.create_log_record();
    record.set_body(format!("task fetched: id={}", id).into());
    record.set_severity_number(Severity::Info);
    record.add_attribute("task.id", id as i64);
    record.add_attribute("duration_secs", elapsed);
    state.logger.emit(record);

    Json(serde_json::json!({ /* ... */ }))
}
```

与 `create_task` 相同的度量记录模式，但属性值为 `method=GET`、`route=/tasks/:id`。

### 5. 度量 + 日志的组合

本课 handler 中同时包含了度量记录和日志记录：

1. `Instant::now()` 开始计时
2. 业务逻辑（sleep 模拟）
3. `counter.add(1, &[...])` 记录请求计数
4. `histogram.record(elapsed, &[...])` 记录请求耗时分布
5. `logger.create_log_record()...emit()` 记录日志（Lesson 02 的内容）

度量数据通过 `periodic_exporter` 批量发送到 Collector；日志通过 `simple_exporter` 即时发送。

### 6. Periodic Exporter 的工作原理

`with_periodic_exporter(exporter)` 创建了一个后台任务，其工作流程为：

```
[请求到达] → counter.add(1, attrs)        ← 同步，立即返回
[请求到达] → histogram.record(0.02, attrs) ← 同步，立即返回
                                     ↓
                    （增量数据暂存在 Provider 内存中）
                                     ↓
              [30秒后] → PeriodicExporter 唤醒
                                     ↓
                    （将所有累积的增量打包为一个 MetricsData 消息）
                                     ↓
                    （通过 gRPC 发送到 Collector:4317）
```

这种模式意味着：
- **`add()` 和 `record()` 不会阻塞请求** — 它们只更新内存中的聚合器
- **度量数据有延迟** — 最长可达 30 秒（默认周期）才能在 Collector 中看到数据
- **网络效率高** — 30 秒内可能积攒了数千次增量，但只需一次 gRPC 调用全部发送

## 运行验证

### 1. 启动基础设施

```bash
docker-compose up -d
```

### 2. 运行本课代码

```bash
cd courses/03-otel-otlp
cargo run -p lesson-03-otel-metrics
```

### 3. 发送多次请求（触发度量累积）

```bash
# 多次创建任务
for i in 1 2 3; do
  curl -s -X POST http://127.0.0.1:3003/tasks \
    -H 'Content-Type: application/json' \
    -d "{\"title\":\"task $i\"}"
  echo
done

# 多次查询任务
for i in 1 2 3; do
  curl -s http://127.0.0.1:3003/tasks/$i
  echo
done
```

### 4. 等待 30 秒后查看 Collector 中的度量数据

```bash
docker-compose logs otel-collector | grep -A 20 "Metric #"
```

预期在 Collector debug 输出中看到类似于：

```
Metric #0
Descriptor:
     -> Name: http.requests.total
     -> Description: Total number of HTTP requests
     -> Unit:
     -> DataType: Sum
```

和 Histogram 数据：

```
Metric #1
Descriptor:
     -> Name: http.request.duration
     -> Description: HTTP request duration in seconds
     -> Unit:
     -> DataType: Histogram
```

每条 Metric 会有两个 `NumberDataPoint`（`method=POST, route=/tasks` 和 `method=GET, route=/tasks/:id`），分别对应 POST 和 GET 请求的累积数据。

### 5. 检查日志同时到达

```bash
docker-compose logs otel-collector | grep -E "task created|task fetched"
```

度量数据和日志数据来自同一次请求，但日志因 `simple_exporter` 而更快出现在 Collector 中。

## 疑难点

- **Counter 和 Histogram 的区别是什么？**

| 仪器 | 数据含义 | 典型查询 | Rust 类型 |
|------|---------|---------|-----------|
| Counter | 累计请求数 | `rate(http.requests.total[1m])` — 每秒请求速率 | `Counter<u64>` |
| Histogram | 请求耗时分布 | `histogram_quantile(0.99, ...)` — 99分位耗时 | `Histogram<f64>` |

- **为什么 `Histogram<f64>` 的类型参数是 `f64` 而 `Counter` 是 `u64`？** Counter 记录累计次数，永远不会减少，自然用无符号整数。Histogram 记录采样值（如 0.021 秒），精度需要用浮点数表示。

- **`KeyValue::new("method", "POST")` 与 Course 2 的 `[("method", "POST")]` 有什么区别？** 语法上的区别。OTel 的 `KeyValue` 是带类型的结构体，属性值必须是 `Into<AnyValue>`，这意味着编译器能验证值类型。`metrics` crate 的宏接受 `&str` 类型的标签，但不做编译时类型检查。

- **Histogram 的边界（buckets）在哪定义的？** 本课使用 `.f64_histogram("name").build()` 的默认边界。你可以通过 builder 方法显式指定：
  ```rust
  meter.f64_histogram("http.request.duration")
      .with_boundaries(vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0])
      .build()
  ```
  默认边界由 SDK 配置决定。在生产环境中应显式指定以匹配你的延迟预期。

- **为什么度量数据要等 30 秒才在 Collector 中出现？** 这是 `PeriodicReader` 的默认导出间隔。如果想要更快的反馈，可以在 SDK 构建器中设置：
  ```rust
  opentelemetry_sdk::metrics::PeriodicReader::builder(exporter, Duration::from_secs(5))
  ```
  但在教学环境中，等待 30 秒可以更快理解和调试。

- **属性集的基数问题：** 每次给 `add()` 传入不同的属性组合，都会在后端创建新的时间序列。如果属性值来自请求参数（如 `task.id`），在 Prod 环境下会产生无限膨胀的时序数（cardinality explosion），应该避免。本课只用 `method` 和 `route` 这类低基数属性。

---

**下一课：** [Lesson 04 — OTel Traces API](../lesson-04-otel-traces/README.md)，最重要的一课 — 学习手动创建 Span，管理其完整生命周期。
