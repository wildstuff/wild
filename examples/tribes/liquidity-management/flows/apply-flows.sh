#!/usr/bin/env bash
# ADR-0118 — author the example's operator FLOWS into a running tribe.
#
# Multi-stage flows are SOVEREIGN runtime records (ADR-0118 D14): the ddd
# compiler emits only the length-1 mirror records (sources → connector,
# trigger_rule → agentic), so a branching flow is authored AFTER boot over MCP
# with `flow_declare` — exactly what the operator does in Elder chat. This
# script replays that authoring so the branching `invoice-triage` flow (a
# connector-style spine → an agentic urgency DECISION → three branches → a
# reconverging merge) is reproducible for the dashboard's Flow-card view.
#
# Prereqs: the tribe is applied (`wild tribe apply … --as acme-liquidity`) and
# the daemon is up with the MCP HTTP listener. Usage:
#   MCP=http://127.0.0.1:56020 TOKEN=$(cat <profile_root>/system/token) \
#     ./apply-flows.sh
set -euo pipefail

MCP="${MCP:-http://127.0.0.1:8080}"
TOKEN="${TOKEN:-}"
HERE="$(cd "$(dirname "$0")" && pwd)"

call() { # $1 = tool name, $2 = JSON arguments
  curl -sS -m 20 -X POST "$MCP/mcp" \
    -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
    -d "$(python3 - "$1" "$2" <<'PY'
import json, sys
print(json.dumps({"jsonrpc":"2.0","id":1,"method":"tools/call",
    "params":{"name":sys.argv[1],"arguments":json.loads(sys.argv[2])}}))
PY
)"
  echo
}

for f in "$HERE"/*.json; do
  slug="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["def"]["slug"])' "$f")"
  echo "→ flow_declare $slug"
  # flow_declare replace is not idempotent on a sovereign record (absent
  # operator origin reads as unreadable) — retire first, ignore "no flow".
  call flow_retire "$(python3 -c 'import json,sys;d=json.load(open(sys.argv[1]));print(json.dumps({"tribe":d["tribe"],"name":d["def"]["slug"]}))' "$f")" >/dev/null 2>&1 || true
  call flow_declare "$(cat "$f")"
done
