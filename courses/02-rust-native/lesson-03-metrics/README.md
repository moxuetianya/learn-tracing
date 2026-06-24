# 第 03 课：Metrics — Prometheus 拉取模式的指标采集

## 本课目标

- 使用 `metrics` + `metrics-exporter-prometheus` crate 实现 Prometheus 指标采集
- 理解 Counter（计数器）和 Histogram（直方图）两种核心指标类型
- 通过 `/metrics` 端点暴露指标供 Prometheus 抓取
- 对比 Course 1 的 OTLP 推送模式：此处指标是 Prometheus 主动拉取（pull-based）

---

## 核心概念

| 术语 | 解释 |
|------|------|
| **Counter** | 只增不减的累计计数器（如 HTTP 请求总数）。适合统计调用次数、处理任务数 |
| **Histogram** | 记录数值分布（如请求延迟）。自动计算分位数（p50/p90/p99） |
| **Label（标签）** | 指标的维度拆分，如 `endpoint="create_task"` 对同一个指标按端点分组 |
| `metrics` crate | Rust 生态中最轻量的指标库，提供全局的 `counter!()` 和 `histogram!()` 宏 |
| `metrics-exporter-prometheus` | 将 `metrics` crate 收集的指标注册到全局 Recorder，并渲染为 Prometheus 文本格式 |
| **Pull-based vs Push-based** | Prometheus 定期 `/metrics` 抓取（拉取），而 OTLP 由应用主动推送到 Collector |
| `PrometheusHandle::render()` | 将当前所有指标渲染为 Prometheus text format 字符串 |

---

## 依赖说明

本课新增依赖：

```toml
metrics = "0.24"
metrics-exporter-prometheus = "0.16"
```

这两个 crate 独立于 `tracing` 生态，它们处理的是**指标数据**（metrics），而非日志/追踪。两者可以同时存在、互不干扰。

---

## 代码逐段讲解

### 1. 安装全局指标 Recorder

```rust
let prometheus_handle = PrometheusBuilder::new()
    .install_recorder()
    .expect("failed to install Prometheus recorder");
```

`metrics` crate 采用**全局 Recorder** 设计：
- `PrometheusBuilder::new()` — 创建一个 Prometheus Recorder 构建器
- `.install_recorder()` — 将其安装为全局指标记录器。此后所有的 `counter!()` 和 `histogram!()` 宏调用都会流向此记录器
- 返回的 `PrometheusHandle` 用于后续渲染指标

> **关键设计差异**：与 `tracing_subscriber::init()` 类似，`install_recorder()` 也只能调用一次（全局单例）。

### 2. 注册指标元数据

```rust
describe_counter!(
    "http_requests_total",
    metrics::Unit::Count,
    "Total number of HTTP requests"
);
describe_counter!(
    "tasks_created_total",
    metrics::Unit::Count,
    "Total number of tasks created"
);
describe_histogram!(
    "http_request_duration_seconds",
    metrics::Unit::Seconds,
    "HTTP request duration in seconds"
);
```

`describe_*!` 宏注册指标的**元数据**（名称、单位、描述）。这步是可选的，但它让 Prometheus 输出中带上 `# HELP` 和 `# TYPE` 注释，提升可读性。

### 3. 在请求处理器中使用 Counter

```rust
#[tracing::instrument]
async fn health() -> Json<serde_json::Value> {
    counter!("http_requests_total", "endpoint" => "health").increment(1);
    // ...
}
```

`counter!` 宏的用法：
- `"http_requests_total"` — 指标名称
- `"endpoint" => "health"` — 标签（label），用于按端点维度拆分指标
- `.increment(1)` — 将计数器加 1

同样的模式在 `create_task` 中：
```rust
counter!("http_requests_total", "endpoint" => "create_task").increment(1);
// ... 业务逻辑 ...
counter!("tasks_created_total").increment(1);
```

`tasks_created_total` 没有标签，因为这反映的是全局任务创建总数。

### 4. 在请求处理器中使用 Histogram

```rust
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTask>,
) -> Json<Task> {
    counter!("http_requests_total", "endpoint" => "create_task").increment(1);
    let start = std::time::Instant::now();

    // ... 业务逻辑 ...

    let duration = start.elapsed().as_secs_f64();
    histogram!("http_request_duration_seconds", "endpoint" => "create_task").record(duration);
    // ...
}
```

Histogram 的标准使用模式：
1. 在操作开始时记录 `Instant::now()`
2. 在操作完成时计算 `elapsed().as_secs_f64()`
3. 调用 `.record(duration)` 记录延迟

> `histogram!` 返回一个对象，调用 `.record(value)` 将一个观测值追加到分布中。Prometheus 会自动计算 `_sum`、`_count`、`_bucket` 等内部指标。

### 5. 暴露 `/metrics` 端点

```rust
// 将 PrometheusHandle 存入 AppState
struct AppState {
    counter: AtomicU64,
    prometheus_handle: PrometheusHandle,
}

// 新增 /metrics 路由
let app = Router::new()
    // ... 其他路由 ...
    .route("/metrics", get(metrics_handler));

// 处理器直接返回渲染后的 Prometheus 格式
async fn metrics_handler(State(state): State<Arc<AppState>>) -> String {
    state.prometheus_handle.render()
}
```

### 6. Prometheus 输出格式

当 Prometheus 抓取 `GET /metrics` 时，返回类似：

```
# HELP http_requests_total Total number of HTTP requests
# TYPE http_requests_total counter
http_requests_total{endpoint="health"} 3
http_requests_total{endpoint="create_task"} 2
http_requests_total{endpoint="get_task"} 1

# HELP tasks_created_total Total number of tasks created
# TYPE tasks_created_total counter
tasks_created_total 2

# HELP http_request_duration_seconds HTTP request duration in seconds
# TYPE http_request_duration_seconds histogram
http_request_duration_seconds_bucket{endpoint="create_task",le="0.005"} 0
http_request_duration_seconds_bucket{endpoint="create_task",le="0.01"} 0
http_request_duration_seconds_bucket{endpoint="create_task",le="0.025"} 1
http_request_duration_seconds_bucket{endpoint="create_task",le="0.05"} 2
http_request_duration_seconds_bucket{endpoint="create_task",le="+Inf"} 2
http_request_duration_seconds_sum{endpoint="create_task"} 0.04023
http_request_duration_seconds_count{endpoint="create_task"} 2
```

---

## 运行验证

### 启动服务

```bash
RUST_LOG=info cargo run -p lesson-03-metrics
```

### 发送请求生成指标

```bash
# 发送几个请求来产生指标数据
curl http://localhost:3001/health
curl http://localhost:3001/health
curl -X POST http://localhost:3001/tasks \
  -H "Content-Type: application/json" \
  -d '{"title":"Task 1"}'
curl -X POST http://localhost:3001/tasks \
  -H "Content-Type: application/json" \
  -d '{"title":"Task 2"}'
curl http://localhost:3001/tasks/1
```

### 查看 Prometheus 指标

```bash
curl http://localhost:3001/metrics
```

预期输出包含：
```
# TYPE http_requests_total counter
http_requests_total{endpoint="health"} 2
http_requests_total{endpoint="create_task"} 2
http_requests_total{endpoint="get_task"} 1

# TYPE tasks_created_total counter
tasks_created_total 2

# TYPE http_request_duration_seconds histogram
...
```

### 配置 Prometheus 抓取（可选）

在 `prometheus.yml` 中添加：
```yaml
scrape_configs:
  - job_name: 'learn-tracing'
    scrape_interval: 5s
    static_configs:
      - targets: ['localhost:3001']
    metrics_path: '/metrics'
```

---

## 疑难点

### 1. 为什么指标是全局的而不是通过 State 传递？

`counter!()` 和 `histogram!()` 宏操作的是**全局静态注册表**，而非需要在线程间传递的对象。这与 Course 1 中使用 OpenTelemetry 的 `Meter` 传参方式截然不同。

**设计理念**：
- `metrics` crate 认为指标是全局资源（类似日志），应该在任何地方都能直接使用
- OTel 的设计倾向于显式依赖注入（传入 `Meter`），便于测试和多租户

**测试时的替代方案**：`metrics` 提供 `with_local_recorder` 用于隔离的测试环境。

### 2. `install_recorder()` 和 `tracing_subscriber::init()` 会发生冲突吗？

**不会**。它们各自管理独立的全局单例（Recorder 和 Subscriber），互不干扰。这正是 Lesson 05 将展示的"多 Layer 共存"。

### 3. Counter 和 Histogram 何时重置？

Counter 和 Histogram 是**进程内累计**的，进程重启后归零。Prometheus 负责处理重启导致的数据重置（通过 `_total` 后缀和 reset detection）。

### 4. Histogram 的 `le` bucket 含义

`http_request_duration_seconds_bucket{le="0.005"}` 中的 `le` 表示 "less than or equal to"。该 bucket 记录了延迟 ≤ 5ms 的请求数。Prometheus 使用这些 bucket 计算分位数。

默认的 bucket 边界为 `[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10]` 秒。可以通过 `PrometheusBuilder` 自定义。

### 5. Pull-based vs Push-based 对比

| 方面 | 本课 (Prometheus Pull) | Course 1 (OTLP Push) |
|------|----------------------|---------------------|
| 数据流向 | Prometheus → 应用 | 应用 → Collector |
| 服务发现 | Prometheus 配置 `static_configs` | 应用配置 Collector endpoint |
| 瞬时指标 | 抓取时快照 | 持续推送 |
| 架构复杂度 | 低（只需暴露 HTTP 端点） | 中（需 Collector 中转） |
| 适用场景 | 固定实例、传统部署 | 弹性伸缩、k8s 环境 |
