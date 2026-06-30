# Changelog

## [Unreleased]

### Fixed
- **Metrics 导出间隔过长导致测试失败**: `lesson-03-metrics` 和 `lesson-05-dashboard` 中 `SdkMeterProvider` 使用 `PeriodicReader` 自定义 5 秒导出间隔，替代默认的 60 秒间隔，确保 `test-observability.sh` 的指标检查不再因导出超时而失败。
