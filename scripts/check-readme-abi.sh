#!/usr/bin/env bash
#
# Checks that every public contract entrypoint in contracts/dao/src/lib.rs
# is documented in README.md's Public interface (ABI) section (#529).
#
set -euo pipefail

cd "$(dirname "$0")/.."

MISSING=0
LIB_FILE="contracts/dao/src/lib.rs"
README_FILE="README.md"

# Extract pub fn names within #[contractimpl] block
methods=$(awk '
  /#[contractimpl]/ { in_impl=1; next }
  in_impl && /^impl OurDao/ { in_block=1; next }
  in_block && /^}/ { in_block=0; in_impl=0 }
  in_block && /pub fn / {
    sub(/.*pub fn /, "");
    sub(/\(.*/, "");
    print
  }
' "${LIB_FILE}")

for method in ${methods}; do
  if ! grep -q "\`${method}" "${README_FILE}"; then
    echo "ERROR: method '${method}' found in lib.rs but missing from README.md" >&2
    MISSING=$((MISSING + 1))
  fi
done

if ((MISSING > 0)); then
  echo "FAIL: ${MISSING} method(s) missing from README.md" >&2
  exit 1
fi

echo "PASS: all lib.rs entrypoints are documented in README.md"
