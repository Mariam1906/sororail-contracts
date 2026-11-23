// time_to_deadline

use super::*;

// ------------------------------------------------------ time_to_deadline

#[test]
fn time_to_deadline_counts_down_on_the_ledger_clock() {
    let f = Fixture::new(false);
    assert_eq!(f.client.time_to_deadline(), DEADLINE - START_TS);

    f.env.ledger().with_mut(|l| l.timestamp = DEADLINE - 1);
    assert_eq!(f.client.time_to_deadline(), 1);
}

#[test]
fn time_to_deadline_is_zero_at_and_after_the_deadline() {
    let f = Fixture::funded(true);
    f.env.ledger().with_mut(|l| l.timestamp = DEADLINE);
    assert_eq!(f.client.time_to_deadline(), 0);

    f.advance_past_deadline();
    assert_eq!(f.client.time_to_deadline(), 0);
}

#[test]
fn time_to_deadline_follows_an_extension() {
    let f = Fixture::funded(true);
    f.client.extend_deadline(&(DEADLINE + 5_000));
    assert_eq!(f.client.time_to_deadline(), DEADLINE + 5_000 - START_TS);
}

#[test]
fn time_to_deadline_requires_init() {
    let env = TestEnv::new().env().clone();
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
    assert_eq!(
        client.try_time_to_deadline(),
        Err(Ok(Error::NotInitialized))
    );
}
