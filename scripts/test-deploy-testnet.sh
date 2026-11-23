#!/usr/bin/env bash
# Test confirming deploy-testnet.sh refuses unrecognized network names (#530).

set -euo pipefail

cd "$(dirname "$0")/.."

output="$(./scripts/deploy-testnet.sh --dry-run --network definitely-not-a-real-network 2>&1 || true)"
if ! grep -q "refusing to deploy to 'definitely-not-a-real-network'" <<<"${output}"; then
  echo "FAIL: deploy-testnet.sh did not refuse unrecognized network" >&2
  exit 1
fi
echo "PASS: deploy-testnet.sh properly refuses unrecognized network"

# Companion test: testnet network is accepted
if ! ./scripts/deploy-testnet.sh --dry-run --network testnet --yes >/dev/null 2>&1; then
  echo "FAIL: deploy-testnet.sh dry-run on testnet failed" >&2
  exit 1
fi
echo "PASS: deploy-testnet.sh accepts testnet"
