# Reproducible WASM Builds

Verifiable builds are a prerequisite for audit sign-off and mainnet trust. The
contracts must produce identical WASM bytes from a clean checkout, regardless
of who builds them.

## Pinned environment

| Item | Value | Source |
| --- | --- | --- |
| Rust | `1.97.1` | `rust-toolchain.toml` |
| Target | `wasm32v1-none` | `rust-toolchain.toml` (not `wasm32-unknown-unknown`) |
| Dependencies | exact versions | `Cargo.lock` (committed) |
| Release profile | `opt-level = "z"`, `lto = true`, `codegen-units = 1`, `strip = "symbols"`, `overflow-checks = true` | `Cargo.toml` |

No source-tree config is read from the host: build in a clean container with
the source mounted read-only and an empty target directory.

## Exact build command

```sh
cargo build --locked --workspace \
  --exclude sororail-integration-tests \
  --target wasm32v1-none --release
```

`sororail-integration-tests` is host-only (`std` testutils), so it is excluded
from the WASM build, exactly as in CI and the Makefile.

## Verifying locally in a clean container

```sh
docker run --rm \
  -v "$PWD:/src:ro" -w /src \
  -e CARGO_TARGET_DIR=/tmp/repro-target \
  rust:1.97.1-bookworm \
  bash -c '
    set -euo pipefail
    rustup target add wasm32v1-none
    cargo build --locked --workspace \
      --exclude sororail-integration-tests \
      --target wasm32v1-none --release
    cd "$CARGO_TARGET_DIR/wasm32v1-none/release"
    sha256sum sororail_*.wasm
  '
```

Compare the output with the `wasm` artifact hashes attached to the release, or
run the command twice with different `CARGO_TARGET_DIR` values. Hashes must
match byte for byte. CI enforces this with the `reproducible build` job, which
builds twice in two fresh containers and diffs the hash lists.
