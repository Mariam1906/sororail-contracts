use super::*;

// create

// ------------------------------------------------------------------ create

// issue #477: retroactive `start`

// ------------------------------------------- issue #477: retroactive `start`
//
// `create` does not compare `start` against the ledger clock: a schedule that
// began in the past is accepted, and whatever has already vested by the
// creation instant is claimable immediately. These tests pin that behaviour so
// any future decision to reject (or clamp) a retroactive `start` is a
// deliberate, visible change rather than an accident.

/// Ledger time at which the retroactive grants below are created.
const NOW: u64 = START + 500;

#[test]
fn create_accepts_a_start_in_the_past() {
    let f = Fixture::created_at(NOW, START, CLIFF, DURATION, true);
    let g = f.client.get();
    assert!(g.start < f.env.ledger().timestamp());
    assert_eq!(g.start, START);
    // Fully funded up front, exactly as for a future-dated grant.
    assert_eq!(f.held(), TOTAL);
    assert_eq!(f.token.balance(&f.grantor), MINT - TOTAL);
    f.assert_conserved();
}

#[test]
fn create_with_a_past_start_makes_the_vested_portion_claimable_immediately() {
    // Half the schedule (500 of 1_000 seconds) elapsed before `create`, and
    // the 100s cliff is already behind us, so half the total is vested.
    let f = Fixture::created_at(NOW, START, CLIFF, DURATION, true);
    let expected = TOTAL / 2;
    assert_eq!(f.client.vested_amount(&NOW), expected);
    assert_eq!(f.client.claimable(), expected);

    // Without advancing the ledger, the beneficiary can take it right away.
    assert_eq!(f.client.claim(), expected);
    assert_eq!(f.token.balance(&f.beneficiary), expected);
    assert_eq!(f.client.claimable(), 0);
    f.assert_conserved();
}

#[test]
fn create_with_a_past_start_before_the_cliff_has_nothing_claimable() {
    // Started in the past, but the cliff (start + 600) is still ahead.
    let f = Fixture::created_at(NOW, START, 600, DURATION, true);
    assert_eq!(f.client.claimable(), 0);
    assert_eq!(f.client.try_claim(), Err(Ok(Error::VestingCliffNotReached)));
    f.assert_conserved();
}

#[test]
fn create_with_a_schedule_that_ended_in_the_past_is_fully_claimable() {
    // The whole schedule (start..start + duration) lies before `create`.
    let now = START + DURATION + 1;
    let f = Fixture::created_at(now, START, CLIFF, DURATION, true);
    assert_eq!(f.client.claimable(), TOTAL);
    assert_eq!(f.client.claim(), TOTAL);
    assert_eq!(f.token.balance(&f.beneficiary), TOTAL);
    assert_eq!(f.client.remaining(), 0);
    f.assert_conserved();
}

#[test]
fn revoking_a_retroactive_grant_returns_only_the_unvested_portion() {
    let f = Fixture::created_at(NOW, START, CLIFF, DURATION, true);
    assert_eq!(f.client.revoke(), TOTAL / 2);
    assert_eq!(f.client.claimable(), TOTAL / 2);
    f.assert_conserved();
}
