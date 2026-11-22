# SoroRail contracts devcontainer (#447).
#
# Goal: a fresh clone opens in Codespaces / VS Code Dev Containers and
# `make test` is green without the contributor installing anything by
# hand. The toolchain pin (`rust-toolchain.toml`) is honoured, the
# wasm target and dev tools are pre-installed, and the Stellar CLI is
# fetched from its GitHub release (verified by SHA256 in post-create).
#
# Pinned base image: a maintained Microsoft Rust devcontainer at a
# version that already carries clippy, rustfmt and common build tools.

FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

ARG RUST_TOOLCHAIN=1.97.1
ARG STELLAR_CLI_VERSION=27.1.0

# Toolchain pin: `rust-toolchain.toml` in the repo will take over as soon
# as the workspace is cloned in, but installing the right channel here
# means the first `cargo` call doesn't pause to download it.
RUN rustup toolchain install ${RUST_TOOLCHAIN} --profile minimal \
        --component rustfmt clippy \
    && rustup target add wasm32v1-none --toolchain ${RUST_TOOLCHAIN} \
    && rustup default ${RUST_TOOLCHAIN}

# cargo-nextest is the test runner `make test-all` invokes.
RUN cargo install cargo-nextest --locked

# Stellar CLI (`stellar contract optimize`, `stellar contract info`). The
# release tarball lives under github.com/stellar/stellar-cli; verify the
# SHA256 in post-create so a tampered network drop can't go unnoticed.
ENV STELLAR_CLI_VERSION=${STELLAR_CLI_VERSION}
RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
      amd64) target=x86_64-unknown-linux-gnu ;; \
      arm64) target=aarch64-unknown-linux-gnu ;; \
      *) echo "unsupported arch: $arch" >&2; exit 1 ;; \
    esac; \
    url="https://github.com/stellar/stellar-cli/releases/download/v${STELLAR_CLI_VERSION}/stellar-cli-${STELLAR_CLI_VERSION}-${target}.tar.gz"; \
    curl -fsSL "$url" -o /tmp/stellar.tgz; \
    tar -xzf /tmp/stellar.tgz -C /usr/local/bin stellar; \
    rm /tmp/stellar.tgz; \
    stellar --version

# `pre-commit` is used by the project's hook config (#446). Install it in
# the base image so `pre-commit install` works out of the box.
RUN apt-get update \
    && apt-get install -y --no-install-recommends pipx \
    && apt-get clean && rm -rf /var/lib/apt/lists/*
USER vscode
RUN pipx install pre-commit
ENV PATH="/home/vscode/.local/bin:${PATH}"
USER root
