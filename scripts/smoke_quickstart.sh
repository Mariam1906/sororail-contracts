#!/usr/bin/env bash
set -euo pipefail

network="${STELLAR_NETWORK:-local}"
source="${STELLAR_SOURCE:-quickstart-admin}"
wasm_dir="${WASM_DIR:-target/wasm32v1-none/release}"

for wasm in "$wasm_dir"/sororail_{escrow,stream,vesting,recurring,batch_payout}.wasm; do
  contract_id="$(stellar contract deploy --wasm "$wasm" --source "$source" --network "$network")"
  case "$contract_id" in
    C*) echo "deployed $(basename "$wasm"): $contract_id" ;;
    *) echo "unexpected contract id for $wasm: $contract_id" >&2; exit 1 ;;
  esac
done
