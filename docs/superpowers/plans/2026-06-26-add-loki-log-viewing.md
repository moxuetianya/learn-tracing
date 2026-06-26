# Add Loki Log Viewing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Loki + Grafana log viewing to the project, enabling log exploration and dashboard log panels.

**Architecture:** Add Loki as a new container service receiving logs from OTel Collector via loki exporter, registered as a Grafana datasource, with 2 new log panels in the existing dashboard.

**Tech Stack:** Grafana Loki (latest), otel-collector-contrib lokiexporter, Grafana logs panel type

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `configs/loki-config.yaml` | Create | Loki storage and server config |
| `docker-compose.yml` | Modify | Add loki service definition |
| `configs/otel-collector-config.yaml` | Modify | Add loki exporter, update logs pipeline |
| `configs/grafana-datasources.yml` | Modify | Add Loki datasource entry |
| `configs/dashboards/app-dashboard.json` | Modify | Add 2 log panels (id:6, id:7) |

---

### Task 1: Create Loki configuration file

**Files:**
- Create: `configs/loki-config.yaml`

- [ ] **Step 1: Write loki-config.yaml**

```yaml
auth_enabled: false

server:
  http_listen_port: 3100

common:
  ring:
    kvstore:
      store: inmemory
  instance_addr: 127.0.0.1
  path_prefix: /loki
  storage:
    filesystem:
      chunks_directory: /loki/chunks
      rules_directory: /loki/rules

schema_config:
  configs:
    - from: 2024-01-01
      store: tsdb
      object_store: filesystem
      schema: v13
      index:
        prefix: index_
        period: 24h

limits_config:
  allow_structured_metadata: true
```

- [ ] **Step 2: Verify file created**

Run: `ls -la configs/loki-config.yaml`
Expected: File exists.

- [ ] **Step 3: Commit**

```bash
git add configs/loki-config.yaml
git commit -m "feat: add Loki configuration file"
```

---

### Task 2: Add Loki service to docker-compose.yml

**Files:**
- Modify: `docker-compose.yml`

- [ ] **Step 1: Add loki service definition**

Insert after the `prometheus` service block (after line 33), before `grafana`:

```yaml
  loki:
    image: docker.io/grafana/loki:latest
    container_name: loki
    command: -config.file=/etc/loki/loki-config.yaml
    volumes:
      - ./configs/loki-config.yaml:/etc/loki/loki-config.yaml
    ports:
      - "3100:3100"
```

The edited file should look like:

```yaml
version: "3.8"
services:
  otel-collector:
    image: docker.io/otel/opentelemetry-collector-contrib:latest
    container_name: otel-collector
    command: ["--config=/etc/otel-collector-config.yaml"]
    environment:
      - no_proxy=jaeger,prometheus,grafana,localhost,127.0.0.1,::1
      - NO_PROXY=jaeger,prometheus,grafana,localhost,127.0.0.1,::1
    volumes:
      - ./configs/otel-collector-config.yaml:/etc/otel-collector-config.yaml
    ports:
      - "4317:4317"
      - "4318:4318"
      - "8889:8889"

  jaeger:
    image: docker.io/jaegertracing/all-in-one:1
    container_name: jaeger
    environment:
      - COLLECTOR_OTLP_ENABLED=true
    ports:
      - "16686:16686"

  prometheus:
    image: docker.io/prom/prometheus:latest
    container_name: prometheus
    extra_hosts:
      - "host.docker.internal:host-gateway"
    volumes:
      - ./configs/prometheus.yml:/etc/prometheus/prometheus.yml
    ports:
      - "9091:9090"

  loki:
    image: docker.io/grafana/loki:latest
    container_name: loki
    command: -config.file=/etc/loki/loki-config.yaml
    volumes:
      - ./configs/loki-config.yaml:/etc/loki/loki-config.yaml
    ports:
      - "3100:3100"

  grafana:
    image: docker.io/grafana/grafana:latest
    container_name: grafana
    environment:
      - GF_AUTH_ANONYMOUS_ENABLED=true
      - GF_AUTH_ANONYMOUS_ORG_ROLE=Admin
    volumes:
      - ./configs/grafana-datasources.yml:/etc/grafana/provisioning/datasources/datasources.yml
      - ./configs/grafana-dashboards.yml:/etc/grafana/provisioning/dashboards/dashboards.yml
      - ./configs/dashboards:/etc/grafana/provisioning/dashboards
    ports:
      - "3000:3000"
```

- [ ] **Step 2: Verify syntax**

Run: `podman-compose config --quiet`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add docker-compose.yml
git commit -m "feat: add Loki service to docker-compose"
```

---

### Task 3: Add loki exporter to OTel Collector config

**Files:**
- Modify: `configs/otel-collector-config.yaml`

- [ ] **Step 1: Add loki to exporters and update logs pipeline**

Add `loki` exporter under `exporters:` and add `loki` to the logs pipeline exporters.

The edited file should look like:

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318

processors:
  batch:
    timeout: 1s
    send_batch_size: 1024

exporters:
  debug:
    verbosity: detailed
  otlp/jaeger:
    endpoint: jaeger:4317
    tls:
      insecure: true
  prometheus:
    endpoint: 0.0.0.0:8889
  loki:
    endpoint: http://loki:3100/loki/api/v1/push

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug, otlp/jaeger]
    metrics:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug, prometheus]
    logs:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug, loki]
```

- [ ] **Step 2: Validate YAML syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('configs/otel-collector-config.yaml'))" 2>&1 || echo "WARNING: yaml not installed, skipping"`
Expected: No errors (or skip warning if yaml not available).

- [ ] **Step 3: Commit**

```bash
git add configs/otel-collector-config.yaml
git commit -m "feat: add Loki exporter to OTel Collector logs pipeline"
```

---

### Task 4: Add Loki datasource to Grafana

**Files:**
- Modify: `configs/grafana-datasources.yml`

- [ ] **Step 1: Add Loki datasource entry**

Append the Loki datasource after Jaeger. The edited file should look like:

```yaml
apiVersion: 1
datasources:
  - name: Prometheus
    type: prometheus
    uid: prometheus
    url: http://prometheus:9090
    access: proxy
    isDefault: true
  - name: Jaeger
    type: jaeger
    uid: jaeger
    url: http://jaeger:16686
    access: proxy
  - name: Loki
    type: loki
    uid: loki
    url: http://loki:3100
    access: proxy
```

- [ ] **Step 2: Commit**

```bash
git add configs/grafana-datasources.yml
git commit -m "feat: add Loki datasource to Grafana"
```

---

### Task 5: Add log panels to Grafana dashboard

**Files:**
- Modify: `configs/dashboards/app-dashboard.json`

**Background:** Existing panels occupy y=0..13 (ids 1-5). New panels go at y=14 (ids 6-7).

- [ ] **Step 1: Add panel 6 (Application Logs) and panel 7 (Error Logs)**

Insert the following two panel entries into the `"panels"` array in the dashboard JSON, after the last existing panel (id:5). The two new panels are:

Panel 6 — Application Logs:
```json
{
    "datasource": {
        "type": "loki",
        "uid": "loki"
    },
    "gridPos": {
        "h": 10,
        "w": 12,
        "x": 0,
        "y": 14
    },
    "id": 6,
    "options": {
        "showTime": true,
        "sortOrder": "Descending",
        "wrapLogMessage": true
    },
    "targets": [
        {
            "datasource": {
                "type": "loki",
                "uid": "loki"
            },
            "expr": "{service_name=\"learn-tracing-cncf\"}",
            "refId": "A"
        }
    ],
    "title": "Application Logs",
    "type": "logs"
}
```

Panel 7 — Error Logs:
```json
{
    "datasource": {
        "type": "loki",
        "uid": "loki"
    },
    "gridPos": {
        "h": 10,
        "w": 12,
        "x": 12,
        "y": 14
    },
    "id": 7,
    "options": {
        "showTime": true,
        "sortOrder": "Descending",
        "wrapLogMessage": true
    },
    "targets": [
        {
            "datasource": {
                "type": "loki",
                "uid": "loki"
            },
            "expr": "{service_name=\"learn-tracing-cncf\"} | level=~\"error|warn\"",
            "refId": "A"
        }
    ],
    "title": "Error Logs",
    "type": "logs"
}
```

The edit should replace the text `"type":"text"}` (end of last panel) + `],` with `"type":"text"},` + panel 6 JSON + `,` + panel 7 JSON + `],`.

Full final file content:

```json
{"annotations":{"list":[]},"editable":true,"fiscalYearStartMonth":0,"graphTooltip":1,"id":null,"links":[],"panels":[{"datasource":{"type":"prometheus","uid":"prometheus"},"fieldConfig":{"defaults":{"color":{"mode":"palette-classic"},"custom":{"axisBorderShow":false,"axisCenteredZero":false,"axisColorMode":"text","axisPlacement":"auto","fillOpacity":20,"gradientMode":"none","lineWidth":2,"pointSize":5,"scaleDistribution":{"type":"linear"},"thresholdsStyle":{"mode":"off"}},"mappings":[],"thresholds":{"mode":"absolute","steps":[{"color":"green","value":null}]},"unit":"reqps"},"overrides":[]},"gridPos":{"h":8,"w":12,"x":0,"y":0},"id":1,"options":{"legend":{"calcs":["mean","max"],"displayMode":"table","placement":"bottom"},"tooltip":{"mode":"multi","sort":"desc"}},"targets":[{"datasource":{"type":"prometheus"},"expr":"rate(http_requests_total[1m])","legendFormat":"{{method}} {{route}}","refId":"A"}],"title":"HTTP Request Rate (QPS)","type":"timeseries"},{"datasource":{"type":"prometheus","uid":"prometheus"},"fieldConfig":{"defaults":{"color":{"mode":"palette-classic"},"custom":{"axisBorderShow":false,"axisCenteredZero":false,"axisColorMode":"text","axisPlacement":"auto","fillOpacity":10,"gradientMode":"none","lineWidth":2,"pointSize":5,"scaleDistribution":{"type":"linear"},"thresholdsStyle":{"mode":"off"}},"mappings":[],"thresholds":{"mode":"absolute","steps":[{"color":"green","value":null}]},"unit":"s"},"overrides":[]},"gridPos":{"h":8,"w":12,"x":12,"y":0},"id":2,"options":{"legend":{"calcs":[],"displayMode":"table","placement":"bottom"},"tooltip":{"mode":"multi","sort":"desc"}},"targets":[{"datasource":{"type":"prometheus"},"expr":"histogram_quantile(0.50, rate(http_request_duration_seconds_bucket[1m]))","legendFormat":"P50","refId":"A"},{"datasource":{"type":"prometheus"},"expr":"histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[1m]))","legendFormat":"P95","refId":"B"},{"datasource":{"type":"prometheus"},"expr":"histogram_quantile(0.99, rate(http_request_duration_seconds_bucket[1m]))","legendFormat":"P99","refId":"C"}],"title":"Request Latency (P50/P95/P99)","type":"timeseries"},{"datasource":{"type":"prometheus","uid":"prometheus"},"fieldConfig":{"defaults":{"color":{"mode":"thresholds"},"mappings":[],"max":1,"min":0,"thresholds":{"mode":"absolute","steps":[{"color":"green","value":null},{"color":"red","value":1}]},"unit":"short"},"overrides":[]},"gridPos":{"h":6,"w":8,"x":0,"y":8},"id":3,"options":{"colorMode":"background","orientation":"auto","reduceOptions":{"calcs":["lastNotNull"]}},"targets":[{"datasource":{"type":"prometheus"},"expr":"rate(http_requests_total{status_code=~\"5..\"}[5m])","refId":"A"}],"title":"5xx Error Rate","type":"stat"},{"datasource":{"type":"prometheus","uid":"prometheus"},"fieldConfig":{"defaults":{"mappings":[],"thresholds":{"mode":"absolute","steps":[{"color":"green","value":null}]},"unit":"short"},"overrides":[]},"gridPos":{"h":6,"w":8,"x":8,"y":8},"id":4,"options":{"colorMode":"value","orientation":"auto","reduceOptions":{"calcs":["lastNotNull"]}},"targets":[{"datasource":{"type":"prometheus"},"expr":"sum(rate(http_requests_total[1m]))","refId":"A"}],"title":"Total Requests (per second)","type":"stat"},{"datasource":{"type":"jaeger","uid":"jaeger"},"gridPos":{"h":6,"w":8,"x":16,"y":8},"id":5,"options":{"content":"# [Open Jaeger UI](http://localhost:16686)\n\nClick the link above to explore traces in Jaeger.","mode":"markdown"},"type":"text"},{"datasource":{"type":"loki","uid":"loki"},"gridPos":{"h":10,"w":12,"x":0,"y":14},"id":6,"options":{"showTime":true,"sortOrder":"Descending","wrapLogMessage":true},"targets":[{"datasource":{"type":"loki","uid":"loki"},"expr":"{service_name=\"learn-tracing-cncf\"}","refId":"A"}],"title":"Application Logs","type":"logs"},{"datasource":{"type":"loki","uid":"loki"},"gridPos":{"h":10,"w":12,"x":12,"y":14},"id":7,"options":{"showTime":true,"sortOrder":"Descending","wrapLogMessage":true},"targets":[{"datasource":{"type":"loki","uid":"loki"},"expr":"{service_name=\"learn-tracing-cncf\"} | level=~\"error|warn\"","refId":"A"}],"title":"Error Logs","type":"logs"}],"refresh":"10s","schemaVersion":39,"tags":["learn-tracing"],"templating":{"list":[]},"time":{"from":"now-15m","to":"now"},"timepicker":{},"timezone":"browser","title":"Learn Tracing - App Dashboard","uid":"learn-tracing-app","version":1}
```

- [ ] **Step 2: Validate JSON**

Run: `python3 -c "import json; json.load(open('configs/dashboards/app-dashboard.json')); print('Valid JSON')"`
Expected: `Valid JSON`

- [ ] **Step 3: Commit**

```bash
git add configs/dashboards/app-dashboard.json
git commit -m "feat: add Application Logs and Error Logs panels to dashboard"
```

---

### Verification (manual)

- [ ] Start all services: `podman-compose up -d`
- [ ] Run the app: `cargo run -p lesson-05-dashboard`
- [ ] Send test requests to generate logs
- [ ] Open Grafana at `http://localhost:3000`, navigate to "Learn Tracing - App Dashboard"
- [ ] Verify "Application Logs" panel shows logs
- [ ] Verify "Error Logs" panel filters to only error/warn level logs
