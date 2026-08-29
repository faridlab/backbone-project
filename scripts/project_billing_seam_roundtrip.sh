#!/usr/bin/env bash
# §5 round-trip: prove the project delivery + billing surface survives a full codegen regen. The
# write path (billing exit over the converged analytic row, confirm-mint, hybrid status, delete
# guards, derived financials) lives in user-owned custom files; regen must leave them byte-identical
# and the tests green.
set -euo pipefail
cd "$(dirname "$0")/.."
export DATABASE_URL="${DATABASE_URL:-postgres://postgres:postgres@localhost:5433/backbone_project}"
SEAM=(
  src/application/service/project_events.rs
  src/application/service/project_ports.rs
  src/application/service/project_write_service.rs
  src/application/service/project_lifecycle.rs
  src/application/service/project_task.rs
  src/application/service/project_financials.rs
  src/application/service/project_billing.rs
  src/application/service/project_service_delivery.rs
  src/application/service/project_delete_guards.rs
  src/infrastructure/persistence/project_repository.rs
  src/infrastructure/persistence/task_repository.rs
  src/infrastructure/persistence/converged_timesheet_repository.rs
)
before=$(shasum "${SEAM[@]}")
echo "== regenerating (--force) =="
metaphor schema generate --force >/dev/null
after=$(shasum "${SEAM[@]}")
if [[ "$before" != "$after" ]]; then echo "FAIL: seam files changed across regen"; diff <(echo "$before") <(echo "$after"); exit 1; fi
echo "OK: seam files byte-identical across regen"
echo "== re-running the oracle + seams =="
SQLX_OFFLINE=false cargo test --test project_golden_cases --test integrity_probes \
  --test convergence_probes --test project_billing_seam 2>&1 | grep -E "test result"
echo "OK: §5 round-trip holds"
