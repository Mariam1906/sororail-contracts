use super::*;

// the schedule (pure, no ledger)

// ----------------------------------------- the schedule (pure, no ledger)

#[test]
fn nothing_vests_before_the_cliff() {
    let env = Env::default();
    let g = bare(&env, CLIFF, DURATION);
    assert_eq!(g.vested_amount(0), Ok(0));
    assert_eq!(g.vested_amount(START), Ok(0));
    assert_eq!(g.vested_amount(START + CLIFF - 1), Ok(0));
}

#[test]
fn the_cliff_vests_the_elapsed_proportion_in_one_step() {
    let env = Env::default();
    let g = bare(&env, CLIFF, DURATION);
    // At the cliff, 100/1000 of the schedule has elapsed.
    assert_eq!(g.vested_amount(START + CLIFF), Ok(TOTAL / 10));
}

/// Issue #222: the cliff timestamp is **inclusive** — `vested_amount` at
/// exactly `start + cliff` must return a non-zero amount equal to the
/// elapsed proportion, and the second immediately before must return zero.
/// This pins the boundary documented in `docs/vesting-safe-range.md` and
/// in `Grant::vested_amount`: "Nothing vests before `start + cliff`; at
/// that moment the elapsed proportion vests in one step."
#[test]
fn vested_amount_at_exact_cliff_is_inclusive_and_nonzero() {
    let env = Env::default();
    let g = bare(&env, CLIFF, DURATION);
    let cliff_at = g.cliff_at(); // == START + CLIFF

    // One second before the cliff: nothing vested.
    assert_eq!(
        g.vested_amount(cliff_at - 1),
        Ok(0),
        "one second before cliff must vest nothing"
    );
    // Exactly at the cliff: the elapsed proportion vests (cliff is inclusive).
    // elapsed = CLIFF (100), duration = DURATION (1000) → 100/1000 of TOTAL.
    let expected = TOTAL * i128::from(CLIFF) / i128::from(DURATION);
    assert_eq!(
        g.vested_amount(cliff_at),
        Ok(expected),
        "exact cliff timestamp must be inclusive and vest the elapsed proportion"
    );
    // One second after the cliff: slightly more has vested (linear increase).
    assert!(
        g.vested_amount(cliff_at + 1).unwrap() > expected,
        "one second after cliff must vest more than at the cliff"
    );
}

#[test]
fn vesting_is_linear_after_the_cliff() {
    let env = Env::default();
    let g = bare(&env, CLIFF, DURATION);
    assert_eq!(g.vested_amount(START + 250), Ok(TOTAL / 4));
    assert_eq!(g.vested_amount(START + 500), Ok(TOTAL / 2));
    assert_eq!(g.vested_amount(START + 750), Ok(TOTAL * 3 / 4));
}

#[test]
fn fully_vested_at_start_plus_duration() {
    let env = Env::default();
    let g = bare(&env, CLIFF, DURATION);
    assert_eq!(g.vested_amount(START + DURATION), Ok(TOTAL));
    assert_eq!(g.vested_amount(START + DURATION + 1), Ok(TOTAL));
    assert_eq!(g.vested_amount(u64::MAX), Ok(TOTAL));
}

#[test]
fn a_cliff_equal_to_duration_is_all_or_nothing() {
    let env = Env::default();
    let g = bare(&env, DURATION, DURATION);
    assert_eq!(g.vested_amount(START + DURATION - 1), Ok(0));
    assert_eq!(g.vested_amount(START + DURATION), Ok(TOTAL));
}

#[test]
fn a_zero_cliff_vests_linearly_from_the_start() {
    let env = Env::default();
    let g = bare(&env, 0, DURATION);
    assert_eq!(g.vested_amount(START), Ok(0));
    assert_eq!(g.vested_amount(START + 1), Ok(TOTAL / i128::from(DURATION)));
    assert_eq!(g.vested_amount(START + 500), Ok(TOTAL / 2));
}

#[test]
fn a_one_second_schedule_vests_in_one_tick() {
    let env = Env::default();
    let g = bare(&env, 0, 1);
    assert_eq!(g.vested_amount(START), Ok(0));
    assert_eq!(g.vested_amount(START + 1), Ok(TOTAL));
}

#[test]
fn revocation_freezes_the_schedule() {
    let env = Env::default();
    let mut g = bare(&env, CLIFF, DURATION);
    g.revoked_at = Some(START + 300);
    assert_eq!(g.vested_amount(START + 300), Ok(TOTAL * 3 / 10));
    assert_eq!(g.vested_amount(START + 900), Ok(TOTAL * 3 / 10));
    assert_eq!(g.vested_amount(u64::MAX), Ok(TOTAL * 3 / 10));
}

#[test]
fn vesting_reports_overflow_rather_than_wrapping() {
    let env = Env::default();
    let mut g = bare(&env, 0, DURATION);
    g.total = i128::MAX;
    assert_eq!(g.vested_amount(START + 500), Err(Error::Overflow));
}

#[test]
fn vested_amount_never_underflows_with_a_start_near_u64_max() {
    // `elapsed = effective - self.start` is a u64 subtraction before the
    // cast to i128; if `effective` could ever fall below `start`, this
    // would panic in debug builds (or wrap in release) rather than return
    // an `Error`. `VestingContract::create` rejects any schedule whose
    // `start + duration` would overflow, which is what keeps `end` (and so
    // `effective`, which is always clamped below it) from ever landing
    // below `start` here.
    let env = Env::default();
    let g = Grant {
        grantor: Address::generate(&env),
        beneficiary: Address::generate(&env),
        token: Address::generate(&env),
        total: TOTAL,
        start: u64::MAX - 1,
        cliff: 0,
        duration: 1,
        revocable: true,
        claimed: 0,
        returned: 0,
        revoked_at: None,
    };
    assert_eq!(g.vested_amount(u64::MAX - 1), Ok(0));
    assert_eq!(g.vested_amount(u64::MAX), Ok(TOTAL));
}

/// Guards the division by `duration` in the linear branch of
/// `vested_amount`: every `(start, cliff, duration)` combination here
/// satisfies `create`'s validation (`duration > 0`, `cliff <= duration`), and
/// no probe timestamp may surface `DivisionByZero`.
#[test]
fn linear_branch_never_divides_by_zero_duration() {
    let env = Env::default();
    let starts = [0u64, 1, START, u64::MAX - 1_000, u64::MAX];
    let schedules = [
        (0u64, 1u64),
        (1, 1),
        (0, DURATION),
        (CLIFF, DURATION),
        (DURATION, DURATION),
        (0, u64::MAX),
    ];

    for start in starts {
        for (cliff, duration) in schedules {
            // Mirror `create`'s validation: only valid schedules are stored.
            assert!(duration > 0);
            assert!(cliff <= duration);
            let g = Grant {
                grantor: Address::generate(&env),
                beneficiary: Address::generate(&env),
                token: Address::generate(&env),
                total: TOTAL,
                start,
                cliff,
                duration,
                revocable: true,
                claimed: 0,
                returned: 0,
                revoked_at: None,
            };
            let cliff_at = g.cliff_at();
            let end = g.end_at();
            let probes = [
                0,
                start.saturating_sub(1),
                start,
                cliff_at.saturating_sub(1),
                cliff_at,
                end.saturating_sub(1),
                end,
                end.saturating_add(1),
                u64::MAX,
            ];
            for at in probes {
                let result = g.vested_amount(at);
                assert_ne!(
                    result,
                    Err(Error::DivisionByZero),
                    "div-by-zero at start={start} cliff={cliff} duration={duration} at={at}"
                );
                // When the linear branch is taken, the divisor is non-zero.
                let in_linear = at >= cliff_at && at < end;
                if in_linear {
                    assert!(
                        g.duration > 0,
                        "linear branch hit with zero duration at start={start}"
                    );
                    assert!(result.is_ok());
                }
            }
        }
    }
}

#[test]
fn long_duration_large_total_stays_inside_documented_safe_range() {
    let env = Env::default();
    let ten_years = 10 * 365 * 24 * 60 * 60;
    let mut g = bare(&env, 0, ten_years);

    // One billion 18-decimal tokens over ten years stays well below i128::MAX
    // during the checked `total * elapsed / duration` calculation.
    g.total = 1_000_000_000_i128 * 1_000_000_000_000_000_000_i128;

    assert_eq!(g.vested_amount(START + ten_years / 2), Ok(g.total / 2));
    assert!(g.vested_amount(START + ten_years - 1).is_ok());
    assert_eq!(g.vested_amount(START + ten_years), Ok(g.total));
}

// schedule view

// ------------------------------------------------------- schedule view

fn checkpoints(f: &Fixture) -> std::vec::Vec<(u64, i128)> {
    f.client
        .schedule()
        .iter()
        .map(|c| (c.at, c.vested))
        .collect()
}

#[test]
fn schedule_lists_start_cliff_and_end_for_a_cliffed_grant() {
    let f = Fixture::new(true);
    assert_eq!(
        checkpoints(&f),
        std::vec![(START, 0), (START + 100, 100_000), (START + 1_000, TOTAL)]
    );
}

#[test]
fn schedule_with_no_cliff_is_start_and_end() {
    let f = Fixture::with_schedule(true, 0, DURATION);
    assert_eq!(
        checkpoints(&f),
        std::vec![(START, 0), (START + DURATION, TOTAL)]
    );
}

#[test]
fn schedule_with_cliff_equal_to_duration_is_start_and_end() {
    let f = Fixture::with_schedule(true, DURATION, DURATION);
    assert_eq!(
        checkpoints(&f),
        std::vec![(START, 0), (START + DURATION, TOTAL)]
    );
}

#[test]
fn schedule_stops_at_the_checkpoint_cap() {
    let f = Fixture::new(true);
    assert_eq!(f.client.get().checkpoint_count(), MAX_CHECKPOINTS);
    assert_eq!(f.client.schedule().len(), MAX_CHECKPOINTS);
}

#[test]
fn checkpoint_capacity_rejects_one_past_the_cap() {
    let env = Env::default();
    let g = bare(&env, CLIFF, DURATION);
    let mut points = soroban_sdk::Vec::new(&env);

    // Fill the vector exactly to the cap; each push must succeed.
    for n in 0..MAX_CHECKPOINTS {
        g.try_push_checkpoint(&mut points, START + u64::from(n))
            .expect("a checkpoint at the cap must be accepted");
    }
    assert_eq!(points.len(), MAX_CHECKPOINTS);

    // Cap + 1 is rejected with the dedicated error and leaves the vector as is.
    assert_eq!(
        g.try_push_checkpoint(&mut points, START + DURATION),
        Err(Error::VestingTooManyCheckpoints)
    );
    assert_eq!(points.len(), MAX_CHECKPOINTS);
}

#[test]
fn schedule_matches_vested_amount_at_every_checkpoint() {
    let f = Fixture::new(true);
    for (at, vested) in checkpoints(&f) {
        assert_eq!(f.client.vested_amount(&at), vested, "at {at}");
    }
}

#[test]
fn schedule_is_unchanged_by_revocation() {
    let f = Fixture::new(true);
    let before = checkpoints(&f);
    f.at(START + 500);
    f.client.revoke();
    assert_eq!(checkpoints(&f), before);
}

#[test]
fn schedule_has_a_maximum_of_three_checkpoints() {
    // The vesting schedule is defined by start, cliff, and duration.
    // This generates at most 3 corner points: start, cliff (if cliff > 0),
    // and end. The checkpoint count never exceeds 3, so vested_amount
    // computation is O(1) with no unbounded loops.
    // Case 1: cliff > 0 and cliff < duration → 3 checkpoints
    let f1 = Fixture::with_schedule(true, CLIFF, DURATION);
    assert_eq!(checkpoints(&f1).len(), 3);

    // Case 2: cliff == 0 (no cliff) → 2 checkpoints (start and end)
    let f2 = Fixture::with_schedule(true, 0, DURATION);
    assert_eq!(checkpoints(&f2).len(), 2);

    // Case 3: cliff == duration (all-or-nothing) → 2 checkpoints
    let f3 = Fixture::with_schedule(true, DURATION, DURATION);
    assert_eq!(checkpoints(&f3).len(), 2);

    // Case 4: very short duration → still at most 3 checkpoints
    let f4 = Fixture::with_schedule(true, 1, 2);
    assert!(checkpoints(&f4).len() <= 3);
}

#[test]
fn documented_cliff_and_rounding_semantics() {
    // Verify the worked examples from docs/vesting-safe-range.md.
    // 1,000-unit grant over 1,000 seconds with 100-second cliff.
    let env = Env::default();
    let grant_total = 1_000i128;
    let cliff_seconds = 100u64;
    let duration_seconds = 1_000u64;
    let start = START;

    let g = Grant {
        grantor: Address::generate(&env),
        beneficiary: Address::generate(&env),
        token: Address::generate(&env),
        total: grant_total,
        start,
        cliff: cliff_seconds,
        duration: duration_seconds,
        revocable: true,
        claimed: 0,
        returned: 0,
        revoked_at: None,
    };

    // Timestamp (start + 0s) → 0 vested
    assert_eq!(
        g.vested_amount(start),
        Ok(0),
        "at start: nothing vests before cliff"
    );

    // Timestamp (start + 99s) → 0 vested (before cliff at 100s)
    assert_eq!(
        g.vested_amount(start + 99),
        Ok(0),
        "at start + 99s: still before cliff"
    );

    // Timestamp (start + 100s) → 100 vested (cliff: 1000 × 100 / 1000)
    assert_eq!(
        g.vested_amount(start + 100),
        Ok(100),
        "at cliff (start + 100s): cliff amount vests"
    );

    // Timestamp (start + 500s) → 500 vested (linear: 1000 × 500 / 1000)
    assert_eq!(
        g.vested_amount(start + 500),
        Ok(500),
        "at start + 500s: linear vesting midpoint"
    );

    // Timestamp (start + 999s) → 999 vested (before end)
    assert_eq!(
        g.vested_amount(start + 999),
        Ok(999),
        "at start + 999s: 1000 × 999 / 1000 = 999"
    );

    // Timestamp (start + 1000s) → 1000 vested (fully vested)
    assert_eq!(
        g.vested_amount(start + 1_000),
        Ok(1_000),
        "at end (start + 1000s): fully vested"
    );

    // Timestamp (start + 1001s) → 1000 vested (after end)
    assert_eq!(
        g.vested_amount(start + 1_001),
        Ok(1_000),
        "after end (start + 1001s): remains at total"
    );

    // Verify cliff is inclusive: at cliff - 1 is 0, at cliff is non-zero.
    assert_eq!(g.vested_amount(start + cliff_seconds - 1), Ok(0));
    assert!(g.vested_amount(start + cliff_seconds).unwrap() > 0);
}

#[test]
fn create_funds_the_whole_grant_up_front() {
    let f = Fixture::new(true);
    assert_eq!(f.held(), TOTAL);
    assert_eq!(f.token.balance(&f.grantor), MINT - TOTAL);
    f.assert_conserved();
}

#[test]
fn create_rejects_a_non_positive_total() {
    let env = Env::default();
    env.mock_all_auths();
    let c = VestingContractClient::new(&env, &env.register(VestingContract, ()));
    let (a, b, t) = (
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    );
    for bad in [0i128, -1, i128::MIN] {
        assert_eq!(
            c.try_create(&a, &b, &t, &bad, &START, &CLIFF, &DURATION, &true),
            Err(Ok(Error::InvalidAmount)),
            "total {bad} was accepted"
        );
    }
}

#[test]
fn create_rejects_a_zero_duration() {
    let env = Env::default();
    env.mock_all_auths();
    let c = VestingContractClient::new(&env, &env.register(VestingContract, ()));
    let (a, b, t) = (
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    );
    assert_eq!(
        c.try_create(&a, &b, &t, &TOTAL, &START, &0, &0, &true),
        Err(Ok(Error::InvalidDuration))
    );
}

#[test]
fn create_rejects_a_cliff_after_the_end() {
    let env = Env::default();
    env.mock_all_auths();
    let c = VestingContractClient::new(&env, &env.register(VestingContract, ()));
    let (a, b, t) = (
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    );
    assert_eq!(
        c.try_create(
            &a,
            &b,
            &t,
            &TOTAL,
            &START,
            &(DURATION + 1),
            &DURATION,
            &true
        ),
        Err(Ok(Error::VestingCliffAfterEnd))
    );
}

/// When several create inputs are invalid at once, the first check in
/// `create`'s validation sequence wins. Locks the order:
/// `require_positive(total)` → `duration == 0` → `cliff > duration`.
#[test]
fn create_reports_the_first_validation_error_when_several_apply() {
    let env = Env::default();
    env.mock_all_auths();
    let c = VestingContractClient::new(&env, &env.register(VestingContract, ()));
    let (a, b, t) = (
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    );

    // total=-1, duration=0, cliff=5: InvalidAmount beats InvalidDuration /
    // VestingCliffAfterEnd.
    assert_eq!(
        c.try_create(&a, &b, &t, &-1, &START, &5, &0, &true),
        Err(Ok(Error::InvalidAmount))
    );

    // total ok, duration=0, cliff=5: InvalidDuration beats VestingCliffAfterEnd.
    assert_eq!(
        c.try_create(&a, &b, &t, &TOTAL, &START, &5, &0, &true),
        Err(Ok(Error::InvalidDuration))
    );
}

#[test]
fn create_rejects_a_schedule_whose_end_would_overflow_u64() {
    // `start + duration` must be representable, or `end_at`'s
    // `saturating_add` would silently cap it and `vested_amount` would be
    // evaluated against a shortened schedule instead of erroring up front.
    let env = Env::default();
    env.mock_all_auths();
    let c = VestingContractClient::new(&env, &env.register(VestingContract, ()));
    let (a, b, t) = (
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    );
    assert_eq!(
        c.try_create(&a, &b, &t, &TOTAL, &u64::MAX, &0, &1, &true),
        Err(Ok(Error::VestingScheduleOverflow))
    );
}

#[test]
fn create_accepts_a_schedule_whose_end_is_exactly_u64_max() {
    let f = Fixture::with_schedule_at(u64::MAX - 1, 0, 1, true);
    assert_eq!(f.client.get().duration, 1);
}

#[test]
fn create_rejects_a_second_call() {
    let f = Fixture::new(true);
    assert_eq!(
        f.client.try_create(
            &f.grantor,
            &f.beneficiary,
            &f.token.address,
            &TOTAL,
            &START,
            &CLIFF,
            &DURATION,
            &true
        ),
        Err(Ok(Error::AlreadyInitialized))
    );
}

/// Issue #228: a second `create` is rejected with `AlreadyInitialized` and
/// must leave the original grant untouched (no field is overwritten and the
/// held balance is unchanged).
#[test]
fn second_create_is_rejected_and_leaves_the_grant_intact() {
    let f = Fixture::new(true);
    let before = f.client.get();
    assert_eq!(
        f.client.try_create(
            &f.grantor,
            &f.beneficiary,
            &f.token.address,
            &(TOTAL * 2),
            &START,
            &CLIFF,
            &DURATION,
            &false,
        ),
        Err(Ok(Error::AlreadyInitialized))
    );
    assert_eq!(f.client.get(), before);
    assert_eq!(f.held(), TOTAL);
    f.assert_conserved();
}

#[test]
fn create_rejects_identical_grantor_and_beneficiary() {
    let env = Env::default();
    env.mock_all_auths();
    let c = VestingContractClient::new(&env, &env.register(VestingContract, ()));
    let party = Address::generate(&env);
    let token = Address::generate(&env);
    assert_eq!(
        c.try_create(&party, &party, &token, &TOTAL, &START, &CLIFF, &DURATION, &true),
        Err(Ok(Error::IdenticalParties))
    );
}

/// A self-grant is refused before any funds move, and leaves no grant behind.
#[test]
fn self_grant_is_refused_without_moving_funds() {
    let te = TestEnv::at(START);
    let (token, grantor) = te.make_token(MINT);
    let env = te.env().clone();
    let client = VestingContractClient::new(&env, &env.register(VestingContract, ()));
    assert_eq!(
        client.try_create(
            &grantor,
            &grantor,
            &token.address,
            &TOTAL,
            &START,
            &CLIFF,
            &DURATION,
            &true
        ),
        Err(Ok(Error::IdenticalParties))
    );
    assert_eq!(token.balance(&grantor), MINT);
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(client.try_get(), Err(Ok(Error::NotInitialized)));
}

/// `create` persists the grant before the token pull. A failed transfer must
/// roll back that write so `get` still returns `NotInitialized`.
#[test]
fn create_with_contract_as_beneficiary_succeeds() {
    let te = TestEnv::at(START);
    let (token_client, grantor) = te.make_token(MINT);
    let token = token_client.address.clone();
    let env = te.env().clone();
    let contract_id = env.register(VestingContract, ());
    let client = VestingContractClient::new(&env, &contract_id);

    let result = client.try_create(
        &grantor,
        &contract_id,
        &token,
        &TOTAL,
        &START,
        &CLIFF,
        &DURATION,
        &true,
    );
    assert!(
        result.is_ok(),
        "create with contract as beneficiary should succeed"
    );
}

#[test]
fn create_rejects_multiple_invalid_args_in_order() {
    let te = TestEnv::at(START);
    let (token_client, grantor) = te.make_token(MINT);
    let token = token_client.address.clone();
    let beneficiary = te.make_address();
    let env = te.env().clone();

    let contract_id = env.register(VestingContract, ());
    let client = VestingContractClient::new(&env, &contract_id);

    // AlreadyInitialized comes before InvalidAmount
    client.create(
        &grantor,
        &beneficiary,
        &token,
        &TOTAL,
        &START,
        &CLIFF,
        &DURATION,
        &true,
    );
    assert_eq!(
        client.try_create(
            &grantor,
            &beneficiary,
            &token,
            &0,
            &START,
            &CLIFF,
            &DURATION,
            &true
        ),
        Err(Ok(Error::AlreadyInitialized))
    );

    // InvalidAmount comes before InvalidDuration
    let contract_id2 = env.register(VestingContract, ());
    let client2 = VestingContractClient::new(&env, &contract_id2);
    assert_eq!(
        client2.try_create(
            &grantor,
            &beneficiary,
            &token,
            &0,
            &START,
            &CLIFF,
            &0,
            &true
        ),
        Err(Ok(Error::InvalidAmount))
    );

    // InvalidDuration comes before VestingCliffAfterEnd
    let contract_id3 = env.register(VestingContract, ());
    let client3 = VestingContractClient::new(&env, &contract_id3);
    assert_eq!(
        client3.try_create(
            &grantor,
            &beneficiary,
            &token,
            &TOTAL,
            &START,
            &CLIFF,
            &0,
            &true
        ),
        Err(Ok(Error::InvalidDuration))
    );
}

#[test]
fn create_reverts_cleanly_when_grantor_cannot_fund() {
    let te = TestEnv::at(START);
    let (token_client, grantor) = te.make_token(1);
    let token = token_client.address.clone();
    let beneficiary = te.make_address();
    let env = te.env().clone();
    let c = VestingContractClient::new(&env, &env.register(VestingContract, ()));

    assert!(c
        .try_create(
            &grantor,
            &beneficiary,
            &token,
            &TOTAL,
            &START,
            &CLIFF,
            &DURATION,
            &true
        )
        .is_err());
    assert_eq!(c.try_get(), Err(Ok(Error::NotInitialized)));
    assert_eq!(token_client.balance(&c.address), 0);
    assert_eq!(token_client.balance(&grantor), 1);
}

/// Pins the wire shape indexers decode (SPEC.md `indexed_events`). The
/// expected value is spelled out literally rather than built from
/// `events::Created`, so renaming a topic or a field fails here.
#[test]
fn create_emits_the_created_event() {
    let te = TestEnv::at(START);
    let (token_client, grantor) = te.make_token(MINT);
    let token_address = token_client.address.clone();
    let beneficiary = te.make_address();
    let env = te.env().clone();

    let client = VestingContractClient::new(&env, &env.register(VestingContract, ()));
    client.create(
        &grantor,
        &beneficiary,
        &token_address,
        &TOTAL,
        &START,
        &CLIFF,
        &DURATION,
        &true,
    );

    let total: Val = TOTAL.into_val(&env);
    let start: Val = START.into_val(&env);
    let cliff: Val = CLIFF.into_val(&env);
    let duration: Val = DURATION.into_val(&env);
    let revocable: Val = true.into_val(&env);
    // Map data: keys are the field names, serialized in sorted order.
    let data: Val = map![
        &env,
        (Symbol::new(&env, "beneficiary"), beneficiary.into_val(&env)),
        (Symbol::new(&env, "cliff"), cliff),
        (Symbol::new(&env, "duration"), duration),
        (Symbol::new(&env, "revocable"), revocable),
        (Symbol::new(&env, "start"), start),
        (Symbol::new(&env, "token"), token_address.into_val(&env)),
        (Symbol::new(&env, "total"), total),
    ]
    .into_val(&env);
    assert_eq!(
        env.events().all().filter_by_contract(&client.address),
        vec![
            &env,
            (
                client.address.clone(),
                vec![
                    &env,
                    Symbol::new(&env, "vesting").into_val(&env),
                    Symbol::new(&env, "created").into_val(&env),
                    grantor.into_val(&env),
                ],
                data,
            ),
        ]
    );
}

#[test]
fn entry_points_error_before_create() {
    let env = Env::default();
    env.mock_all_auths();
    let c = VestingContractClient::new(&env, &env.register(VestingContract, ()));
    assert_eq!(c.try_claim(), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_revoke(), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_claimable(), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_remaining(), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_vested_amount(&START), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_schedule(), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_bump(), Err(Ok(Error::NotInitialized)));
    assert_eq!(c.try_get(), Err(Ok(Error::NotInitialized)));
}

/// Issue #227: `get` on a contract that has not run `create` yet must return
/// `NotInitialized` rather than panicking or returning a zeroed grant.
#[test]
fn get_errors_before_create() {
    let env = Env::default();
    env.mock_all_auths();
    let c = VestingContractClient::new(&env, &env.register(VestingContract, ()));
    assert_eq!(c.try_get(), Err(Ok(Error::NotInitialized)));
}

#[test]
#[should_panic]
fn create_requires_the_grantors_authorization() {
    let env = Env::default();
    let c = VestingContractClient::new(&env, &env.register(VestingContract, ()));
    c.create(
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &TOTAL,
        &START,
        &CLIFF,
        &DURATION,
        &true,
    );
}

// issue #223: vested_amount before start

// ------------------------------------------------------- issue #223: vested_amount before start

#[test]
fn vested_amount_is_zero_before_the_start() {
    let env = Env::default();
    let g = bare(&env, CLIFF, DURATION);
    // Any timestamp strictly before `start` must return exactly 0 — never negative.
    assert_eq!(g.vested_amount(0), Ok(0));
    assert_eq!(g.vested_amount(START - 1), Ok(0));
    // Sanity: the boundary itself also returns 0 (nothing vested at t=start).
    assert_eq!(g.vested_amount(START), Ok(0));
}
