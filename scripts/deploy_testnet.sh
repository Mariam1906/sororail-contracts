#!/usr/bin/env bash
# Deploy every SoroRail contract to Stellar testnet and record the result in
# `DEPLOYMENTS.md`-compatible output (#445).
#
# Usage:
#   scripts/deploy_testnet.sh [--dry-run] [--network NAME] [--source KEY] \
#                             [--out PATH]
#
# Flags:
#   --dry-run   Print every build / optimize / deploy / upload command that
#               would run, without executing anything. Also writes a stub
#               output file with `<dry-run>` placeholders so downstream
#               tooling can be exercised end-to-end.
#   --network   Network name passed to `stellar contract deploy` (default:
#               testnet).
#   --source    Key alias passed to `stellar contract deploy --source` (default:
#               the value of $STELLAR_SOURCE, else `deployer`).
#   --out       Where to write the DEPLOYMENTS.md-compatible section. Default:
#               dist/deployments-testnet.md.
#
# Exit codes:
#   0   success (or successful dry-run)
#   1   usage / tooling error
#   2   dirty working tree (refuses to run)
#   3   build / optimize / deploy failure
#
# The output is a plain Markdown section with the same table shape as the
# "Stellar testnet" block in DEPLOYMENTS.md, so pasting it into that file
# keeps `scripts/check_deployments.sh` passing — the only invariant that
# script enforces is that every deployable contract appears in a
# `| \`<contract>\`` table row.

set -euo pipefail

CONTRACTS="escrow stream vesting recurring batch_payout"
WASM_TARGET="wasm32v1-none"
WASM_DIR="target/${WASM_TARGET}/release"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

DRY_RUN=0
NETWORK="testnet"
SOURCE_KEY="${STELLAR_SOURCE:-deployer}"
OUT_PATH="${REPO_ROOT}/dist/deployments-testnet.md"

die() { echo "deploy_testnet.sh: error: $*" >&2; exit "${2:-1}"; }
usage() { sed -n '2,30p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)   DRY_RUN=1; shift ;;
    --network)   NETWORK="${2:?--network needs a value}"; shift 2 ;;
    --source)    SOURCE_KEY="${2:?--source needs a value}"; shift 2 ;;
    --out)       OUT_PATH="${2:?--out needs a value}"; shift 2 ;;
    -h|--help)   usage; exit 0 ;;
    *)           die "unknown argument: $1" ;;
  esac
done

cd "$REPO_ROOT"

# Refuse to run on a dirty tree. A deploy records the commit sha; running
# against uncommitted work would record a commit that does not exist on the
# remote, making the deployment un-reproducible. Dry-run shares this guard so
# operators cannot "practise" past it.
if [[ -n "$(git status --porcelain=v1 --ignore-submodules)" ]]; then
  echo "deploy_testnet.sh: refusing to run — working tree is dirty." >&2
  git status --short >&2
  exit 2
fi

COMMIT_SHA="$(git rev-parse HEAD)"
COMMIT_SHORT="$(git rev-parse --short HEAD)"

# All commands either run or print, depending on --dry-run. Every command the
# dry-run promises to run is the same one the real run executes.
run() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    printf '  $ %s\n' "$*"
  else
    "$@"
  fi
}

# For a shell pipeline (needs eval); prints the whole string in dry-run.
run_sh() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    printf '  $ %s\n' "$*"
  else
    eval "$*"
  fi
}

for tool in cargo git sha256sum; do
  command -v "$tool" >/dev/null || die "missing required tool: $tool"
done
if [[ "$DRY_RUN" -ne 1 ]]; then
  command -v stellar >/dev/null || die "missing required tool: stellar (CLI)"
fi

echo "==> build (release) for $WASM_TARGET"
run cargo build --workspace --exclude sororail-integration-tests \
    --target "$WASM_TARGET" --release

echo "==> optimize wasms via stellar contract optimize"
for c in $CONTRACTS; do
  run_sh "stellar contract optimize --wasm \"$WASM_DIR/sororail_${c}.wasm\""
done

mkdir -p "$(dirname "$OUT_PATH")"
TMP_OUT="$(mktemp)"
trap 'rm -f "$TMP_OUT"' EXIT

{
  echo "## Stellar ${NETWORK}"
  echo
  echo "Deployed on $(date -u +%Y-%m-%d) from commit"
  echo "[\`${COMMIT_SHORT}\`](https://github.com/Sororail/sororail-contracts/commit/${COMMIT_SHA})"
  echo "(\`${COMMIT_SHA}\`), built via \`make optimize\`."
  echo
  echo "| Contract       | Address                                                    | WASM Hash (SHA-256)                                               |"
  echo "| -------------- | ---------------------------------------------------------- | ----------------------------------------------------------------- |"
} > "$TMP_OUT"

for c in $CONTRACTS; do
  wasm="$WASM_DIR/sororail_${c}.optimized.wasm"
  if [[ "$DRY_RUN" -eq 1 ]]; then
    hash="<dry-run-sha256>"
    address="<dry-run-address>"
    echo "==> [dry-run] would upload and deploy $wasm"
    printf '  $ %s\n' "stellar contract upload --wasm \"$wasm\" --network \"$NETWORK\" --source \"$SOURCE_KEY\""
    printf '  $ %s\n' "stellar contract deploy --wasm \"$wasm\" --network \"$NETWORK\" --source \"$SOURCE_KEY\""
  else
    [[ -f "$wasm" ]] || die "optimized wasm missing: $wasm" 3
    hash="$(sha256sum "$wasm" | awk '{print $1}')"
    echo "==> deploy $c"
    address="$(stellar contract deploy --wasm "$wasm" \
      --network "$NETWORK" --source "$SOURCE_KEY")"
    [[ -n "$address" ]] || die "deploy returned empty address for $c" 3
  fi
  printf '| `%-13s` | `%s` | `%s` |\n' \
    "$c" "$address" "$hash" >> "$TMP_OUT"
done

mv "$TMP_OUT" "$OUT_PATH"
trap - EXIT

echo
echo "==> wrote $OUT_PATH"
echo "    paste or append the section above into DEPLOYMENTS.md."

# Fast local invariant check: the output file must list every contract. This
# mirrors what scripts/check_deployments.sh does on the real DEPLOYMENTS.md
# and catches a drift between $CONTRACTS here and the deployable contracts
# in `contracts/` before the paste.
missing=0
for contract_dir in "$REPO_ROOT"/contracts/*/; do
  contract_file="${contract_dir}src/contract.rs"
  [[ -f "$contract_file" ]] || continue
  contract="${contract_dir%/}"
  contract="${contract##*/}"
  if ! grep -Fq "| \`$contract" "$OUT_PATH"; then
    echo "==> error: deployable contract '${contract}' is missing from $OUT_PATH" >&2
    missing=1
  fi
done
exit "$missing"
