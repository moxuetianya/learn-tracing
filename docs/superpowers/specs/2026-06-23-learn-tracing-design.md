# Learn Tracing — 可观测性学习项目设计

## 概述

一个渐进式学习可观测性三大支柱（Logs、Metrics、Traces）的 Rust 项目。通过 3 个独立课程，分别覆盖 3 种主流方案，每个课程 5 课时，逐步搭建完整可观测性栈。

## 项目结构

```
learn-tracing/
├── docker-compose.yml              # 统一的后端可观测性服务
├── README.md                       # 项目总览
└── courses/
    ├── 01-cncf-standard/           # Cargo workspace: CNCF 标准生态
    │   ├── README.md
    │   ├── Cargo.toml
    │   ├── lesson-01-setup/
    │   ├── lesson-02-logs/
    │   ├── lesson-03-metrics/
    │   ├── lesson-04-traces/
    │   └── lesson-05-dashboard/
    ├── 02-rust-native/             # Cargo workspace: Rust 原生 tracing 生态
    │   ├── README.md
    │   ├── Cargo.toml
    │   ├── lesson-01-tracing-subscriber/
    │   ├── lesson-02-span/
    │   ├── lesson-03-metrics/
    │   ├── lesson-04-traces/
    │   └── lesson-05-integration/
    └── 03-otel-otlp/               # Cargo workspace: 纯 OpenTelemetry OTLP
        ├── README.md
        ├── Cargo.toml
        ├── lesson-01-otel-init/
        ├── lesson-02-otel-logs/
        ├── lesson-03-otel-metrics/
        ├── lesson-04-otel-traces/
        └── lesson-05-collector-pipeline/
```

## 演示应用

基于 **axum 0.8** + **tokio** 的 HTTP 服务，模拟「用户任务管理」API：

| 端点 | 方法 | 说明 | 可观测性要点 |
|---|---|---|---|
| `/health` | GET | 健康检查 | 无额外埋点，展示基础 metrics |
| `/tasks` | POST | 创建任务 | Span 包裹业务逻辑，模拟 DB 写入延迟 |
| `/tasks/:id` | GET | 查询任务 | Span 包裹缓存查询，模拟缓存命中/未命中 |
| `/metrics` | GET | Prometheus 端点 | 仅在课程 1/2 中暴露，课程 3 用 OTLP |

所有端点统一返回 JSON。

## 后端服务（docker-compose.yml）

| 服务 | 端口 | 用途 |
|---|---|---|
| OpenTelemetry Collector | 4317 (gRPC) / 4318 (HTTP) | 接收 OTLP 数据，路由到后端 |
| Jaeger | 16686 (UI), 14250 (gRPC) | Trace 存储与查询 |
| Prometheus | 9090 | Metrics 抓取与存储 |
| Grafana | 3000 | 统一仪表盘，预配置 Jaeger + Prometheus 数据源 |

## 三个课程详细设计

### 课程 1: CNCF 标准生态

**核心依赖**: `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, `opentelemetry-appender-tracing`

**架构**: 应用 → OTel SDK → OTLP → Collector → Jaeger (traces) / Prometheus (metrics)

| 课时 | 文件 | 核心内容 |
|---|---|---|
| Lesson 01 | `lesson-01-setup/` | 裸 axum 服务，无任何可观测性。Docker Compose 启动验证。 |
| Lesson 02 | `lesson-02-logs/` | `opentelemetry-appender-tracing` 接入，结构化日志通过 OTLP 导出。每行日志携带 trace_id/span_id。 |
| Lesson 03 | `lesson-03-metrics/` | OTel Meter 创建 Counter (请求总数) 和 Histogram (延迟分布)。OTLP exporter 推送模式导出。 |
| Lesson 04 | `lesson-04-traces/` | TracerProvider 初始化，axum 中间件自动创建 span。手动 `instrument` 标注业务 span。context propagation 通过 W3C TraceContext。 |
| Lesson 05 | `lesson-05-dashboard/` | Grafana 导入预置 Dashboard：QPS 面板 + P50/P99 延迟 + 错误率 + Jaeger trace 链接。pre-provisioned datasource。 |

### 课程 2: Rust 原生 tracing 生态

**核心依赖**: `tracing`, `tracing-subscriber`, `tracing-opentelemetry`, `metrics` + `metrics-exporter-prometheus`

**架构**: tracing crate 作为统一门面 → subscriber 层分发 → fmt layer (stdout JSON) + opentelemetry layer (Jaeger)

| 课时 | 文件 | 核心内容 |
|---|---|---|
| Lesson 01 | `lesson-01-tracing-subscriber/` | `tracing-subscriber` Registry 初始化，fmt layer 输出 JSON 结构化日志。理解 subscriber 组合模式。 |
| Lesson 02 | `lesson-02-span/` | tracing span 体系：`#[instrument]` 宏，`Span::current()`，span 字段与生命周期。axum 自动 span。 |
| Lesson 03 | `lesson-03-metrics/` | `metrics` crate 宏 (`counter!`, `histogram!`)，`metrics-exporter-prometheus` 暴露 `/metrics` 端点。 |
| Lesson 04 | `lesson-04-traces/` | `tracing-opentelemetry` layer 桥接，将 tracing span 导出到 OTLP → Collector → Jaeger。 |
| Lesson 05 | `lesson-05-integration/` | 多层 subscriber 组合：JSON stdout + OTel + Prometheus 同时工作。对比课程1与课程2的差异。 |

### 课程 3: 纯 OpenTelemetry OTLP

**核心依赖**: `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`（无 tracing crate 依赖）

**架构**: 应用 → OTel SDK API 直接编程 → OTLP → Collector 完整配置 pipeline

| 课时 | 文件 | 核心内容 |
|---|---|---|
| Lesson 01 | `lesson-01-otel-init/` | 纯 OTel SDK 初始化，`TracerProvider` + `MeterProvider` + `LoggerProvider` 的完整配置。无 tracing crate。 |
| Lesson 02 | `lesson-02-otel-logs/` | OTel Logs API：`Logger` 创建，`LogRecord` 构建，与 span context 关联。 |
| Lesson 03 | `lesson-03-otel-metrics/` | OTel Metrics API：`Counter`, `Histogram` 的创建和记录。属性 (Attributes) 的使用。 |
| Lesson 04 | `lesson-04-otel-traces/` | OTel Traces API：`Span` 的 start/end 生命周期，`Context` 传播，`SpanKind` 区分 server/client。 |
| Lesson 05 | `lesson-05-collector-pipeline/` | 自定义 Collector 配置：receivers、processors (batch, memory_limiter)、exporters、pipelines。展示完整的数据流转。 |

## 每课时文档结构

每个 `lesson-XX-*` 目录下包含 `README.md`，按以下结构编写：

1. **本课目标** — 学完能做什么
2. **核心概念** — 关键术语解释（如 Span、Meter、Exporter）
3. **依赖说明** — Cargo.toml 新增依赖及作用
4. **代码逐段讲解** — 分步骤展示代码，每段配解释
5. **运行验证** — 启动方式 + 期望输出 + 如何在 Jaeger/Grafana 查看
6. **疑难点** — 常见坑和注意事项

## 技术选型

| 组件 | 选择 | 版本 |
|---|---|---|
| Rust | stable | 1.85+ |
| Web 框架 | axum | 0.8 |
| 异步运行时 | tokio | 1 |
| JSON 序列化 | serde + serde_json | 1 |
| 可观测性 (课程1/3) | opentelemetry + opentelemetry-otlp | 0.29 |
| 可观测性 (课程2) | tracing + tracing-subscriber | 0.1 / 0.3 |
| Collector | otel/opentelemetry-collector-contrib | latest |
| Jaeger | jaegertracing/all-in-one | 1 |
| Prometheus | prom/prometheus | latest |
| Grafana | grafana/grafana | latest |

## 不包含的内容

- 生产级部署（K8s、服务网格）
- 告警规则（Alertmanager）
- eBPF 级别的可观测性
- 分布式多服务场景（仅单服务演示）
