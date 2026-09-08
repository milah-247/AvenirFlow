//! End-to-end contract tests, driven through the generated test client
//! exactly the way an off-chain caller would invoke the deployed contract.
//!
//! A real Stellar Asset Contract (deployed via
//! `register_stellar_asset_contract_v2`) stands in for the token, so
//! transfers, balances, and authorization all go through the genuine
//! SEP-41 token interface rather than a mock.

use crate::{AvenirFlowContract, AvenirFlowContractClient, Error};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger, LedgerInfo, MockAuth, MockAuthInvoke},
    token, Address, Env, IntoVal,
};

struct Setup<'a> {
    env: Env,
    contract: AvenirFlowContractClient<'a>,
    token: token::Client<'a>,
    token_admin: token::StellarAssetClient<'a>,
    creator: Address,
    recipient: Address,
}

fn setup() -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set(LedgerInfo {
        timestamp: 0,
        protocol_version: 27,
        sequence_number: 0,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 16 * 17_280,
        min_persistent_entry_ttl: 16 * 17_280,
        max_entry_ttl: 365 * 17_280,
    });

    let contract_id = env.register(AvenirFlowContract, ());
    let contract = AvenirFlowContractClient::new(&env, &contract_id);

    let token_admin_addr = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin_addr);
    let token = token::Client::new(&env, &sac.address());
    let token_admin = token::StellarAssetClient::new(&env, &sac.address());

    let creator = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Fund the creator generously; individual tests top up further if
    // they need to test insufficient-funding scenarios precisely.
    token_admin.mint(&creator, &1_000_000_000);

    Setup {
        env,
        contract,
        token,
        token_admin,
        creator,
        recipient,
    }
}

fn advance_to(env: &Env, timestamp: u64) {
    env.ledger().set_timestamp(timestamp);
}

// ----------------------------------------------------------------------
// Vesting: creation
// ----------------------------------------------------------------------

#[test]
fn create_vesting_escrows_tokens_and_returns_id() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &0,
        &1_000,
        &true,
    );
    assert_eq!(id, 1);
    assert_eq!(s.token.balance(&s.creator), 1_000_000_000 - 1_000_000);
    assert_eq!(s.token.balance(&s.contract.address), 1_000_000);

    let schedule = s.contract.get_vesting(&id);
    assert_eq!(schedule.creator, s.creator);
    assert_eq!(schedule.recipient, s.recipient);
    assert_eq!(schedule.token, s.token.address);
    assert_eq!(schedule.total_amount, 1_000_000);
    assert_eq!(schedule.claimed_amount, 0);
    assert!(!schedule.cancelled);
}

#[test]
fn vesting_ids_increment_independently_per_schedule() {
    let s = setup();
    let id1 = s
        .contract
        .create_vesting(&s.creator, &s.recipient, &s.token.address, &100, &0, &0, &100, &true);
    let id2 = s
        .contract
        .create_vesting(&s.creator, &s.recipient, &s.token.address, &100, &0, &0, &100, &true);
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
}

#[test]
fn create_vesting_rejects_zero_amount() {
    let s = setup();
    let res =
        s.contract
            .try_create_vesting(&s.creator, &s.recipient, &s.token.address, &0, &0, &0, &1_000, &true);
    assert_eq!(res, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn create_vesting_rejects_negative_amount() {
    let s = setup();
    let res = s.contract.try_create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &-100,
        &0,
        &0,
        &1_000,
        &true,
    );
    assert_eq!(res, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn create_vesting_rejects_zero_duration() {
    let s = setup();
    let res =
        s.contract
            .try_create_vesting(&s.creator, &s.recipient, &s.token.address, &1_000, &0, &0, &0, &true);
    assert_eq!(res, Err(Ok(Error::InvalidDuration)));
}

#[test]
fn create_vesting_rejects_cliff_longer_than_duration() {
    let s = setup();
    let res = s.contract.try_create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000,
        &0,
        &2_000,
        &1_000,
        &true,
    );
    assert_eq!(res, Err(Ok(Error::InvalidDuration)));
}

#[test]
fn create_vesting_rejects_insufficient_funding() {
    let s = setup();
    // creator only has 1_000_000_000; ask for more than that.
    let res = s.contract.try_create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &2_000_000_000,
        &0,
        &0,
        &1_000,
        &true,
    );
    // The token contract itself panics on insufficient balance, which
    // surfaces to the client as an error rather than a decoded contract
    // Error variant.
    assert!(res.is_err());
}

#[test]
fn create_vesting_fails_when_funding_is_exactly_one_short() {
    let s = setup();
    let poor_creator = Address::generate(&s.env);
    s.token_admin.mint(&poor_creator, &999);

    let res = s.contract.try_create_vesting(
        &poor_creator,
        &s.recipient,
        &s.token.address,
        &1_000,
        &0,
        &0,
        &1_000,
        &true,
    );
    assert!(res.is_err());
    // The failed transfer must not have partially debited the creator.
    assert_eq!(s.token.balance(&poor_creator), 999);
}

#[test]
#[should_panic]
fn create_vesting_requires_creator_auth() {
    let s = setup();
    let impostor = Address::generate(&s.env);

    // Mock an auth entry for `impostor` only. The contract's
    // `creator.require_auth()` needs an entry for `s.creator`, which
    // this does not provide, so the call panics.
    s.env.mock_auths(&[MockAuth {
        address: &impostor,
        invoke: &MockAuthInvoke {
            contract: &s.contract.address,
            fn_name: "create_vesting",
            args: (
                &s.creator,
                &s.recipient,
                &s.token.address,
                1_000_000i128,
                0u64,
                0u64,
                1_000u64,
                true,
            )
                .into_val(&s.env),
            sub_invokes: &[],
        },
    }]);
    s.contract
        .create_vesting(&s.creator, &s.recipient, &s.token.address, &1_000_000, &0, &0, &1_000, &true);
}

// ----------------------------------------------------------------------
// Vesting: claiming
// ----------------------------------------------------------------------

#[test]
fn claim_before_cliff_yields_nothing_to_claim() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &100,
        &1_000,
        &true,
    );
    advance_to(&s.env, 50);
    let res = s.contract.try_claim(&id);
    assert_eq!(res, Err(Ok(Error::NothingToClaim)));
}

#[test]
fn claim_at_cliff_releases_pro_rated_amount() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &100,
        &1_000,
        &true,
    );
    advance_to(&s.env, 100);
    let claimed = s.contract.claim(&id);
    assert_eq!(claimed, 100_000);
    assert_eq!(s.token.balance(&s.recipient), 100_000);
}

#[test]
fn partial_claims_only_pay_the_new_delta() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &0,
        &1_000,
        &true,
    );
    advance_to(&s.env, 250);
    let first = s.contract.claim(&id);
    assert_eq!(first, 250_000);

    advance_to(&s.env, 500);
    let second = s.contract.claim(&id);
    assert_eq!(second, 250_000);

    assert_eq!(s.token.balance(&s.recipient), 500_000);
    let schedule = s.contract.get_vesting(&id);
    assert_eq!(schedule.claimed_amount, 500_000);
}

#[test]
fn multiple_claims_never_double_pay() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &0,
        &1_000,
        &true,
    );
    advance_to(&s.env, 300);
    s.contract.claim(&id);

    // Calling claim again at the same timestamp with nothing new vested
    // must fail rather than re-paying the same tranche.
    let res = s.contract.try_claim(&id);
    assert_eq!(res, Err(Ok(Error::NothingToClaim)));
    assert_eq!(s.token.balance(&s.recipient), 300_000);
}

#[test]
fn claim_after_full_vesting_pays_out_remaining_total() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &0,
        &1_000,
        &true,
    );
    advance_to(&s.env, 400);
    s.contract.claim(&id);

    advance_to(&s.env, 10_000); // long past vesting_duration
    let claimed = s.contract.claim(&id);
    assert_eq!(claimed, 600_000);
    assert_eq!(s.token.balance(&s.recipient), 1_000_000);
    assert_eq!(s.token.balance(&s.contract.address), 0);
}

#[test]
#[should_panic]
fn claim_unauthorized_recipient_panics() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &0,
        &1_000,
        &true,
    );
    advance_to(&s.env, 500);

    let impostor = Address::generate(&s.env);
    // Mock an auth entry for `impostor` only. The contract looks up the
    // schedule's stored recipient and calls
    // `schedule.recipient.require_auth()` on *that* address, which this
    // mocked entry does not cover, so the call panics.
    s.env.mock_auths(&[MockAuth {
        address: &impostor,
        invoke: &MockAuthInvoke {
            contract: &s.contract.address,
            fn_name: "claim",
            args: (id,).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);
    s.contract.claim(&id);
}

#[test]
#[should_panic]
fn cancel_by_non_creator_panics() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &0,
        &1_000,
        &true,
    );
    advance_to(&s.env, 500);

    // Even the recipient cannot cancel -- only the creator can.
    s.env.mock_auths(&[MockAuth {
        address: &s.recipient,
        invoke: &MockAuthInvoke {
            contract: &s.contract.address,
            fn_name: "cancel",
            args: (id,).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);
    s.contract.cancel(&id);
}

// ----------------------------------------------------------------------
// Vesting: cancellation
// ----------------------------------------------------------------------

#[test]
fn cancel_preserves_vested_and_refunds_unvested() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &0,
        &1_000,
        &true,
    );
    advance_to(&s.env, 400);
    let (vested, refunded) = s.contract.cancel(&id);
    assert_eq!(vested, 400_000);
    assert_eq!(refunded, 600_000);
    assert_eq!(s.token.balance(&s.creator), 1_000_000_000 - 400_000);

    let schedule = s.contract.get_vesting(&id);
    assert!(schedule.cancelled);
    assert_eq!(schedule.total_amount, 400_000);

    // Recipient can still claim their vested share after cancellation.
    let claimed = s.contract.claim(&id);
    assert_eq!(claimed, 400_000);
    assert_eq!(s.token.balance(&s.recipient), 400_000);
}

#[test]
fn cancel_after_partial_claim_only_refunds_remaining_unvested() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &0,
        &1_000,
        &true,
    );
    advance_to(&s.env, 200);
    s.contract.claim(&id); // claims 200_000

    advance_to(&s.env, 500);
    let (vested, refunded) = s.contract.cancel(&id);
    assert_eq!(vested, 500_000);
    assert_eq!(refunded, 500_000);

    // 200_000 already claimed, 300_000 still claimable post-cancel.
    let claimed = s.contract.claim(&id);
    assert_eq!(claimed, 300_000);
    assert_eq!(s.token.balance(&s.recipient), 500_000);
}

#[test]
fn cancel_at_or_after_full_vesting_refunds_nothing() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &0,
        &1_000,
        &true,
    );
    advance_to(&s.env, 5_000);
    let (vested, refunded) = s.contract.cancel(&id);
    assert_eq!(vested, 1_000_000);
    assert_eq!(refunded, 0);
}

#[test]
fn cancel_non_cancellable_schedule_fails() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &0,
        &1_000,
        &false,
    );
    advance_to(&s.env, 100);
    let res = s.contract.try_cancel(&id);
    assert_eq!(res, Err(Ok(Error::NotCancellable)));
}

#[test]
fn cancel_twice_fails_second_time() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &0,
        &1_000,
        &true,
    );
    advance_to(&s.env, 100);
    s.contract.cancel(&id);
    let res = s.contract.try_cancel(&id);
    assert_eq!(res, Err(Ok(Error::AlreadyCancelled)));
}

// ----------------------------------------------------------------------
// Vesting: lookups & invalid ids
// ----------------------------------------------------------------------

#[test]
fn operations_on_unknown_vesting_id_fail_cleanly() {
    let s = setup();
    assert_eq!(s.contract.try_get_vesting(&999), Err(Ok(Error::VestingNotFound)));
    assert_eq!(s.contract.try_claim(&999), Err(Ok(Error::VestingNotFound)));
    assert_eq!(s.contract.try_cancel(&999), Err(Ok(Error::VestingNotFound)));
}

// ----------------------------------------------------------------------
// Streams
// ----------------------------------------------------------------------

#[test]
fn create_stream_escrows_full_committed_amount() {
    let s = setup();
    let id = s
        .contract
        .create_stream(&s.creator, &s.recipient, &s.token.address, &10, &0, &1_000);
    assert_eq!(id, 1);
    assert_eq!(s.token.balance(&s.contract.address), 10_000);

    let stream = s.contract.get_stream(&id);
    assert_eq!(stream.deposited_amount, 10_000);
    assert_eq!(stream.withdrawn_amount, 0);
}

#[test]
fn create_stream_rejects_zero_rate() {
    let s = setup();
    let res = s
        .contract
        .try_create_stream(&s.creator, &s.recipient, &s.token.address, &0, &0, &1_000);
    assert_eq!(res, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn create_stream_rejects_end_before_start() {
    let s = setup();
    let res = s
        .contract
        .try_create_stream(&s.creator, &s.recipient, &s.token.address, &10, &1_000, &500);
    assert_eq!(res, Err(Ok(Error::InvalidTimestamps)));
}

#[test]
fn create_stream_rejects_end_equal_start() {
    let s = setup();
    let res = s
        .contract
        .try_create_stream(&s.creator, &s.recipient, &s.token.address, &10, &1_000, &1_000);
    assert_eq!(res, Err(Ok(Error::InvalidTimestamps)));
}

#[test]
fn create_stream_rejects_insufficient_funding() {
    let s = setup();
    // rate * duration vastly exceeds the creator's 1_000_000_000 balance.
    let res = s.contract.try_create_stream(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &1_000_000,
    );
    assert!(res.is_err());
}

#[test]
fn create_stream_rejects_overflowing_deposit() {
    let s = setup();
    let res = s.contract.try_create_stream(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &i128::MAX,
        &0,
        &u64::MAX,
    );
    assert_eq!(res, Err(Ok(Error::ArithmeticError)));
}

#[test]
fn withdraw_before_start_yields_nothing_to_claim() {
    let s = setup();
    let id = s
        .contract
        .create_stream(&s.creator, &s.recipient, &s.token.address, &10, &1_000, &2_000);
    advance_to(&s.env, 500);
    let res = s.contract.try_withdraw_from_stream(&id);
    assert_eq!(res, Err(Ok(Error::NothingToClaim)));
}

#[test]
fn partial_withdrawals_only_pay_the_new_delta() {
    let s = setup();
    let id = s
        .contract
        .create_stream(&s.creator, &s.recipient, &s.token.address, &10, &0, &1_000);

    advance_to(&s.env, 100);
    let first = s.contract.withdraw_from_stream(&id);
    assert_eq!(first, 1_000);

    advance_to(&s.env, 300);
    let second = s.contract.withdraw_from_stream(&id);
    assert_eq!(second, 2_000);

    assert_eq!(s.token.balance(&s.recipient), 3_000);
}

#[test]
fn withdraw_never_exceeds_funded_amount_even_long_after_end() {
    let s = setup();
    let id = s
        .contract
        .create_stream(&s.creator, &s.recipient, &s.token.address, &10, &0, &1_000);
    advance_to(&s.env, 1_000_000); // far past end_time
    let withdrawn = s.contract.withdraw_from_stream(&id);
    assert_eq!(withdrawn, 10_000); // capped at deposited_amount
    assert_eq!(s.token.balance(&s.contract.address), 0);

    // A second withdrawal has nothing left to pay out.
    let res = s.contract.try_withdraw_from_stream(&id);
    assert_eq!(res, Err(Ok(Error::NothingToClaim)));
}

#[test]
fn repeated_withdrawals_never_double_pay() {
    let s = setup();
    let id = s
        .contract
        .create_stream(&s.creator, &s.recipient, &s.token.address, &10, &0, &1_000);
    advance_to(&s.env, 200);
    s.contract.withdraw_from_stream(&id);

    let res = s.contract.try_withdraw_from_stream(&id);
    assert_eq!(res, Err(Ok(Error::NothingToClaim)));
    assert_eq!(s.token.balance(&s.recipient), 2_000);
}

#[test]
fn operations_on_unknown_stream_id_fail_cleanly() {
    let s = setup();
    assert_eq!(s.contract.try_get_stream(&42), Err(Ok(Error::StreamNotFound)));
    assert_eq!(
        s.contract.try_withdraw_from_stream(&42),
        Err(Ok(Error::StreamNotFound))
    );
}

// ----------------------------------------------------------------------
// Read-only helper views
// ----------------------------------------------------------------------

#[test]
fn claimable_vesting_view_matches_actual_claim() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &0,
        &1_000,
        &true,
    );
    advance_to(&s.env, 300);
    let previewed = s.contract.claimable_vesting(&id);
    let claimed = s.contract.claim(&id);
    assert_eq!(previewed, claimed);
}

#[test]
fn withdrawable_stream_view_matches_actual_withdrawal() {
    let s = setup();
    let id = s
        .contract
        .create_stream(&s.creator, &s.recipient, &s.token.address, &10, &0, &1_000);
    advance_to(&s.env, 300);
    let previewed = s.contract.withdrawable_stream(&id);
    let withdrawn = s.contract.withdraw_from_stream(&id);
    assert_eq!(previewed, withdrawn);
}

// ----------------------------------------------------------------------
// Events
// ----------------------------------------------------------------------

#[test]
fn creation_and_claim_emit_events() {
    let s = setup();
    let id = s.contract.create_vesting(
        &s.creator,
        &s.recipient,
        &s.token.address,
        &1_000_000,
        &0,
        &0,
        &1_000,
        &true,
    );
    advance_to(&s.env, 500);
    s.contract.claim(&id);

    // At least one event was published for creation and one for claim;
    // exact ordering/count of surrounding token-transfer events is an
    // implementation detail of the SAC we don't assert on.
    let events = s.env.events().all();
    assert!(events.events().len() >= 2);
}
