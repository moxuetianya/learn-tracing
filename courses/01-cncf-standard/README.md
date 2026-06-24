# 课程 1: CNCF 标准生态

## 概览

本课程使用 **OpenTelemetry Rust SDK + OTLP 协议** 搭建可观测性三大支柱。这是 CNCF 推荐的标准方案，所有遥测数据通过 OTLP 统一导出到 OpenTelemetry Collector，再由 Collector 分发到 Jaeger（Traces）、Prometheus（Metrics）等后端。

## 架构

```
┌─────────────┐    OTLP     ┌──────────────┐    ┌──────────┐
│  axum 服务   │ ──────────→ │ OTel Collector│───→│  Jaeger  │
│ (Rust OTel) │             │              │    └──────────┘
└─────────────┘             │              │    ┌─────────────┐
                            │  (Logs debug)│    │ Prometheus  │
                            └──────────────┘    └─────────────┘
                                                     ↑
                            ┌──────────────┐         │ scrape
                            │   Grafana    │─────────┘
                            └──────────────┘
```

## 课时

| # | 课时 | 核心内容 |
|---|------|---------|
| 1 | [lesson-01-setup](./lesson-01-setup/) | axum 基础服务，无可观测性 |
| 2 | [lesson-02-logs](./lesson-02-logs/) | Logs: opentelemetry-appender-tracing 结构化日志 |
| 3 | [lesson-03-metrics](./lesson-03-metrics/) | Metrics: Meter + Histogram + OTLP 导出 |
| 4 | [lesson-04-traces](./lesson-04-traces/) | Traces: TracerProvider + Span + Jaeger 可视化 |
| 5 | [lesson-05-dashboard](./lesson-05-dashboard/) | Grafana Dashboard 整合三支柱 |

## 核心依赖

- `axum` 0.8 — HTTP 框架
- `opentelemetry` 0.29 — OTel API
- `opentelemetry_sdk` 0.29 — OTel SDK
- `opentelemetry-otlp` 0.29 — OTLP 导出器
- `opentelemetry-appender-tracing` 0.29 — 日志桥接
- `tracing` 0.1 — Rust 门面宏
- `tracing-subscriber` 0.3 — 订阅者框架
- `tracing-opentelemetry` 0.29 — tracing → OTel 桥接

## 启动方式

```bash
# 1. 启动后端服务
cd ../.. && podman-compose up -d

# 2. 运行课程代码
cargo run -p lesson-01-setup

# 3. 停止后端服务
cd ../.. && podman-compose down
```
