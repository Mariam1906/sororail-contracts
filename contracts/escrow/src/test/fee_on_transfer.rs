//! Smoke test using the shared fee-on-transfer mock token (#438).
//!
//! The mock lives in `sororail-common::testutils::deploy_fee_on_transfer_token`
//! and skims a configurable basis-point fee on every transfer. Here we show
//! that an escrow funded with a FOT token lands the NET amount in the
//! contract, so a downstream test can reason about how much actually
//! settles when a hostile token is on the other side of `transfer`.

use soroban_sdk::{testutils::Address as _, Address, Env};
use sororail_common::testutils::{deploy_fee_on_transfer_token, FeeOnTransferTokenClient};

use crate::contract::{EscrowContract, EscrowContractClient};

#[test]
fn escrow_funded_with_fee_on_transfer_token_sees_the_net_amount() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    // 2% fee on every transfer.
    let (token_addr, _sink) = deploy_fee_on_transfer_token(&env, 200);
    let fot = FeeOnTransferTokenClient::new(&env, &token_addr);

    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    fot.mint_for_tests(&depositor, &1_000_000);

    let escrow = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
    let amount: i128 = 1_000_000;
    escrow.init(
        &depositor,
        &beneficiary,
        &None,
        &token_addr,
        &amount,
        &(1_000_000 + 10_000),
        &None,
    );
    escrow.fund();

    // The escrow asked for `amount`; the FOT token skimmed 2% in-flight, so
    // the contract actually holds 98%. The depositor paid the gross.
    assert_eq!(fot.balance(&escrow.address), 980_000);
    assert_eq!(fot.balance(&depositor), 0);
}
