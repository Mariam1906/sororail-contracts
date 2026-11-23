// resolve

use super::*;

// --------------------------------------------------------------- resolve

#[test]
fn resolve_splits_between_the_parties() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.beneficiary);
    f.client.resolve(&2_500);

    assert_eq!(f.token.balance(&f.beneficiary), AMOUNT / 4);
    assert_eq!(f.escrow_balance(), 0);
    assert_eq!(f.client.state(), State::Resolved);
}

#[test]
fn resolve_requires_a_dispute() {
    let f = Fixture::funded(true);
    assert_eq!(
        f.client.try_resolve(&5_000),
        Err(Ok(Error::EscrowNotDisputed))
    );
}

#[test]
fn resolve_guards_against_a_missing_arbiter_even_if_disputed_is_reached() {
    // `dispute` already refuses to run without an arbiter, so reaching
    // `State::Disputed` without one is impossible through the public API.
    // `resolve` re-checks with `ok_or(Error::EscrowNoArbiter)` as
    // defense-in-depth (#464): returns a typed error rather than a panic.
    let f = Fixture::funded(false);
    f.env.as_contract(&f.client.address, || {
        let mut escrow = crate::storage::load(&f.env).unwrap();
        escrow.state = State::Disputed;
        crate::storage::save(&f.env, &escrow);
    });

    assert_eq!(
        f.client.try_resolve(&5_000),
        Err(Ok(Error::EscrowNoArbiter))
    );
}

#[test]
fn resolve_rejects_basis_points_above_the_maximum() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    assert_eq!(
        f.client.try_resolve(&10_001),
        Err(Ok(Error::InvalidBasisPoints))
    );
    // Still resolvable afterwards -- the failed call changed nothing.
    assert_eq!(f.client.state(), State::Disputed);
}

#[test]
fn resolve_rejects_arbiter_fee_above_the_cap() {
    let te = TestEnv::at(START_TS);
    let (token, depositor) = te.make_token(AMOUNT * 10);
    let beneficiary = te.make_address();
    let arbiter = te.make_address();
    let env = te.env();
    let client = EscrowContractClient::new(env, &env.register(EscrowContract, ()));
    assert_eq!(
        client.try_init(
            &depositor,
            &beneficiary,
            &Some(arbiter),
            &token.address,
            &AMOUNT,
            &DEADLINE,
            &None,
            &None,
            &1_001,
        ),
        Err(Ok(Error::InvalidBasisPoints))
    );
}

#[test]
fn resolve_rejects_a_split_whose_product_overflows_at_the_max_amount() {
    // `split_bps` multiplies `amount * bps` in i128 before dividing, so an
    // escrow holding exactly i128::MAX can only be resolved at 0 or 1 bps;
    // 2 bps is the first split whose intermediate product overflows. (The
    // shared `Fixture` mints `amount * 10`, which cannot be built at MAX.)
    let te = TestEnv::at(START_TS);
    let (token, depositor) = te.make_token(i128::MAX);
    let token_address = token.address.clone();
    let beneficiary = te.make_address();
    let arbiter = te.make_address();
    let env = te.env().clone();
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
    client.init(
        &depositor,
        &beneficiary,
        &Some(arbiter),
        &token_address,
        &i128::MAX,
        &DEADLINE,
        &None,
        &None,
        &0,
        &None,
        &0,
    );
    client.fund();
    client.dispute(&depositor);

    assert_eq!(client.try_resolve(&2), Err(Ok(Error::Overflow)));
    // The failed call changed nothing.
    assert_eq!(client.state(), State::Disputed);
    assert_eq!(token.balance(&client.address), i128::MAX);

    // 1 bps is the largest split that still resolves, and it conserves the
    // full amount exactly.
    client.resolve(&1);
    let to_beneficiary = i128::MAX / 10_000;
    assert_eq!(token.balance(&beneficiary), to_beneficiary);
    assert_eq!(token.balance(&depositor), i128::MAX - to_beneficiary);
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(client.state(), State::Resolved);
}

#[test]
fn resolve_endpoints_pay_one_side_in_full() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    f.client.resolve(&10_000);
    assert_eq!(f.token.balance(&f.beneficiary), AMOUNT);

    let g = Fixture::funded(true);
    let depositor_before = g.token.balance(&g.depositor);
    g.client.dispute(&g.depositor);
    g.client.resolve(&0);
    assert_eq!(g.token.balance(&g.depositor), depositor_before + AMOUNT);
    assert_eq!(g.token.balance(&g.beneficiary), 0);
}

// Issue #476: boundary splits with a token that rejects zero-value transfers.
//
// The Stellar Asset Contract happily accepts a transfer of 0, so the tests
// above cannot tell whether `resolve` skips the empty side of a 0 or 10_000
// bps split. Some tokens are stricter and trap on a zero amount; against one
// of those, an unconditional `pay` of the empty share would abort the whole
// resolution and strand the funds. `StrictToken` is a minimal token that
// panics on any non-positive transfer, so both boundaries must pay exactly one
// side, leave no dust, and never attempt a zero transfer.
mod strict_token {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

    #[contracttype]
    enum DataKey {
        Balance(Address),
        Transfers,
    }

    #[contract]
    pub struct StrictToken;

    #[contractimpl]
    impl StrictToken {
        pub fn mint(env: Env, to: Address, amount: i128) {
            let balance = Self::balance(env.clone(), to.clone());
            let updated = balance.checked_add(amount).expect("mint overflow");
            env.storage()
                .persistent()
                .set(&DataKey::Balance(to), &updated);
        }

        pub fn balance(env: Env, id: Address) -> i128 {
            env.storage()
                .persistent()
                .get(&DataKey::Balance(id))
                .unwrap_or(0)
        }

        /// Number of successful transfers, so tests can count exactly how many
        /// payments a call made.
        pub fn transfers(env: Env) -> u32 {
            env.storage()
                .instance()
                .get(&DataKey::Transfers)
                .unwrap_or(0)
        }

        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            assert!(amount > 0, "strict token: zero-value transfer rejected");
            from.require_auth();
            let from_balance = Self::balance(env.clone(), from.clone());
            let from_after = from_balance
                .checked_sub(amount)
                .filter(|b| *b >= 0)
                .expect("strict token: insufficient balance");
            env.storage()
                .persistent()
                .set(&DataKey::Balance(from), &from_after);
            let to_after = Self::balance(env.clone(), to.clone())
                .checked_add(amount)
                .expect("strict token: balance overflow");
            env.storage()
                .persistent()
                .set(&DataKey::Balance(to), &to_after);
            let count = Self::transfers(env.clone())
                .checked_add(1)
                .expect("transfer count overflow");
            env.storage().instance().set(&DataKey::Transfers, &count);
        }
    }
}

use strict_token::{StrictToken, StrictTokenClient};

/// A funded, disputed escrow whose token is `StrictToken`.
fn disputed_with_strict_token(
    env: &Env,
) -> (
    EscrowContractClient<'_>,
    StrictTokenClient<'_>,
    Address,
    Address,
) {
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);
    let token = StrictTokenClient::new(env, &env.register(StrictToken, ()));
    let depositor = Address::generate(env);
    let beneficiary = Address::generate(env);
    let arbiter = Address::generate(env);
    token.mint(&depositor, &AMOUNT);

    let client = EscrowContractClient::new(env, &env.register(EscrowContract, ()));
    client.init(
        &depositor,
        &beneficiary,
        &Some(arbiter),
        &token.address,
        &AMOUNT,
        &DEADLINE,
        &None,
        &None,
        &0,
    );
    client.fund();
    client.dispute(&depositor);
    (client, token, depositor, beneficiary)
}

#[test]
fn strict_token_rejects_a_zero_value_transfer() {
    // Sanity-check the mock itself: without this, the boundary tests below
    // would pass vacuously if the mock silently accepted zero.
    let env = Env::default();
    env.mock_all_auths();
    let token = StrictTokenClient::new(&env, &env.register(StrictToken, ()));
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    token.mint(&a, &10);
    assert!(token.try_transfer(&a, &b, &0).is_err());
    assert_eq!(token.balance(&a), 10);
    assert_eq!(token.transfers(), 0);
}

#[test]
fn resolve_at_zero_bps_pays_only_the_depositor_with_a_strict_token() {
    let env = Env::default();
    let (client, token, depositor, beneficiary) = disputed_with_strict_token(&env);
    // `fund` was the only transfer so far.
    assert_eq!(token.transfers(), 1);

    client.resolve(&0);

    assert_eq!(token.balance(&depositor), AMOUNT);
    assert_eq!(token.balance(&beneficiary), 0);
    assert_eq!(token.balance(&client.address), 0, "dust left in escrow");
    // Exactly one payout: the empty beneficiary share was skipped.
    assert_eq!(token.transfers(), 2);
    assert_eq!(client.state(), State::Resolved);
}

#[test]
fn resolve_at_ten_thousand_bps_pays_only_the_beneficiary_with_a_strict_token() {
    let env = Env::default();
    let (client, token, depositor, beneficiary) = disputed_with_strict_token(&env);
    assert_eq!(token.transfers(), 1);

    client.resolve(&10_000);

    assert_eq!(token.balance(&beneficiary), AMOUNT);
    assert_eq!(token.balance(&depositor), 0);
    assert_eq!(token.balance(&client.address), 0, "dust left in escrow");
    // Exactly one payout: the empty depositor share was skipped.
    assert_eq!(token.transfers(), 2);
    assert_eq!(client.state(), State::Resolved);
}

#[test]
fn resolve_conserves_the_escrowed_amount_at_every_split() {
    // The invariant: paid_to_beneficiary + paid_to_depositor == amount,
    // exactly, including at splits that divide badly.
    for bps in [0u32, 1, 3, 333, 1_234, 5_000, 6_667, 9_999, 10_000] {
        let f = Fixture::funded(true);
        let depositor_before = f.token.balance(&f.depositor);
        f.client.dispute(&f.depositor);
        f.client.resolve(&bps);

        let to_beneficiary = f.token.balance(&f.beneficiary);
        let to_depositor = f.token.balance(&f.depositor) - depositor_before;

        assert_eq!(
            to_beneficiary + to_depositor,
            AMOUNT,
            "{bps}bps split leaked value"
        );
        assert_eq!(f.escrow_balance(), 0, "{bps}bps split stranded funds");
        assert!(to_beneficiary >= 0 && to_depositor >= 0);
    }
}

#[test]
fn arbiter_fee_split_conserves_every_supported_fee() {
    for fee_bps in [0u32, 1, 333, 500, 999, 1_000] {
        let f = Fixture::funded(true);
        f.env.as_contract(&f.client.address, || {
            let mut escrow = crate::storage::load(&f.env).unwrap();
            escrow.arbiter_fee_bps = fee_bps;
            crate::storage::save(&f.env, &escrow);
        });
        let depositor_before = f.token.balance(&f.depositor);
        let arbiter_before = f.token.balance(&f.arbiter);
        f.client.dispute(&f.depositor);
        f.client.resolve(&3_333);

        let fee = f.token.balance(&f.arbiter) - arbiter_before;
        let beneficiary = f.token.balance(&f.beneficiary);
        let depositor_gain = f.token.balance(&f.depositor) - depositor_before;
        assert_eq!(
            fee + beneficiary + depositor_gain,
            AMOUNT,
            "{fee_bps}bps arbiter fee leaked value"
        );
        assert_eq!(fee, sororail_common::math::mul_bps(AMOUNT, fee_bps).unwrap());
        assert_eq!(f.escrow_balance(), 0);
    }
}

mod resolve_properties {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn resolve_conserves_amount_for_any_split(
            amount in 1i128..=1_000_000_000_000,
            bps in 0u32..=10_000,
        ) {
            let f = Fixture::funded_amount(amount);
            let depositor_before = f.token.balance(&f.depositor);
            f.client.dispute(&f.depositor);
            f.client.resolve(&bps);

            let paid_to_beneficiary = f.token.balance(&f.beneficiary);
            let paid_to_depositor = f
                .token
                .balance(&f.depositor)
                .checked_sub(depositor_before)
                .expect("depositor balance delta remains in range");
            let total_paid = paid_to_beneficiary
                .checked_add(paid_to_depositor)
                .expect("total paid remains in range");
            prop_assert_eq!(
                total_paid,
                amount,
                "amount {}, split {}bps",
                amount,
                bps
            );
            prop_assert_eq!(f.escrow_balance(), 0, "resolved escrow retained funds");
        }
    }
}

#[test]
fn init_with_beneficiary_as_contract_address_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);

    let depositor = Address::generate(&env);
    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_address = sac.address();
    StellarAssetClient::new(&env, &token_address).mint(&depositor, &(AMOUNT * 10));

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    // Using contract address as beneficiary should succeed (no-op transfer is harmless)
    let result = client.try_init(
        &depositor,
        &contract_id,
        &None,
        &token_address,
        &AMOUNT,
        &DEADLINE,
        &None,
        &None,
        &0,
    );
    assert!(
        result.is_ok(),
        "init with contract as beneficiary should succeed"
    );
}

#[test]
fn init_with_depositor_as_contract_address_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);

    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_address = sac.address();
    StellarAssetClient::new(&env, &token_address).mint(&depositor, &(AMOUNT * 10));

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    // Using contract address as depositor should succeed
    let result = client.try_init(
        &contract_id,
        &beneficiary,
        &None,
        &token_address,
        &AMOUNT,
        &DEADLINE,
        &None,
        &None,
        &0,
    );
    assert!(
        result.is_ok(),
        "init with contract as depositor should succeed"
    );
}

#[test]
fn init_rejects_multiple_invalid_args_in_order() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);

    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_address = sac.address();
    StellarAssetClient::new(&env, &token_address).mint(&depositor, &(AMOUNT * 10));

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    // Both AlreadyInitialized AND InvalidAmount: AlreadyInitialized comes first
    client.init(
        &depositor,
        &beneficiary,
        &None,
        &token_address,
        &AMOUNT,
        &DEADLINE,
        &None,
        &None,
        &0,
    );
    assert_eq!(
        client.try_init(
            &depositor,
            &beneficiary,
            &None,
            &token_address,
            &0,
            &DEADLINE,
            &None
        ),
        Err(Ok(Error::AlreadyInitialized))
    );

    // InvalidAmount AND InvalidTimeRange: InvalidAmount comes first
    let contract_id2 = env.register(EscrowContract, ());
    let client2 = EscrowContractClient::new(&env, &contract_id2);
    assert_eq!(
        client2.try_init(
            &depositor,
            &beneficiary,
            &None,
            &token_address,
            &0,
            &(START_TS - 1),
            &None
        ),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn resolve_rejects_a_second_call() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    f.client.resolve(&5_000);
    assert_eq!(
        f.client.try_resolve(&5_000),
        Err(Ok(Error::EscrowNotDisputed))
    );
}

#[test]
fn resolve_emits_resolved_event_with_correct_topics_and_data() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    f.client.resolve(&2_500);

    // `resolve` emits up to two token transfer events before its own
    // `Resolved`, so take the last event rather than assuming an index.
    let all = f.env.events().all();
    let events = all.events();
    let resolved_event = events.last().expect("no events emitted");
    let expected = Resolved {
        arbiter: f.arbiter.clone(),
        fee: 0,
        split_bps: 2_500,
        to_beneficiary: AMOUNT / 4,
        to_depositor: AMOUNT * 3 / 4,
    };
    assert_eq!(resolved_event, &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
#[should_panic]
fn resolve_requires_the_arbiters_authorization() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    // Clear auths so require_auth bites.
    f.env.set_auths(&[]);
    f.client.resolve(&5_000);
}

// ================================================================
// Issue #467: golden-file tests for the full escrow dispute token flow.
//
// These tests pin exact token balances after `resolve` at the five canonical
// bps checkpoints. Together they document the dispute resolution token flow
// and catch regressions in the split math.
//
// AMOUNT = 1_000_000 (defined in mod.rs)
// beneficiary = floor(AMOUNT * bps / 10_000)
// depositor   = AMOUNT - beneficiary  (no rounding leakage)
// ================================================================

mod golden_file {
    use super::*;

    fn resolve_at(bps: u32) -> (i128, i128, i128) {
        let f = Fixture::funded(true);
        let depositor_before = f.token.balance(&f.depositor);
        f.client.dispute(&f.depositor);
        f.client.resolve(&bps);
        let beneficiary = f.token.balance(&f.beneficiary);
        let depositor_gain = f.token.balance(&f.depositor) - depositor_before;
        (beneficiary, depositor_gain, f.escrow_balance())
    }

    #[test]
    fn golden_bps_0_depositor_receives_all() {
        let (beneficiary, depositor_gain, escrow) = resolve_at(0);
        assert_eq!(beneficiary, 0, "bps=0: beneficiary gets nothing");
        assert_eq!(depositor_gain, AMOUNT, "bps=0: depositor gets full amount");
        assert_eq!(escrow, 0, "escrow drained");
    }

    #[test]
    fn golden_bps_1_minimum_split() {
        // floor(1_000_000 * 1 / 10_000) = 100
        let (beneficiary, depositor_gain, escrow) = resolve_at(1);
        assert_eq!(beneficiary, 100);
        assert_eq!(depositor_gain, AMOUNT - 100);
        assert_eq!(beneficiary + depositor_gain, AMOUNT);
        assert_eq!(escrow, 0);
    }

    #[test]
    fn golden_bps_5000_even_split() {
        // floor(1_000_000 * 5_000 / 10_000) = 500_000
        let (beneficiary, depositor_gain, escrow) = resolve_at(5_000);
        assert_eq!(beneficiary, 500_000);
        assert_eq!(depositor_gain, 500_000);
        assert_eq!(beneficiary + depositor_gain, AMOUNT);
        assert_eq!(escrow, 0);
    }

    #[test]
    fn golden_bps_9999_maximum_before_full() {
        // floor(1_000_000 * 9_999 / 10_000) = 999_900
        let (beneficiary, depositor_gain, escrow) = resolve_at(9_999);
        assert_eq!(beneficiary, 999_900);
        assert_eq!(depositor_gain, AMOUNT - 999_900);
        assert_eq!(beneficiary + depositor_gain, AMOUNT);
        assert_eq!(escrow, 0);
    }

    #[test]
    fn golden_bps_10000_beneficiary_receives_all() {
        let (beneficiary, depositor_gain, escrow) = resolve_at(10_000);
        assert_eq!(
            beneficiary, AMOUNT,
            "bps=10000: beneficiary gets full amount"
        );
        assert_eq!(depositor_gain, 0, "bps=10000: depositor gets nothing");
        assert_eq!(escrow, 0);
    }

    #[test]
    fn golden_no_dust_remains_in_escrow() {
        for bps in [0u32, 1, 100, 500, 1_000, 5_000, 9_000, 9_999, 10_000] {
            let (beneficiary, depositor_gain, escrow) = resolve_at(bps);
            assert_eq!(
                beneficiary + depositor_gain,
                AMOUNT,
                "bps={bps}: split != AMOUNT"
            );
            assert_eq!(escrow, 0, "bps={bps}: escrow not drained");
        }
    }
}

mod proptest_resolve {
    use super::*;
    use proptest::prelude::*;
    use sororail_common::strategies;

    proptest! {
        #[test]
        fn resolve_split_sums_to_amount_for_any_bps(bps in strategies::arb_bps()) {
            let f = Fixture::funded(true);
            let depositor_before = f.token.balance(&f.depositor);
            f.client.dispute(&f.depositor);
            if f.client.try_resolve(&bps).is_ok() {
                let beneficiary = f.token.balance(&f.beneficiary);
                let depositor_gain = f.token.balance(&f.depositor) - depositor_before;
                prop_assert_eq!(beneficiary + depositor_gain, AMOUNT,
                    "split did not sum to AMOUNT at bps={}", bps);
                prop_assert_eq!(f.escrow_balance(), 0, "escrow not drained at bps={}", bps);
            }
        }
    }
}
