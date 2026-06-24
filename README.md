# Learn Tracing — 可观测性学习项目

通过 Rust + Podman Compose 渐进式学习可观测性三大支柱：Logs、Metrics、Traces。

## 课程结构

| 课程 | 方案 | 核心依赖 | 课时 |
|---|---|---|---|
| [01-cncf-standard](./courses/01-cncf-standard/) | CNCF 标准生态 | opentelemetry SDK + OTLP | 5 |
| [02-rust-native](./courses/02-rust-native/) | Rust 原生 tracing 生态 | tracing crate 家族 | 5 |
| [03-otel-otlp](./courses/03-otel-otlp/) | 纯 OpenTelemetry OTLP | opentelemetry SDK（无 tracing） | 5 |

## 三大方案对比

| 维度 | CNCF 标准 | Rust 原生 | 纯 OTel |
|---|---|---|---|
| 统一门面 | opentelemetry SDK | tracing crate | opentelemetry SDK |
| Span 创建 | `#[instrument]` 宏 | `#[instrument]` 宏 | 手动 start/end |
| 日志集成 | appender-tracing bridge | subscriber fmt layer | Logger API 手动 emit |
| Metrics | OTLP → Collector | metrics crate → /metrics | OTLP → Collector |
| 学习曲线 | 中等 | 简单 | 较陡 |
| 业界趋势 | 主流标准 | Rust 社区首选 | 长远方向 |

## 快速开始

### 1. 启动可观测性后端

```bash
podman-compose up -d
```

### 2. 运行课程代码

```bash
# 以课程 1 第 4 课为例
cd courses/01-cncf-standard
cargo run -p lesson-04-traces
```

### 3. 测试服务

```bash
curl http://127.0.0.1:3001/health
curl -X POST http://127.0.0.1:3001/tasks -H 'Content-Type: application/json' -d '{"title":"learn"}'
curl http://127.0.0.1:3001/tasks/42
```

### 4. 查看结果

- **Jaeger UI**: http://localhost:16686
- **Prometheus**: http://localhost:9091
- **Grafana**: http://localhost:3000

### 5. 停止服务

```bash
podman-compose down
```

## 学习路径推荐

1. **新手** → 先学 [课程 2（Rust 原生）](./courses/02-rust-native/) 理解 tracing 概念，API 最简洁
2. **进阶** → 学 [课程 1（CNCF 标准）](./courses/01-cncf-standard/) 掌握行业标准 OTLP 协议
3. **深入** → 学 [课程 3（纯 OTel）](./courses/03-otel-otlp/) 理解底层 Span 生命周期

## 要求

- Rust 1.85+
- Podman 或 Docker
- Podman Compose 或 Docker Compose
