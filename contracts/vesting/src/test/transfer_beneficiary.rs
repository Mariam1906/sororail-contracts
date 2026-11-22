use super::*;

use soroban_sdk::testutils::{Address as _, Events as _};

use crate::events::BeneficiaryTransferred;

// transfer_beneficiary

// -------------------------------------------------- transfer_beneficiary

/// Claims after a transfer pay the new beneficiary, and the old one gets
/// nothing further.
#[test]
fn claims_after_a_transfer_pay_the_new_beneficiary() {
    let f = Fixture::new(true);
    let new_beneficiary = Address::generate(&f.env);
    f.at(START + 300);
    f.client.transfer_beneficiary(&new_beneficiary);

    f.at(START + 600);
    assert_eq!(f.client.claim(), TOTAL * 6 / 10);
    assert_eq!(f.token.balance(&new_beneficiary), TOTAL * 6 / 10);
    assert_eq!(f.token.balance(&f.beneficiary), 0);
    f.assert_conserved();
}

/// Transferring changes only the beneficiary. `claimed`, the schedule and the
/// rest of the record stay as they were, and vesting carries on unaffected.
#[test]
fn transfer_changes_only_the_beneficiary_and_keeps_claimed() {
    let f = Fixture::new(true);
    f.at(START + 300);
    f.client.claim();
    let before = f.client.get();
    assert_eq!(before.claimed, TOTAL * 3 / 10);

    let new_beneficiary = Address::generate(&f.env);
    f.client.transfer_beneficiary(&new_beneficiary);

    let mut expected = before;
    expected.beneficiary = new_beneficiary;
    assert_eq!(f.client.get(), expected);
    assert_eq!(f.client.claimable(), 0);
    f.assert_conserved();
}

#[test]
fn transfer_emits_the_beneficiary_transferred_event() {
    let f = Fixture::new(true);
    let new_beneficiary = Address::generate(&f.env);
    f.client.transfer_beneficiary(&new_beneficiary);

    let all = f.env.events().all();
    let events = all.events();
    let last = events.last().expect("no events emitted");
    let expected = BeneficiaryTransferred {
        from: f.beneficiary.clone(),
        to: new_beneficiary,
    };
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
}

/// A revoked grant cannot be transferred. The vested portion stays with the
/// beneficiary who was already entitled to it, and they can still claim it.
#[test]
fn transfer_is_rejected_on_a_revoked_grant() {
    let f = Fixture::new(true);
    f.at(START + 300);
    f.client.revoke();

    let new_beneficiary = Address::generate(&f.env);
    assert_eq!(
        f.client.try_transfer_beneficiary(&new_beneficiary),
        Err(Ok(Error::VestingRevoked))
    );
    assert_eq!(f.client.get().beneficiary, f.beneficiary);

    f.at(START + 900);
    assert_eq!(f.client.claim(), TOTAL * 3 / 10);
    assert_eq!(f.token.balance(&f.beneficiary), TOTAL * 3 / 10);
    f.assert_conserved();
}

/// The new beneficiary must differ from the current beneficiary and from the
/// grantor. A rejected transfer changes nothing.
#[test]
fn transfer_rejects_the_current_beneficiary_and_the_grantor() {
    let f = Fixture::new(true);
    assert_eq!(
        f.client.try_transfer_beneficiary(&f.beneficiary),
        Err(Ok(Error::IdenticalParties))
    );
    assert_eq!(
        f.client.try_transfer_beneficiary(&f.grantor),
        Err(Ok(Error::IdenticalParties))
    );
    assert_eq!(f.client.get().beneficiary, f.beneficiary);
}
