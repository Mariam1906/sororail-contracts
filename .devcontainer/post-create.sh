#!/usr/bin/env bash
# devcontainer post-create smoke test (#447): validate that `make test`
# is green on a fresh checkout. The acceptance criterion for the issue is
# "Opening in a codespace yields a green `make test`", and this script is
# the automated form of that promise.

set -euo pipefail

echo "==> Rust toolchain:"
rustup show active-toolchain || true
cargo --version
rustc --version

echo "==> Stellar CLI:"
stellar --version

# Pre-commit is optional for CI, required for local development. Register
# the hooks here so a new contributor's first commit is already linted.
if command -v pre-commit >/dev/null 2>&1; then
    echo "==> Installing pre-commit hooks"
    pre-commit install --install-hooks || echo "pre-commit install failed (non-fatal)"
fi

echo "==> Running make test (post-create smoke test)"
make test
