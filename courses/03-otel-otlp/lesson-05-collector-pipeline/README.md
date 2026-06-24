# Lesson 05: Collector Pipeline — 遥测数据的路由中枢

## 本课目标

本课代码与 Lesson 04 完全相同（因为应用程序只需发送数据到 Collector，不需要关心后端）。焦点转向 **OpenTelemetry Collector 的配置文件** — 理解 Collector 如何作为遥测数据的中央路由中枢，将 Traces、Metrics、Logs 分发到不同的后端。

你将学习 Collector 管线的三个核心组件：Receivers（接收器）、Processors（处理器）、Exporters（导出器），以及如何通过 `pipelines` 将三者串联起来。

## 核心概念

| 概念 | 说明 |
|------|------|
| **Collector** | OTel 的核心组件 — 一个独立部署的二进制文件，负责接收、处理、导出遥测数据。应用程序不直接对接后端 |
| **Receivers** | 接收器，定义 Collector 监听哪些协议和端口来接收数据。如 OTLP over gRPC（4317）、OTLP over HTTP（4318） |
| **Processors** | 处理器，在数据被导出前对其进行转换/过滤/批处理。如 `batch`（批量）、`memory_limiter`（内存限制）、`attributes`（属性修改） |
| **Exporters** | 导出器，定义数据最终发送到哪个后端。如 `debug`（stdout 输出）、`otlp`（转发到 Jaeger/Tempo）、`prometheus`（暴露 /metrics 端点） |
| **Pipelines** | 管线，将 Receiver → Processor(s) → Exporter(s) 串联，区分 traces/metrics/logs 三种信号类型 |
| **Signal separation** | 信号分离 — Collector 对 traces、metrics、logs 三条管线独立配置，可分别路由到不同后端 |
| **Telemetry routing** | 应用程序只发送到 Collector，由 Collector 决定数据到哪去 — 实现后端解耦 |

## 配置文件路径

本课的核心是配置文件，而非 Rust 代码。

配置文件位于项目根目录：**`../../configs/otel-collector-config.yaml`**

Docker Compose 将其挂载到 Collector 容器内：

```yaml
# docker-compose.yml
otel-collector:
  image: docker.io/otel/opentelemetry-collector-contrib:latest
  command: ["--config=/etc/otel-collector-config.yaml"]
  volumes:
    - ./configs/otel-collector-config.yaml:/etc/otel-collector-config.yaml
  ports:
    - "4317:4317"  # OTLP gRPC
    - "4318:4318"  # OTLP HTTP
```

## 代码逐段讲解（配置文件）

### 完整的配置文件

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318

processors:
  batch:
    timeout: 1s
    send_batch_size: 1024

exporters:
  debug:
    verbosity: detailed
  otlp/jaeger:
    endpoint: jaeger:4317
    tls:
      insecure: true

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug, otlp/jaeger]
    metrics:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug]
    logs:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug]
```

---

### 1. Receivers（接收器）— 第 1-7 行

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318
```

**作用：** 定义 Collector 的"入口" — 监听什么协议、在什么端口接收遥测数据。

**`otlp`** — 接收器类型。`otlp` 原生协议接收器是 Collector 最基本的接收器，理解 OTLP 协议格式（Protobuf 编码）。

**两种协议并存：**

| 协议 | 端口 | 传输方式 | 标准 |
|------|------|---------|------|
| gRPC | 4317 | HTTP/2 + Protobuf | OTLP 规范默认 |
| HTTP | 4318 | HTTP/1.1 + Protobuf（或 JSON） | 备选方案 |

- **gRPC（4317）** — 应用程序运行时（本课 Rust 代码）通过 `with_tonic()` 连接的端口。gRPC 支持双向流，性能最高。
- **HTTP（4318）** — 适合浏览器端、移动端、或无法使用 gRPC 的场景。可以通过简单的 `curl` 或 Postman 测试。
- **`endpoint: 0.0.0.0:4317`** — `0.0.0.0` 表示监听所有网络接口。在容器内，这样才能从宿主机（`localhost:4317`）访问。

> **可以添加更多接收器：** 例如 `filelog`（读取日志文件）、`kafka`（从 Kafka 消费）、`prometheus`（抓取 /metrics 端点）。Collector 可以从多种来源同时接收数据。

---

### 2. Processors（处理器）— 第 9-13 行

```yaml
processors:
  batch:
    timeout: 1s
    send_batch_size: 1024
```

**作用：** 在 Receiver 和 Exporter 之间对数据进行处理/转换/优化。

**`batch` 处理器** — 最常用的处理器，将多个 Span/Metric/Log 记录打包为一批后一次性发送。

| 参数 | 值 | 含义 |
|------|-----|------|
| `timeout` | `1s` | 最多等待 1 秒，到期后强制发送当前批次 |
| `send_batch_size` | `1024` | 最多打包 1024 个记录后立即发送 |

**批处理的价值：**

```
不使用 batch:                   使用 batch:
                                
Span 1 → [gRPC] → Collector     Span 1 ─┐
Span 2 → [gRPC] → Collector     Span 2 ─┤ 累积到 1024 条
Span 3 → [gRPC] → Collector     Span 3 ─┤ 或 1 秒到期
  ...                              ...   ─┤
Span N → [gRPC] → Collector     Span N ─┘ → [一次 gRPC] → Collector

N 次网络往返                     1 次网络往返
```

这在高流量的生产环境中节省巨大网络开销。`batch` 处理器对 Traces 管线尤为重要（高并发应用中每秒可能产生上万条 Span）。

**其他常用处理器：**

| 处理器 | 用途 |
|--------|------|
| `memory_limiter` | 限制 Collector 的内存使用，防止 OOM |
| `tail_sampling` | 仅采样部分 Span（如错误 Span 100% 保留，正常 Span 保留 10%） |
| `attributes` | 添加/删除/重命名遥测数据的属性（如添加 `environment=production`） |
| `filter` | 排除某些数据（如不发送 `healthcheck` 相关的 Span） |
| `k8sattributes` | 自动添加 Kubernetes Pod/Namespace 元数据 |

---

### 3. Exporters（导出器）— 第 14-20 行

```yaml
exporters:
  debug:
    verbosity: detailed
  otlp/jaeger:
    endpoint: jaeger:4317
    tls:
      insecure: true
```

**作用：** 定义数据最终的目的地 — 将处理过的数据发送到哪里。

**3.1 `debug` exporter**

```yaml
debug:
  verbosity: detailed
```

- 将接收到的数据**打印到 Collector 的 stdout**（stderr）。
- `verbosity: detailed` — 显示详细的数据内容（包括 attributes、resource 等）。可选 `basic`（仅显示摘要）或 `normal`。

**用途：** 开发调试。`docker-compose logs otel-collector` 看到的 Span/Log/Metric 输出就是 debug exporter 产生的。

> **生产环境中 debug exporter 通常被移除**，它会产生大量 stdout 日志，对磁盘和日志系统造成压力。

**3.2 `otlp/jaeger` exporter**

```yaml
otlp/jaeger:
  endpoint: jaeger:4317
  tls:
    insecure: true
```

- **`otlp/jaeger`** — 导出器名称为 `otlp/jaeger`。`/` 后面的 `jaeger` 是用户自定义的标识，可以取任意名字（如 `otlp/grafana`、`otlp/prod`）。
- **`endpoint: jaeger:4317`** — Jaeger 的接收地址。Docker Compose 网络内，`jaeger` 即容器的 hostname。Jaeger 的内置 OTLP Collector 监听 4317 端口。
- **`tls.insecure: true`** — 使用纯 HTTP/2 连接，不做 TLS 握手。仅限本地开发和内网环境。

**工作原理：**

```
Rust App → [gRPC:4317] → Collector (receiver)
                           ↓
                        (process: batch 打包 1024 条)
                           ↓
                    ┌──────────────┬──────────────┬──────────────┐
                    ↓              ↓              ↓              ↓
              debug exporter  otlp/jaeger    (可扩展更多     (可扩展
               (stdout 打印)   (发到 Jaeger)    exporter)     exporter)
```

Collector 支持同时向多个 exporter 发送数据。一个 Span 可以同时被 `debug` 打印和 `otlp/jaeger` 转发到 Jaeger。

---

### 4. Pipelines（管线）— 第 22-35 行

```yaml
service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug, otlp/jaeger]
    metrics:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug]
    logs:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug]
```

**作用：** 为每种信号类型定义独立的数据处理路径。

**管线是 Collector 的最核心概念** — 它定义了从接收数据到输出数据的完整流程。

#### Traces 管线

```
[otlp receiver] → [batch processor] → [debug exporter]  ← stdout
                                     → [otlp/jaeger exporter] → Jaeger UI (16686)
```

Traces 同时导出到两个目的地：
1. **Collector 的 stdout**（`docker-compose logs otel-collector` 可见）
2. **Jaeger**（`http://localhost:16686` 可见）

这使得你同时可以在本地终端**和** Jaeger UI 中看到 Traces。

#### Metrics 管线

```
[otlp receiver] → [batch processor] → [debug exporter]  ← stdout 仅调试
```

Metrics 只输出到 stdout。如果要将 Metrics 发送到 Prometheus，可以添加：

```yaml
exporters:
  prometheus:
    endpoint: "0.0.0.0:8889"    # 新增

service:
  pipelines:
    metrics:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug, prometheus]  # 添加 prometheus
```

这会让 Collector 暴露一个 `/metrics` 端点供 Prometheus 抓取。

#### Logs 管线

```
[otlp receiver] → [batch processor] → [debug exporter]  ← stdout 仅调试
```

Logs 同样是只输出到 stdout。

---

### 5. 为什么信号管线要分开？

三种信号在后端的处理方式差异巨大：

| 维度 | Traces | Metrics | Logs |
|------|--------|---------|------|
| 典型后端 | Jaeger / Tempo / Zipkin | Prometheus / VictoriaMetrics / InfluxDB | Loki / Elasticsearch / Splunk |
| 导出协议 | OTLP、Jaeger Thrift、Zipkin | OTLP、Prometheus Remote Write | OTLP、Loki HTTP、Elasticsearch |
| 采样策略 | 需要（tail sampling） | 不需要（聚合） | 通常不需要 |
| 数据量 | 适中（~1000 spans/s） | 可预测（恒定推送） | 巨大（~10000+ lines/s） |
| 保留期 | 短期（几小时~几天） | 长期（按月/年） | 按合规要求（天~年） |

分离管线允许你：
- 为每种信号使用不同的 processor（如 traces 做 tail sampling，metrics 不做）
- 为每种信号使用不同的 exporter（如 traces 到 Jaeger，logs 到 Loki）
- 独立扩展 Collector 实例（如部署专门的 metrics collector 集群）

### 6. 整体架构图

```
┌──────────────────────────────────────────────────────────────────┐
│                         OTEL COLLECTOR                            │
│                                                                   │
│  ┌─────────────┐    ┌──────────────┐    ┌──────────────────────┐ │
│  │             │    │              │    │  debug               │ │
│  │  otlp       │───▶│  batch       │───▶│  (stdout 打印)       │ │
│  │  receiver   │    │  processor   │    │                      │ │
│  │  (4317)     │    │  (1s/1024)   │    ├──────────────────────┤ │
│  │             │    │              │    │  otlp/jaeger         │ │
│  └─────────────┘    └──────────────┘    │  (jaeger:4317)       │ │
│       ↑                                 │                      │ │
│    OTLP/gRPC                             └──────────────────────┘ │
│       ↑                                       │
│  ┌────┴─────────────────┐                     ▼
│  │  Rust Application    │              ┌──────────┐
│  │  (TracerProvider +   │              │  Jaeger  │
│  │   MeterProvider +    │              │  (16686) │
│  │   LoggerProvider)    │              └──────────┘
│  └──────────────────────┘
│
│  三种信号的管线独立配置：
│    traces:   otlp → batch → [debug, otlp/jaeger]
│    metrics:  otlp → batch → [debug]
│    logs:     otlp → batch → [debug]
└──────────────────────────────────────────────────────────────────┘
```

### 7. 生产环境的 Collector 配置

本课的配置是教学环境的最小配置。生产环境通常需要：

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

processors:
  batch:
    timeout: 200ms
    send_batch_size: 8192
  memory_limiter:
    check_interval: 1s
    limit_mib: 512
  tail_sampling:
    policies:
      - name: error-policy
        type: status_code
        status_code: { status_codes: [ERROR] }
  attributes:
    actions:
      - key: environment
        value: production
        action: upsert

exporters:
  otlp/grafana:
    endpoint: tempo.example.com:4317
    tls:
      insecure: false
  prometheusremotewrite:
    endpoint: "https://mimir.example.com/api/v1/push"

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [memory_limiter, tail_sampling, batch, attributes]
      exporters: [otlp/grafana]
    metrics:
      receivers: [otlp]
      processors: [memory_limiter, batch]
      exporters: [prometheusremotewrite]
    logs:
      receivers: [otlp]
      processors: [memory_limiter, batch]
      exporters: [otlp/loki]
```

关键生产配置要点：
- **`memory_limiter`** — 防止 Collector OOM
- **`tail_sampling`** — 减少 Span 存储量（错误 Span 100% 保留，正常 Span 保留~10%）
- **`attributes`** — 为所有数据注入环境标签（便于多环境隔离）
- **`tls.insecure: false`** — 生产环境必须启用 TLS
- **分离的后端地址** — Traces → Tempo，Metrics → Mimir，Logs → Loki

## 运行验证

### 1. 确认 Collector 启动并加载了配置

```bash
docker-compose up -d
```

检查 Collector 是否成功读取配置：

```bash
docker-compose logs otel-collector | head -20
```

预期输出包含配置加载信息（如 `Everything is ready. Begin running and processing data.`）。

### 2. 运行应用程序

```bash
cd courses/03-otel-otlp
cargo run -p lesson-05-collector-pipeline
```

### 3. 发送请求

```bash
curl -X POST http://127.0.0.1:3003/tasks \
  -H 'Content-Type: application/json' \
  -d '{"title":"pipeline test"}' && \
curl http://127.0.0.1:3003/tasks/1
```

### 4. 验证 Collector 管线

**4.1 验证 Traces exporter：**

```bash
docker-compose logs otel-collector | grep -E "TracesExporter"
```

应该看到 debug exporter 输出的 Span 信息。

**4.2 验证 Metrics exporter：**

```bash
docker-compose logs otel-collector | grep -E "MetricsExporter"
```

应该看到 debug exporter 输出的 Metrics 信息。

**4.3 验证 Logs exporter：**

```bash
docker-compose logs otel-collector | grep -E "LogsExporter"
```

应该看到 debug exporter 输出的 LogRecord 信息。

**4.4 验证完整管线：**

```bash
docker-compose logs otel-collector | grep -E "TracesExporter|MetricsExporter|LogsExporter"
```

你会看到类似输出：

```
2026-06-24T... TracesExporter  {"kind": "exporter", "name": "debug", "resource spans": 1, "spans": 2}
2026-06-24T... MetricsExporter {"kind": "exporter", "name": "debug", "resource metrics": 1, "metrics": 2}
2026-06-24T... LogsExporter    {"kind": "exporter", "name": "debug", "resource logs": 1, "logs": 4}
```

这证实了三条管线都在工作，数据和预期一致。

### 5. 在 Jaeger 中查看 Traces（需 waiting 一段时间）

打开浏览器访问 **http://localhost:16686**：

1. Service: 选择 `learn-tracing-otel`
2. 点击 **Find Traces**
3. 查看 Span 详情（`POST /tasks` 和 `GET /tasks/:id`）

Jaeger 中的 Span 是通过 `otlp/jaeger` exporter 到达的 — 验证了 `traces` 管线的第二个 exporter 也在工作。

### 6. 确认 Collector 端口监听正常

```bash
# 测试 gRPC 端口（需要 grpcurl）
grpcurl -plaintext localhost:4317 list

# 测试 HTTP 端口
curl -v http://localhost:4318/v1/traces -H "Content-Type: application/json" -d '{}'
```

如果 `4317` 和 `4318` 均可访问，说明 receiver 配置正确。

## 疑难点

- **`batch` processor 的 `timeout: 1s` 意味着什么？** 如果 1 秒内没有积攒到 1024 条数据，Collector 会强制发送已积累的数据。这意味着低流量环境下的数据最多延迟 1 秒。高流量时则按 `send_batch_size` 限制发送。

- **为什么 `otlp/jaeger` exporter 中要指定 `insecure: true`？** 本地 Docker 网络内没有 TLS 证书。Jaeger 的 OTLP 端口 4317 默认接受无 TLS 的 OTLP 数据（因为容器间通信不需要 TLS）。生产环境应设置 `insecure: false` 并配置 CA 证书。

- **如果某个 exporter 失败（如 Jaeger 不可用），会影响其他 exporter 吗？** 取决于配置。默认情况下，exporter 失败不会中断其他 exporter 的处理。但某些 collector 的 exporter 有 `sending_queue` 机制，队列满了会反压到 pipeline 上游。这个行为在 Collector 的实现中很复杂，建议配置 `sending_queue` 的大小来避免。

- **多个 exporter 是否复制同一份数据？** 是的。在同一个 pipeline 中，processor 处理后的一份数据会**复制**到每个 exporter。`traces` 管线的 Span 数据会同时被 `debug` 和 `otlp/jaeger` 处理，互不影响。

- **为什么配置文件没有 `prometheus` exporter 也没有 `loki` exporter？** 本课 Collector 的 scope 限定在 OTLP 协议内 — 对所有后端都使用 OTLP 协议通信。Jaeger 支持 OTLP（通过 `--collector.otlp.enabled=true`），所以 Traces 可以直接用 `otlp/jaeger` exporter 导出到 Jaeger。

  Metrics 和 Logs 在本课中只通过 debug exporter 输出。这是因为：
  - Prometheus 通过 **pull** 模式（scrape `/metrics`）采集数据，不需要 Collector push
  - Loki 虽然支持 OTLP（`loki` exporter），但不是本课的重点后端

- **Collector 配置更改后如何生效？** Collector 不支持热加载配置。必须重启 Collector 容器：

  ```bash
  docker-compose restart otel-collector
  ```

  这会导致短暂的数据丢失（Collector 在重启时无法接收数据）。

- **如何验证 Collector 的后端连接是否正常？** 启动时检查 Collector 日志中的错误消息。如果 Jaeger 不可达：
  ```
  error   exporterhelper/queued_retry.go:xxx  Exporting failed. ...
  ```
  collector 的重试机制会在 Jaeger 恢复后重新发送。重启 Collector 后可以直接 `docker-compose logs` 查看连接错误。

- **为什么 Course 3 用 `docker-compose` 而 Course 1/2 用 `podman-compose`？** 本项目的 README 中使用 `podman-compose`（Podman 相当于无守护进程的 Docker）。二者的 Compose 文件使用相同的语法和语义。如果你的环境有 Docker Compose，直接替换命令即可。二者的差异和本课无关。

---

**课程总结：** 你已经完成了 Course 3（纯 OpenTelemetry OTLP）的全部 5 课。你应该理解：

1. OTel SDK 的初始化需要大量样板代码（约 50 行 vs Course 2 的 1 行）
2. Logger API 需要手动构建 `LogRecord`（~5 行 vs `tracing::info!` 的 1 行）
3. Metrics API 的 `Counter<u64>` 和 `Histogram<f64>` 是强类型的仪器
4. Traces 需要手动管理 Span 的完整生命周期：`start()` → `set_attribute()` → 业务逻辑 → `set_status()` → `end()`
5. Collector 是遥测数据的中央路由器，通过 Receiver → Processor → Exporter 管线处理每种信号

**下一步：** 回顾 [项目 README](../../../README.md) 查看三个课程的全局对比，选择最适合你技术栈的方案。
