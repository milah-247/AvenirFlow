# AvenirFlow

A Stellar / Soroban smart contract for **token vesting** and **continuous
payment streaming** — built for DAOs, ecosystem grants, contributor rewards,
and other long-term, trust-minimized funding arrangements.

AvenirFlow escrows the full committed amount in the contract at creation
time (a real token transfer, not just bookkeeping), so a schedule or stream
can never promise more than it can pay.

> **Live on testnet:** [`CDVC7T4D2LY6JRKKURNPQ3FFBJ36EZYJZXSHPDJPJCX3V57TMPCGSY3K`](https://stellar.expert/explorer/testnet/contract/CDVC7T4D2LY6JRKKURNPQ3FFBJ36EZYJZXSHPDJPJCX3V57TMPCGSY3K)
> ([wasm hash `21f774d318b38289d5c1ea8571183dd9fe78067c748dbe56327ebc9153e958ce`](https://stellar.expert/explorer/testnet/contract/CDVC7T4D2LY6JRKKURNPQ3FFBJ36EZYJZXSHPDJPJCX3V57TMPCGSY3K)) —
> see [`deployments/`](deployments/) for the current id per network.

## Contents

- [Primitives](#primitives)
- [Installation](#installation)
- [Testing](#testing)
- [Deployment to testnet](#deployment-to-testnet)
- [Contract methods](#contract-methods)
- [Storage layout](#storage-layout)
- [Events](#events)
- [Security considerations](#security-considerations)
- [Example usage](#example-usage)
- [Project layout](#project-layout)
- [Production readiness notes](#production-readiness-notes)

## Primitives

### 1. Linear vesting with an optional cliff

`create_vesting` escrows `total_amount` of a token and releases it linearly
from `start_time` to `start_time + vesting_duration`. Nothing is claimable
before `start_time + cliff_duration`; at that instant the amount that would
have accrued since `start_time` unlocks all at once, and it vests linearly
from there. The recipient calls `claim` any number of times to withdraw
whatever has vested and hasn't been claimed yet. If the schedule was
created with `cancellable = true`, the creator can `cancel` it: the
recipient's vested share is preserved (claimable now or later) and every
unvested token is returned to the creator immediately.

### 2. Continuous payment streaming

`create_stream` escrows `rate_per_second * (end_time - start_time)` of a
token up front and releases it continuously, one unit per second, between
`start_time` and `end_time`. The recipient calls `withdraw_from_stream` any
number of times to pull whatever has streamed and hasn't been withdrawn
yet. A stream can never pay out more than it was funded with, even if
called long after `end_time`.

Both primitives share the same design principles:

- **Escrow up front.** Tokens move into the contract at creation, via the
  real SEP-41 `transfer` call — there's no separate "funding" step to
  forget, and a schedule/stream is fully collateralized from the moment it
  exists.
- **Pull-based payouts.** Recipients claim/withdraw when they want to;
  nothing is pushed automatically, which keeps every state-changing call
  attributable to an explicit, authorized actor.
- **Idempotent accounting.** Every claim/withdrawal only ever pays out the
  *delta* since the last one (`vested_amount - claimed_amount` /
  `streamed_amount - withdrawn_amount`), computed fresh from the current
  ledger timestamp, so calling twice in the same block, or a thousand
  times over a year, can never double-pay.
- **Checked arithmetic throughout.** All amount and timestamp math uses
  `checked_add` / `checked_sub` / `checked_mul` / `checked_div` and returns
  `Error::ArithmeticError` on overflow/underflow instead of wrapping or
  panicking.

## Installation

### Prerequisites

- **Rust** 1.84+ (stable). This repo was built and tested against Rust
  1.98 / soroban-sdk 27.0.6.
- The **`wasm32v1-none`** target:

  ```bash
  rustup target add wasm32v1-none
  ```

- **stellar-cli** (formerly `soroban-cli`) for building, testing against a
  local network, and deploying:

  ```bash
  cargo install --locked stellar-cli
  # or, if you already have it:
  stellar --version
  ```

### Clone and build

```bash
git clone https://github.com/milah-247/AvenirFlow.git
cd AvenirFlow
./scripts/build.sh
```

This produces an optimized, deploy-ready WASM binary at `out/avenirflow.wasm`
(and the unoptimized artifact at
`target/wasm32v1-none/release/avenirflow.wasm`).

## Testing

The contract has two layers of automated tests:

- **Pure arithmetic unit tests** (`src/vesting.rs`, `src/stream.rs`) —
  exercise the vesting/streaming curve math directly (cliffs, linear
  interpolation, full vesting, dust from integer division, overflow
  guards) with no `Env`/storage/token involvement.
- **End-to-end contract tests** (`src/test.rs`) — drive the contract
  through its generated test client exactly as an off-chain caller would,
  using a real Stellar Asset Contract (deployed in-test via
  `register_stellar_asset_contract_v2`) as the token, so transfers,
  balances, and authorization all go through the genuine SEP-41 interface.

Run everything with:

```bash
cargo test -p avenirflow --features testutils
```

At the time of writing this is **50 tests, all passing**, covering:

- cliffs (before/at/after)
- partial and multiple claims (no double-spending)
- claiming after full vesting
- cancellation (mid-vesting, after a partial claim, at/after full vesting)
- cancelling twice, cancelling a non-cancellable schedule
- stream withdrawals (partial, repeated, capped at the funded amount)
- unauthorized `create_vesting` / `claim` / `cancel` calls (wrong signer)
- invalid parameters (zero/negative amounts, zero duration, cliff longer
  than duration, `end_time <= start_time`)
- insufficient funding (including exactly-one-token-short)
- arithmetic overflow on stream funding (`rate_per_second * duration`
  exceeding `i128`)
- unknown schedule/stream ids
- event emission

Lint cleanliness:

```bash
cargo clippy -p avenirflow --all-targets --features testutils -- -D warnings
```

## Deployment to testnet

Three scripts under `scripts/` cover the full flow:

```bash
# 1. Create and fund a local signing identity on testnet (Friendbot).
./scripts/setup_identity.sh avenirflow-deployer

# 2. Build (optimized) and deploy; writes the contract id to
#    deployments/testnet-contract-id.txt
./scripts/deploy_testnet.sh avenirflow-deployer

# 3. Optional: walk through a live example — create a short vesting
#    schedule and a stream using native XLM as the token, claim/withdraw
#    from both. Takes a couple of minutes (it sleeps through real time so
#    the schedule/stream can actually vest/stream on a public network).
./scripts/invoke_examples.sh
```

Under the hood, `deploy_testnet.sh` is just:

```bash
stellar keys generate --global avenirflow-deployer --network testnet --fund
stellar contract build --package avenirflow --out-dir out
stellar contract deploy \
  --wasm out/avenirflow.wasm \
  --source avenirflow-deployer \
  --network testnet
```

Deploying to **mainnet** follows the same shape with `--network mainnet`
and a funded mainnet identity; do a full [security review](#security-considerations)
first (see [Production readiness notes](#production-readiness-notes)).

## Contract methods

All amounts are `i128`; all timestamps/durations are `u64` unix seconds
(matching `env.ledger().timestamp()`). Every mutating method returns
`Result<T, Error>` — see [`src/errors.rs`](contracts/avenirflow/src/errors.rs)
for the full list of error variants.

### Vesting

| Method | Auth required | Description |
|---|---|---|
| `create_vesting(creator, recipient, token, total_amount, start_time, cliff_duration, vesting_duration, cancellable) -> Result<u64, Error>` | `creator` | Escrows `total_amount` of `token` from `creator` and creates a new schedule. Returns its id. |
| `claim(vesting_id) -> Result<i128, Error>` | schedule's `recipient` | Transfers whatever has vested and not yet been claimed. Errors with `NothingToClaim` if the delta is `0`. |
| `cancel(vesting_id) -> Result<(i128, i128), Error>` | schedule's `creator` | Only for `cancellable` schedules. Freezes the vested amount (still claimable by the recipient) and refunds the rest to the creator immediately. Returns `(vested_amount, refunded_to_creator)`. |
| `get_vesting(vesting_id) -> Result<VestingSchedule, Error>` | none (read-only) | Full schedule state. |
| `claimable_vesting(vesting_id) -> Result<i128, Error>` | none (read-only) | Previews what `claim` would pay out right now, without mutating state. |

### Streaming

| Method | Auth required | Description |
|---|---|---|
| `create_stream(creator, recipient, token, rate_per_second, start_time, end_time) -> Result<u64, Error>` | `creator` | Escrows `rate_per_second * (end_time - start_time)` of `token` and creates a new stream. Returns its id. |
| `withdraw_from_stream(stream_id) -> Result<i128, Error>` | stream's `recipient` | Transfers whatever has streamed and not yet been withdrawn. Errors with `NothingToClaim` if the delta is `0`. |
| `get_stream(stream_id) -> Result<Stream, Error>` | none (read-only) | Full stream state. |
| `withdrawable_stream(stream_id) -> Result<i128, Error>` | none (read-only) | Previews what `withdraw_from_stream` would pay out right now. |

### Validation summary

- `create_vesting`: `total_amount > 0`; `vesting_duration > 0`;
  `cliff_duration <= vesting_duration`; `start_time + vesting_duration`
  must not overflow `u64`.
- `create_stream`: `rate_per_second > 0`; `end_time > start_time`;
  `rate_per_second * (end_time - start_time)` must not overflow `i128`.
- Both also implicitly require `creator` to hold and authorize at least
  the escrowed amount — the token contract's own `transfer` call enforces
  this and panics (surfacing as a host error to the caller) if not.

## Storage layout

Defined in [`src/types.rs`](contracts/avenirflow/src/types.rs) and
[`src/storage.rs`](contracts/avenirflow/src/storage.rs).

| Key | Storage class | Contents |
|---|---|---|
| `DataKey::VestingCounter` | instance | Next vesting id to assign (monotonic). |
| `DataKey::StreamCounter` | instance | Next stream id to assign (monotonic). |
| `DataKey::Vesting(id)` | persistent | One `VestingSchedule`: `creator`, `recipient`, `token`, `total_amount`, `claimed_amount`, `start_time`, `cliff_duration`, `vesting_duration`, `cancellable`, `cancelled`. |
| `DataKey::Stream(id)` | persistent | One `Stream`: `creator`, `recipient`, `token`, `rate_per_second`, `start_time`, `end_time`, `deposited_amount`, `withdrawn_amount`. |

- The two counters live in **instance** storage (cheap, always resident
  alongside the contract's own instance) since they're touched on every
  creation call.
- Individual schedules/streams live in **persistent** storage since a
  vesting schedule can be active for years; every read and write to one
  also extends its TTL (see the constants at the top of `storage.rs`) so
  active entries don't expire out from under long-running grants.
- There is deliberately **no on-chain index of "all vestings for
  recipient X"** — that kind of list grows unbounded and gets expensive to
  maintain in contract storage. Off-chain indexers should reconstruct this
  from the [events](#events) instead (each is emitted with the
  recipient/creator as an indexed topic for exactly this reason).

## Events

Defined with the `#[contractevent]` macro in
[`src/events.rs`](contracts/avenirflow/src/events.rs), so their shapes are
part of the contract's published interface spec.

| Event | Topics | Emitted by |
|---|---|---|
| `VestingCreated` | `vesting_created`, `vesting_id`, `recipient` | `create_vesting` |
| `VestingClaimed` | `vesting_claimed`, `vesting_id`, `recipient` | `claim` |
| `VestingCancelled` | `vesting_cancelled`, `vesting_id`, `creator` | `cancel` |
| `StreamCreated` | `stream_created`, `stream_id`, `recipient` | `create_stream` |
| `StreamWithdrawn` | `stream_withdrawn`, `stream_id`, `recipient` | `withdraw_from_stream` |

Every other field (amounts, timestamps, token address, etc.) is carried in
the event's data payload. Filter by the fixed first topic to watch a whole
category of activity, or by `vesting_id`/`stream_id`/`recipient` to follow
one schedule, one stream, or one account.

## Security considerations

- **Authorization is looked up from storage, never trusted from
  arguments.** `claim` and `withdraw_from_stream` call
  `require_auth()` on the schedule/stream's *stored* `recipient`; `cancel`
  does the same for the stored `creator`. There is no `recipient`/`creator`
  parameter on those calls for an attacker to substitute.
- **Escrow, not allowance.** `create_vesting`/`create_stream` pull the
  full committed amount into the contract atomically as part of creation.
  If the transfer fails (insufficient balance or missing authorization),
  the whole call reverts — a schedule or stream is never left partially
  funded or created without backing.
- **Checked arithmetic everywhere.** Every addition, subtraction,
  multiplication, and division on amounts and timestamps uses `checked_*`
  and maps failure to `Error::ArithmeticError`, so a pathological
  `total_amount` / `rate_per_second` / duration combination fails cleanly
  instead of wrapping silently (see `vesting::vested_amount` and
  `stream::deposit_amount`/`streamed_amount`, and their overflow-focused
  unit tests).
- **Cancellation cannot claw back what already vested.** `cancel` computes
  the vested amount at the moment of cancellation and permanently caps the
  schedule there (`schedule.total_amount = vested`); the recipient can
  still `claim` that amount at any point afterward. It is not possible for
  a creator to cancel a schedule and recover tokens the recipient had
  already earned.
- **No reentrancy surface.** State (`claimed_amount` / `withdrawn_amount`)
  is updated and persisted *before* the outbound token transfer in both
  `claim` and `withdraw_from_stream`, so even a malicious/non-standard
  token implementation that tried to call back into the contract during
  `transfer` would see the already-updated balance and get nothing extra.
- **Integer-division dust favors nobody unfairly.** Linear vesting uses
  `total_amount * elapsed / vesting_duration`, which truncates during
  the linear phase; the `now >= vesting_end` branch always returns exactly
  `total_amount`, so any truncated dust from intermediate claims is
  guaranteed to be paid out in full by the time the schedule completes
  (see `vesting::tests::integer_division_does_not_lose_dust_permanently`).
- **This contract does not implement upgradability, pausing, or an
  admin role.** That's a deliberate scope decision, not an oversight —
  every schedule and stream is self-contained and controlled only by its
  own `creator`/`recipient`, with no privileged account able to touch
  funds that aren't theirs. If your deployment needs an emergency pause or
  upgrade path, add it explicitly (see
  [Production readiness notes](#production-readiness-notes)) and have it
  reviewed; don't bolt it on silently.
- **This has not undergone an external security audit.** Treat it as a
  solid, well-tested reference implementation, not an audited,
  battle-tested mainnet primitive, until it has been through one.

## Example usage

### Via stellar-cli

```bash
# Addresses/ids used below are placeholders.
CONTRACT=CA...           # this contract's id
TOKEN=CB...              # a SEP-41 token contract id (e.g. a wrapped asset)
CREATOR=alice
RECIPIENT_ADDR=GRECIPIENT...

# Create a 1-year grant for 100_000 tokens (7 decimals => 1_000_000_000_000
# base units), with a 90-day cliff, cancellable by the creator.
NOW=$(date +%s)
stellar contract invoke --id "$CONTRACT" --source "$CREATOR" --network testnet -- \
  create_vesting \
  --creator "$(stellar keys address $CREATOR)" \
  --recipient "$RECIPIENT_ADDR" \
  --token "$TOKEN" \
  --total_amount 1000000000000 \
  --start_time "$NOW" \
  --cliff_duration 7776000 \
  --vesting_duration 31536000 \
  --cancellable true
# => 1  (the new vesting_id)

# Later, the recipient claims whatever has vested so far.
stellar contract invoke --id "$CONTRACT" --source recipient --network testnet -- \
  claim --vesting_id 1

# The creator can cancel (if still cancellable); the recipient keeps
# whatever had already vested and can still claim it.
stellar contract invoke --id "$CONTRACT" --source "$CREATOR" --network testnet -- \
  cancel --vesting_id 1

# A contributor salary stream: 0.01 tokens/second for 30 days.
stellar contract invoke --id "$CONTRACT" --source "$CREATOR" --network testnet -- \
  create_stream \
  --creator "$(stellar keys address $CREATOR)" \
  --recipient "$RECIPIENT_ADDR" \
  --token "$TOKEN" \
  --rate_per_second 100000 \
  --start_time "$NOW" \
  --end_time $((NOW + 2592000))
# => 1  (the new stream_id)

stellar contract invoke --id "$CONTRACT" --source recipient --network testnet -- \
  withdraw_from_stream --stream_id 1
```

See `./scripts/invoke_examples.sh` for a runnable, end-to-end version of
this against a live testnet deployment.

### From Rust (e.g. in tests or another contract)

```rust
let vesting_id = client.create_vesting(
    &creator,
    &recipient,
    &token_address,
    &1_000_000_000_000i128, // total_amount
    &start_time,
    &7_776_000u64,           // cliff_duration: 90 days
    &31_536_000u64,          // vesting_duration: 365 days
    &true,                   // cancellable
);

// ... time passes on the ledger ...

let claimed: i128 = client.claim(&vesting_id);
```

## Project layout

```
.
├── Cargo.toml                      # workspace root
├── contracts/
│   └── avenirflow/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs               # contract entry points (#[contractimpl])
│           ├── types.rs             # VestingSchedule, Stream, DataKey
│           ├── vesting.rs           # pure vesting-curve math + unit tests
│           ├── stream.rs            # pure streaming math + unit tests
│           ├── storage.rs           # storage access + TTL management
│           ├── events.rs            # #[contractevent] definitions
│           ├── errors.rs            # #[contracterror] Error enum
│           └── test.rs              # end-to-end contract tests
├── scripts/
│   ├── build.sh                     # build + optimize the WASM
│   ├── setup_identity.sh            # create/fund a testnet identity
│   ├── deploy_testnet.sh            # build, optimize, and deploy
│   └── invoke_examples.sh           # live walkthrough on testnet
└── README.md
```

The arithmetic for each primitive is deliberately factored out of
`lib.rs` into `vesting.rs`/`stream.rs` as plain functions over
`Env`-free structs, so the vesting/streaming curves themselves can be
unit tested in isolation from storage, auth, and token transfers, in
addition to being covered end-to-end in `test.rs`.

## Production readiness notes

This contract is structured to be extended for a production deployment,
but ships intentionally minimal:

- **No admin/upgrade key.** Add one (with its own `require_auth` and a
  well-reviewed upgrade path) if your deployment needs it — don't assume
  one implicitly.
- **No pause switch.** Same reasoning: adding one changes the trust model
  (a privileged account gains the power to freeze funds), so it should be
  a deliberate, reviewed decision, not a default.
- **TTL management is call-driven only.** Every read/write to a schedule
  or stream extends its TTL, but a schedule nobody interacts with for a
  long stretch (e.g. a multi-year cliff with no activity) could still
  approach expiry. A production deployment should run an off-chain keeper
  that periodically calls a read method (or a dedicated "ping"/extend
  method) on long-lived, low-activity entries well before they'd expire.
- **No built-in indexing of "all schedules for address X".** By design
  (see [Storage layout](#storage-layout)) — build that off-chain from
  emitted events.
- **Get an external audit** before moving real value through this on
  mainnet. The test suite is thorough for the primitives implemented here,
  but an audit is a different, necessary bar.
