# 添加 Loki 日志查看功能

## 目标

为项目添加完整的日志查看能力，通过 Loki + Grafana 实现日志聚合、存储和可视化查询。

## 范围

1. 新增 Loki 服务到 docker-compose.yml
2. OTel Collector 日志管道新增 loki exporter
3. Grafana 新增 Loki 数据源
4. Grafana Dashboard 新增日志面板（Application Logs + Error Logs）

## 详细设计

### 1. Docker Compose — 新增 Loki 服务

在 `docker-compose.yml` 中添加：

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

### 2. Loki 配置 — `configs/loki-config.yaml`

```yaml
auth_enabled: false

server:
  http_listen_port: 3100

common:
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

### 3. OTel Collector — 新增 loki exporter

修改 `configs/otel-collector-config.yaml`：

在 `exporters` 中添加：
```yaml
  loki:
    endpoint: http://loki:3100/loki/api/v1/push
```

在 `service.pipelines.logs` 中修改 exporters：
```yaml
    logs:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug, loki]
```

### 4. Grafana Datasource — 新增 Loki

修改 `configs/grafana-datasources.yml`，添加：
```yaml
  - name: Loki
    type: loki
    uid: loki
    url: http://loki:3100
    access: proxy
```

### 5. Grafana Dashboard — 新增日志面板

修改 `configs/dashboards/app-dashboard.json`，在 panels 数组中已有 5 个面板（id: 1-5, y: 0-8），新增：

| panel id | 类型 | 位置 | 说明 |
|----------|------|------|------|
| 6 | logs | 0, y=8, w=12, h=10 | Application Logs — `{service_name="learn-tracing-cncf"}` |
| 7 | logs | 12, y=8, w=12, h=10 | Error Logs — `{service_name="learn-tracing-cncf"} | level=~"error|warn"` |

面板关键字段：
- `datasource: { type: "loki", uid: "loki" }`
- `type: "logs"`
- `expr` 使用 LogQL label matcher
- 保留搜索框、时间范围同步等默认功能

## 验证方式

1. `podman-compose up -d loki` 启动 Loki
2. 启动 lesson-05-dashboard app
3. 发送请求触发日志
4. 在 Grafana Dashboard 中验证日志面板有数据
5. `podman-compose logs loki` 检查 Loki 无错误

## 影响的文件

| 文件 | 操作 |
|------|------|
| `docker-compose.yml` | 新增 loki 服务定义 |
| `configs/loki-config.yaml` | 新建 |
| `configs/otel-collector-config.yaml` | 修改 exporters + logs pipeline |
| `configs/grafana-datasources.yml` | 新增 Loki 数据源 |
| `configs/dashboards/app-dashboard.json` | 新增 2 个日志面板 |
