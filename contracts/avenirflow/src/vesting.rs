//! Pure arithmetic for linear vesting with an optional cliff.
//!
//! Kept free of `Env`/storage so the vesting curve itself can be unit
//! tested directly, in addition to the end-to-end contract tests.

use crate::errors::Error;
use crate::types::VestingSchedule;

/// Total amount that has vested for `schedule` as of `now`.
///
/// - Before `start_time + cliff_duration`: `0`.
/// - At/after `start_time + vesting_duration`: `total_amount` (capped).
/// - In between: `total_amount * (now - start_time) / vesting_duration`,
///   computed with `i128` checked arithmetic so a large `total_amount`
///   times elapsed seconds can never silently wrap.
///
/// Once `schedule.cancelled` is `true`, `total_amount` has already been
/// frozen (by `cancel`) at exactly the amount vested at cancellation time,
/// so this function naturally returns that frozen amount for any `now`
/// at or after cancellation.
pub fn vested_amount(schedule: &VestingSchedule, now: u64) -> Result<i128, Error> {
    // `cancel` freezes `total_amount` at exactly what had vested at the
    // moment of cancellation. Short-circuit here rather than letting the
    // linear formula below run again with the now-reduced total_amount,
    // which would incorrectly re-scale it down further.
    if schedule.cancelled {
        return Ok(schedule.total_amount);
    }

    let cliff_end = schedule
        .start_time
        .checked_add(schedule.cliff_duration)
        .ok_or(Error::ArithmeticError)?;

    if now < cliff_end {
        return Ok(0);
    }

    let vesting_end = schedule
        .start_time
        .checked_add(schedule.vesting_duration)
        .ok_or(Error::ArithmeticError)?;

    if now >= vesting_end {
        return Ok(schedule.total_amount);
    }

    // now is strictly between start_time and vesting_end here, so this
    // subtraction never underflows.
    let elapsed = now - schedule.start_time;

    let elapsed_i128: i128 = elapsed.into();
    let duration_i128: i128 = schedule.vesting_duration.into();

    schedule
        .total_amount
        .checked_mul(elapsed_i128)
        .ok_or(Error::ArithmeticError)?
        .checked_div(duration_i128)
        .ok_or(Error::ArithmeticError)
}

/// Amount currently claimable: `vested_amount(now) - claimed_amount`.
///
/// Never negative in practice (claimed_amount can't exceed what had
/// vested when it was claimed, and vested_amount is monotonic in `now`
/// outside of cancellation, which only ever freezes it in place).
pub fn claimable_amount(schedule: &VestingSchedule, now: u64) -> Result<i128, Error> {
    let vested = vested_amount(schedule, now)?;
    vested
        .checked_sub(schedule.claimed_amount)
        .ok_or(Error::ArithmeticError)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    fn schedule(env: &Env, total: i128, start: u64, cliff: u64, duration: u64) -> VestingSchedule {
        VestingSchedule {
            creator: Address::generate(env),
            recipient: Address::generate(env),
            token: Address::generate(env),
            total_amount: total,
            claimed_amount: 0,
            start_time: start,
            cliff_duration: cliff,
            vesting_duration: duration,
            cancellable: true,
            cancelled: false,
        }
    }

    #[test]
    fn before_cliff_nothing_vests() {
        let env = Env::default();
        let s = schedule(&env, 1_000_000, 1_000, 100, 1_000);
        assert_eq!(vested_amount(&s, 1_000).unwrap(), 0);
        assert_eq!(vested_amount(&s, 1_099).unwrap(), 0);
    }

    #[test]
    fn at_cliff_pro_rated_amount_unlocks_at_once() {
        let env = Env::default();
        // 10% cliff duration relative to total duration.
        let s = schedule(&env, 1_000_000, 0, 100, 1_000);
        // at the cliff boundary, 100/1000 = 10% has "vested" by the linear
        // formula and becomes claimable immediately.
        assert_eq!(vested_amount(&s, 100).unwrap(), 100_000);
    }

    #[test]
    fn linear_between_cliff_and_end() {
        let env = Env::default();
        let s = schedule(&env, 1_000_000, 0, 0, 1_000);
        assert_eq!(vested_amount(&s, 250).unwrap(), 250_000);
        assert_eq!(vested_amount(&s, 500).unwrap(), 500_000);
        assert_eq!(vested_amount(&s, 999).unwrap(), 999_000);
    }

    #[test]
    fn fully_vested_at_and_after_end() {
        let env = Env::default();
        let s = schedule(&env, 1_000_000, 0, 0, 1_000);
        assert_eq!(vested_amount(&s, 1_000).unwrap(), 1_000_000);
        assert_eq!(vested_amount(&s, 10_000_000).unwrap(), 1_000_000);
    }

    #[test]
    fn claimable_subtracts_prior_claims() {
        let env = Env::default();
        let mut s = schedule(&env, 1_000_000, 0, 0, 1_000);
        s.claimed_amount = 400_000;
        assert_eq!(claimable_amount(&s, 500).unwrap(), 100_000);
    }

    #[test]
    fn integer_division_does_not_lose_dust_permanently() {
        let env = Env::default();
        // total not evenly divisible by duration -- exercises truncation.
        let s = schedule(&env, 1_000_000_001, 0, 0, 3);
        let at_1 = vested_amount(&s, 1).unwrap();
        let at_2 = vested_amount(&s, 2).unwrap();
        let at_end = vested_amount(&s, 3).unwrap();
        // fully vested amount always exactly equals total_amount, so any
        // truncation dust from intermediate steps is paid out at the end.
        assert_eq!(at_end, 1_000_000_001);
        assert!(at_1 <= at_2);
        assert!(at_2 <= at_end);
    }

    #[test]
    fn huge_amount_does_not_overflow_i128() {
        let env = Env::default();
        // near i128::MAX, still resolves without overflow because we
        // multiply then divide with checked ops and duration is small.
        let s = schedule(&env, i128::MAX / 2, 0, 0, 2);
        assert!(vested_amount(&s, 1).is_ok());
        assert_eq!(vested_amount(&s, 2).unwrap(), i128::MAX / 2);
    }
}
