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

TMPDIR="${TMPDIR:-/tmp}"
TMPF="$TMPDIR/test-observability.tmp"
trap 'rm -f "$TMPF" 2>/dev/null' EXIT

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

podman ps --filter "name=otel-collector" --filter "status=running" --format "{{.Names}}" > "$TMPF"
check "otel-collector running"      rg -q otel-collector "$TMPF"
podman ps --filter "name=jaeger" --filter "status=running" --format "{{.Names}}" > "$TMPF"
check "jaeger running"              rg -q jaeger "$TMPF"
podman ps --filter "name=prometheus" --filter "status=running" --format "{{.Names}}" > "$TMPF"
check "prometheus running"          rg -q prometheus "$TMPF"
podman ps --filter "name=loki" --filter "status=running" --format "{{.Names}}" > "$TMPF"
check "loki running"                rg -q loki "$TMPF"
podman ps --filter "name=grafana" --filter "status=running" --format "{{.Names}}" > "$TMPF"
check "grafana running"             rg -q grafana "$TMPF"

podman logs otel-collector 2>&1 | tail -3 > "$TMPF"
check "collector no error in logs"  rg -v "error|Error|ERROR|Fatal|panic" "$TMPF"

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

curl -sf http://127.0.0.1:3001/health > "$TMPF"
check "app health endpoint"         python3 -c "
import sys,json
with open('$TMPF') as f: d=json.load(f)
assert d['status']=='ok'
"

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
curl -s http://localhost:8889/metrics > "$TMPF"
check "collector exports metrics"   rg -q "http_requests_total" "$TMPF"

# Check Prometheus target health
curl -s 'http://localhost:9091/api/v1/targets' > "$TMPF"
check "prometheus target UP"        python3 -c "
import sys,json
with open('$TMPF') as f: d=json.load(f)
targets=d['data']['activeTargets']
assert len(targets)>0,'no targets'
t=targets[0]
assert t['health']=='up',f'health={t[\"health\"]}'
"

# Check Prometheus has metrics
curl -s 'http://localhost:9091/api/v1/query?query=http_requests_total' > "$TMPF"
check "prometheus has http_requests_total"  python3 -c "
import sys,json
with open('$TMPF') as f: d=json.load(f)
assert d['status']=='success'
assert len(d['data']['result'])>0,'no results'
for r in d['data']['result']:
    val=float(r['value'][1])
    assert val>0,f'value={val}'
    print(f'  {r[\"metric\"][\"method\"]} {r[\"metric\"][\"route\"]}: {val}')
"

# Check Prometheus has histogram
curl -s 'http://localhost:9091/api/v1/query?query=http_request_duration_seconds_bucket' > "$TMPF"
check "prometheus has duration histogram"  python3 -c "
import sys,json
with open('$TMPF') as f: d=json.load(f)
assert d['status']=='success'
assert len(d['data']['result'])>0,'no results'
"

# Check metric name has _seconds suffix
curl -s http://localhost:8889/metrics > "$TMPF"
check "duration metric has _seconds suffix"  rg -q "http_request_duration_seconds_bucket" "$TMPF"

echo ""
echo "── Phase 5: Verify traces (Jaeger) ──"
curl -s 'http://localhost:16686/api/traces?service=learn-tracing-cncf&limit=1' > "$TMPF"
check "jaeger has traces"           python3 -c "
import sys,json
with open('$TMPF') as f: d=json.load(f)
assert len(d['data'])>0,'no traces'
t=d['data'][0]
assert len(t['spans'])>0,'no spans'
print(f'  traceID={t[\"traceID\"][:16]}..., spans={len(t[\"spans\"])}')
"

echo ""
echo "── Phase 6: Verify Grafana ──"
curl -sf http://localhost:3000/api/health > "$TMPF"
check "grafana health"              python3 -c "
import sys,json
with open('$TMPF') as f: d=json.load(f)
assert d['database']=='ok'
"

curl -s http://localhost:3000/api/datasources > "$TMPF"
check "grafana datasource prometheus"  python3 -c "
import sys,json
with open('$TMPF') as f: ds=json.load(f)
prom=[d for d in ds if d['name']=='Prometheus']
assert len(prom)>0,'prometheus not found'
assert prom[0]['uid']=='prometheus',f'uid={prom[0][\"uid\"]}'
"

curl -s http://localhost:3000/api/datasources > "$TMPF"
check "grafana datasource jaeger"   python3 -c "
import sys,json
with open('$TMPF') as f: ds=json.load(f)
jaeger=[d for d in ds if d['name']=='Jaeger']
assert len(jaeger)>0,'jaeger not found'
assert jaeger[0]['uid']=='jaeger',f'uid={jaeger[0][\"uid\"]}'
"

curl -s http://localhost:3000/api/datasources > "$TMPF"
check "grafana datasource loki"     python3 -c "
import sys,json
with open('$TMPF') as f: ds=json.load(f)
loki=[d for d in ds if d['name']=='Loki']
assert len(loki)>0,'loki not found'
assert loki[0]['uid']=='loki',f'uid={loki[0][\"uid\"]}'
"

curl -s http://localhost:3000/api/search > "$TMPF"
check "grafana dashboard provisioned"  python3 -c "
import sys,json
with open('$TMPF') as f: items=json.load(f)
dbs=[i for i in items if i.get('type')=='dash-db']
assert len(dbs)>0,'no dashboard'
print(f'  dashboard: {dbs[0][\"title\"]} (uid={dbs[0][\"uid\"]})')
"

echo ""
echo "── Phase 7: Collector log check ──"
podman logs otel-collector > "$TMPF" 2>&1
check "collector no fatal errors"   rg -v "error|Error|ERROR|Fatal|panic|warn" "$TMPF"

echo ""
echo "============================================"
printf "Results: ${GREEN}%d passed${NC}, ${RED}%d failed${NC}\n" $PASS $FAIL
echo "============================================"

# Stop app
kill $APP_PID 2>/dev/null || true

exit $FAIL
