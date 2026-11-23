//! Smoke test using the shared fee-on-transfer mock token (#438).
//!
//! The mock lives in `sororail-common::testutils::deploy_fee_on_transfer_token`
//! and skims a configurable basis-point fee on every transfer. Here we show
//! that funding is rejected because the net transfer is not the agreed amount.

use soroban_sdk::{testutils::Address as _, Address, Env};
use sororail_common::testutils::{deploy_fee_on_transfer_token, FeeOnTransferTokenClient};

use crate::contract::{EscrowContract, EscrowContractClient};

#[test]
fn escrow_rejects_fee_on_transfer_funding() {
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
        &None,
        &0,
    );
    assert_eq!(
        escrow.try_fund(),
        Err(Ok(sororail_common::Error::EscrowBalanceMismatch))
    );
    assert_eq!(fot.balance(&escrow.address), 0);
    assert_eq!(escrow.state(), crate::types::State::Created);
}
