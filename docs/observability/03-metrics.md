# 指标（Metrics）

## 原理

指标（Metrics）是**带时间戳的数值型测量数据**，通常以时间序列形式存储和查询。与日志不同，指标在采集时就经过聚合或计算，因此存储成本低、查询速度快，是设置告警和看趋势的理想选择。

### 核心特征

| 特征 | 说明 |
|---|---|
| **数值型** | 指标值必须是数字（整数或浮点），不能是文本 |
| **时间序列** | 每个指标值对应一个时间点，按时间排列形成序列 |
| **聚合性** | 原始事件被聚合为统计量（count、sum、avg、分位数） |
| **低基数** | 标签（label）的取值组合应有限，避免维度爆炸 |

### 为什么需要指标

日志可以告诉你"某个请求用了 200ms"，但无法直接回答：

- "过去 5 分钟所有请求的 P99 延迟是多少？"（需要遍历日志并计算）
- "POST /tasks 的 QPS 在过去一小时的变化趋势？"（需要日志聚合）

指标天然适合这类**聚合查询和时间趋势分析**。

## 指标类型

在 OpenTelemetry 和 Prometheus 的指标模型中，指标分为以下核心类型：

### 1. Counter（计数器）

**只能增加或重置为 0**，不能减少。用于累计值。

```
示例值：0 → 1 → 2 → 3 → ... → 100
```

| 典型场景 | 指标名示例 |
|---|---|
| HTTP 请求总数 | `http_requests_total` |
| 任务创建数量 | `tasks_created_total` |
| 错误次数 | `errors_total` |
| 处理字节数 | `bytes_processed_total` |

**重要**：不要用 Counter 记录当前值（如"当前在线用户数"），那是 Gauge 的职责。

### 2. Gauge（仪表盘）

**可增可减**。用于记录瞬时值。

```
示例值：100 → 85 → 120 → 95 → ...（涨落均可）
```

| 典型场景 | 指标名示例 |
|---|---|
| 当前内存使用量 | `memory_usage_bytes` |
| 当前活跃连接数 | `active_connections` |
| 队列深度 | `queue_depth` |
| CPU 温度 | `cpu_temperature_celsius` |

### 3. Histogram（直方图）

记录值的**分布**。直方图会将观测值分到不同的桶（bucket）中，并累计每个桶的计数。

```
示例：请求延迟
  桶边界: [10ms, 50ms, 100ms, 500ms, 1s, 5s]

  观测到 5 个请求: 8ms, 25ms, 45ms, 200ms, 3s
  →
  bucket[0-10ms]:   1 (8ms)
  bucket[0-50ms]:   3 (8ms, 25ms, 45ms)  ← 累计
  bucket[0-100ms]:  3
  bucket[0-500ms]:  4 (+200ms)
  bucket[0-1s]:     4
  bucket[0-5s]:     5 (+3s)
  count: 5
  sum: 3278ms
```

通过 Histogram，你可以计算：
- **百分位数（P50 / P95 / P99）**：如 P99 = 表示 99% 的请求延迟低于此值
- **Apdex 分数**：用户体验的满意度指数
- **分布热力图**：找到延迟的长尾

**Histogram 与 Summary 的区别**：

| | Histogram | Summary |
|---|---|---|
| 分位数计算时机 | 查询时（服务端） | 收集时（客户端） |
| 可聚合性 | 多个实例的 Histogram 可合并 | 不可合并 |
| 精度 | 受桶边界影响 | 客户端针对单实例精确 |
| 推荐 | 大多数场景（Prometheus 默认） | 需要精确客户端分位数时 |

## 黄金信号

对于服务监控，业界普遍采用以下两种"黄金信号"策略：

### RED 方法（面向请求驱动型服务）

| 信号 | 含义 | 用法 |
|---|---|---|
| **R**ate（速率） | 每秒请求数（QPS） | 判断流量是否异常 |
| **E**rrors（错误） | 错误请求的比例 | 判断服务是否出错 |
| **D**uration（延迟） | 请求处理时间分布 | 判断性能是否退化 |

适用场景：HTTP 服务、RPC 服务、API 网关。

### USE 方法（面向资源型服务）

| 信号 | 含义 | 用法 |
|---|---|---|
| **U**tilization（利用率） | 资源的使用百分比 | CPU 使用率、内存使用率 |
| **S**aturation（饱和度） | 资源等待队列的长度 | CPU run queue、连接池等待 |
| **E**rrors（错误） | 资源层面的错误数 | 网络丢包、磁盘 IO 错误 |

适用场景：数据库、消息队列、存储系统。

### 四个黄金信号（Google SRE）

Google SRE 提出了四个最核心的监控信号：

1. **延迟（Latency）**：响应请求所需的时间
2. **流量（Traffic）**：系统承受的请求量
3. **错误（Errors）**：请求失败率
4. **饱和度（Saturation）**：系统资源的使用程度

## 采集模型

指标采集有两种基本模型：**Pull（拉取）** 和 **Push（推送）**。

### Pull 模型（Prometheus 为代表）

```
┌──────────┐  HTTP GET /metrics  ┌────────────┐
│  应用程序  │ ←───────────────── │  Prometheus │
│ (暴露端口) │ ─────────────────→ │  (Scraper)  │
└──────────┘   文本格式返回指标    └────────────┘
                                   (定期拉取)
```

- 应用暴露一个 `/metrics` 端点，以文本格式（Prometheus exposition format）返回当前的指标快照
- Prometheus Server 定期（如每 15s）向目标发起 HTTP GET 请求拉取数据
- **优点**：应用无感知，无需知道监控系统的地址；Prometheus 可自动发现目标
- **缺点**：需要应用暴露 HTTP 端口；短生命周期任务（批处理）可能未来得及被拉取就退出
- **解决方案**：对于短生命周期任务，使用 Pushgateway（谨慎，仅用于批处理任务）

### Push 模型（OTLP / Graphite / StatsD 为代表）

```
┌──────────┐  主动推送 (gRPC/HTTP)  ┌─────────────────┐
│  应用程序  │ ───────────────────→  │ Collector/Server │
└──────────┘                        └─────────────────┘
```

- 应用定期（如每 10s）将指标数据推送到 Collector 或后端
- 不需要应用暴露端口，适合防火墙后或动态 IP 的场景
- **优点**：适合短生命周期任务和 NAT 环境
- **缺点**：应用需要知道后端地址；如果有大量推送者，Collector 可能成为瓶颈

### Pull vs Push 对比

| | Pull（Prometheus 模式） | Push（OTLP 模式） |
|---|---|---|
| **协议** | HTTP GET | gRPC / HTTP POST |
| **服务发现** | 由监控系统负责 | 由应用配置指定 |
| **短生命周期任务** | 差（需 Pushgateway） | 好 |
| **健康检查** | 天然支持（拉取失败即不健康） | 需要额外机制 |
| **背压处理** | 监控系统自己控制频率 | 需要 Collector 实现流控 |
| **代表实现** | Prometheus | OTLP Metrics, Graphite, StatsD |

## 指标命名规范

良好的指标命名让查询和理解变得简单：

```
<namespace>_<subsystem>_<name>_<unit>

示例：
  http_requests_total          ← Prometheus 风格（带 _total 后缀表示 Counter）
  http_request_duration_seconds ← 带单位
  rpc_server_handled_total
```

- 使用 `snake_case`
- Counter 后缀 `_total`
- 单位后缀如 `_seconds`、`_bytes`
- 相同含义的指标在不同服务中应使用相同名称

## 本项目中的指标实现

| 课程 | 方式 | 指标类型 | 采集模型 |
|---|---|---|---|
| 课程 1（CNCF 标准）| OTel `Counter<u64>` + `Histogram<f64>`，OTLP 导出 | Counter, Histogram | Push (OTLP → Collector) |
| 课程 2（Rust 原生）| Rust `metrics` 库 + `metrics-exporter-prometheus` | Counter, Histogram | Pull (/metrics 端点) |
| 课程 3（纯 OTel）| `SdkMeterProvider` + 手动 `Counter.add()` / `Histogram.record()` | Counter, Histogram | Push (OTLP → Collector) |

课程 1 和课程 3 均通过 OTel Collector 将指标路由到 Debug exporter（控制台输出），课程 2 通过 Prometheus 的 Pull 模式采集并展示在 Grafana 中。
