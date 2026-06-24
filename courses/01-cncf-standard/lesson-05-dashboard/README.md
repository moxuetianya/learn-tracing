# Lesson 05: Grafana Dashboard — 统一仪表盘

## 本课目标

本课**不涉及新代码编写**（代码与 `lesson-04-traces` 完全相同）。重点学习：

1. **Grafana 仪表盘** 如何通过 Prometheus 数据源展示 OpenTelemetry 指标
2. **PromQL** 基本查询语法
3. 在 Grafana 中从指标数据**跳转到 Jaeger** 进行链路分析
4. 形成完整的 **可观测性闭环**：请求 → Traces（Jaeger）→ Metrics（Grafana）→ Logs（Collector debug）

## 核心概念

| 概念 | 说明 |
|------|------|
| Grafana | 开源可视化平台，支持多种数据源（Prometheus、Jaeger、Loki 等），通过 Dashboard 面板展示数据 |
| Grafana Provisioning | 通过配置文件在启动时自动创建数据源（datasources）和加载仪表盘（dashboards），无需手动操作 UI |
| PromQL | Prometheus Query Language，用于从 Prometheus 中查询时间序列数据 |
| `rate()` | PromQL 函数，计算时间序列的每秒增长率（QPS = Queries Per Second） |
| `histogram_quantile()` | PromQL 函数，从 Histogram 的 bucket 数据中计算指定分位数（如 P50/P95/P99） |
| Data Source Link | Grafana 面板中嵌入的超链接，能从指标视图直接跳转到 Jaeger 查看对应 trace |

## 依赖说明

无新增 Rust 依赖。本课代码与 `lesson-04-traces` 完全相同：

```toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
opentelemetry = "0.29"
opentelemetry_sdk = { version = "0.29", features = ["logs", "metrics", "trace"] }
opentelemetry-otlp = { version = "0.29", features = ["logs", "metrics", "grpc-tonic"] }
opentelemetry-appender-tracing = "0.29"
tracing-opentelemetry = "0.30"
tower-http = { version = "0.6", features = ["trace"] }
```

本课重点在基础设施配置文件：

```
configs/
├── grafana-datasources.yml     # Grafana 数据源自动化配置
├── grafana-dashboards.yml      # Grafana 仪表盘加载配置
├── dashboards/
│   └── app-dashboard.json      # 仪表盘定义（JSON 模型）
├── otel-collector-config.yaml  # OTel Collector 管线配置
└── prometheus.yml              # Prometheus scrape 配置
```

## 代码逐段讲解

### 代码为何与 Lesson 04 相同？

本课的核心目标是将**可观测性数据流水线**中的最后一环——**可视化**——完整串联起来。Lesson 04 的代码已经产出了全部三种信号（Log、Metric、Trace），本课只是通过 Grafana 将这些信号以统一的方式呈现出来。

代码结构回顾：

```rust
fn init_observability() -> SdkTracerProvider {
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
    // ... log_exporter → logger_provider → otel_log_layer ...
    // ... metric_exporter → meter_provider → global set_meter_provider ...
    // ... span_exporter  → tracer_provider → otel_trace_layer ...

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(otel_log_layer)
        .with(otel_trace_layer)
        .init();

    tracer_provider
}
```

完整初始化了所有三条管线后，App 的业务代码通过 `#[instrument]`、Counter/Histogram 记录、`info!` 日志输出这三种方式产生遥测数据，都通过 OTLP 流向 Collector。

### Grafana 数据源配置

```yaml
# configs/grafana-datasources.yml
apiVersion: 1
datasources:
  - name: Prometheus
    type: prometheus
    url: http://prometheus:9090
    access: proxy
    isDefault: true
  - name: Jaeger
    type: jaeger
    url: http://jaeger:16686
    access: proxy
```

- 两个数据源：**Prometheus**（默认）和 **Jaeger**
- `access: proxy` — Grafana 服务端代理查询请求，浏览器不直接访问数据源
- `isDefault: true` — Prometheus 设为默认，新建面板时自动选择

此文件通过 docker-compose 挂载到 Grafana 容器中：
```yaml
volumes:
  - ./configs/grafana-datasources.yml:/etc/grafana/provisioning/datasources/datasources.yml
```

### Dashboard 自动加载

```yaml
# configs/grafana-dashboards.yml
apiVersion: 1
providers:
  - name: "default"
    folder: "Learn Tracing"
    type: file
    options:
      path: /etc/grafana/provisioning/dashboards
```

- Grafana 启动时自动扫描 `/etc/grafana/provisioning/dashboards` 目录下的 JSON 文件
- 将所有仪表盘加载到名为 **"Learn Tracing"** 的文件夹中

### Dashboard JSON 详解

仪表盘定义在 `configs/dashboards/app-dashboard.json`，包含 5 个面板：

#### Panel 1: HTTP Request Rate (QPS)

```json
{
  "title": "HTTP Request Rate (QPS)",
  "type": "timeseries",
  "targets": [{
    "expr": "rate(http_requests_total[1m])",
    "legendFormat": "{{method}} {{route}}"
  }]
}
```

**PromQL 解析：**

- `http_requests_total` — Counter 指标名（来自代码中 `meter.u64_counter("http.requests.total")`）
  - OpenTelemetry SDK 会将 `.` 转换为 `_`，所以 `http.requests.total` → `http_requests_total`
- `[1m]` — 时间范围选择器，取最近 1 分钟的数据
- `rate(...[1m])` — 计算每秒增长率
  - Counter 是单调递增的累计值，`rate()` 将其转换为 **每秒请求数（QPS）**
  - 例如：Counter 在 1 分钟内从 100 增加到 160，则 `rate(...[1m])` = (160-100)/60s = 1.0 QPS
- `legendFormat: "{{method}} {{route}}"` — 使用标签值作为图例，显示如 `POST /tasks` 和 `GET /tasks/:id` 两条线

#### Panel 2: Request Latency (P50/P95/P99)

```json
{
  "title": "Request Latency (P50/P95/P99)",
  "type": "timeseries",
  "targets": [
    {
      "expr": "histogram_quantile(0.50, rate(http_request_duration_seconds_bucket[1m]))",
      "legendFormat": "P50"
    },
    {
      "expr": "histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[1m]))",
      "legendFormat": "P95"
    },
    {
      "expr": "histogram_quantile(0.99, rate(http_request_duration_seconds_bucket[1m]))",
      "legendFormat": "P99"
    }
  ]
}
```

**PromQL 解析：**

- `http_request_duration_seconds_bucket` — Histogram 的 bucket 时间序列
  - 来自代码中 `meter.f64_histogram("http.request.duration")`
  - OTel SDK 自动添加 `_seconds_bucket` 后缀
  - 每个 bucket（如 `le="0.005"`、`le="0.01"`、...、`le="+Inf"`）是一条单独的时间序列
- `rate(...[1m])` — 同上，将累计值转为每秒速率
- `histogram_quantile(0.95, ...)` — 从 bucket 分布中计算 **第 95 百分位数**
  - P50（中位数）：50% 的请求延迟 ≤ 该值
  - P95：95% 的请求延迟 ≤ 该值（常用的 SLA 指标）
  - P99：99% 的请求延迟 ≤ 该值（长尾延迟）
- 三条查询分别计算 P50、P95、P99，在同一图表中对比显示
- 单位：`"unit": "s"` — 秒

> **为什么需要 `histogram_quantile` 而不直接查某个 bucket？**
> Histogram 并不存储每个请求的原始延迟值，而是将延迟落入不同的 bucket 中计数。`histogram_quantile()` 通过对 bucket 分布进行线性插值来近似计算分位数。

#### Panel 3: 5xx Error Rate

```json
{
  "title": "5xx Error Rate",
  "type": "stat",
  "targets": [{
    "expr": "rate(http_requests_total{status_code=~\"5..\"}[5m])"
  }]
}
```

- `type: "stat"` — 显示单个数字/状态值
- `{status_code=~"5.."}` — 标签选择器，筛选 status_code 标签匹配正则 `5..`（即 5xx 状态码）
- `[5m]` — 取最近 5 分钟数据
- `"colorMode": "background"` — 数字为 0 时背景绿色，>0 时背景红色

> **注意：** `status_code` 标签是由 `TraceLayer::new_for_http()` 自动添加的。在实际代码中，Counter 并没有显式记录 status_code 维度。此面板在实际数据（无 status_code 标签）中会显示 "No data"，作为教学示例展示标签筛选器的用法。如果希望该面板有效，需要在代码中添加 `KeyValue::new("status_code", ...)` 标签。

#### Panel 4: Total Requests (per second)

```json
{
  "title": "Total Requests (per second)",
  "type": "stat",
  "targets": [{
    "expr": "sum(rate(http_requests_total[1m]))"
  }]
}
```

- `sum(rate(...[1m]))` — 对所有标签维度求和，得到全局 QPS
- 数字面板展示实时 QPS 快照

#### Panel 5: Open Jaeger UI

```json
{
  "title": "",
  "type": "text",
  "options": {
    "content": "# [Open Jaeger UI](http://localhost:16686)\n\nClick the link above to explore traces in Jaeger.",
    "mode": "markdown"
  }
}
```

- `type: "text"` — Markdown 文本面板
- 嵌入一个超链接指向 Jaeger UI
- 实现从 Grafana 到 Jaeger 的**导航跳转**

### 可观测性闭环

整个系统的数据流：

```
                    ┌──────────────────┐
    curl ──────────→│  axum 服务        │
                    │  (lesson-05)      │
                    └───┬──────┬───────┘
                        │      │
                  OTLP  │      │ OTLP
                 (logs  │      │ (metrics
                 traces)│      │  only)
                        ↓      ↓
                ┌──────────────────────┐
                │  OTel Collector      │
                │  ┌────────────────┐  │
                │  │ Traces pipeline│──│────→ Jaeger     → Jaeger UI (http://localhost:16686)
                │  ├────────────────┤  │
                │  │ Logs pipeline  │──│────→ Debug stdout (podman-compose logs otel-collector)
                │  ├────────────────┤  │
                │  │ Metrics pipeline│──│────→ Debug stdout + Prometheus
                │  └────────────────┘  │
                └──────────────────────┘
                                          ┌────────────┐
                              scrape ──→  │ Prometheus │   Prometheus UI (http://localhost:9091)
                                          └─────┬──────┘
                                                │
                                          ┌─────┴──────┐
                              query   ←──→│  Grafana    │  Grafana UI  (http://localhost:3000)
                                          └────────────┘
```

**完整闭环：**
1. 发送 `curl` 请求 → App 记录 trace span + metrics + logs
2. Traces 经 Collector 转发到 Jaeger（`http://localhost:16686`）查看请求全链路
3. Metrics 经 Collector 处理后由 Prometheus scrape，在 Grafana（`http://localhost:3000`）仪表盘中查看 QPS 和延迟趋势
4. Logs 在 Collector debug 输出中查看（`podman-compose logs otel-collector`）
5. 从 Grafana 面板点击链接跳转到 Jaeger，追踪具体慢请求的调用链

## 运行验证

### 1. 启动全部后端服务

```bash
cd ../.. && podman-compose up -d
```

确认所有容器正常运行：
```bash
podman-compose ps
# 应看到: otel-collector, jaeger, prometheus, grafana 全部 Up
```

### 2. 运行本课代码

```bash
cargo run -p lesson-05-dashboard
```

### 3. 生成流量（模拟负载）

在另一个终端运行循环请求，积累足够的指标数据：

```bash
# 混合 POST 和 GET，持续约 30 秒
for i in $(seq 1 50); do
  curl -s -X POST http://127.0.0.1:3001/tasks \
    -H 'Content-Type: application/json' \
    -d "{\"title\":\"task-$i\"}" > /dev/null
  curl -s http://127.0.0.1:3001/tasks/$i > /dev/null
  sleep 0.3
done
```

这段循环每秒约产生 3 个请求（1 POST + 2 GET），持续约 30 秒生成 ~100 个请求。

### 4. 查看 Grafana 仪表盘

浏览器打开 **http://localhost:3000**（匿名登录已启用）：

1. 左侧菜单 → **Dashboards** → **Learn Tracing** 文件夹
2. 打开 **"Learn Tracing - App Dashboard"**
3. 观察面板：
   - **HTTP Request Rate**：随时间变化显示 QPS 曲线
   - **Request Latency**：显示 P50/P95/P99 延迟曲线（P50 约 ~5ms-20ms，P95 约 ~20ms）
4. 点击右上角时间选择器设为 **"Last 5 minutes"** 以查看完整数据范围

### 5. 从 Grafana 跳转到 Jaeger

1. 在 Grafana 仪表盘的 **"Open Jaeger UI"** 面板中点击链接
2. 或直接打开 **http://localhost:16686**
3. Service 选择 `learn-tracing-cncf` → **Find Traces**
4. 点击某条 trace 查看 span 详情：
   - `HTTP POST /tasks` 包裹 `create_task`
   - `HTTP GET /tasks/{id}` 包裹 `get_task`
   - 每个 span 展示耗时、属性（`task_title`/`task_id`）

### 6. 验证完整闭环

```
① 发送请求:
   curl -X POST http://127.0.0.1:3001/tasks -H 'Content-Type: application/json' -d '{"title":"test"}'
   
② Jaeger 查看 Trace:
   http://localhost:16686 → 搜索 → 看到该请求的完整 trace

③ Grafana 查看 Metrics:
   http://localhost:3000 → Dashboard → QPS 和延迟图出现该请求贡献的数据点

④ Collector 查看 Logs:
   podman-compose logs otel-collector | grep "test"
   看到日志条目包含 title=test
```

## 疑难点

- **Dashboard 手动修改后的持久化：** Grafana 通过 provisioning 加载的仪表盘是**只读**的（不可在 UI 中保存覆盖 JSON 文件）。如果需要修改，编辑 `configs/dashboards/app-dashboard.json` 并重启 Grafana。

- **PromQL `rate` vs `irate`：** `rate()` 计算指定时间窗口内的平均增长率，适合查看趋势。`irate()` 计算最后两个数据点之间的瞬时增长率，适合查看尖刺。仪表盘使用 `rate` 以获得更平滑的曲线。

- **Histogram 分位数的精度：** `histogram_quantile()` 的精度受 bucket 分布限制。如果延迟分布在某个 bucket 内非常集中（如所有请求都在 20ms bucket 内），P95 和 P99 可能相同。在生产环境中选择合适的 bucket 边界非常重要。

- **Prometheus scrape_interval 与 Grafana 刷新：** 
  - Prometheus `scrape_interval: 5s` — 每 5 秒拉取一次
  - Grafana dashboard `refresh: 10s` — 每 10 秒刷新一次
  - 这意味着指标数据最多有 ~15 秒延迟（scrape 间隔 + Grafana 刷新）

- **为什么 Grafana 中指标名称有 `_seconds` 后缀？** OpenTelemetry SDK 的 histogram 使用秒作为单位时会自动追加 `_seconds` 后缀以符合 Prometheus 命名约定。可在代码中使用 `set_unit(Unit::new("s"))` 显式指定单位来避免歧义。

- **Grafana 匿名登录安全问题：** 本课配置了 `GF_AUTH_ANONYMOUS_ENABLED=true` 以跳过登录，仅用于本地学习。生产环境务必关闭并配置认证。
