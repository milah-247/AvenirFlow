use soroban_sdk::contracterror;

/// All error conditions the AvenirFlow contract can return.
///
/// Every public entry point returns `Result<T, Error>` instead of panicking
/// directly, so callers (and tests) can assert on the precise failure mode
/// instead of matching on panic strings.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// `total_amount` (vesting) or the implied stream funding amount was <= 0.
    InvalidAmount = 1,
    /// `vesting_duration` was zero, or `cliff_duration > vesting_duration`.
    InvalidDuration = 2,
    /// `end_time <= start_time` for a stream, or a timestamp calculation
    /// would wrap around u64.
    InvalidTimestamps = 3,
    /// No vesting schedule exists for the given id.
    VestingNotFound = 4,
    /// No stream exists for the given id.
    StreamNotFound = 5,
    /// The schedule was created with `cancellable = false`.
    NotCancellable = 6,
    /// `cancel` was called twice on the same schedule.
    AlreadyCancelled = 7,
    /// `claim` / `withdraw_from_stream` was called but nothing new has
    /// vested/streamed since the last claim.
    NothingToClaim = 8,
    /// A checked arithmetic operation would have overflowed or underflowed.
    ArithmeticError = 9,
}
