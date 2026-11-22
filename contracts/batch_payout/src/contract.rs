use soroban_sdk::{contract, contractimpl, token, Address, Env, Vec};
use sororail_common::{math, Error};

use crate::{
    events,
    types::{MultiTokenPayment, Payment, Receipt, MAX_RECIPIENTS},
};

#[contract]
pub struct BatchPayoutContract;

#[contractimpl]
impl BatchPayoutContract {
    /// Pays each recipient their own amount, in one transaction.
    ///
    /// All-or-nothing: if any transfer fails, the host reverts the entire
    /// invocation including the transfers already made. Nobody is paid unless
    /// everybody is.
    ///
    /// The whole batch is validated and totalled **before** any money moves,
    /// so an invalid line is rejected without a partial payout ever being
    /// attempted.
    ///
    /// Paying the funder's own address is intentionally permitted. Such a
    /// line counts toward the receipt total but transfers no net funds;
    /// only transfers to this contract's own address are rejected.
    ///
    /// When `only_eoa` is `true`, every recipient must be a Stellar account
    /// address (not a smart contract). `BatchRecipientIsContract` is returned
    /// for the first recipient that fails that check.
    pub fn execute(
        env: Env,
        funder: Address,
        token: Address,
        recipients: Vec<Payment>,
        only_eoa: bool,
    ) -> Result<Receipt, Error> {
        let count = Self::check_size(&recipients)?;
        let contract_address = env.current_contract_address();

        let mut total: i128 = 0;
        for payment in recipients.iter() {
            if payment.to == contract_address {
                return Err(Error::IdenticalParties);
            }
            if only_eoa && is_contract_addr(&env, &payment.to) {
                return Err(Error::BatchRecipientIsContract);
            }
            math::require_positive(payment.amount)?;
            total = math::add(total, payment.amount)?;
        }

        funder.require_auth();

        let client = token::TokenClient::new(&env, &token);
        for payment in recipients.iter() {
            client.transfer(&funder, &payment.to, &payment.amount);
        }

        let receipt = Receipt { count, total };
        events::Executed {
            funder,
            token,
            count,
            total,
        }
        .publish(&env);
        Ok(receipt)
    }

    /// Pays every recipient the same amount, in one transaction.
    ///
    /// The common payroll shape, and cheaper to submit than `execute`
    /// because the amount crosses the wire once rather than once per line.
    /// The funder may appear among recipients, including more than once;
    /// those self-payments count in the receipt but have no net balance effect.
    ///
    /// When `only_eoa` is `true`, every recipient must be a Stellar account
    /// address (not a smart contract). `BatchRecipientIsContract` is returned
    /// for the first recipient that fails that check.
    pub fn execute_equal(
        env: Env,
        funder: Address,
        token: Address,
        recipients: Vec<Address>,
        amount_each: i128,
        only_eoa: bool,
    ) -> Result<Receipt, Error> {
        math::require_positive(amount_each)?;

        let count = Self::check_size_of(recipients.len())?;
        let contract_address = env.current_contract_address();

        for to in recipients.iter() {
            if to == contract_address {
                return Err(Error::IdenticalParties);
            }
            if only_eoa && is_contract_addr(&env, &to) {
                return Err(Error::BatchRecipientIsContract);
            }
        }

        let total = math::mul(amount_each, i128::from(count))?;

        funder.require_auth();

        let client = token::TokenClient::new(&env, &token);
        for to in recipients.iter() {
            client.transfer(&funder, &to, &amount_each);
        }

        let receipt = Receipt { count, total };
        events::Executed {
            funder,
            token,
            count,
            total,
        }
        .publish(&env);
        Ok(receipt)
    }

    /// Pays each recipient with its own token and amount, in one transaction.
    ///
    /// A multi-token batch: each line specifies which token to draw from,
    /// the recipient, and the amount. The funder must hold sufficient balance
    /// of every token used. All transfers are all-or-nothing: a failure on any
    /// line reverts the entire invocation.
    ///
    /// All validation happens before any money moves, so an invalid line is
    /// rejected without a partial payout ever being attempted.
    ///
    /// Because the payments span different tokens, the receipt `total` is the
    /// arithmetic sum of all raw amounts regardless of token denominations; it
    /// is provided for reference only and should not be compared across tokens.
    ///
    /// When `only_eoa` is `true`, every recipient must be a Stellar account
    /// address. `BatchRecipientIsContract` is returned for the first recipient
    /// that fails that check.
    pub fn execute_many_tokens(
        env: Env,
        funder: Address,
        payments: Vec<MultiTokenPayment>,
        only_eoa: bool,
    ) -> Result<Receipt, Error> {
        let count = Self::check_size_multi(&payments)?;
        let contract_address = env.current_contract_address();

        let mut total: i128 = 0;
        for payment in payments.iter() {
            if payment.to == contract_address {
                return Err(Error::IdenticalParties);
            }
            if only_eoa && is_contract_addr(&env, &payment.to) {
                return Err(Error::BatchRecipientIsContract);
            }
            math::require_positive(payment.amount)?;
            total = math::add(total, payment.amount)?;
        }

        funder.require_auth();

        for payment in payments.iter() {
            token::TokenClient::new(&env, &payment.token).transfer(
                &funder,
                &payment.to,
                &payment.amount,
            );
        }

        let receipt = Receipt { count, total };
        events::ExecutedMany { funder, count }.publish(&env);
        Ok(receipt)
    }

    /// The maximum recipients permitted in one batch.
    ///
    /// Exposed so a client can split an oversized payroll before submitting
    /// rather than discovering the cap by having a transaction rejected.
    pub fn max_recipients(_env: Env) -> u32 {
        MAX_RECIPIENTS
    }

    /// Totals a batch without moving anything, for a pre-signature preview.
    ///
    /// Runs exactly the validation `execute` does, so a preview that
    /// succeeds means the batch itself will not be rejected for size, an
    /// invalid amount, an overflowing total, or (when `only_eoa` is `true`)
    /// a contract-address recipient.
    pub fn preview(env: Env, recipients: Vec<Payment>, only_eoa: bool) -> Result<Receipt, Error> {
        let count = Self::check_size(&recipients)?;
        let contract_address = env.current_contract_address();
        let mut total: i128 = 0;
        for payment in recipients.iter() {
            if payment.to == contract_address {
                return Err(Error::IdenticalParties);
            }
            if only_eoa && is_contract_addr(&env, &payment.to) {
                return Err(Error::BatchRecipientIsContract);
            }
            math::require_positive(payment.amount)?;
            total = math::add(total, payment.amount)?;
        }
        Ok(Receipt { count, total })
    }
}

// Helpers. Outside `#[contractimpl]` so they are not contract entry points.
impl BatchPayoutContract {
    fn check_size(recipients: &Vec<Payment>) -> Result<u32, Error> {
        Self::check_size_of(recipients.len())
    }

    fn check_size_multi(payments: &Vec<MultiTokenPayment>) -> Result<u32, Error> {
        Self::check_size_of(payments.len())
    }

    fn check_size_of(len: u32) -> Result<u32, Error> {
        if len == 0 {
            return Err(Error::BatchEmpty);
        }
        if len > MAX_RECIPIENTS {
            return Err(Error::BatchTooLarge);
        }
        Ok(len)
    }
}

/// Returns `true` when `addr` is a smart-contract address.
///
/// `ScAddress` is XDR-encoded as a 4-byte big-endian discriminant followed by
/// the address payload. Discriminant 0 = Stellar account, 1 = contract.
fn is_contract_addr(env: &Env, addr: &Address) -> bool {
    let xdr = env.to_xdr(addr.clone());
    xdr.get(3).map_or(false, |b| b == 1)
}
