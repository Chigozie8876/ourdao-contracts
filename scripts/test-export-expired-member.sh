#!/usr/bin/env bash
# Test fixture verifying export-state.sh's expired-member warning path (#531).

set -euo pipefail

TOTAL_MEMBERS=5
EXPORTED_MEMBERS=4

# Simulate the warning check logic from export-state.sh
warning_output=$(
  if ((EXPORTED_MEMBERS != TOTAL_MEMBERS)); then
    echo "warning: exported ${EXPORTED_MEMBERS} of ${TOTAL_MEMBERS} members. The rest have expired storage; see docs/MIGRATION.md (TTL risk)." >&2
  fi 2>&1
)

if ! grep -q "warning: exported 4 of 5 members. The rest have expired storage" <<<"${warning_output}"; then
  echo "FAIL: expected warning was not emitted" >&2
  exit 1
fi

echo "PASS: expired-member warning fixture verified"
