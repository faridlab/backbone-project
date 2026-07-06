#!/usr/bin/env bash
# §5 round-trip: prove the project billing seam survives a full codegen regen. The delivery + billing
# path lives in user-owned custom files; regen must leave them byte-identical and the tests green.
set -euo pipefail
cd "$(dirname "$0")/.."
export DATABASE_URL="${DATABASE_URL:-postgres://postgres:postgres@localhost:5433/backbone_project}"
SEAM=(
  src/application/service/project_events.rs
  src/application/service/project_ports.rs
  src/application/service/project_write_service.rs
)
before=$(shasum "${SEAM[@]}")
echo "== regenerating (--force) =="
metaphor schema schema generate --force >/dev/null
after=$(shasum "${SEAM[@]}")
if [[ "$before" != "$after" ]]; then echo "FAIL: seam files changed across regen"; diff <(echo "$before") <(echo "$after"); exit 1; fi
echo "OK: seam files byte-identical across regen"
echo "== re-running the oracle + seams =="
SQLX_OFFLINE=false cargo test --test project_golden_cases --test integrity_probes \
  --test project_billing_seam 2>&1 | grep -E "test result"
echo "OK: §5 round-trip holds"
