#!/usr/bin/env bash
# Build and optimize the AvenirFlow contract WASM.
#
# Usage:
#   ./scripts/build.sh
#
# Produces:
#   target/wasm32v1-none/release/avenirflow.wasm  (optimized build, ready to deploy)
#   ./out/avenirflow.wasm                         (copy, for convenience)

set -euo pipefail
cd "$(dirname "$0")/.."

if command -v stellar >/dev/null 2>&1; then
  echo "==> Building + optimizing with stellar-cli"
  stellar contract build --package avenirflow --out-dir out
  echo "==> Optimized artifact: out/avenirflow.wasm"
  echo "    (also at target/wasm32v1-none/release/avenirflow.wasm)"
else
  echo "==> stellar-cli not found; falling back to plain cargo build (unoptimized)."
  echo "    Install stellar-cli for a smaller, deploy-ready artifact:"
  echo "      cargo install --locked stellar-cli"
  cargo build --target wasm32v1-none --release -p avenirflow
  echo "==> Artifact: target/wasm32v1-none/release/avenirflow.wasm"
fi
