use super::*;

// revoke

// ------------------------------------------------------------------ revoke

#[test]
fn revoke_returns_only_the_unvested_portion() {
    let f = Fixture::new(true);
    let grantor_before = f.token.balance(&f.grantor);
    f.at(START + 300);
    assert_eq!(f.client.revoke(), TOTAL * 7 / 10);

    assert_eq!(f.token.balance(&f.grantor), grantor_before + TOTAL * 7 / 10);
    // The vested 30% stays in the contract for the beneficiary.
    assert_eq!(f.held(), TOTAL * 3 / 10);
    f.assert_conserved();
}

#[test]
fn the_vested_portion_stays_claimable_after_revocation() {
    let f = Fixture::new(true);
    f.at(START + 300);
    f.client.revoke();
    f.at(START + 900);
    assert_eq!(f.client.claim(), TOTAL * 3 / 10);
    assert_eq!(f.token.balance(&f.beneficiary), TOTAL * 3 / 10);
    assert_eq!(f.held(), 0);
    f.assert_conserved();
}

#[test]
fn revoke_accounts_for_what_was_already_claimed() {
    let f = Fixture::new(true);
    f.at(START + 200);
    f.client.claim();
    f.at(START + 600);
    f.client.revoke();

    // 60% vested overall, 20% already taken, 40% still claimable, 40% back.
    assert_eq!(f.client.claimable(), TOTAL * 4 / 10);
    f.assert_conserved();
    f.client.claim();
    assert_eq!(f.token.balance(&f.beneficiary), TOTAL * 6 / 10);
    assert_eq!(f.held(), 0);
    f.assert_conserved();
}

#[test]
fn revoke_before_the_cliff_returns_everything() {
    let f = Fixture::new(true);
    let grantor_before = f.token.balance(&f.grantor);
    assert_eq!(f.client.revoke(), TOTAL);
    assert_eq!(f.token.balance(&f.grantor), grantor_before + TOTAL);
    assert_eq!(f.held(), 0);
    // Nothing ever vested, so there is nothing to claim.
    assert_eq!(f.client.try_claim(), Err(Ok(Error::VestingCliffNotReached)));
    f.assert_conserved();
}

#[test]
fn revoke_before_the_cliff_then_advance_time_and_claim() {
    let f = Fixture::new(true);
    // Revoke before the cliff.
    f.client.revoke();
    // Advance time past the cliff -- but revoked_at freezes effective time.
    f.at(START + CLIFF + 1_000);
    // After revocation before the cliff, claim still reports CliffNotReached
    // because effective time is frozen at revoked_at (before the cliff).
    // Nothing was ever vested, so nothing is claimable.
    assert_eq!(f.client.try_claim(), Err(Ok(Error::VestingCliffNotReached)));
    f.assert_conserved();
}

#[test]
fn revoke_after_the_end_returns_nothing() {
    let f = Fixture::new(true);
    f.at(START + DURATION + 500);
    assert_eq!(f.client.revoke(), 0);
    assert_eq!(f.client.claimable(), TOTAL);
    f.assert_conserved();
}

#[test]
fn revoke_is_rejected_on_a_non_revocable_grant() {
    let f = Fixture::new(false);
    f.at(START + 300);
    assert_eq!(f.client.try_revoke(), Err(Ok(Error::VestingNotRevocable)));
    assert_eq!(f.held(), TOTAL);
}

#[test]
fn revoke_rejects_a_second_call() {
    let f = Fixture::new(true);
    f.at(START + 300);
    f.client.revoke();
    assert_eq!(f.client.try_revoke(), Err(Ok(Error::VestingRevoked)));
}

/// Issue #221: a second revoke must return `VestingRevoked` regardless of
/// how much time passes between the two calls.
#[test]
fn revoke_twice_is_rejected_with_already_revoked_error() {
    let f = Fixture::new(true);
    f.at(START + 300);
    f.client.revoke();
    // Advance time past the end of the schedule and try again.
    f.at(START + DURATION + 1_000);
    assert_eq!(
        f.client.try_revoke(),
        Err(Ok(Error::VestingRevoked)),
        "second revoke must return VestingRevoked"
    );
}

/// Issue #220: after a revoke the beneficiary can only claim the vested
/// portion; a claim that would exceed the vested amount errors and no
/// tokens are transferred.
#[test]
fn claim_after_revoke_only_pays_the_vested_portion_and_no_more() {
    let f = Fixture::new(true);
    // Revoke at 30 % through the schedule — 300 000 tokens are vested,
    // 700 000 are returned to the grantor.
    f.at(START + 300);
    let grantor_before = f.token.balance(&f.grantor);
    f.client.revoke();
    assert_eq!(f.token.balance(&f.grantor), grantor_before + TOTAL * 7 / 10);

    // Advance time well past the end — the frozen vested amount is still
    // 300 000; no additional tokens are accrued after revocation.
    f.at(START + DURATION + 10_000);

    // Claim the vested portion — this must succeed.
    assert_eq!(f.client.claim(), TOTAL * 3 / 10);
    assert_eq!(f.token.balance(&f.beneficiary), TOTAL * 3 / 10);
    f.assert_conserved();

    // A second claim must fail because nothing is left to claim.
    assert_eq!(
        f.client.try_claim(),
        Err(Ok(Error::VestingNothingToClaim)),
        "claim after revoke exhausted must error"
    );
    // Beneficiary balance is unchanged.
    assert_eq!(f.token.balance(&f.beneficiary), TOTAL * 3 / 10);
    f.assert_conserved();
}

#[test]
#[should_panic]
fn revoke_requires_the_grantors_authorization() {
    let f = Fixture::new(true);
    f.at(START + 300);
    f.env.set_auths(&[]);
    f.client.revoke();
}

#[test]
fn revoke_emits_the_revoked_event() {
    let f = Fixture::new(true);
    f.at(START + 300);
    f.client.revoke();

    // At t = START + 300 with DURATION = 1_000 and TOTAL = 1_000_000:
    // vested = TOTAL * 300 / 1000 = 300_000 (still claimable)
    // returned = TOTAL * 700 / 1000 = 700_000
    let expected = Revoked {
        grantor: f.grantor.clone(),
        returned_to_grantor: TOTAL * 7 / 10,
        still_claimable: TOTAL * 3 / 10,
        revoked_at: START + 300,
    };
    // filter_by_contract returns ContractEvents; .events() gives &[ContractEvent].
    // The contract emits `created` at setup and `revoked` here — take the last one.
    let all = f.env.events().all().filter_by_contract(&f.client.address);
    let events = all.events();
    let last = events.last().expect("no events emitted");
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
}

/// Regression test for the `still_claimable: math::sub(vested, claimed)?`
/// dead-calculation concern: claiming *exactly* the vested amount and then
/// revoking in the same ledger leaves `claimed == vested`, so the event's
/// subtraction is exactly zero. `revoke` must succeed rather than abort with
/// `Underflow`, and there is nothing left claimable afterwards.
#[test]
fn revoke_succeeds_immediately_after_claiming_all_vested_tokens() {
    let f = Fixture::new(true);
    // Partial vest: at START + 300 with CLIFF = 100 and DURATION = 1_000,
    // vested = TOTAL * 300 / 1_000 = 300_000.
    f.at(START + 300);

    let claimed = f.client.claim();
    let vested_now = f.client.vested_amount(&(START + 300));
    assert_eq!(
        claimed, vested_now,
        "claim must take the full vested amount"
    );
    assert_eq!(f.client.get().claimed, vested_now);

    // Revoke at the very same timestamp: claimed == vested, so the event's
    // `still_claimable` is exactly zero and `revoke` must not underflow.
    let returned = f.client.revoke();
    assert_eq!(
        returned,
        TOTAL - vested_now,
        "only the unvested remainder is returned"
    );

    let events = f.env.events().all().filter_by_contract(&f.client.address);
    let last = events.events().last().expect("no events emitted");
    let expected = Revoked {
        grantor: f.grantor.clone(),
        returned_to_grantor: TOTAL - vested_now,
        still_claimable: 0,
        revoked_at: START + 300,
    };
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));

    // Nothing is left to claim, and conservation still holds.
    assert_eq!(f.client.try_claim(), Err(Ok(Error::VestingNothingToClaim)));
    f.assert_conserved();
}
