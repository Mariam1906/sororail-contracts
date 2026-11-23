// extend_deadline

use super::*;
use soroban_sdk::{
    testutils::{MockAuth, MockAuthInvoke},
    IntoVal,
};

// ------------------------------------------------------- extend_deadline

const NEW_DEADLINE: u64 = DEADLINE + 5_000;

#[test]
fn extend_deadline_moves_the_deadline_later() {
    let f = Fixture::funded(true);
    f.client.extend_deadline(&NEW_DEADLINE);
    assert_eq!(f.client.get().deadline, NEW_DEADLINE);
    assert_eq!(f.client.state(), State::Funded);
    // Extending moves no funds.
    assert_eq!(f.escrow_balance(), AMOUNT);
}

#[test]
fn extend_deadline_requires_both_parties_to_sign() {
    let f = Fixture::funded(true);
    let args: soroban_sdk::Vec<soroban_sdk::Val> =
        soroban_sdk::vec![&f.env, NEW_DEADLINE.into_val(&f.env)];

    // Only the depositor signs.
    f.env.mock_auths(&[MockAuth {
        address: &f.depositor,
        invoke: &MockAuthInvoke {
            contract: &f.client.address,
            fn_name: "extend_deadline",
            args: args.clone(),
            sub_invokes: &[],
        },
    }]);
    assert!(f.client.try_extend_deadline(&NEW_DEADLINE).is_err());
    assert_eq!(f.client.get().deadline, DEADLINE);

    // Only the beneficiary signs.
    f.env.mock_auths(&[MockAuth {
        address: &f.beneficiary,
        invoke: &MockAuthInvoke {
            contract: &f.client.address,
            fn_name: "extend_deadline",
            args: args.clone(),
            sub_invokes: &[],
        },
    }]);
    assert!(f.client.try_extend_deadline(&NEW_DEADLINE).is_err());
    assert_eq!(f.client.get().deadline, DEADLINE);

    // Both sign: the same invocation succeeds, so the mocks above failed for
    // the reason under test and not because of a mismatched invocation.
    f.env.mock_auths(&[
        MockAuth {
            address: &f.depositor,
            invoke: &MockAuthInvoke {
                contract: &f.client.address,
                fn_name: "extend_deadline",
                args: args.clone(),
                sub_invokes: &[],
            },
        },
        MockAuth {
            address: &f.beneficiary,
            invoke: &MockAuthInvoke {
                contract: &f.client.address,
                fn_name: "extend_deadline",
                args: args.clone(),
                sub_invokes: &[],
            },
        },
    ]);
    f.client.extend_deadline(&NEW_DEADLINE);
    assert_eq!(f.client.get().deadline, NEW_DEADLINE);
}

#[test]
fn extend_deadline_rejects_a_deadline_that_is_not_strictly_later() {
    let f = Fixture::funded(true);
    for bad in [DEADLINE, DEADLINE - 1, 0] {
        assert_eq!(
            f.client.try_extend_deadline(&bad),
            Err(Ok(Error::InvalidTimeRange)),
            "deadline {bad} was accepted"
        );
    }
    assert_eq!(f.client.get().deadline, DEADLINE);
}

#[test]
fn extend_deadline_succeeds_one_second_before_the_deadline() {
    let f = Fixture::funded(true);
    f.env.ledger().with_mut(|l| l.timestamp = DEADLINE - 1);
    f.client.extend_deadline(&NEW_DEADLINE);
    assert_eq!(f.client.get().deadline, NEW_DEADLINE);
}

#[test]
fn extend_deadline_is_rejected_at_and_after_the_deadline() {
    // The refund right begins at the deadline, so it is already fixed there.
    for now in [DEADLINE, DEADLINE + 1] {
        let f = Fixture::funded(true);
        f.env.ledger().with_mut(|l| l.timestamp = now);
        assert_eq!(
            f.client.try_extend_deadline(&NEW_DEADLINE),
            Err(Ok(Error::DeadlinePassed)),
            "extended at ledger time {now}"
        );
        assert_eq!(f.client.get().deadline, DEADLINE);
    }
}

#[test]
fn extend_deadline_requires_a_funded_escrow() {
    // Created: nothing is held yet.
    let created = Fixture::new(true);
    assert_eq!(
        created.client.try_extend_deadline(&NEW_DEADLINE),
        Err(Ok(Error::EscrowNotFunded))
    );

    // Disputed: the deadline is frozen while the arbiter decides.
    let disputed = Fixture::funded(true);
    disputed.client.dispute(&disputed.depositor);
    assert_eq!(
        disputed.client.try_extend_deadline(&NEW_DEADLINE),
        Err(Ok(Error::EscrowNotFunded))
    );

    // Terminal: a released escrow is closed.
    let released = Fixture::funded(true);
    released.client.release(&released.depositor);
    assert_eq!(
        released.client.try_extend_deadline(&NEW_DEADLINE),
        Err(Ok(Error::EscrowClosed))
    );
}

#[test]
fn extend_deadline_emits_event_with_old_and_new_deadline() {
    let f = Fixture::funded(true);
    f.client.extend_deadline(&NEW_DEADLINE);

    let all = f.env.events().all();
    let events = all.events();
    let last = events.last().expect("no events emitted");
    let expected = DeadlineExtended {
        depositor: f.depositor.clone(),
        beneficiary: f.beneficiary.clone(),
        old_deadline: DEADLINE,
        new_deadline: NEW_DEADLINE,
    };
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
fn extended_deadline_governs_the_depositor_refund() {
    let f = Fixture::funded(true);
    f.client.extend_deadline(&NEW_DEADLINE);

    // The original deadline has passed, but the agreed one has not.
    f.env.ledger().with_mut(|l| l.timestamp = DEADLINE + 1);
    assert_eq!(
        f.client.try_refund(&f.depositor),
        Err(Ok(Error::DeadlineNotReached))
    );

    // The refund right opens at the new deadline.
    f.env.ledger().with_mut(|l| l.timestamp = NEW_DEADLINE);
    f.client.refund(&f.depositor);
    assert_eq!(f.client.state(), State::Refunded);
    assert_eq!(f.escrow_balance(), 0);
}
