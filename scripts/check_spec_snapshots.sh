#!/usr/bin/env bash
# Checks that every contract's committed spec snapshot matches the spec the
# contract currently emits.
#
# Requires: stellar CLI  (cargo install --locked stellar-cli --features opt)
#           and the contracts built for wasm32v1-none  (make build)
#
# To regenerate all snapshots:  make update-spec-snapshots
# To check for drift:           make check-spec-snapshots  (or run this script)
#
# This script exits 0 when stellar CLI is not installed so that CI jobs that
# haven't yet wired up the CLI continue to pass. Once the CLI is available in
# CI, every drift is a build failure.
set -euo pipefail

WASM_TARGET="${WASM_TARGET:-wasm32v1-none}"
WASM_DIR="target/${WASM_TARGET}/release"
CONTRACTS="${CONTRACTS:-escrow stream vesting recurring batch_payout}"

if ! command -v stellar &>/dev/null; then
    echo "stellar CLI not found — skipping spec snapshot check."
    echo "  Install: cargo install --locked stellar-cli --features opt"
    exit 0
fi

DRIFT=0
for c in ${CONTRACTS}; do
    wasm="${WASM_DIR}/sororail_${c}.wasm"
    snapshot="contracts/${c}/spec_snapshot.json"

    if [ ! -f "${wasm}" ]; then
        echo "SKIP ${c}: wasm not built (run 'make build' first)"
        continue
    fi

    generated="$(stellar contract info interface --output json --wasm "${wasm}")"

    if [ ! -f "${snapshot}" ]; then
        echo "MISSING ${c}: no snapshot — run 'make update-spec-snapshots' and commit the result"
        DRIFT=1
        continue
    fi

    committed="$(cat "${snapshot}")"
    if [ "${generated}" = "${committed}" ]; then
        echo "OK ${c}"
    else
        echo "DRIFT ${c}: spec has changed"
        diff <(echo "${committed}") <(echo "${generated}") || true
        echo "  → run 'make update-spec-snapshots' and commit the updated snapshot"
        DRIFT=1
    fi
done

exit "${DRIFT}"
