# 为什么 `with_simple_exporter` 会阻塞程序？

## 现象

`lesson-04-traces` 程序启动后，第一个 `info!("Server starting...")` 日志正常输出，但后续在 HTTP handler 中的 `info!()` 调用会**永久阻塞**，导致请求无法响应。即使 otel-collector 已正常启动，阻塞依然发生。

## 根因：`SimpleLogProcessor` + tracing bridge 造成循环依赖死锁

### 完整调用链

`info!()` 宏触发的事件经过以下路径：

```
info!("health check called")                     // main.rs:143
  → OpenTelemetryTracingBridge::on_event()        // layer.rs:214 (同步回调)
    → self.logger.emit(log_record)               // layer.rs:268
      → SdkLogger::emit()                          // logger.rs:31
        → SimpleLogProcessor::emit()              // simple_log_processor.rs:78
          → self.exporter.lock()                  // ← 步骤①: 获取 std::sync::Mutex
          → futures_executor::block_on(            // ← 步骤②: 持有锁，阻塞线程
              exporter.export(batch)               // ← 步骤③: 异步 gRPC 导出
            )
```

### 死锁形成机制

关键代码在 `opentelemetry_sdk-0.29.0/src/logs/simple_log_processor.rs:88-94`：

```rust
fn emit(&self, record: &mut SdkLogRecord, ...) {
    let result = self
        .exporter
        .lock()                                      // std::sync::Mutex
        .and_then(|exporter| {
            futures_executor::block_on(              // 持有锁的同时阻塞
                exporter.export(LogBatch::new(...))   // tonic gRPC 异步调用
            )
        });
}
```

死锁发生需要两个线程：

```
线程 A（处理 HTTP 请求的 tokio worker）:
  ① 获取 SimpleLogProcessor.exporter（std::sync::Mutex）
  ② 调用 futures_executor::block_on()
  ③ 在 block_on 内部运行 tonic gRPC 导出
  ④ 导出被阻塞——需要等待线程 B 完成

线程 B（tonic 的 tower::buffer::worker 后台任务）:
  tower::buffer::worker 内部发射 tracing::trace! 事件
  → tracing subscriber 分发到 OpenTelemetryTracingBridge
  → SimpleLogProcessor::emit()
  → self.exporter.lock()  ← 尝试获取同一把锁！
  → 阻塞！锁已被线程 A 持有
```

**循环依赖：**
- 线程 A 持有锁，等待 gRPC 完成（依赖线程 B）
- 线程 B 被锁卡住（依赖线程 A 释放锁）
- 彼此等待 → 死锁

### SDK 自身的文档

SDK 有一个 `#[ignore]` 的测试（`simple_log_processor.rs:342-383`）专门演示此场景：

```rust
// This test demonstrates a potential deadlock scenario in a multi-threaded Tokio runtime.
// It spawns Tokio tasks equal to the number of runtime worker threads (4) to emit log events.
// Each task attempts to acquire a mutex on the exporter in `SimpleLogProcessor::emit`.
// Only one task obtains the lock, while the others are blocked, waiting for its release.
//
// The task holding the lock invokes the LogExporterThatRequiresTokio, which performs an
// asynchronous operation (e.g., network I/O simulated by `tokio::sleep`). This operation
// requires yielding control back to the Tokio runtime to make progress.
//
// However, all worker threads are occupied:
// - One thread is executing the async exporter operation
// - Three threads are blocked waiting for the mutex
//
// This leads to a deadlock as there are no available threads to drive the async operation
// to completion, preventing the mutex from being released.
```

## 为什么只有 Logs 的 `with_simple_exporter` 有问题？

lesson-04 中三种信号类型的导出方式不同：

| 信号类型 | 导出方式 | 是否阻塞 |
|---------|---------|---------|
| **Logs** | `with_simple_exporter` | **阻塞（死锁）** |
| Metrics | `with_periodic_exporter` | 不阻塞（后台定时推送） |
| Traces | `with_batch_exporter` | 不阻塞（异步批量推送） |

lesson-03 使用 `with_batch_exporter` 处理日志，不存在此问题。

## 为什么不是"网络慢"的问题？

即便 collector 已启动且网络通畅，死锁依然发生。原因不是 gRPC 调用慢，而是 **`futures_executor::block_on()` 持有 `std::sync::Mutex` 的同时阻塞线程**，这切断了 tokio 运行时调度其他任务的能力。

## 实验验证

### 测试 1：直接调用 SimpleLogProcessor（无 tracing bridge）

```rust
let processor = SimpleLogProcessor::new(exporter);
processor.emit(&mut record, &scope);  // ✓ 正常完成，1.6ms
processor.emit(&mut record2, &scope); // ✓ 正常完成，0.46ms
```

### 测试 2：有 tracing bridge，不在 axum handler 中

```rust
info!("first log from main");         // ✓ 正常
tokio::spawn(async {
    info!("second log from spawn");   // ✓ 正常
});
```

### 测试 3：有 tracing bridge，在 axum handler 中

```rust
async fn health() -> Json<...> {
    info!("health check called");     // ✗ 死锁！永不返回
}
```

## 修复

将 `lesson-04-traces/src/main.rs:60`（以及 `lesson-05-dashboard/src/main.rs:60`）：

```rust
// 改前：
.with_simple_exporter(log_exporter)

// 改后：
.with_batch_exporter(log_exporter)
```

`BatchLogProcessor` 使用后台线程异步批量导出日志，不会在调用线程上同步等待 gRPC 完成。

## 关键文件引用

| 文件 | 行号 | 内容 |
|------|------|------|
| `opentelemetry_sdk/.../simple_log_processor.rs` | 88-94 | `block_on()` + Mutex 的核心代码 |
| `opentelemetry_sdk/.../simple_log_processor.rs` | 342-383 | SDK 自身的死锁演示测试 |
| `opentelemetry_sdk/.../logger.rs` | 47-49 | `SdkLogger::emit()` 遍历 processor |
| `opentelemetry_appender_tracing/.../layer.rs` | 214-269 | `on_event()` 同步调用 `logger.emit()` |
| `tower-0.5.2/.../buffer/worker.rs` | 多处 | `tracing::trace!()` 触发死锁的事件源 |

## 排查方法论

采用 **systematic-debugging** 四阶段法：

1. **Phase 1 - 复现**：确认程序在请求处理时阻塞，`info!()` 调用永不返回
2. **Phase 1 - 追溯数据流**：从 `info!()` 宏一路追踪到 `block_on()`
3. **Phase 2 - 对照实验**：分离 tracing bridge / axum handler 等变量，逐个排除
4. **Phase 3 - 源码证据**：阅读 SDK 源码，验证假设
5. **Phase 4 - 修复**：用 `with_batch_exporter` 替代
