//! Contract event definitions.
//!
//! Each mutating entry point publishes exactly one of these, defined with
//! the `#[contractevent]` macro so the event shapes are part of the
//! contract's published interface spec and off-chain indexers get typed
//! topics/data instead of having to guess a schema. `vesting_id` /
//! `stream_id` and `recipient` are marked `#[topic]` so events can be
//! filtered by schedule or by recipient without scanning event data
//! payloads.

use soroban_sdk::{contractevent, Address};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VestingCreated {
    #[topic]
    pub vesting_id: u64,
    #[topic]
    pub recipient: Address,
    pub creator: Address,
    pub token: Address,
    pub total_amount: i128,
    pub start_time: u64,
    pub cliff_duration: u64,
    pub vesting_duration: u64,
    pub cancellable: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VestingClaimed {
    #[topic]
    pub vesting_id: u64,
    #[topic]
    pub recipient: Address,
    pub amount: i128,
    pub total_claimed: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VestingCancelled {
    #[topic]
    pub vesting_id: u64,
    #[topic]
    pub creator: Address,
    pub recipient: Address,
    pub vested_amount: i128,
    pub refunded_to_creator: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamCreated {
    #[topic]
    pub stream_id: u64,
    #[topic]
    pub recipient: Address,
    pub creator: Address,
    pub token: Address,
    pub rate_per_second: i128,
    pub start_time: u64,
    pub end_time: u64,
    pub deposited_amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamWithdrawn {
    #[topic]
    pub stream_id: u64,
    #[topic]
    pub recipient: Address,
    pub amount: i128,
    pub total_withdrawn: i128,
}
