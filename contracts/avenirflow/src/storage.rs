//! Storage access helpers and TTL (time-to-live) management.
//!
//! Vesting schedules and streams live in *persistent* storage because they
//! must survive for as long as the schedule is active, which can be years.
//! Soroban expires persistent entries that aren't kept alive, so every read
//! and write here extends the entry's TTL. The two id counters live in
//! *instance* storage, alongside the contract's own instance, and are
//! extended once per invocation.

use crate::errors::Error;
use crate::types::{DataKey, Stream, VestingSchedule};
use soroban_sdk::Env;

/// ~5 seconds per ledger on the Stellar network.
const LEDGERS_PER_DAY: u32 = 17_280;

/// Instance storage (holds the two counters): kept alive 30 days at a
/// time, renewed once it drops under 29 days remaining.
const INSTANCE_BUMP: u32 = 30 * LEDGERS_PER_DAY;
const INSTANCE_THRESHOLD: u32 = INSTANCE_BUMP - LEDGERS_PER_DAY;

/// Persistent storage (individual schedules/streams): kept alive ~180 days
/// at a time. Vesting schedules can run for years, so a production
/// deployment should also run a keeper that periodically touches
/// long-lived entries to extend them well before they expire.
const PERSISTENT_BUMP: u32 = 180 * LEDGERS_PER_DAY;
const PERSISTENT_THRESHOLD: u32 = 100 * LEDGERS_PER_DAY;

/// Extend the contract instance's own TTL. Call once at the top of every
/// public entry point.
pub fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_THRESHOLD, INSTANCE_BUMP);
}

fn next_id(env: &Env, key: DataKey) -> u64 {
    let current: u64 = env.storage().instance().get(&key).unwrap_or(0);
    let next = current + 1;
    env.storage().instance().set(&key, &next);
    next
}

pub fn next_vesting_id(env: &Env) -> u64 {
    next_id(env, DataKey::VestingCounter)
}

pub fn next_stream_id(env: &Env) -> u64 {
    next_id(env, DataKey::StreamCounter)
}

pub fn set_vesting(env: &Env, id: u64, schedule: &VestingSchedule) {
    let key = DataKey::Vesting(id);
    env.storage().persistent().set(&key, schedule);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
}

pub fn get_vesting(env: &Env, id: u64) -> Result<VestingSchedule, Error> {
    let key = DataKey::Vesting(id);
    let schedule: VestingSchedule = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(Error::VestingNotFound)?;
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
    Ok(schedule)
}

pub fn set_stream(env: &Env, id: u64, stream: &Stream) {
    let key = DataKey::Stream(id);
    env.storage().persistent().set(&key, stream);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
}

pub fn get_stream(env: &Env, id: u64) -> Result<Stream, Error> {
    let key = DataKey::Stream(id);
    let stream: Stream = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(Error::StreamNotFound)?;
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
    Ok(stream)
}
