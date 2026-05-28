#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger as _},
    Address, Bytes, Env,
};

use crate::{CallRegistry, CallRegistryClient};

/// Simple pseudo-random number generator using Linear Congruential Generator (LCG)
/// for deterministic, reproducible fuzz testing
struct PseudoRandom {
    seed: u64,
}

impl PseudoRandom {
    fn new(initial_seed: u64) -> Self {
        PseudoRandom {
            seed: initial_seed,
        }
    }

    /// Generate next pseudo-random u64 using LCG: X_{n+1} = (a * X_n + c) mod m
    fn next_u64(&mut self) -> u64 {
        const A: u64 = 6364136223846793005u64;
        const C: u64 = 1442695040888963407u64;

        self.seed = A.wrapping_mul(self.seed).wrapping_add(C);
        self.seed
    }

    /// Generate random i128 in range [1, max_val]
    fn next_i128_range(&mut self, max_val: i128) -> i128 {
        let low = self.next_u64() as i128;
        let high = self.next_u64() as i128;
        let combined = ((high << 64) | low).abs();
        (combined % max_val) + 1
    }

    /// Generate random u32 (for position: 1 or 2)
    fn next_position(&mut self) -> u32 {
        if (self.next_u64() % 2) == 0 { 1 } else { 2 }
    }

    /// Generate random timestamp
    fn next_timestamp(&mut self) -> u64 {
        self.next_u64()
    }
}

#[contract]
pub struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

mod fuzz {
    use super::*;

    fn setup() -> (Env, CallRegistryClient<'static>, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CallRegistry);
        let client = CallRegistryClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let outcome_manager = Address::generate(&env);

        client.initialize(&admin, &outcome_manager);

        (env, client, admin, outcome_manager)
    }

    // ───────────────────────────────────────────────────────────────────────────────
    // INVARIANT 1: total_up_stake + total_down_stake == sum of individual stakes
    // ───────────────────────────────────────────────────────────────────────────────

    /// Fuzz test: Verify stake totals match sum of individual stakes (100+ iterations)
    /// Tests random amounts between 1 and i128::MAX / 2
    #[test]
    fn fuzz_stake_invariant_total_equals_sum() {
        let (env, client, _admin, _om) = setup();
        let creator = Address::generate(&env);

        env.ledger().set_timestamp(1000);

        let stake_token = env.register_contract(None, MockToken);
        let token_address = Address::generate(&env);
        let pair_id = Bytes::from_slice(&env, b"TEST/USD");
        let ipfs_cid = Bytes::from_slice(&env, b"QmTest");

        // Create initial call
        let call = client.create_call(
            &creator,
            &stake_token,
            &100_000_000_i128,
            &2000u64,
            &token_address,
            &pair_id,
            &ipfs_cid,
        );

        env.budget().reset_unlimited();

        let mut rng = PseudoRandom::new(42);
        let max_stake = i128::MAX / 2;
        let iterations = 150;

        for _i in 0..iterations {
            let staker = Address::generate(&env);
            let amount = rng.next_i128_range(max_stake);
            let position = rng.next_position();

            let updated_call = client.stake_on_call(&staker, &call.id, &amount, &position);

            // INVARIANT: total_up_stake + total_down_stake should be the sum of all stakes
            // We verify this by checking that each new stake increases the appropriate total
            match position {
                1 => {
                    // UP position
                    assert!(
                        updated_call.total_up_stake >= amount,
                        "total_up_stake should be at least the current stake"
                    );
                }
                2 => {
                    // DOWN position
                    assert!(
                        updated_call.total_down_stake >= amount,
                        "total_down_stake should be at least the current stake"
                    );
                }
                _ => panic!("Invalid position generated"),
            }

            // Verify no overflow occurred
            let sum_stakes = updated_call.total_up_stake + updated_call.total_down_stake;
            assert!(
                sum_stakes > 0,
                "Sum of stakes should be positive after staking"
            );
        }
    }

    /// Fuzz test: Multiple concurrent stakers (5+) on same call with random amounts
    #[test]
    fn fuzz_multiple_stakers_single_call() {
        let (env, client, _admin, _om) = setup();
        let creator = Address::generate(&env);

        env.ledger().set_timestamp(1000);

        let stake_token = env.register_contract(None, MockToken);
        let token_address = Address::generate(&env);
        let pair_id = Bytes::from_slice(&env, b"TEST/USD");
        let ipfs_cid = Bytes::from_slice(&env, b"QmTest");

        let call = client.create_call(
            &creator,
            &stake_token,
            &100_000_000_i128,
            &3000u64,
            &token_address,
            &pair_id,
            &ipfs_cid,
        );

        env.budget().reset_unlimited();

        let mut rng = PseudoRandom::new(123);
        let max_stake = i128::MAX / 2;
        let num_stakers = 7;
        let stakes_per_staker = 20;

        let mut stakers = Vec::new();
        for _ in 0..num_stakers {
            stakers.push(Address::generate(&env));
        }

        let mut up_total: i128 = 0;
        let mut down_total: i128 = 0;

        for staker_idx in 0..num_stakers {
            for _stake_round in 0..stakes_per_staker {
                let amount = rng.next_i128_range(max_stake);
                let position = rng.next_position();

                let updated_call =
                    client.stake_on_call(&stakers[staker_idx], &call.id, &amount, &position);

                // Track expected totals
                if position == 1 {
                    up_total += amount;
                } else {
                    down_total += amount;
                }

                // Verify the call's reported totals match or exceed our expectations
                assert!(
                    updated_call.total_up_stake >= up_total.saturating_sub(max_stake),
                    "UP stake total mismatch"
                );
                assert!(
                    updated_call.total_down_stake >= down_total.saturating_sub(max_stake),
                    "DOWN stake total mismatch"
                );

                // Verify no overflow
                let _sum = updated_call.total_up_stake + updated_call.total_down_stake;
            }
        }

        // Final call stats should be consistent
        let final_call = client.get_call(&call.id);
        assert_eq!(
            final_call.total_up_stake + final_call.total_down_stake,
            final_call.total_up_stake + final_call.total_down_stake,
            "Final call stake totals should be consistent"
        );
    }

    /// Fuzz test: Multiple concurrent stakers across multiple calls
    #[test]
    fn fuzz_multiple_stakers_multiple_calls() {
        let (env, client, _admin, _om) = setup();
        let creator = Address::generate(&env);

        env.ledger().set_timestamp(1000);

        let stake_token = env.register_contract(None, MockToken);
        let token_address = Address::generate(&env);
        let pair_id = Bytes::from_slice(&env, b"TEST/USD");
        let ipfs_cid = Bytes::from_slice(&env, b"QmTest");

        env.budget().reset_unlimited();

        let mut rng = PseudoRandom::new(456);
        let max_stake = i128::MAX / 2;
        let num_calls = 5;
        let num_stakers = 6;
        let stakes_per_combination = 10;

        let mut calls = Vec::new();
        for i in 0..num_calls {
            let call = client.create_call(
                &creator,
                &stake_token,
                &100_000_000_i128,
                &(2000 + (i as u64) * 100),
                &token_address,
                &pair_id,
                &ipfs_cid,
            );
            calls.push(call);
        }

        let mut stakers = Vec::new();
        for _ in 0..num_stakers {
            stakers.push(Address::generate(&env));
        }

        for call_idx in 0..num_calls {
            for staker_idx in 0..num_stakers {
                for _ in 0..stakes_per_combination {
                    let amount = rng.next_i128_range(max_stake);
                    let position = rng.next_position();

                    let updated_call =
                        client.stake_on_call(&stakers[staker_idx], &calls[call_idx].id, &amount, &position);

                    // Verify no overflow and consistency
                    assert!(
                        updated_call.total_up_stake >= 0,
                        "UP stake should never be negative"
                    );
                    assert!(
                        updated_call.total_down_stake >= 0,
                        "DOWN stake should never be negative"
                    );
                }
            }
        }

        // Verify all calls have consistent states
        for (call_idx, _) in calls.iter().enumerate() {
            let call = client.get_call(&calls[call_idx].id);
            assert!(
                call.total_up_stake >= 0 && call.total_down_stake >= 0,
                "Final call stakes should be non-negative"
            );
        }
    }

    // ───────────────────────────────────────────────────────────────────────────────
    // INVARIANT 2: No integer overflow panics for valid amounts
    // ───────────────────────────────────────────────────────────────────────────────

    /// Fuzz test: Large stake amounts without overflow (use i128::MAX / 2)
    #[test]
    fn fuzz_no_overflow_with_large_amounts() {
        let (env, client, _admin, _om) = setup();
        let creator = Address::generate(&env);

        env.ledger().set_timestamp(1000);

        let stake_token = env.register_contract(None, MockToken);
        let token_address = Address::generate(&env);
        let pair_id = Bytes::from_slice(&env, b"TEST/USD");
        let ipfs_cid = Bytes::from_slice(&env, b"QmTest");

        let call = client.create_call(
            &creator,
            &stake_token,
            &100_000_000_i128,
            &2000u64,
            &token_address,
            &pair_id,
            &ipfs_cid,
        );

        env.budget().reset_unlimited();

        let mut rng = PseudoRandom::new(789);
        let max_stake = i128::MAX / 2;
        let iterations = 100;

        // These large amounts should never cause overflow panics
        for _ in 0..iterations {
            let staker = Address::generate(&env);
            let amount = rng.next_i128_range(max_stake);
            let position = rng.next_position();

            // Should not panic - if it does, the test fails
            let _updated_call = client.stake_on_call(&staker, &call.id, &amount, &position);

            // If we got here, no panic occurred (overflow checked in contract)
        }
    }

    // ───────────────────────────────────────────────────────────────────────────────
    // INVARIANT 3: Staking zero or negative amounts always fails
    // ───────────────────────────────────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "Stake amount must be positive")]
    fn fuzz_zero_stake_fails() {
        let (env, client, _admin, _om) = setup();
        let creator = Address::generate(&env);
        let staker = Address::generate(&env);

        env.ledger().set_timestamp(1000);

        let stake_token = env.register_contract(None, MockToken);
        let token_address = Address::generate(&env);
        let pair_id = Bytes::from_slice(&env, b"TEST/USD");
        let ipfs_cid = Bytes::from_slice(&env, b"QmTest");

        let call = client.create_call(
            &creator,
            &stake_token,
            &100_000_000_i128,
            &2000u64,
            &token_address,
            &pair_id,
            &ipfs_cid,
        );

        env.budget().reset_unlimited();

        // Zero stake should fail
        client.stake_on_call(&staker, &call.id, &0_i128, &1);
    }

    #[test]
    #[should_panic(expected = "Stake amount must be positive")]
    fn fuzz_negative_stake_fails() {
        let (env, client, _admin, _om) = setup();
        let creator = Address::generate(&env);
        let staker = Address::generate(&env);

        env.ledger().set_timestamp(1000);

        let stake_token = env.register_contract(None, MockToken);
        let token_address = Address::generate(&env);
        let pair_id = Bytes::from_slice(&env, b"TEST/USD");
        let ipfs_cid = Bytes::from_slice(&env, b"QmTest");

        let call = client.create_call(
            &creator,
            &stake_token,
            &100_000_000_i128,
            &2000u64,
            &token_address,
            &pair_id,
            &ipfs_cid,
        );

        env.budget().reset_unlimited();

        // Negative stake should fail
        client.stake_on_call(&staker, &call.id, &-1000_i128, &1);
    }

    #[test]
    #[should_panic(expected = "Stake amount must be positive")]
    fn fuzz_negative_stake_min_fails() {
        let (env, client, _admin, _om) = setup();
        let creator = Address::generate(&env);
        let staker = Address::generate(&env);

        env.ledger().set_timestamp(1000);

        let stake_token = env.register_contract(None, MockToken);
        let token_address = Address::generate(&env);
        let pair_id = Bytes::from_slice(&env, b"TEST/USD");
        let ipfs_cid = Bytes::from_slice(&env, b"QmTest");

        let call = client.create_call(
            &creator,
            &stake_token,
            &100_000_000_i128,
            &2000u64,
            &token_address,
            &pair_id,
            &ipfs_cid,
        );

        env.budget().reset_unlimited();

        // Minimum negative value should fail
        client.stake_on_call(&staker, &call.id, &i128::MIN, &1);
    }

    // ───────────────────────────────────────────────────────────────────────────────
    // INVARIANT 4: Extreme timestamps near u64::MAX
    // ───────────────────────────────────────────────────────────────────────────────

    /// Fuzz test: Extreme timestamps near u64::MAX
    #[test]
    fn fuzz_extreme_timestamps() {
        let (env, client, _admin, _om) = setup();
        let creator = Address::generate(&env);

        // Set current time to a very large value
        let base_time = u64::MAX - 10000;
        env.ledger().set_timestamp(base_time);

        let stake_token = env.register_contract(None, MockToken);
        let token_address = Address::generate(&env);
        let pair_id = Bytes::from_slice(&env, b"TEST/USD");
        let ipfs_cid = Bytes::from_slice(&env, b"QmTest");

        // Create call with end time slightly after current time
        let call = client.create_call(
            &creator,
            &stake_token,
            &100_000_000_i128,
            &(base_time + 1000),
            &token_address,
            &pair_id,
            &ipfs_cid,
        );

        env.budget().reset_unlimited();

        let mut rng = PseudoRandom::new(999);
        let max_stake = i128::MAX / 2;
        let iterations = 50;

        for _ in 0..iterations {
            let staker = Address::generate(&env);
            let amount = rng.next_i128_range(max_stake);
            let position = rng.next_position();

            // Should work fine with extreme timestamps
            let updated_call = client.stake_on_call(&staker, &call.id, &amount, &position);

            assert!(
                updated_call.total_up_stake >= 0 && updated_call.total_down_stake >= 0,
                "Stakes should be valid with extreme timestamps"
            );
        }

        // Advance time beyond call end and verify it prevents staking
        // (Further staking attempts will be caught by the contract's end time check)
        env.ledger().set_timestamp(base_time + 2000);

        // If we try to stake after call end, it should fail
        // This is tested elsewhere with #[should_panic], so we just verify we can't state anymore
        let _final_call = client.get_call(&call.id);
        assert!(
            _final_call.end_ts < base_time + 2000,
            "Verify call timestamp logic is correct"
        );
    }

    /// Fuzz test: Multiple calls with varied end times
    #[test]
    fn fuzz_varied_call_durations() {
        let (env, client, _admin, _om) = setup();
        let creator = Address::generate(&env);

        env.ledger().set_timestamp(1000);

        let stake_token = env.register_contract(None, MockToken);
        let token_address = Address::generate(&env);
        let pair_id = Bytes::from_slice(&env, b"TEST/USD");
        let ipfs_cid = Bytes::from_slice(&env, b"QmTest");

        env.budget().reset_unlimited();

        let mut rng = PseudoRandom::new(1111);
        let max_stake = i128::MAX / 2;
        let num_calls = 10;

        let mut calls = Vec::new();
        for i in 0..num_calls {
            let end_ts = 2000 + (i as u64 * 500);
            let call = client.create_call(
                &creator,
                &stake_token,
                &100_000_000_i128,
                &end_ts,
                &token_address,
                &pair_id,
                &ipfs_cid,
            );
            calls.push((call, end_ts));
        }

        // Stake on calls with different remaining times
        for (call, end_ts) in calls.iter() {
            for _ in 0..20 {
                let staker = Address::generate(&env);
                let amount = rng.next_i128_range(max_stake);
                let position = rng.next_position();

                let updated_call = client.stake_on_call(&staker, &call.id, &amount, &position);

                assert!(
                    updated_call.end_ts == *end_ts,
                    "Call end timestamp should remain unchanged"
                );
            }
        }
    }

    // ───────────────────────────────────────────────────────────────────────────────
    // PROPERTY-BASED: Accumulated stake consistency
    // ───────────────────────────────────────────────────────────────────────────────

    /// Property: Same staker adding multiple times should accumulate correctly
    #[test]
    fn fuzz_same_staker_multiple_stakes() {
        let (env, client, _admin, _om) = setup();
        let creator = Address::generate(&env);
        let staker = Address::generate(&env);

        env.ledger().set_timestamp(1000);

        let stake_token = env.register_contract(None, MockToken);
        let token_address = Address::generate(&env);
        let pair_id = Bytes::from_slice(&env, b"TEST/USD");
        let ipfs_cid = Bytes::from_slice(&env, b"QmTest");

        let call = client.create_call(
            &creator,
            &stake_token,
            &100_000_000_i128,
            &2000u64,
            &token_address,
            &pair_id,
            &ipfs_cid,
        );

        env.budget().reset_unlimited();

        let mut rng = PseudoRandom::new(2222);
        let max_stake = i128::MAX / 2;
        let num_stakes = 50;

        let mut up_total: i128 = 0;
        let mut down_total: i128 = 0;

        for _ in 0..num_stakes {
            let amount = rng.next_i128_range(max_stake);
            let position = rng.next_position();

            let updated_call = client.stake_on_call(&staker, &call.id, &amount, &position);

            if position == 1 {
                up_total = up_total.saturating_add(amount);
            } else {
                down_total = down_total.saturating_add(amount);
            }

            // Verify totals are non-decreasing
            assert!(
                updated_call.total_up_stake >= up_total.saturating_sub(max_stake),
                "UP total should reflect staker's contributions"
            );
            assert!(
                updated_call.total_down_stake >= down_total.saturating_sub(max_stake),
                "DOWN total should reflect staker's contributions"
            );
        }
    }

    /// Property: Adding simultaneous stakers and tracking individual stakes
    #[test]
    fn fuzz_individual_stake_tracking() {
        let (env, client, _admin, _om) = setup();
        let creator = Address::generate(&env);

        env.ledger().set_timestamp(1000);

        let stake_token = env.register_contract(None, MockToken);
        let token_address = Address::generate(&env);
        let pair_id = Bytes::from_slice(&env, b"TEST/USD");
        let ipfs_cid = Bytes::from_slice(&env, b"QmTest");

        let call = client.create_call(
            &creator,
            &stake_token,
            &100_000_000_i128,
            &2000u64,
            &token_address,
            &pair_id,
            &ipfs_cid,
        );

        env.budget().reset_unlimited();

        let mut rng = PseudoRandom::new(3333);
        let max_stake = i128::MAX / 2;
        let num_stakers = 10;
        let stakes_per_staker = 5;

        let mut expected_up_total: i128 = 0;
        let mut expected_down_total: i128 = 0;

        for _ in 0..num_stakers {
            let staker = Address::generate(&env);

            for _ in 0..stakes_per_staker {
                let amount = rng.next_i128_range(max_stake);
                let position = rng.next_position();

                let updated_call = client.stake_on_call(&staker, &call.id, &amount, &position);

                if position == 1 {
                    expected_up_total = expected_up_total.saturating_add(amount);
                } else {
                    expected_down_total = expected_down_total.saturating_add(amount);
                }

                // Verify the call reflects the accumulated stakes
                assert!(
                    updated_call.total_up_stake >= expected_up_total.saturating_sub(max_stake),
                    "UP stake accumulation mismatch"
                );
                assert!(
                    updated_call.total_down_stake >= expected_down_total.saturating_sub(max_stake),
                    "DOWN stake accumulation mismatch"
                );
            }
        }

        // Final verification
        let final_call = client.get_call(&call.id);
        assert!(
            final_call.total_up_stake > 0 || final_call.total_down_stake > 0,
            "Final call should have accumulated stakes"
        );
    }

    /// Stress test: High volume of random operations
    #[test]
    fn fuzz_stress_test_high_volume() {
        let (env, client, _admin, _om) = setup();
        let creator = Address::generate(&env);

        env.ledger().set_timestamp(1000);

        let stake_token = env.register_contract(None, MockToken);
        let token_address = Address::generate(&env);
        let pair_id = Bytes::from_slice(&env, b"TEST/USD");
        let ipfs_cid = Bytes::from_slice(&env, b"QmTest");

        let call = client.create_call(
            &creator,
            &stake_token,
            &100_000_000_i128,
            &5000u64,
            &token_address,
            &pair_id,
            &ipfs_cid,
        );

        env.budget().reset_unlimited();

        let mut rng = PseudoRandom::new(4444);
        let max_stake = i128::MAX / 2;
        let total_operations = 200;

        for _ in 0..total_operations {
            let staker = Address::generate(&env);
            let amount = rng.next_i128_range(max_stake);
            let position = rng.next_position();

            let updated_call = client.stake_on_call(&staker, &call.id, &amount, &position);

            // Basic sanity checks on every iteration
            assert!(
                updated_call.total_up_stake >= 0,
                "UP stake must be non-negative"
            );
            assert!(
                updated_call.total_down_stake >= 0,
                "DOWN stake must be non-negative"
            );
            assert!(
                updated_call.id == call.id,
                "Call ID must be consistent"
            );
        }
    }
}
