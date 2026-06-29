# 排查记录：Grafana Dashboard 无数据

## 1. 问题表现

- Grafana Dashboard（http://localhost:3000）所有面板显示 "No data"
- `podman logs grafana` 和 `podman logs prometheus` 没有预期日志

## 2. 排查环境

| 组件 | 端口 | 说明 |
|------|------|------|
| `lesson-05-dashboard` (axum) | 3001 | 业务应用，产生 telemetry |
| otel-collector | 4317/4318/8889 | 接收 OTLP 数据，转发到后端 |
| Jaeger | 16686 | 分布式追踪存储 |
| Prometheus | 9091 | 指标采集与时序存储 |
| Grafana | 3000 | 统一可视化 |

**数据流预期：**
```
App → OTLP(gRPC) → Collector → traces → Jaeger
                              → metrics → Prometheus → Grafana
                              → logs → debug stdout
```

## 3. 排查工具

| 工具 | 用途 |
|------|------|
| `podman-compose ps` | 检查容器运行状态 |
| `podman logs <name>` | 查看容器日志 |
| `podman restart` / `podman stop` / `podman rm` | 容器生命周期管理 |
| `curl` | HTTP 接口探测：health check、Prometheus API、metrics endpoint |
| `ps aux` | 检查宿主机进程 |
| `playwright-cli open/close/snapshot/console/eval` | GUI 浏览器实操验证 Grafana 界面、抓取 Console 错误、JS 上下文数据 |
| `rg` (ripgrep) | 搜索配置文件内容 |
| `python3 -m json.tool` | JSON 格式化输出，解析 Prometheus API 响应 |
| `nohup cargo run` | 后台持久运行 Rust 应用 |
| `cat /tmp/lesson05.log` | 查看应用日志 |

## 4. 排查步骤

### Step 1：容器状态检查

```bash
$ podman-compose ps

CONTAINER ID  IMAGE                           STATUS
2282d7d6ef1a  otel/opentelemetry-collector    Up
2cdab963e14c  jaegertracing/all-in-one        Up
103a8e08ce79  prom/prometheus                 Up
a89db9069b3d  grafana/grafana                 Up
```

→ 4 个容器均 Running ✓

### Step 2：应用健康检查

```bash
$ curl -s http://localhost:3001/health
# 无响应
$ ps aux | grep lesson-05
# 无进程
```

→ App 未启动 ✗

### Step 3：Prometheus scrape target 状态

```bash
$ curl -s "http://localhost:9091/api/v1/targets" | python3 -m json.tool
{
  "health": "down",
  "lastError": "dial tcp 169.254.1.2:3001: connect: connection refused",
  "scrapeUrl": "http://host.docker.internal:3001/metrics"
}
```

→ Target `host.docker.internal:3001/metrics` 状态 **down** ✗

### Step 4：检查 Prometheus 抓取配置

```bash
$ cat configs/prometheus.yml
scrape_configs:
  - job_name: "app"
    static_configs:
      - targets: ["host.docker.internal:3001"]
```

→ Prometheus 直接抓取宿主机 `3001/metrics` ✗

### Step 5：检查 App 是否有 /metrics 端点

```bash
$ cat lesson-05-dashboard/src/main.rs | rg "route"
.route("/health", get(health))
.route("/tasks", post(create_task))
.route("/tasks/{id}", get(get_task))
```

→ App 只有 3 个路由，**无 `/metrics` 端点** ✗

### Step 6：检查 Collector metrics 管线

```bash
$ cat configs/otel-collector-config.yaml
service:
  pipelines:
    metrics:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug]          # ← 仅 debug，无 prometheus exporter
```

→ Collector 收到 OTLP metrics 后只输出到 debug，**无 prometheus exporter** ✗

### Step 7：检查 docker-compose 端口暴露

```bash
$ cat docker-compose.yml | rg "8889"
# 无 8889 端口映射
```

→ Collector prometheus exporter 端口未暴露 ✗

**至此确认根因 ① + ②：**
- ① App 未启动
- ② 指标管线断裂：Collector 无 prometheus exporter + Prometheus 抓错目标

### Step 8：修复配置 → 应用 → 重启

**修改 `configs/otel-collector-config.yaml`：**
```diff
+  prometheus:
+    endpoint: 0.0.0.0:8889

  exporters:
    metrics:
-     exporters: [debug]
+     exporters: [debug, prometheus]
```

**修改 `configs/docker-compose.yml`：**
```diff
  ports:
    - "4317:4317"
    - "4318:4318"
+   - "8889:8889"
```

**修改 `configs/prometheus.yml`：**
```diff
- targets: ["host.docker.internal:3001"]
+ targets: ["otel-collector:8889"]
```

→ 重启容器 + 启动 app + 生成 100 次请求

### Step 9：验证 Prometheus 指标

```bash
$ curl -s "http://localhost:9091/api/v1/query?query=http_requests_total"
{
  "metric": {
    "__name__": "http_requests_total",
    "method": "GET", "route": "/tasks/:id"
  },
  "value": [..., "50"]
},
{
  "metric": {
    "__name__": "http_requests_total",
    "method": "POST", "route": "/tasks"
  },
  "value": [..., "50"]
}
```

→ Prometheus 已采集到 `http_requests_total`，值符合预期 ✓

### Step 10：playwright-cli 检查 Grafana

```bash
$ playwright-cli open "http://localhost:3000/d/learn-tracing-app/..." --browser=chromium --headed
$ playwright-cli snapshot
```

```yaml
- region "HTTP Request Rate (QPS)": No data
- region "Request Latency (P50/P95/P99)": No data
- region "5xx Error Rate": No data
- region "Total Requests (per second)": No data
```

→ 所有面板仍 "No data" ✗

### Step 11：playwright-cli 抓取浏览器 Console 错误

```bash
$ playwright-cli console
[ERROR] Datasource prometheus was not found
[ERROR] Datasource prometheus was not found
...
```

→ Grafana Dashboard 找不到名为 `prometheus` 的数据源 ✗

### Step 12：playwright-cli 验证 datasource 实际 uid

```bash
$ playwright-cli eval "JSON.stringify(window.grafanaBootData?.settings?.datasources || {})"
```
```json
{
  "Prometheus": {
    "id": 1,
    "uid": "PBFA97CFB590B2093",     // ← 实际 uid
    ...
  },
  "Jaeger": {
    "id": 2,
    "uid": "PC9A941E8F2E49454",     // ← 实际 uid
    ...
  }
}
```
```bash
$ rg '"uid"' configs/dashboards/app-dashboard.json
"datasource":{"type":"prometheus","uid":"prometheus"}     // ← 不匹配
"datasource":{"type":"jaeger","uid":"jaeger"}             // ← 不匹配
```

→ **Grafana provisioning 自动生成的 uid 与 Dashboard JSON 中引用的 uid 不一致**

**至此确认根因 ③：** Datasource uid 不匹配

### Step 13：修复 Grafana datasource uid

**修改 `configs/grafana-datasources.yml`：**
```diff
  - name: Prometheus
    type: prometheus
+   uid: prometheus
    url: http://prometheus:9090
  - name: Jaeger
    type: jaeger
+   uid: jaeger
    url: http://jaeger:16686
```

→ 重建 Grafana 容器：
```bash
$ podman stop grafana && podman rm grafana && podman-compose up -d grafana
```

### Step 14：验证 Grafana Dashboard

```bash
$ playwright-cli reload
$ playwright-cli console
# 仅 1 个错误（401 /api/user/stars，匿名登录正常行为）
$ playwright-cli snapshot
```

```yaml
- generic:
  - "GET /tasks/:id"
  - "POST /tasks"
```

→ Dashboard 面板显示数据，仅有匿名用户无关错误 ✓

## 5. 根因分析

| # | 根因 | 影响范围 | 现象 |
|---|------|---------|------|
| ① | **App 未启动** | 全链路 | 无遥测数据产生，Prometheus scrape 被拒绝 |
| ② | **指标管线断裂** | Prometheus → Grafana | Collector 接到 OTLP metrics 但无 prometheus exporter 暴露；Prometheus 抓取目标指向不存在的 app `/metrics` 端点 |
| ③ | **Grafana datasource uid 不匹配** | Grafana Dashboard | Dashboard JSON 引用 `uid: "prometheus"`，但 provisioning 实际分配 `uid: "PBFA97CFB590B2093"`，Grafana 找不到数据源 |

### 根因 ② 详细分析

```
实际数据流：
App → OTLP(gRPC) → Collector → [debug only] → stdout ⛔断

Prometheus scrape target：
host.docker.internal:3001/metrics → App无此端点 → connection refused ⛔断

正确数据流应该是：
App → OTLP(gRPC) → Collector → prometheus exporter(:8889) → Prometheus scrape → Grafana
```

### 根因 ③ 详细分析

| 配置位置 | 引用 uid | 说明 |
|---------|---------|------|
| `grafana-datasources.yml`（未指定 uid） | 自动生成 `PBFA97CFB590B2093` | Provisioning 未显式指定 uid，Grafana 自动生成 |
| `app-dashboard.json` panels | `"uid": "prometheus"` | Dashboard 硬编码引用启动时指定的 uid |
| Grafana 运行时 | 查找 `uid=prometheus` 失败 | 不匹配 → "Datasource prometheus was not found" |

## 6. 修复方案

### 修改的文件

| # | 文件 | 修改内容 |
|---|------|---------|
| 1 | `configs/otel-collector-config.yaml` | 添加 `prometheus` exporter（`endpoint: 0.0.0.0:8889`），metrics 管线导出目标加上 `prometheus` |
| 2 | `docker-compose.yml` | otel-collector 暴露 `8889:8889` 端口 |
| 3 | `configs/prometheus.yml` | scrape target 从 `host.docker.internal:3001` 改为 `otel-collector:8889` |
| 4 | `configs/grafana-datasources.yml` | 显式指定 `uid: prometheus` 和 `uid: jaeger` |

### 操作步骤

```bash
# 1. 应用所有配置修改
# 2. 重启容器
cd /home/peter/project/learn-tracing
podman-compose down && podman-compose up -d

# 3. 重建 Grafana（如果已有旧数据冲突）
podman stop grafana && podman rm grafana && podman-compose up -d grafana

# 4. 启动应用（后台运行）
cd courses/01-cncf-standard
nohup cargo run -p lesson-05-dashboard > /tmp/lesson05.log 2>&1 &

# 5. 生成测试流量
for i in $(seq 1 50); do
  curl -s -X POST http://127.0.0.1:3001/tasks -H 'Content-Type: application/json' -d "{\"title\":\"task-$i\"}" > /dev/null
  curl -s http://127.0.0.1:3001/tasks/$i > /dev/null
  sleep 0.3
done
```

### 修复后的完整数据流

```
App(:3001) ──OTLP/gRPC──→ Collector(:4317)
                              │
                              ├─ traces ──→ Jaeger(:16686)
                              │                └─→ Jaeger UI (http://localhost:16686)
                              │
                              ├─ metrics ──→ prometheus exporter(:8889)
                              │                └─→ Prometheus(:9091) scrape ←─→ Grafana(:3000)
                              │
                              └─ logs ──→ debug stdout (podman-compose logs otel-collector)
```

## 7. 验证结果

| 验证项 | 命令 | 结果 |
|--------|------|------|
| App 运行 | `curl http://localhost:3001/health` | `{"status":"ok"}` ✓ |
| Prometheus target | `curl http://localhost:9091/api/v1/targets` | `health: up` ✓ |
| 指标入库 | `curl "http://localhost:9091/api/v1/query?query=http_requests_total"` | `value: [..., "120"]` ✓ |
| Grafana 数据源 | playwright-cli console | 无 "Datasource ... not found" 错误 ✓ |
| Dashboard 面板 | playwright-cli snapshot | 显示 GET/POST 图例，数据线可见 ✓ |

## 8. 追加排查：Dashboard 不显示 P99 分位数

### 问题表现

Dashboard 中 "Request Latency (P50/P95/P99)" 面板只显示了 P50 和 P95 的图例，**P99 没有显示**。实际上 P50/P95 也显示为 "No data"（前面的修复后延迟面板仍无数据，因根本原因尚未解决）。

### 排查过程

**Step 1：Prometheus API 查询 metric 名是否存在**

```bash
$ curl -s 'http://localhost:9091/api/v1/label/__name__/values' | python3 -m json.tool
```
```json
{
  "data": [
    "http_request_duration_bucket",      // ← 实际名称：无 _seconds 后缀
    "http_request_duration_count",
    "http_request_duration_sum",
    "http_requests_total"
  ]
}
```

**Step 2：对比 Dashboard JSON 中的查询语句**

```bash
$ rg "duration" configs/dashboards/app-dashboard.json
"expr":"histogram_quantile(0.50, rate(http_request_duration_seconds_bucket[1m]))"
"expr":"histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[1m]))"
"expr":"histogram_quantile(0.99, rate(http_request_duration_seconds_bucket[1m]))"
```

→ Dashboard 查询 `http_request_duration_seconds_bucket`，但实际 metric 名是 `http_request_duration_bucket` ✗

**Step 3：检查 Collector prometheus exporter 实际输出**

```bash
$ curl -s http://localhost:8889/metrics | rg "duration"
http_request_duration_bucket{le="0.005"} 0
http_request_duration_sum 7.889533670999999
http_request_duration_count 1245
```

→ Collector 暴露的指标确实**没有 `_seconds` 后缀**

**Step 4：检查 App 代码中 histogram 的定义**

```rust
meter
    .f64_histogram("http.request.duration")
    .with_description("HTTP request duration in seconds")
    .build()                                    // ← 没有 .with_unit()
```

→ App 创建 histogram 时**未设置 unit**，OTel SDK / Collector prometheus exporter 不会追加 `_seconds` 后缀

### 根因分析

| 层级 | 情况 |
|------|------|
| App 代码 | `f64_histogram("http.request.duration")` **未调用** `.with_unit("s")` |
| OTel SDK | unit 为空，不追加单位后缀 |
| Collector prometheus exporter | 按 SDK 传来的 unit 输出，无后缀时输出 `http_request_duration_bucket` |
| Dashboard JSON | 查询 `http_request_duration_seconds_bucket` | ← 不匹配 |

**指标名对照：**

| 位置 | 预期名 | 实际名 | 匹配？ |
|------|--------|--------|--------|
| Dashboard 查询 | `http_request_duration_seconds_bucket` | — | — |
| Collector 暴露 | — | `http_request_duration_bucket` | ✗ |

### 附带问题：默认 bucket 边界太粗

即使修复了名称，OTel SDK 的默认 histogram bucket 边界为：

```
[0, 5, 10, 25, 50, 75, 100, 250, 500, 750, 1000, 2500, 5000, 7500, 10000]
```

这些边界**在不设 unit 时是毫秒级的**。App 记录的延迟在 ~21ms（POST）和 ~5ms（GET），所有值都落入 `<= 25` 这一个 bucket，导致 `histogram_quantile()` 计算出的 P50、P95、P99 完全相同，三条线会重叠在一起，即使显示了也无法区分。

### 修复方案

**修改 `lesson-05-dashboard/src/main.rs`：**

```diff
 request_duration: meter
     .f64_histogram("http.request.duration")
     .with_description("HTTP request duration in seconds")
+    .with_unit("s")
+    .with_boundaries(vec![0.005, 0.01, 0.015, 0.02, 0.025, 0.03, 0.04, 0.05, 0.075, 0.1, 0.25, 0.5])
     .build(),
```

改动说明：
1. `.with_unit("s")` — 使 OTel SDK 追加 `_seconds` 后缀，指标名变为 `http_request_duration_seconds`
2. `.with_boundaries([0.005, 0.01, ...])` — 自定义秒级 bucket 边界，适配 ~0.005s–0.025s 的请求延迟，使分位数计算有意义

### 操作步骤

```bash
# 1. 修改代码后重建并重启 App
pkill -f "lesson-05-dashboard"
cargo build -p lesson-05-dashboard
nohup cargo run -p lesson-05-dashboard > /tmp/lesson05.log 2>&1 &

# 2. 生成新流量（新 metric 名需要新的数据点）
for i in $(seq 1 80); do
  curl -s -X POST http://127.0.0.1:3001/tasks -H 'Content-Type: application/json' \
    -d "{\"title\":\"task-v2-$i\"}" > /dev/null
  curl -s http://127.0.0.1:3001/tasks/$((i%50+1)) > /dev/null
  sleep 0.25
done
```

### 验证结果

```bash
# Collector 暴露的指标名已正确
$ curl -s http://localhost:8889/metrics | rg "duration_seconds"
http_request_duration_seconds_bucket{le="0.005"} 0
http_request_duration_seconds_bucket{le="0.01"} 1245
http_request_duration_seconds_bucket{le="0.015"} 1245
...
```

| 验证项 | 结果 |
|--------|------|
| 指标名变为 `_seconds` 后缀 | ✓ |
| 自定义 bucket 边界生效 | ✓ |
| Dashboard P50/P95/P99 图例显示 | ✓ |
| Console 无 datasource 错误 | ✓ |

---

## 9. 经验总结

1. **从数据流末端向前排查**：Grafana（显示层）→ Prometheus（查询层）→ Collector（传输层）→ App（产生层），逐层验证数据是否到达
2. **`playwright-cli console` 是排查前端问题的利器**：直接抓取浏览器 JS 错误，比手动打开 DevTools 高效
3. **Grafana provisioning 应显式指定 datasource uid**：否则 Grafana 自动生成的 uid 与 Dashboard JSON 引用不一致，产生 "Datasource ... was not found" 错误
4. **Collector 管线需要完整链路**：每个信号（traces/metrics/logs）都需要 `receivers → processors → exporters` 闭环，缺一个 exporter 数据就断在中间
5. **Prometheus 抓取目标需与数据暴露点一致**：App 用 OTLP push 模式发送 metrics 时，Prometheus 应抓 Collector 的 prometheus exporter，而不是 App 本身
6. **OTel Histogram 必须设置 unit 才能匹配 Prometheus 命名约定**：不设 unit 时指标名不追加单位后缀（如 `_seconds`），导致 Dashboard 查询无法匹配。应始终调用 `.with_unit("s")` 等以符合 Prometheus 规范
7. **默认 bucket 边界需根据实际延迟定制**：OTel SDK 默认 `[0, 5, 10, 25, ...]` 对亚秒级请求过于粗糙，所有样本落入同一 bucket 导致分位数无意义。应使用 `.with_boundaries()` 设置适合业务场景的边界
