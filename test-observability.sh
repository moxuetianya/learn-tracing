#!/bin/bash
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASS=0
FAIL=0
check() {
    local desc="$1"
    shift
    if "$@" > /dev/null 2>&1; then
        echo -e "${GREEN}[PASS]${NC} $desc"
        PASS=$((PASS + 1))
    else
        echo -e "${RED}[FAIL]${NC} $desc"
        FAIL=$((FAIL + 1))
    fi
}

PROJECT_DIR="/home/peter/project/learn-tracing"
APP_DIR="$PROJECT_DIR/courses/01-cncf-standard"

echo "============================================"
echo "  Observability Stack Integration Test"
echo "============================================"
echo ""

# ── Phase 1: Start services ──────────────────────────────────
echo "── Phase 1: Starting services ──"
cd "$PROJECT_DIR"
podman-compose down 2>/dev/null || true
podman-compose up -d 2>&1
sleep 8

check "otel-collector running"      podman ps --filter "name=otel-collector" --filter "status=running" --format "{{.Names}}" | rg -q otel-collector
check "jaeger running"              podman ps --filter "name=jaeger" --filter "status=running" --format "{{.Names}}" | rg -q jaeger
check "prometheus running"          podman ps --filter "name=prometheus" --filter "status=running" --format "{{.Names}}" | rg -q prometheus
check "loki running"                podman ps --filter "name=loki" --filter "status=running" --format "{{.Names}}" | rg -q loki
check "grafana running"             podman ps --filter "name=grafana" --filter "status=running" --format "{{.Names}}" | rg -q grafana

check "collector no error in logs"  podman logs otel-collector 2>&1 | tail -3 | rg -v "error|Error|ERROR|Fatal|panic"

echo ""
echo "── Phase 2: Build and start app ──"
cd "$APP_DIR"
cargo build -p lesson-05-dashboard 2>&1

# Kill any existing instance
pkill -f "lesson-05-dashboard" 2>/dev/null || true
sleep 1

nohup cargo run -p lesson-05-dashboard > /tmp/lesson05.log 2>&1 &
APP_PID=$!
sleep 3

check "app health endpoint"         curl -sf http://127.0.0.1:3001/health | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['status']=='ok'"

echo ""
echo "── Phase 3: Generate load ──"
echo "Sending 400 requests (200 POST + 200 GET)..."
for i in $(seq 1 200); do
    curl -s -X POST http://127.0.0.1:3001/tasks \
        -H 'Content-Type: application/json' \
        -d "{\"title\":\"task-$i\"}" > /dev/null
    curl -s http://127.0.0.1:3001/tasks/$i > /dev/null
done
echo "Load generation complete"

# Wait for data propagation
sleep 8

echo ""
echo "── Phase 4: Verify metrics pipeline ──"

# Check collector metrics endpoint
check "collector exports metrics"   curl -s http://localhost:8889/metrics | rg -q "http_requests_total"

# Check Prometheus target health
check "prometheus target UP"        curl -s 'http://localhost:9091/api/v1/targets' | python3 -c "
import sys,json
d=json.load(sys.stdin)
targets=d['data']['activeTargets']
assert len(targets)>0,'no targets'
t=targets[0]
assert t['health']=='up',f'health={t[\"health\"]}'
"

# Check Prometheus has metrics
check "prometheus has http_requests_total"  curl -s 'http://localhost:9091/api/v1/query?query=http_requests_total' | python3 -c "
import sys,json
d=json.load(sys.stdin)
assert d['status']=='success'
assert len(d['data']['result'])>0,'no results'
for r in d['data']['result']:
    val=float(r['value'][1])
    assert val>0,f'value={val}'
    print(f'  {r[\"metric\"][\"method\"]} {r[\"metric\"][\"route\"]}: {val}')
"

# Check Prometheus has histogram
check "prometheus has duration histogram"  curl -s 'http://localhost:9091/api/v1/query?query=http_request_duration_seconds_bucket' | python3 -c "
import sys,json
d=json.load(sys.stdin)
assert d['status']=='success'
assert len(d['data']['result'])>0,'no results'
"

# Check metric name has _seconds suffix
check "duration metric has _seconds suffix"  curl -s http://localhost:8889/metrics | rg -q "http_request_duration_seconds_bucket"

echo ""
echo "── Phase 5: Verify traces (Jaeger) ──"
check "jaeger has traces"           curl -s 'http://localhost:16686/api/traces?service=learn-tracing-cncf&limit=1' | python3 -c "
import sys,json
d=json.load(sys.stdin)
assert len(d['data'])>0,'no traces'
t=d['data'][0]
assert len(t['spans'])>0,'no spans'
print(f'  traceID={t[\"traceID\"][:16]}..., spans={len(t[\"spans\"])}')
"

echo ""
echo "── Phase 6: Verify Grafana ──"
check "grafana health"              curl -sf http://localhost:3000/api/health | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['database']=='ok'"

check "grafana datasource prometheus"  curl -s http://localhost:3000/api/datasources | python3 -c "
import sys,json
ds=json.load(sys.stdin)
prom=[d for d in ds if d['name']=='Prometheus']
assert len(prom)>0,'prometheus not found'
assert prom[0]['uid']=='prometheus',f'uid={prom[0][\"uid\"]}'
"

check "grafana datasource jaeger"   curl -s http://localhost:3000/api/datasources | python3 -c "
import sys,json
ds=json.load(sys.stdin)
jaeger=[d for d in ds if d['name']=='Jaeger']
assert len(jaeger)>0,'jaeger not found'
assert jaeger[0]['uid']=='jaeger',f'uid={jaeger[0][\"uid\"]}'
"

check "grafana datasource loki"     curl -s http://localhost:3000/api/datasources | python3 -c "
import sys,json
ds=json.load(sys.stdin)
loki=[d for d in ds if d['name']=='Loki']
assert len(loki)>0,'loki not found'
assert loki[0]['uid']=='loki',f'uid={loki[0][\"uid\"]}'
"

check "grafana dashboard provisioned"  curl -s http://localhost:3000/api/search | python3 -c "
import sys,json
items=json.load(sys.stdin)
dbs=[i for i in items if i.get('type')=='dash-db']
assert len(dbs)>0,'no dashboard'
print(f'  dashboard: {dbs[0][\"title\"]} (uid={dbs[0][\"uid\"]})')
"

echo ""
echo "── Phase 7: Collector log check ──"
check "collector no fatal errors"   podman logs otel-collector 2>&1 | rg -v "error|Error|ERROR|Fatal|panic|warn"

echo ""
echo "============================================"
printf "Results: ${GREEN}%d passed${NC}, ${RED}%d failed${NC}\n" $PASS $FAIL
echo "============================================"

# Stop app
kill $APP_PID 2>/dev/null || true

exit $FAIL
