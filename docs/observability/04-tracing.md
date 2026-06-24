# 链路追踪（Tracing）

## 原理

链路追踪（Distributed Tracing）记录**一次请求在分布式系统中穿过多个服务/组件的完整路径**。它回答的问题是："这个请求经过了哪些环节？每个环节花了多少时间？哪里是瓶颈？"

### 核心概念

一次完整的追踪由一组 **Span** 组成，Span 之间通过父子关系形成一棵树。

```
Trace (trace_id = "abc123")
│
├── Span A (name = "HTTP POST /tasks", span_id = "001", parent = none)
│   │    start: 10:00:00.000,  end: 10:00:00.150,  duration: 150ms
│   │
│   ├── Span B (name = "validate_input", span_id = "002", parent = "001")
│   │       start: 10:00:00.005,  end: 10:00:00.010,  duration: 5ms
│   │
│   ├── Span C (name = "db_insert", span_id = "003", parent = "001")
│   │       start: 10:00:00.010,  end: 10:00:00.050,  duration: 40ms
│   │
│   └── Span D (name = "cache_warmup", span_id = "004", parent = "001")
│           start: 10:00:00.050,  end: 10:00:00.145,  duration: 95ms  ← 瓶颈！
```

### Span 数据结构

每个 Span 包含以下关键信息：

| 字段 | 含义 | 示例 |
|---|---|---|
| `trace_id` | 全局唯一的追踪 ID | `a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6` |
| `span_id` | 当前 Span 的唯一 ID | `0f1e2d3c4b5a6978` |
| `parent_span_id` | 父 Span 的 ID（根 Span 为空） | `a7b8c9d0e1f2a3b4` |
| `name` | Span 的名称，描述操作 | `HTTP POST /tasks` |
| `start_time` / `end_time` | 起始与结束时间戳 | 纳秒精度 |
| `kind` | Span 的类型 | `Server` / `Client` / `Internal` / `Producer` / `Consumer` |
| `attributes` | 键值对形式的标签 | `http.method=POST`, `db.statement=INSERT...` |
| `status` | Span 的状态 | `Ok` / `Error` |
| `events` | 带时间戳的事件（类似日志） | `"retry_attempt"`, `"cache_hit"` |
| `links` | 关联到其他 Trace 的 Span | 用于异步场景（消息队列等） |

### SpanKind 的含义

SpanKind 表示 Span 在分布式调用链中扮演的角色：

| SpanKind | 含义 | 典型场景 |
|---|---|---|
| **Server** | 接收外部请求 | HTTP Server 收到请求，RPC 服务端处理 |
| **Client** | 发起对外请求 | HTTP Client 请求下游，数据库查询 |
| **Internal** | 内部操作 | 参数校验、数据处理、缓存操作 |
| **Producer** | 发送异步消息 | 向 Kafka / RabbitMQ 发送消息 |
| **Consumer** | 接收异步消息 | 从消息队列消费消息 |

在实际的 Trace 中，一个服务的 Server Span 往往对应上游服务的 Client Span：

```
服务 A                               服务 B
┌──────────────────┐                ┌──────────────────┐
│ Span (Client)    │ ─── HTTP ───→  │ Span (Server)    │
│ kind = Client    │                │ kind = Server    │
│ 调用 B 的 /api   │ ←── 返回 ────  │ 处理 /api 请求    │
└──────────────────┘                └──────────────────┘
```

## 上下文传播（Context Propagation）

在分布式系统中，一次请求往往经过多个服务。要让这些服务产生的 Span 被关联到同一个 Trace，需要**跨进程传递上下文**。

### W3C TraceContext 标准

W3C 制定了 TraceContext 标准（[W3C Recommendation](https://www.w3.org/TR/trace-context/)），定义了如何通过 HTTP Header 传递追踪上下文：

```
traceparent: 00-<trace_id>-<span_id>-<trace_flags>
               ↑  32 hex      ↑ 16 hex    ↑ 2 hex

示例：
traceparent: 00-a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6-0f1e2d3c4b5a6978-01
              │  │                                │                │
              │  trace_id (16 bytes)              span_id (8 bytes) flags (sampled)
              version
```

同时还有一个标准 Header 用于传递厂商特定的追踪状态：
```
tracestate: vendor1=value1,vendor2=value2
```

### 传播流程

```
请求进入                         请求发出
┌──────────┐                    ┌──────────┐
│ 服务 A    │                    │ 服务 B    │
│          │                    │          │
│ 1. 收到请求                     │ 3. 从 Header 提取 trace_id │
│    (无 traceparent)            │    将当前 Span 设为子 Span  │
│    创建根 Span                  │    生成新的 span_id       │
│    trace_id = 随机生成          │                          │
│    span_id = "001"            │ 4. 准备调用下游             │
│                               │    更新 traceparent:       │
│ 2. 调用服务 B                   │    span_id = "002"        │
│    设置 Header:                │    发送请求               │
│    traceparent:                │                          │
│    trace_id=xxx,              │ 5. 收到响应，结束 Span      │
│    span_id="001"              │                          │
│    → 发送请求                  └──────────┘
└──────────┘
```

这样，无论请求穿过多少层服务，所有的 Span 都共享同一个 `trace_id`，构成一棵完整的 Span 树。

## 采样（Sampling）

在生产环境中，不可能 100% 记录所有请求的 Trace——那会产生巨大的数据量和性能开销。采样决定了哪些请求被追踪。

### 常见采样策略

| 策略 | 原理 | 适用场景 |
|---|---|---|
| **固定概率采样** | 以固定比例（如 10%）随机决定是否追踪 | 常规生产环境，了解整体延迟分布 |
| **速率限制采样** | 每秒最多追踪 N 个请求 | 保护后端不被追踪数据压垮 |
| **头部采样（Head Sampling）** | 在 Trace 开始时（第一个 Span）决定 | 实现简单，但可能丢失有趣的慢请求 |
| **尾部采样（Tail Sampling）** | 在 Trace 完成后根据结果决定是否保留 | 保留所有错误 + P99 慢请求，精准但复杂 |

### 头部采样 vs 尾部采样

```
头部采样（在请求入口决定）:
  收到请求 → 随机决定(10%) → 是 → 记录所有 Span
                            → 否 → 不记录（丢失）
  问题：命中 10% 的请求可能全部正常，而某个非 10% 的请求可能是 P99 慢请求，永远被丢弃。

尾部采样（在请求完成后决定）:
  收到请求 → 先全部记录（或缓冲）→ Trace 完成后判断：
    - 是否有 Error？→ 保留
    - 延迟 > P99 阈值？→ 保留
    - 否则 → 丢弃
  优势：精准保留"有问题的 Trace"
  代价：需要缓冲所有 Span 直到 Trace 结束，内存和计算开销更大
```

**注意**：尾部采样需要 Collector 层面的支持（OTel Collector 的 `tail_sampling` processor），且需要 Trace 完整结束后才能决策。在实际实践中，经常采用**混合策略**：头部按 10% 随机采样 + 尾部强制保留所有 Error / P99 慢请求。

## Span 可视化

### Gantt 图（瀑布图）

Jaeger UI 中最经典的可视化方式，横轴是时间，每行是一个 Span：

```
Span A (HTTP POST /tasks)      ████████████████████████████  150ms
  Span B (validate_input)      ██                            5ms
  Span C (db_insert)           ████████                      40ms
  Span D (cache_warmup)        ████████████████████          95ms
  └────────────────────────────────────────────────────────→ 时间
```

一目了然：**cache_warmup 占了总时间的 63%，是主要瓶颈。**

### 火焰图（服务/依赖视角）

以服务为单位展示调用关系和耗时占比，适合在微服务架构中快速定位瓶颈服务。

## 插桩方式

### 自动插桩（Auto-instrumentation）

无需修改代码，通过 Agent / 字节码增强 / RPC 框架中间件自动创建 Span：

- Java：通过 `-javaagent:opentelemetry-javaagent.jar` 自动追踪 Spring/HTTP 调用
- Rust：通过 `tower_http::TraceLayer` 等中间件自动为每个 HTTP 请求创建 Span
- Python：`opentelemetry-instrument` 命令自动注入 Django/Flask 等

**优点**：零代码侵入  
**缺点**：只能追踪框架和库的调用，业务逻辑不可见

### 手动插桩（Manual Instrumentation）

开发者在业务代码中显式创建 Span、添加属性和事件：

```rust
let span = tracer.span_builder("db_insert")
    .with_kind(SpanKind::Client)
    .start(&tracer);

span.set_attribute(KeyValue::new("db.table", "tasks"));
span.add_event("row_written", vec![KeyValue::new("row_count", 1)]);
// ... 执行操作 ...
span.set_status(Status::Ok);
span.end();
```

**优点**：业务语义精确，可以标记关键操作  
**缺点**：代码侵入性强，维护成本高

### 注解 / 宏方式（框架提供）

Rust `tracing` 库的 `#[instrument]` 宏是一种半自动方式：

```rust
#[tracing::instrument(name = "create_task", skip_all, fields(task_id))]
async fn create_task(payload: TaskPayload) -> Result<Task> {
    // 函数内代码自动处于此 Span 下
    // task_id 字段可通过 tracing::Span::current().record() 注入
}
```

**优点**：声明式，简洁；自动记录参数和返回值  
**缺点**：受限于语言/框架支持

## 本项目中的链路追踪实现

| 课程 | 插桩方式 | 上下文传播 | 导出方式 |
|---|---|---|---|
| 课程 1 | `#[tracing::instrument]` 宏 + `tower_http::TraceLayer` | W3C TraceContext 自动传播（`TraceContextPropagator`） | OTLP → Jaeger |
| 课程 2 | `#[tracing::instrument]` 宏，通过 `tracing-opentelemetry` 桥接 | 同课程 1 | OTLP → Jaeger |
| 课程 3 | 手动 `span_builder.start()` / `span.end()` | W3C TraceContext（手动配置 `TextMapPropagator`） | OTLP → Jaeger |

三种方式的核心区别在于**对开发者的侵入程度**：课程 1/2 使用 Rust 生态的声明式宏，开发体验最好；课程 3 完全手动，帮助你理解 Span 生命周期的底层细节。
