//! Pure arithmetic for continuous per-second payment streams.

use crate::errors::Error;
use crate::types::Stream;

/// The total amount a stream must be funded with at creation:
/// `rate_per_second * (end_time - start_time)`.
///
/// Callers must have already validated `end_time > start_time` and
/// `rate_per_second > 0`.
pub fn deposit_amount(rate_per_second: i128, start_time: u64, end_time: u64) -> Result<i128, Error> {
    let duration = end_time
        .checked_sub(start_time)
        .ok_or(Error::InvalidTimestamps)?;
    let duration_i128: i128 = duration.into();
    rate_per_second
        .checked_mul(duration_i128)
        .ok_or(Error::ArithmeticError)
}

/// Total amount streamed (earned) so far as of `now`, capped at
/// `deposited_amount` so a stream can never be asked to pay out more than
/// it was funded with, even if `now` is far past `end_time`.
pub fn streamed_amount(stream: &Stream, now: u64) -> Result<i128, Error> {
    if now <= stream.start_time {
        return Ok(0);
    }
    let capped_now = if now > stream.end_time {
        stream.end_time
    } else {
        now
    };
    // capped_now is always >= start_time here.
    let elapsed = capped_now - stream.start_time;
    let elapsed_i128: i128 = elapsed.into();

    let earned = stream
        .rate_per_second
        .checked_mul(elapsed_i128)
        .ok_or(Error::ArithmeticError)?;

    Ok(earned.min(stream.deposited_amount))
}

/// Amount currently withdrawable: `streamed_amount(now) - withdrawn_amount`.
pub fn withdrawable_amount(stream: &Stream, now: u64) -> Result<i128, Error> {
    let streamed = streamed_amount(stream, now)?;
    streamed
        .checked_sub(stream.withdrawn_amount)
        .ok_or(Error::ArithmeticError)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    fn stream(env: &Env, rate: i128, start: u64, end: u64) -> Stream {
        Stream {
            creator: Address::generate(env),
            recipient: Address::generate(env),
            token: Address::generate(env),
            rate_per_second: rate,
            start_time: start,
            end_time: end,
            deposited_amount: deposit_amount(rate, start, end).unwrap(),
            withdrawn_amount: 0,
        }
    }

    #[test]
    fn nothing_before_start() {
        let env = Env::default();
        let s = stream(&env, 10, 1_000, 2_000);
        assert_eq!(streamed_amount(&s, 500).unwrap(), 0);
        assert_eq!(streamed_amount(&s, 1_000).unwrap(), 0);
    }

    #[test]
    fn linear_mid_stream() {
        let env = Env::default();
        let s = stream(&env, 10, 0, 1_000);
        assert_eq!(streamed_amount(&s, 100).unwrap(), 1_000);
        assert_eq!(streamed_amount(&s, 999).unwrap(), 9_990);
    }

    #[test]
    fn capped_at_deposit_after_end() {
        let env = Env::default();
        let s = stream(&env, 10, 0, 1_000);
        assert_eq!(streamed_amount(&s, 1_000).unwrap(), 10_000);
        assert_eq!(streamed_amount(&s, 50_000).unwrap(), 10_000);
    }

    #[test]
    fn withdrawable_subtracts_prior_withdrawals() {
        let env = Env::default();
        let mut s = stream(&env, 10, 0, 1_000);
        s.withdrawn_amount = 4_000;
        assert_eq!(withdrawable_amount(&s, 500).unwrap(), 1_000);
    }

    #[test]
    fn deposit_amount_zero_duration_is_zero() {
        // Equal start/end is a zero-length range; the pure function
        // itself just computes 0 for it. `create_stream` is the layer
        // responsible for rejecting `end_time <= start_time` outright.
        assert_eq!(deposit_amount(10, 1_000, 1_000).unwrap(), 0);
    }

    #[test]
    fn deposit_amount_rejects_end_before_start() {
        assert_eq!(
            deposit_amount(10, 2_000, 1_000).unwrap_err(),
            Error::InvalidTimestamps
        );
    }

    #[test]
    fn deposit_amount_overflow_is_caught() {
        assert_eq!(
            deposit_amount(i128::MAX, 0, u64::MAX).unwrap_err(),
            Error::ArithmeticError
        );
    }
}
