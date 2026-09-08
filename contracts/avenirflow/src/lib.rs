//! # AvenirFlow
//!
//! A Stellar / Soroban smart contract implementing two funding primitives:
//!
//! - **Linear vesting with an optional cliff** — `create_vesting`, `claim`,
//!   `cancel`. Suited to contributor grants, team allocations, and
//!   ecosystem funding that unlocks gradually over time.
//! - **Continuous payment streaming** — `create_stream`,
//!   `withdraw_from_stream`. Suited to salaries, subscriptions, or any
//!   payment that should accrue per-second rather than unlock in one shot.
//!
//! Both primitives escrow the full committed amount in the contract at
//! creation time (a real SEP-41 token transfer, not just bookkeeping), so
//! a schedule or stream can never promise more than it can pay.
//!
//! See the crate's `README.md` for storage layout, security notes, and
//! usage examples.

#![no_std]
// The public entry points intentionally mirror the primitives' full
// parameter lists (e.g. `create_vesting`'s signature is fixed by spec);
// splitting them into a config struct would just move the same fields
// around without reducing real complexity.
#![allow(clippy::too_many_arguments)]

mod errors;
mod events;
mod storage;
mod stream;
mod types;
mod vesting;

pub use errors::Error;
pub use types::{DataKey, Stream, VestingSchedule};

use soroban_sdk::{contract, contractimpl, token, Address, Env};

#[contract]
pub struct AvenirFlowContract;

#[contractimpl]
impl AvenirFlowContract {
    // ------------------------------------------------------------------
    // Vesting
    // ------------------------------------------------------------------

    /// Create a linear vesting schedule and escrow `total_amount` of
    /// `token` into the contract.
    ///
    /// Authorization: requires `creator.require_auth()`. `creator` must
    /// also hold (and have authorized spending of) at least
    /// `total_amount` of `token`; the tokens are transferred from
    /// `creator` to this contract as part of this call.
    ///
    /// Validation:
    /// - `total_amount` must be `> 0`.
    /// - `vesting_duration` must be `> 0`.
    /// - `cliff_duration` must be `<= vesting_duration`.
    ///
    /// Returns the new schedule's id.
    pub fn create_vesting(
        env: Env,
        creator: Address,
        recipient: Address,
        token: Address,
        total_amount: i128,
        start_time: u64,
        cliff_duration: u64,
        vesting_duration: u64,
        cancellable: bool,
    ) -> Result<u64, Error> {
        storage::extend_instance_ttl(&env);
        creator.require_auth();

        if total_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if vesting_duration == 0 {
            return Err(Error::InvalidDuration);
        }
        if cliff_duration > vesting_duration {
            return Err(Error::InvalidDuration);
        }
        // Reject configurations whose end-of-vesting timestamp would wrap
        // around u64 -- caught early rather than surfacing later as a
        // confusing arithmetic error deep inside `claim`.
        start_time
            .checked_add(vesting_duration)
            .ok_or(Error::InvalidTimestamps)?;

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&creator, env.current_contract_address(), &total_amount);

        let schedule = VestingSchedule {
            creator: creator.clone(),
            recipient: recipient.clone(),
            token: token.clone(),
            total_amount,
            claimed_amount: 0,
            start_time,
            cliff_duration,
            vesting_duration,
            cancellable,
            cancelled: false,
        };

        let id = storage::next_vesting_id(&env);
        storage::set_vesting(&env, id, &schedule);

        events::VestingCreated {
            vesting_id: id,
            recipient,
            creator,
            token,
            total_amount,
            start_time,
            cliff_duration,
            vesting_duration,
            cancellable,
        }
        .publish(&env);

        Ok(id)
    }

    /// Claim whatever has vested and not yet been claimed for
    /// `vesting_id`, transferring it to the recipient.
    ///
    /// Authorization: requires `recipient.require_auth()`, where
    /// `recipient` is the schedule's stored recipient (looked up from
    /// storage, not taken as a caller-supplied argument, so it cannot be
    /// spoofed).
    ///
    /// Can be called any number of times; each call only pays out the
    /// delta since the last claim. Returns the amount transferred.
    pub fn claim(env: Env, vesting_id: u64) -> Result<i128, Error> {
        storage::extend_instance_ttl(&env);
        let mut schedule = storage::get_vesting(&env, vesting_id)?;
        schedule.recipient.require_auth();

        let now = env.ledger().timestamp();
        let claimable = vesting::claimable_amount(&schedule, now)?;

        if claimable <= 0 {
            return Err(Error::NothingToClaim);
        }

        schedule.claimed_amount = schedule
            .claimed_amount
            .checked_add(claimable)
            .ok_or(Error::ArithmeticError)?;
        storage::set_vesting(&env, vesting_id, &schedule);

        let token_client = token::Client::new(&env, &schedule.token);
        token_client.transfer(&env.current_contract_address(), &schedule.recipient, &claimable);

        events::VestingClaimed {
            vesting_id,
            recipient: schedule.recipient,
            amount: claimable,
            total_claimed: schedule.claimed_amount,
        }
        .publish(&env);

        Ok(claimable)
    }

    /// Cancel a cancellable vesting schedule.
    ///
    /// Authorization: requires `creator.require_auth()` for the
    /// schedule's stored creator.
    ///
    /// Effects: the amount already vested as of the current ledger
    /// timestamp is locked in permanently (the recipient can still
    /// `claim` it, now or later — cancellation never revokes tokens the
    /// recipient had already earned). Every token beyond that amount is
    /// transferred back to `creator` immediately.
    ///
    /// Returns `(vested_amount, amount_returned_to_creator)`.
    pub fn cancel(env: Env, vesting_id: u64) -> Result<(i128, i128), Error> {
        storage::extend_instance_ttl(&env);
        let mut schedule = storage::get_vesting(&env, vesting_id)?;
        schedule.creator.require_auth();

        if !schedule.cancellable {
            return Err(Error::NotCancellable);
        }
        if schedule.cancelled {
            return Err(Error::AlreadyCancelled);
        }

        let now = env.ledger().timestamp();
        let vested = vesting::vested_amount(&schedule, now)?;
        let refund = schedule
            .total_amount
            .checked_sub(vested)
            .ok_or(Error::ArithmeticError)?;

        // Freeze the schedule: total_amount now equals exactly what had
        // vested, so future `claim` calls are naturally capped and
        // `cancelled` short-circuits vested_amount() to this value
        // regardless of how much later `claim` is eventually called.
        schedule.total_amount = vested;
        schedule.cancelled = true;
        storage::set_vesting(&env, vesting_id, &schedule);

        if refund > 0 {
            let token_client = token::Client::new(&env, &schedule.token);
            token_client.transfer(&env.current_contract_address(), &schedule.creator, &refund);
        }

        events::VestingCancelled {
            vesting_id,
            creator: schedule.creator,
            recipient: schedule.recipient,
            vested_amount: vested,
            refunded_to_creator: refund,
        }
        .publish(&env);

        Ok((vested, refund))
    }

    /// Read-only lookup of a vesting schedule.
    pub fn get_vesting(env: Env, vesting_id: u64) -> Result<VestingSchedule, Error> {
        storage::get_vesting(&env, vesting_id)
    }

    /// Read-only: amount currently claimable for `vesting_id` at the
    /// current ledger timestamp, without mutating state.
    pub fn claimable_vesting(env: Env, vesting_id: u64) -> Result<i128, Error> {
        let schedule = storage::get_vesting(&env, vesting_id)?;
        let now = env.ledger().timestamp();
        vesting::claimable_amount(&schedule, now)
    }

    // ------------------------------------------------------------------
    // Streaming
    // ------------------------------------------------------------------

    /// Create a continuous per-second payment stream and escrow
    /// `rate_per_second * (end_time - start_time)` of `token` into the
    /// contract up front.
    ///
    /// Authorization: requires `creator.require_auth()`.
    ///
    /// Validation:
    /// - `rate_per_second` must be `> 0`.
    /// - `end_time` must be `> start_time`.
    ///
    /// Returns the new stream's id.
    pub fn create_stream(
        env: Env,
        creator: Address,
        recipient: Address,
        token: Address,
        rate_per_second: i128,
        start_time: u64,
        end_time: u64,
    ) -> Result<u64, Error> {
        storage::extend_instance_ttl(&env);
        creator.require_auth();

        if rate_per_second <= 0 {
            return Err(Error::InvalidAmount);
        }
        if end_time <= start_time {
            return Err(Error::InvalidTimestamps);
        }

        let deposited_amount = stream::deposit_amount(rate_per_second, start_time, end_time)?;

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&creator, env.current_contract_address(), &deposited_amount);

        let s = Stream {
            creator: creator.clone(),
            recipient: recipient.clone(),
            token: token.clone(),
            rate_per_second,
            start_time,
            end_time,
            deposited_amount,
            withdrawn_amount: 0,
        };

        let id = storage::next_stream_id(&env);
        storage::set_stream(&env, id, &s);

        events::StreamCreated {
            stream_id: id,
            recipient,
            creator,
            token,
            rate_per_second,
            start_time,
            end_time,
            deposited_amount,
        }
        .publish(&env);

        Ok(id)
    }

    /// Withdraw whatever has streamed and not yet been withdrawn for
    /// `stream_id`, transferring it to the recipient.
    ///
    /// Authorization: requires `recipient.require_auth()`, where
    /// `recipient` is the stream's stored recipient.
    ///
    /// Can be called any number of times; each call only pays out the
    /// delta since the last withdrawal, and total payouts across the
    /// stream's lifetime can never exceed `deposited_amount`. Returns the
    /// amount transferred.
    pub fn withdraw_from_stream(env: Env, stream_id: u64) -> Result<i128, Error> {
        storage::extend_instance_ttl(&env);
        let mut s = storage::get_stream(&env, stream_id)?;
        s.recipient.require_auth();

        let now = env.ledger().timestamp();
        let withdrawable = stream::withdrawable_amount(&s, now)?;

        if withdrawable <= 0 {
            return Err(Error::NothingToClaim);
        }

        s.withdrawn_amount = s
            .withdrawn_amount
            .checked_add(withdrawable)
            .ok_or(Error::ArithmeticError)?;
        storage::set_stream(&env, stream_id, &s);

        let token_client = token::Client::new(&env, &s.token);
        token_client.transfer(&env.current_contract_address(), &s.recipient, &withdrawable);

        events::StreamWithdrawn {
            stream_id,
            recipient: s.recipient,
            amount: withdrawable,
            total_withdrawn: s.withdrawn_amount,
        }
        .publish(&env);

        Ok(withdrawable)
    }

    /// Read-only lookup of a stream.
    pub fn get_stream(env: Env, stream_id: u64) -> Result<Stream, Error> {
        storage::get_stream(&env, stream_id)
    }

    /// Read-only: amount currently withdrawable for `stream_id` at the
    /// current ledger timestamp, without mutating state.
    pub fn withdrawable_stream(env: Env, stream_id: u64) -> Result<i128, Error> {
        let s = storage::get_stream(&env, stream_id)?;
        let now = env.ledger().timestamp();
        stream::withdrawable_amount(&s, now)
    }
}

#[cfg(test)]
mod test;
