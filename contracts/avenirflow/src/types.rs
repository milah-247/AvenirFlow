use soroban_sdk::{contracttype, Address};

/// Storage keys used by the contract.
///
/// `Vesting`/`Stream` entries are stored in *persistent* storage (they must
/// survive for the lifetime of the schedule, which can be years). The two
/// counters live in *instance* storage since they are read/written on
/// every creation call and are cheap, small, and always needed alongside
/// the contract's own instance data.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Monotonically increasing id counter for vesting schedules.
    VestingCounter,
    /// Monotonically increasing id counter for streams.
    StreamCounter,
    /// A single vesting schedule, keyed by its id.
    Vesting(u64),
    /// A single stream, keyed by its id.
    Stream(u64),
}

/// A linear vesting schedule with an optional cliff.
///
/// Vesting is linear from `start_time` to `start_time + vesting_duration`.
/// Nothing is claimable before `start_time + cliff_duration`; at that exact
/// moment the pro-rated amount that would have vested since `start_time`
/// becomes claimable all at once (the classic "cliff then linear" shape).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VestingSchedule {
    /// The account that funded the schedule and (if `cancellable`) may
    /// cancel it and reclaim unvested tokens.
    pub creator: Address,
    /// The account entitled to the vested tokens.
    pub recipient: Address,
    /// The SEP-41 token contract being vested.
    pub token: Address,
    /// Total amount deposited into the schedule at creation time. After a
    /// cancellation this is lowered to the amount that had actually vested
    /// at the moment of cancellation, which permanently caps future claims.
    pub total_amount: i128,
    /// Amount already transferred out to the recipient via `claim`.
    pub claimed_amount: i128,
    /// Ledger timestamp (unix seconds) at which vesting begins.
    pub start_time: u64,
    /// Seconds after `start_time` before anything is claimable.
    pub cliff_duration: u64,
    /// Seconds after `start_time` at which the schedule is 100% vested.
    pub vesting_duration: u64,
    /// Whether `creator` is allowed to cancel this schedule.
    pub cancellable: bool,
    /// Set once `cancel` has been called; blocks further cancellation and
    /// freezes `total_amount` at the vested-at-cancellation-time value.
    pub cancelled: bool,
}

/// A continuous, per-second token stream funded up front.
///
/// The stream releases `rate_per_second` tokens for every second between
/// `start_time` and `end_time`. The full amount
/// (`rate_per_second * (end_time - start_time)`) is transferred into the
/// contract at creation time, so a stream can never pay out more than it
/// was funded with.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stream {
    /// The account that funded the stream.
    pub creator: Address,
    /// The account entitled to withdraw streamed tokens.
    pub recipient: Address,
    /// The SEP-41 token contract being streamed.
    pub token: Address,
    /// Tokens released per second while the stream is active.
    pub rate_per_second: i128,
    /// Ledger timestamp (unix seconds) at which streaming begins.
    pub start_time: u64,
    /// Ledger timestamp (unix seconds) at which streaming ends.
    pub end_time: u64,
    /// Total amount deposited at creation:
    /// `rate_per_second * (end_time - start_time)`.
    pub deposited_amount: i128,
    /// Amount already withdrawn by the recipient.
    pub withdrawn_amount: i128,
}
