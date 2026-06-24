# Course 3: Pure OpenTelemetry OTLP

**No `tracing` crate. No `#[instrument]` macros. Pure OpenTelemetry Rust SDK.**

This course demonstrates raw OpenTelemetry usage in Rust without the `tracing` crate.
You will learn:

1. **Lesson 01 (otel-init):** OTel SDK initialization boilerplate. Set up TracerProvider,
   MeterProvider, LoggerProvider with OTLP gRPC exporters. Compare the verbosity with
   Course 2's concise `tracing-subscriber` approach.

2. **Lesson 02 (otel-logs):** Use the OTel Logs API directly. Build `LogRecord` instances
   with severity, attributes, and body. Emit them through a configured `Logger`.

3. **Lesson 03 (otel-metrics):** Use the OTel Metrics API directly. Create Counters and
   Histograms, record measurements with attributes.

4. **Lesson 04 (otel-traces):** Manual span management. Call `span.start()` and `span.end()`
   explicitly. Set span kind, attributes, and status. No macros.

5. **Lesson 05 (collector-pipeline):** Same code as Lesson 04, with commentary on how the
   OTLP Collector pipeline processes and exports telemetry data.

## Key differences from Course 2

| Aspect | Course 2 (tracing crate) | Course 3 (Pure OTel) |
|--------|-------------------------|---------------------|
| Instrumentation | `#[instrument]` macro | Manual `span.start()` / `span.end()` |
| Logging | `tracing::info!()` macro | `logger.emit(LogRecord)` |
| Context propagation | Automatic via `TraceLayer` | Manual or via OTel propagators |
| Setup | `tracing_subscriber::fmt().init()` | 30+ lines of exporter/provider boilerplate |
| Span fields | `tracing::Span::record()` | `span.set_attribute(KeyValue::new(...))` |

## Running

Start the OTLP Collector first:
```bash
docker-compose up -d collector
```

Then run any lesson:
```bash
cd courses/03-otel-otlp
cargo run -p lesson-01-otel-init
```
