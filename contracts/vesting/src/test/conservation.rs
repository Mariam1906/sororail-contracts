use super::*;

// conservation

// ------------------------------------------------------- conservation

#[test]
fn conservation_holds_across_claim_then_revoke_then_claim() {
    let f = Fixture::new(true);
    f.assert_conserved();

    f.at(START + CLIFF);
    f.client.claim();
    f.assert_conserved();

    f.at(START + 450);
    f.client.claim();
    f.assert_conserved();

    f.at(START + 700);
    f.client.revoke();
    f.assert_conserved();

    f.client.claim();
    f.assert_conserved();

    let g = f.client.get();
    assert_eq!(f.held(), 0);
    assert_eq!(g.claimed + g.returned, g.total);
    assert_conserved(
        &f.token,
        &[
            f.grantor.clone(),
            f.beneficiary.clone(),
            f.client.address.clone(),
        ],
        MINT,
    );
}

#[test]
fn a_schedule_that_divides_badly_still_conserves() {
    // 7 into 3 -- deliberately lossy proportions.
    let f = Fixture::with_schedule(true, 0, 3);
    f.at(START + 1);
    f.client.claim();
    f.assert_conserved();
    f.at(START + 2);
    f.client.claim();
    f.assert_conserved();
    f.at(START + 3);
    f.client.claim();
    f.assert_conserved();
    assert_eq!(f.token.balance(&f.beneficiary), TOTAL);
    assert_eq!(f.held(), 0);
}

#[test]
fn vesting_with_long_duration_and_large_total_probes_overflow_boundary() {
    // `mul_div` computes `total * elapsed` before dividing by `duration`, so
    // the overflow ceiling is on that product, not on `total` alone: at the
    // midpoint of a 10-year (≈315,576,000s) schedule, `elapsed` is
    // ≈157,788,000, and overflow needs `total > i128::MAX / elapsed`
    // (≈1.08e30) -- many orders of magnitude above a realistic token supply.
    const TEN_YEARS_SECONDS: u64 = 315_576_000;
    const SAFE_TOTAL: i128 = 29_000_000;
    const OVERFLOW_TOTAL: i128 = i128::MAX / 2;

    let env = Env::default();

    // Test that a safe total with long duration vests successfully.
    let g_safe = Grant {
        grantor: Address::generate(&env),
        beneficiary: Address::generate(&env),
        token: Address::generate(&env),
        total: SAFE_TOTAL,
        start: START,
        cliff: 0,
        duration: TEN_YEARS_SECONDS,
        revocable: true,
        claimed: 0,
        returned: 0,
        revoked_at: None,
    };
    // At the midpoint (half elapsed), vesting should succeed.
    let half_elapsed = TEN_YEARS_SECONDS / 2;
    let vested_halfway = g_safe.vested_amount(START + half_elapsed);
    assert!(
        vested_halfway.is_ok(),
        "safe total should not overflow at midpoint"
    );
    // At full duration, should have vested the full amount.
    assert_eq!(
        g_safe.vested_amount(START + TEN_YEARS_SECONDS),
        Ok(SAFE_TOTAL)
    );

    // Test that an unsafe total with long duration overflows.
    let g_unsafe = Grant {
        grantor: Address::generate(&env),
        beneficiary: Address::generate(&env),
        token: Address::generate(&env),
        total: OVERFLOW_TOTAL,
        start: START,
        cliff: 0,
        duration: TEN_YEARS_SECONDS,
        revocable: true,
        claimed: 0,
        returned: 0,
        revoked_at: None,
    };
    // At the midpoint (half elapsed), the multiplication should overflow.
    assert_eq!(
        g_unsafe.vested_amount(START + half_elapsed),
        Err(Error::Overflow),
        "unsafe total should overflow at midpoint of long duration"
    );
}
