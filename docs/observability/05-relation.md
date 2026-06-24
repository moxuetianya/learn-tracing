# 三大支柱的关系与关联

## 为什么需要三个

单独使用任何一个支柱都有盲区，三者互补才能形成完整的可观测性视图：

| 场景 | 仅用日志 | 仅用指标 | 仅用链路追踪 |
|---|---|---|---|
| **发现异常** | 靠人肉搜索，低效 | 自动发现（告警），高效 | 需要主动查看，中等 |
| **定位根因** | 可以（如果有足够上下文） | 无法定位具体请求 | 可以快速定位瓶颈服务/操作 |
| **看历史趋势** | 日志量大，难以长期保留 | 天然优势，可保留数月 | 采样后丢失信息 |
| **单请求细节** | 可以（结构化日志） | 无法（聚合后失去单次信息） | 可以（但缺少业务 Payload 细节） |

### 组合使用场景

**场景 1：收到"P99 延迟突增"告警**

```
指标告警 → 打开 Grafana 看延迟面板 → 确认哪个接口、哪个时间段
                                         ↓
                                    在 Jaeger 中筛选该时间段的高延迟 Trace
                                         ↓
                                    定位到某个 Span 耗时异常
                                         ↓
                                    通过 trace_id 在日志中搜索该 Span 的详细日志
                                         ↓
                                    发现数据库慢查询 → 修复
```

**场景 2：排查单用户投诉"我的请求返回了错误"**

```
用户提供 request_id → 在日志系统中搜索 → 找到错误日志 + trace_id
                                           ↓
                                      在 Jaeger 中打开该 Trace
                                           ↓
                                      发现调用下游时返回 500
                                           ↓
                                      结合下游的指标面板确认下游异常 → 联动排查
```

## 通过 trace_id 和 span_id 串联

这是现代可观测性体系中最关键的纽带：

```
┌─────────────────────────────────────────────────────────┐
│                  一次 HTTP 请求的处理                       │
│                                                         │
│  Trace: trace_id = "a1b2c3..."                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │ Root Span: span_id = "001", name = "POST /tasks" │   │
│  │                                                   │   │
│  │  Metrics:  http_requests_total{route="/tasks"}++ │   │
│  │            http_request_duration.observe(150ms)   │   │
│  │                                                   │   │
│  │  Log: {timestamp, level:"INFO",                  │   │
│  │         message:"收到创建任务请求",                 │   │
│  │         trace_id:"a1b2c3...", span_id:"001"}     │   │
│  │                                                   │   │
│  │  ┌─────────────────────────────────────────────┐  │   │
│  │  │ Child Span: span_id = "002", db_insert      │  │   │
│  │  │                                             │  │   │
│  │  │  Log: {trace_id:"a1b2c3...",               │  │   │
│  │  │        span_id:"002",                       │  │   │
│  │  │        message:"写入成功, task_id=42"}       │  │   │
│  │  └─────────────────────────────────────────────┘  │   │
│  └──────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

**核心原理**：
- 每个 Span 产生时，将 `trace_id` 和 `span_id` 注入到所在线程的上下文中
- 日志库在输出每条日志时，自动附加上当前的 `trace_id` 和 `span_id`
- 指标在记录时，如果平台支持（如 OTel Metrics 的 `exemplar` 特性），也可以关联 `trace_id`

这样，你在 Jaeger 中看到一个异常 Span → 拷贝 trace_id → 在日志系统搜索 → 找到所有相关日志。

## 采样的取舍

采样率的不同设置会影响三大支柱的关联程度：

| 采样策略 | 对指标的影响 | 对日志的影响 | 对追踪的影响 |
|---|---|---|---|
| 全量 Trace（100%） | 指标完整（如带 Exemplar） | 日志全部带 trace_id | Trace 完整，无信息丢失 |
| 头部 10% 采样 | 指标不受影响（独立采集） | 90% 的日志无 trace_id | 90% 的请求没有 Trace |
| 尾部采样（保留 P99 + Error） | 指标不受影响 | 大部分日志无 trace_id，但 Error 相关的保留 | 精准保留有问题的 Trace |

**实践建议**：日志和指标永远全量采集（成本可控），链路追踪根据需要设置采样率。即便只有 1% 的 Trace，10% 的错误 Trace 中的 `trace_id` 也能帮你在日志系统中定位到错误请求的完整日志。

## 不同角色的视角

| 角色 | 关注点 | 最常用的信号 | 典型问题 |
|---|---|---|---|
| **SRE** | 系统可靠性 | 指标 + 告警 | "系统现在健康吗？" |
| **开发** | 功能正确性、性能 | 链路追踪 + 日志 | "为什么这个接口慢了？" |
| **DBA** | 数据库性能 | 指标（慢查询）+ 追踪（数据来源） | "哪个服务在大量全表扫描？" |
| **安全** | 审计与合规 | 日志（不可篡改） | "谁在什么时候访问了什么？" |

## 本项目三门课程的覆盖

| 信号 | 课程 1 (CNCF 标准) | 课程 2 (Rust 原生) | 课程 3 (纯 OTel) |
|---|---|---|---|
| **日志** | OTLP Logs Bridge | `tracing-subscriber` JSON → stdout | 手动 `SdkLogger.emit()` |
| **指标** | OTel Counter + Histogram, OTLP Push | `metrics` crate, Prometheus Pull | 手动 `Counter.add()` / `Histogram.record()`, OTLP Push |
| **链路追踪** | `#[instrument]` + TraceLayer, OTLP → Jaeger | `#[instrument]` + tracing-opentelemetry bridge | 手动 `span.start()` / `span.end()` |
| **trace_id 关联** | 自动（trace_id 在日志中） | 自动 | 手动注入 LogRecord 的 trace_id/span_id |

### 课程对比

- **课程 1 最接近生产实践**：使用 OpenTelemetry SDK 的标准范式，所有信号通过 OTLP 统一导出，适合大多数新项目直接采用
- **课程 2 最贴近 Rust 习惯**：使用 `tracing` / `metrics` 等 Rust 生态库，学习曲线最平缓，适合已有 Rust 项目引入
- **课程 3 最适合理解底层**：完全手动控制 API，让你理解 SDK 为你做了什么，适合想深入理解 OTel 细节的开发者

## 推荐学习顺序

1. 先阅读本系列文档（01 → 02 → 03 → 04 → 05），理解理论基础
2. 按课程 1 → 课程 2 → 课程 3 的顺序学习：
   - 课程 1 给你一个"标准做法"的全局视图
   - 课程 2 展示 Rust 生态的另一种路径
   - 课程 3 深入底层细节，帮助你理解 SDK 的封装
3. 理解后，在实际项目中根据需求选择最适合的方式

## 延伸阅读

- [OpenTelemetry 官方文档](https://opentelemetry.io/docs/)
- [W3C TraceContext 标准](https://www.w3.org/TR/trace-context/)
- [Google SRE Book - 监控分布式系统](https://sre.google/sre-book/monitoring-distributed-systems/)
- [Prometheus 文档 - 指标类型](https://prometheus.io/docs/concepts/metric_types/)
- [RED 方法 - Tom Wilkie](https://grafana.com/blog/2018/08/02/the-red-method-how-to-instrument-your-services/)
