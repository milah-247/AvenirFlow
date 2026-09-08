#!/usr/bin/env bash
# Build, optimize, and deploy the AvenirFlow contract to Stellar testnet.
#
# Usage:
#   ./scripts/deploy_testnet.sh [identity-name]
#
# Defaults to the "avenirflow-deployer" identity created by
# setup_identity.sh. Writes the deployed contract id to
# .stellar/contract-id.testnet for the other scripts to pick up.

set -euo pipefail
cd "$(dirname "$0")/.."

IDENTITY="${1:-avenirflow-deployer}"
NETWORK="testnet"
OUT_DIR=".stellar"
OUT_FILE="$OUT_DIR/contract-id.$NETWORK"

if ! command -v stellar >/dev/null 2>&1; then
  echo "error: stellar-cli not found. Install it with:" >&2
  echo "  cargo install --locked stellar-cli" >&2
  exit 1
fi

if ! stellar keys address "$IDENTITY" >/dev/null 2>&1; then
  echo "error: identity '$IDENTITY' not found. Run ./scripts/setup_identity.sh $IDENTITY first." >&2
  exit 1
fi

./scripts/build.sh

WASM="out/avenirflow.wasm"
if [ ! -f "$WASM" ]; then
  WASM="target/wasm32v1-none/release/avenirflow.wasm"
fi

echo "==> Deploying $WASM to $NETWORK as '$IDENTITY'"
CONTRACT_ID="$(stellar contract deploy \
  --wasm "$WASM" \
  --source "$IDENTITY" \
  --network "$NETWORK")"

mkdir -p "$OUT_DIR"
echo "$CONTRACT_ID" >"$OUT_FILE"

echo "==> Deployed."
echo "    Contract ID: $CONTRACT_ID"
echo "    Saved to:    $OUT_FILE"
echo ""
echo "Next steps:"
echo "  - Inspect on Stellar Expert:"
echo "    https://stellar.expert/explorer/testnet/contract/$CONTRACT_ID"
echo "  - Try ./scripts/invoke_examples.sh to walk through creating a vesting"
echo "    schedule and a stream against a freshly issued test token."
