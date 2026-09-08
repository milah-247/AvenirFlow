#!/usr/bin/env bash
# Walk through a full example against a deployed AvenirFlow contract on
# Stellar testnet: create a short vesting schedule, claim from it, create a
# stream, and withdraw from it. Uses the native XLM Stellar Asset Contract
# as the token, since every funded testnet account already holds XLM (no
# custom asset issuance needed for the demo).
#
# Usage:
#   ./scripts/invoke_examples.sh
#
# Requires:
#   - ./scripts/setup_identity.sh avenirflow-deployer   (creator/funder)
#   - ./scripts/setup_identity.sh avenirflow-recipient  (vesting/stream recipient)
#   - ./scripts/deploy_testnet.sh                       (writes .stellar/contract-id.testnet)

set -euo pipefail
cd "$(dirname "$0")/.."

NETWORK="testnet"
CREATOR="avenirflow-deployer"
RECIPIENT="avenirflow-recipient"
CONTRACT_FILE=".stellar/contract-id.testnet"

if ! command -v stellar >/dev/null 2>&1; then
  echo "error: stellar-cli not found. Install it with:" >&2
  echo "  cargo install --locked stellar-cli" >&2
  exit 1
fi

if [ ! -f "$CONTRACT_FILE" ]; then
  echo "error: $CONTRACT_FILE not found. Run ./scripts/deploy_testnet.sh first." >&2
  exit 1
fi
CONTRACT_ID="$(cat "$CONTRACT_FILE")"

for id in "$CREATOR" "$RECIPIENT"; do
  if ! stellar keys address "$id" >/dev/null 2>&1; then
    echo "==> Creating and funding missing identity: $id"
    stellar keys generate --global "$id" --network "$NETWORK" --fund
  fi
done

CREATOR_ADDR="$(stellar keys address "$CREATOR")"
RECIPIENT_ADDR="$(stellar keys address "$RECIPIENT")"
TOKEN_ID="$(stellar contract id asset --asset native --network "$NETWORK")"

echo "==> Contract:        $CONTRACT_ID"
echo "==> Token (native):  $TOKEN_ID"
echo "==> Creator:         $CREATOR_ADDR"
echo "==> Recipient:       $RECIPIENT_ADDR"

invoke() {
  stellar contract invoke \
    --id "$CONTRACT_ID" \
    --source "$CREATOR" \
    --network "$NETWORK" \
    -- "$@"
}

invoke_as() {
  local source="$1"
  shift
  stellar contract invoke \
    --id "$CONTRACT_ID" \
    --source "$source" \
    --network "$NETWORK" \
    -- "$@"
}

# --- Vesting: 1 XLM (10_000_000 stroops), no cliff, vests over 60 seconds ---
START="$(date +%s)"
echo ""
echo "==> create_vesting: 10_000_000 stroops over 60s, no cliff, cancellable"
VESTING_ID="$(invoke create_vesting \
  --creator "$CREATOR_ADDR" \
  --recipient "$RECIPIENT_ADDR" \
  --token "$TOKEN_ID" \
  --total_amount 10000000 \
  --start_time "$START" \
  --cliff_duration 0 \
  --vesting_duration 60 \
  --cancellable true)"
echo "    vesting_id = $VESTING_ID"

echo "==> Waiting 65s for the schedule to fully vest..."
sleep 65

echo "==> claimable_vesting (preview)"
invoke get_vesting --vesting_id "$VESTING_ID"
invoke claimable_vesting --vesting_id "$VESTING_ID"

echo "==> claim (as recipient)"
invoke_as "$RECIPIENT" claim --vesting_id "$VESTING_ID"

# --- Stream: 1000 stroops/second for 30 seconds (30_000 stroops total) ---
S_START="$(date +%s)"
S_END="$((S_START + 30))"
echo ""
echo "==> create_stream: 1000 stroops/sec for 30s"
STREAM_ID="$(invoke create_stream \
  --creator "$CREATOR_ADDR" \
  --recipient "$RECIPIENT_ADDR" \
  --token "$TOKEN_ID" \
  --rate_per_second 1000 \
  --start_time "$S_START" \
  --end_time "$S_END")"
echo "    stream_id = $STREAM_ID"

echo "==> Waiting 15s, then withdrawing the partial amount streamed so far..."
sleep 15
invoke_as "$RECIPIENT" withdraw_from_stream --stream_id "$STREAM_ID"

echo "==> Waiting for the stream to finish, then withdrawing the remainder..."
sleep 20
invoke_as "$RECIPIENT" withdraw_from_stream --stream_id "$STREAM_ID"

echo ""
echo "==> Done. Explore events at:"
echo "    https://stellar.expert/explorer/testnet/contract/$CONTRACT_ID"
