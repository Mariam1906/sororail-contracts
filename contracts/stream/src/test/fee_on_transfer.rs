// Smoke test for the shared fee-on-transfer mock token (#438). The mock
// skims a basis-point fee on each `transfer`, so a stream created against
// it ends up holding the NET deposited amount rather than the gross.

use sororail_common::testutils::{deploy_fee_on_transfer_token, FeeOnTransferTokenClient};

use crate::contract::{StreamContract, StreamContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

#[test]
fn stream_create_with_fee_on_transfer_token_deposits_the_net_amount() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    // 1% fee on every transfer.
    let (token_addr, _sink) = deploy_fee_on_transfer_token(&env, 100);
    let fot = FeeOnTransferTokenClient::new(&env, &token_addr);

    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    fot.mint_for_tests(&sender, &10_000_000);

    let stream = StreamContractClient::new(&env, &env.register(StreamContract, ()));
    let rate: i128 = 1_000;
    let start = 1_000_000u64;
    let stop = start + 1_000u64;
    let gross: i128 = rate * i128::from(stop - start);
    stream.create(
        &sender,
        &recipient,
        &token_addr,
        &rate,
        &start,
        &stop,
        &true,
    );

    // The mock skimmed 1% in-flight, so the contract holds 99%.
    assert_eq!(fot.balance(&stream.address), gross - gross / 100);
    assert_eq!(fot.balance(&sender), 10_000_000 - gross);
}
