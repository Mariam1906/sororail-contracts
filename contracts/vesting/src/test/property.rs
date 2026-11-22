use super::*;

// property

// ---------------------------------------------------------------- property

mod vested_amount_properties {
    use proptest::prelude::*;

    use super::*;

    fn arb_grant(env: &Env) -> Grant {
        Grant {
            grantor: Address::generate(env),
            beneficiary: Address::generate(env),
            token: Address::generate(env),
            total: 0,
            start: 0,
            cliff: 0,
            duration: 1,
            revocable: false,
            claimed: 0,
            returned: 0,
            revoked_at: None,
        }
    }

    proptest! {
        /// `vested_amount` never decreases as `at` increases, for an
        /// unrevoked grant. Vesting only ever moves forward in time.
        #[test]
        fn vested_amount_is_monotonic_over_time(
            total in 1i128..=1_000_000_000_000,
            start in 0u64..=1_000_000_000,
            duration in 1u64..=10_000_000,
            cliff_frac in 0u64..=100,
            t0_offset in 0u64..=20_000_000,
            dt in 0u64..=20_000_000,
        ) {
            let env = Env::default();
            let cliff = duration.saturating_mul(cliff_frac) / 100;

            let grant = Grant {
                total,
                start,
                cliff,
                duration,
                ..arb_grant(&env)
            };

            let t0 = start.saturating_add(t0_offset);
            let t1 = t0.saturating_add(dt);

            let v0 = grant.vested_amount(t0).expect("vested_amount(t0) must not error");
            let v1 = grant.vested_amount(t1).expect("vested_amount(t1) must not error");

            prop_assert!(
                v1 >= v0,
                "vested_amount went from {v0} at t={t0} to {v1} at t={t1} (start={start}, cliff={cliff}, duration={duration}, total={total})"
            );
        }

        /// Revocation freezes the schedule: nothing vests past the
        /// revocation timestamp, no matter how far `at` is pushed beyond it.
        #[test]
        fn vested_amount_is_frozen_after_revocation(
            total in 1i128..=1_000_000_000_000,
            start in 0u64..=1_000_000_000,
            duration in 1u64..=10_000_000,
            revoked_after in 0u64..=10_000_000,
            dt in 0u64..=20_000_000,
        ) {
            let env = Env::default();
            let revoked_at = start.saturating_add(revoked_after);

            let grant = Grant {
                total,
                start,
                cliff: 0,
                duration,
                revoked_at: Some(revoked_at),
                ..arb_grant(&env)
            };

            let at_revocation = grant
                .vested_amount(revoked_at)
                .expect("vested_amount(revoked_at) must not error");
            let after = grant
                .vested_amount(revoked_at.saturating_add(dt))
                .expect("vested_amount(after revocation) must not error");

            prop_assert_eq!(at_revocation, after);
        }
    }
}

mod conservation_properties {
    use proptest::prelude::*;

    use super::*;

    /// One step of a random call sequence against a live grant. Invalid calls
    /// (e.g. claiming before the cliff) are included on purpose: a rejected
    /// call must roll back and preserve conservation.
    #[derive(Clone, Copy, Debug)]
    enum Op {
        Advance(u64),
        Claim,
        Revoke,
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// The vesting conservation invariant
        /// `claimed + returned + remaining == total` holds after every call of
        /// an arbitrary sequence of claim / revoke calls, and the contract's
        /// real token balance always equals `remaining`.
        #[test]
        fn conservation_holds_after_any_call_sequence(
            total in 1i128..=1_000_000_000,
            duration in 1u64..=1_000,
            cliff_frac in 0u64..=100,
            ops in proptest::collection::vec(prop_oneof![
                (1u64..=500).prop_map(Op::Advance),
                Just(Op::Claim),
                Just(Op::Revoke),
            ], 1..=24),
        ) {
            let start = 1_000_000u64;
            let cliff = duration.saturating_mul(cliff_frac) / 100;
            let te = TestEnv::at(start);
            let (token, grantor) = te.make_token(1_000_000_000_000_000);
            let token_address = token.address.clone();
            let beneficiary = te.make_address();
            let env = te.env().clone();

            let client = VestingContractClient::new(&env, &env.register(VestingContract, ()));
            client.create(
                &grantor,
                &beneficiary,
                &token_address,
                &total,
                &start,
                &cliff,
                &duration,
                &true,
            );

            for op in ops {
                match op {
                    Op::Advance(delta) => {
                        te.advance_to(te.timestamp().saturating_add(delta));
                    }
                    Op::Claim => {
                        let _ = client.try_claim();
                    }
                    Op::Revoke => {
                        let _ = client.try_revoke();
                    }
                }

                let g = client.get();
                let remaining = client.remaining();
                prop_assert!(
                    g.claimed >= 0 && g.returned >= 0 && remaining >= 0,
                    "negative component after {:?}: {:?}", op, g
                );
                prop_assert_eq!(
                    g.claimed + g.returned + remaining,
                    g.total,
                    "conservation violated after {:?}: {:?}", op, g
                );
                prop_assert_eq!(
                    token.balance(&client.address),
                    remaining,
                    "held balance diverged after {:?}: {:?}", op, g
                );
            }
        }
    }
}
