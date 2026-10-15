#!/usr/bin/env bash
# ADR-0083 — activate the generic CRUD effects (create / update / delete) on an
# entity of a running tribe.
#
# CRUD effects are MATERIALISED, never authored (ADR-0083): the DDD compiler does
# NOT emit them from `model.yaml` — they are a runtime bind (`crud_bind` writes a
# `VerbSpec` carrying `crud_op`). So, exactly like the operator FLOWS in `../flows/`,
# the reproducible artifact is a post-apply script that binds them, not a model
# section. This mirrors what an operator does from the console (or, since the
# ADR-0178 exposure, in Elder chat: "enable creating dunning notices").
#
# We bind create/update/delete on `dunning` — an AUTHORED aggregate (not a
# source_mirror), so a hand `create`/`delete` is meaningful (unlike `invoice`,
# whose records are owned by the ingest feed and which therefore has NO create
# effect — a "create an invoice" ask is correctly refused). `read` is auto-bound
# by default (reads stay free); `crud_unbind` on read makes the type internal-only.
#
# FOR WHOM: a CRUD effect's customer exposure derives from the bound type's LIVE
# `customer_visible` — bind on a customer-visible type ⇒ the effect is customer-
# invocable; on an internal type ⇒ operator/worker only.
#
# Prereqs: the tribe is applied (`wild tribe apply … --as acme-liquidity`) and the
# daemon is up with the REST listener. Usage:
#   REST=http://127.0.0.1:7585 TOKEN=$(cat <profile_root>/system/token) \
#     TRIBE=acme-liquidity ./apply-crud.sh
set -euo pipefail

REST="${REST:-http://127.0.0.1:7585}"
TOKEN="${TOKEN:-}"
TRIBE="${TRIBE:-acme-liquidity}"
TYPE="${TYPE:-dunning}"

if [ -z "$TOKEN" ]; then
  echo "error: set TOKEN=\$(cat <profile_root>/system/token)" >&2
  exit 1
fi

bind() { # $1 = op (create|update|delete)
  curl -sS -m 20 -X POST "$REST/api/v1/tools/crud_bind" \
    -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
    -d "$(python3 -c "import json,sys;print(json.dumps({'tribe':sys.argv[1],'type':sys.argv[2],'op':sys.argv[3]}))" "$TRIBE" "$TYPE" "$1")"
}

for op in create update delete; do
  echo "→ crud_bind $TYPE op=$op"
  bind "$op" | python3 -c "import sys,json;o=json.load(sys.stdin);o=o.get('output',o);print('  ', 'ok' if o.get('ok') else o, '→', o.get('verb',''))" 2>/dev/null \
    || echo "  (bind call failed — is the daemon up on $REST?)"
done

echo
echo "Done. \`$TYPE\` now exposes create/update/delete as gated CRUD effects."
echo "Verify: the dashboard detail card shows a '+ Add new' action; the domain"
echo "surface lists ${TYPE}_create / ${TYPE}_update / ${TYPE}_delete."
echo "Retire again with crud_unbind (op=create|update|delete) → internal-only."
