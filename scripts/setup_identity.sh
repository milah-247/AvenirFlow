#!/usr/bin/env bash
# Create (or reuse) a local stellar-cli identity and fund it on testnet via
# Friendbot. Run this once per identity before deploying/testing.
#
# Usage:
#   ./scripts/setup_identity.sh [identity-name]
#
# Defaults to an identity named "avenirflow-deployer".

set -euo pipefail

IDENTITY="${1:-avenirflow-deployer}"
NETWORK="testnet"

if ! command -v stellar >/dev/null 2>&1; then
  echo "error: stellar-cli not found. Install it with:" >&2
  echo "  cargo install --locked stellar-cli" >&2
  exit 1
fi

if stellar keys address "$IDENTITY" >/dev/null 2>&1; then
  echo "==> Identity '$IDENTITY' already exists."
else
  echo "==> Generating identity '$IDENTITY'"
  stellar keys generate --global "$IDENTITY" --network "$NETWORK" --fund
fi

ADDRESS="$(stellar keys address "$IDENTITY")"
echo "==> Address: $ADDRESS"

echo "==> Ensuring the account is funded on $NETWORK (Friendbot)"
stellar keys fund "$IDENTITY" --network "$NETWORK" || true

echo "==> Ready. Use --source $IDENTITY (or --source-account $ADDRESS) in later commands."
