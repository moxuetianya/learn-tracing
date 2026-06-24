# Course 2: Rust Native Tracing

This course teaches observability using the Rust-native ecosystem: the `tracing` crate, its subscriber infrastructure, and bridges to Prometheus metrics and OpenTelemetry traces.

Unlike Course 1 (which uses OpenTelemetry SDKs directly), this course shows the *idiomatic Rust* approach—instrumenting code with `#[tracing::instrument]` and `tracing::info!()` macros, then composing subscribers to export data to multiple backends.

## Lessons

| # | Lesson | What You Learn |
|---|--------|----------------|
| 01 | tracing-subscriber | Initialize `tracing-subscriber` with JSON-formatted output; structured logging |
| 02 | span | Add `#[tracing::instrument]` to create parent/child spans automatically |
| 03 | metrics | Register counter/histogram metrics and expose a `/metrics` endpoint for Prometheus |
| 04 | traces | Bridge `tracing` spans to OpenTelemetry via OTLP gRPC exporter |
| 05 | integration | Compose all layers: JSON fmt layer + Prometheus metrics + OTel traces |

## Running

Each lesson is an independent binary:

```bash
cd lesson-01-tracing-subscriber && cargo run
```

All lessons listen on `http://127.0.0.1:3001`.

## Prerequisites

- Rust toolchain (edition 2021)
- For Lesson 04/05: a local OpenTelemetry Collector or backend accepting OTLP on `localhost:4317`
- For Lesson 03/05: Prometheus configured to scrape `http://localhost:3001/metrics`
