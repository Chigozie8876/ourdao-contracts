use proptest::prelude::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, Vec};

use super::common::*;
use crate::util::{MAX_STAKE_BONUS, STAKE_WEIGHT_UNIT};
use crate::{OurDao, OurDaoClient};

// ==================== issue #7: property tests ====================
//
// The example tests in the sibling modules pin down behavior at specific,
// hand-picked numbers. These instead generate wide ranges of inputs and check
// invariants that must hold for any input.

const AMOUNT_BOUND: i128 = i128::MAX / 20_000;

fn contract_with_treasury(treasury: i128) -> (Env, OurDaoClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_id = sac.address();
    let token_mint = token::StellarAssetClient::new(&env, &token_id);

    let admin = Address::generate(&env);
    let contract_id = env.register(OurDao, ());
    let client = OurDaoClient::new(&env, &contract_id);
    let mut admins = Vec::new(&env);
    admins.push_back(admin);
    client.initialize(&admins, &5_100u32, &FEE, &token_id, &policy());

    if treasury > 0 {
        token_mint.mint(&contract_id, &treasury);
    }

    (env, client)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    #[test]
    fn loan_terms_rate_stays_within_policy_bounds(
        amount in 0i128..=AMOUNT_BOUND,
        treasury in 0i128..=AMOUNT_BOUND,
    ) {
        let (_env, client) = contract_with_treasury(treasury);
        let terms = client.calculate_loan_terms(&amount);
        let p = policy();

        prop_assert!(terms.interest_rate >= p.min_interest_rate);
        prop_assert!(terms.interest_rate <= p.max_interest_rate);
        prop_assert!(terms.total_repayment >= amount);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(80))]

    #[test]
    fn voting_weight_stays_in_bounds(stake in 0i128..=i128::MAX) {
        let env = Env::default();
        let contract_id = env.register(OurDao, ());
        let who = Address::generate(&env);

        let weight = env.as_contract(&contract_id, || {
            crate::storage::set_stake(&env, &who, stake);
            crate::util::voting_weight(&env, &who)
        });

        prop_assert!((1..=6).contains(&weight));
    }

    #[test]
    fn voting_weight_is_monotonic_in_stake(
        a in 0i128..=i128::MAX,
        delta in 0i128..=i128::MAX,
    ) {
        let b = a.saturating_add(delta);
        let env = Env::default();
        let contract_id = env.register(OurDao, ());
        let who = Address::generate(&env);

        let (wa, wb) = env.as_contract(&contract_id, || {
            crate::storage::set_stake(&env, &who, a);
            let wa = crate::util::voting_weight(&env, &who);

            crate::storage::set_stake(&env, &who, b);
            let wb = crate::util::voting_weight(&env, &who);

            (wa, wb)
        });

        prop_assert!(wb >= wa);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(80))]

    #[test]
    fn boost_is_constant_within_each_square_band(
        stake in 0i128..=AMOUNT_BOUND,
    ) {
        let band = crate::util::stake_boost(stake);
        let low = STAKE_WEIGHT_UNIT * band * band;
        let next = STAKE_WEIGHT_UNIT * (band + 1) * (band + 1);

        prop_assert!(stake >= low);
        prop_assert_eq!(crate::util::stake_boost(low), band);

        if band < MAX_STAKE_BONUS {
            prop_assert!(stake < next);
            prop_assert_eq!(crate::util::stake_boost(next - 1), band);
            prop_assert_eq!(crate::util::stake_boost(next), band + 1);
        }
    }

    #[test]
    fn boost_saturates_at_the_cap(surplus in 0i128..=AMOUNT_BOUND) {
        let stake =
            STAKE_WEIGHT_UNIT * MAX_STAKE_BONUS * MAX_STAKE_BONUS + surplus;
        prop_assert_eq!(
            crate::util::stake_boost(stake),
            MAX_STAKE_BONUS,
        );
    }

    #[test]
    fn boost_never_exceeds_the_linear_rule(
        stake in 0i128..=AMOUNT_BOUND,
    ) {
        let linear =
            (stake / STAKE_WEIGHT_UNIT).min(MAX_STAKE_BONUS);
        prop_assert!(crate::util::stake_boost(stake) <= linear);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(80))]

    #[test]
    fn isqrt_never_rounds_up(r in 1i128..=1i128 << 60) {
        prop_assert_eq!(crate::util::isqrt(r * r), r);
        prop_assert_eq!(crate::util::isqrt(r * r - 1), r - 1);
        prop_assert_eq!(crate::util::isqrt(r * r + 2 * r), r);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn distributed_interest_never_exceeds_collected(
        active in 1u32..=8,
        interest in 0i128..=i128::MAX,
    ) {
        let (env, client) = contract_with_treasury(0);
        let contract_id = client.address.clone();

        let mut members = Vec::new(&env);
        for _ in 0..active {
            let m = Address::generate(&env);
            let sac =
                token::StellarAssetClient::new(&env, &client.get_token());
            sac.mint(&m, &FEE);
            client.register_member(&m);
            members.push_back(m);
        }

        let asset = client.get_token();
        env.as_contract(&contract_id, || {
            crate::loans::distribute_interest(&env, interest, &asset);
        });

        let per_member = interest / active as i128;
        let mut sum = 0i128;

        for m in members.iter() {
            let pending = client.get_pending_yield(&m);
            prop_assert_eq!(pending, per_member);
            sum += pending;
        }

        prop_assert!(sum <= interest);
        prop_assert_eq!(interest - sum, interest % active as i128);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    /// `calculate_exit_share` is a pro-rata slice of the treasury.
    #[test]
    fn exit_shares_never_exceed_treasury(
        contributions in prop::collection::vec(0i128..=FEE, 1..=5),
        extra_treasury in 0i128..=AMOUNT_BOUND,
    ) {
        let (env, client) = contract_with_treasury(extra_treasury);
        let contract_id = client.address.clone();
        let token_id = client.get_token();

        let mut members = Vec::new(&env);
        for &c in contributions.iter() {
            let m = Address::generate(&env);
            let sac =
                token::StellarAssetClient::new(&env, &token_id);
            sac.mint(&m, &FEE);
            client.register_member(&m);

            env.as_contract(&contract_id, || {
                let mut rec =
                    crate::storage::get_member(&env, &m).unwrap();
                let delta = c - rec.contribution;
                rec.contribution = c;
                crate::storage::set_member(&env, &rec);
                crate::storage::set_active_contributions(
                    &env,
                    crate::storage::get_active_contributions(&env) + delta,
                );
            });

            members.push_back(m);
        }

        let treasury = client.get_treasury_balance();
        let sum: i128 = members
            .iter()
            .map(|m| client.calculate_exit_share(&m))
            .sum();

        prop_assert!(sum <= treasury);
    }

    #[test]
    fn quote_terms_overflow_fallbacks_never_panic_or_wrap(
        amount in prop::strategy::Union::new([
            (1i128..1_000_000i128).boxed(),
            (i128::MAX - 1_000_000..=i128::MAX).boxed(),
        ]),
        treasury in prop::strategy::Union::new([
            (0i128..=10i128).boxed(),
            (AMOUNT_BOUND..=i128::MAX).boxed(),
        ]),
    ) {
        let (_env, client) =
            contract_with_treasury(treasury.min(AMOUNT_BOUND));

        let terms = client.calculate_loan_terms(&amount);
        prop_assert!(terms.total_repayment >= amount);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn distribute_interest_preserves_tokens(
        active in 1u32..=8,
        interest in 0i128..=AMOUNT_BOUND,
    ) {
        let (env, client) = contract_with_treasury(0);
        let contract_id = client.address.clone();

        let mut members = Vec::new(&env);
        for _ in 0..active {
            let member = Address::generate(&env);
            let mint =
                token::StellarAssetClient::new(&env, &client.get_token());
            mint.mint(&member, &FEE);
            client.register_member(&member);
            members.push_back(member);
        }

        let asset = client.get_token();
        env.as_contract(&contract_id, || {
            crate::loans::distribute_interest(&env, interest, &asset);
        });

        let per_member = interest / active as i128;
        let remainder = interest % active as i128;
        let mut paid = 0i128;

        for member in members.iter() {
            let pending = client.get_pending_yield(&member);
            prop_assert_eq!(pending, per_member);
            paid = paid.checked_add(pending).unwrap_or(paid);
        }

        prop_assert!(paid <= interest);
        prop_assert_eq!(interest - paid, remainder);

        let retained = env.as_contract(&contract_id, || {
            crate::storage::get_yield_remainder(&env)
        });
        prop_assert_eq!(retained, remainder);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    #[test]
    fn member_counters_match_member_records_across_join_exit_rejoin(
        joins in prop::collection::vec(
            prop::bool::ANY,
            1..=12,
        ),
    ) {
        let s = setup(0);

        for rejoin in joins {
            let member = Address::generate(&s.env);
            let mint = token::StellarAssetClient::new(
                &s.env,
                &s.client.get_token(),
            );
            mint.mint(&member, &MINT);

            s.client.register_member(&member);
            prop_assert_eq!(s.client.get_active_members(), 1);
            prop_assert_eq!(s.client.get_total_members(), 1);

            s.client.exit_dao(&member);
            prop_assert_eq!(s.client.get_active_members(), 0);

            if rejoin {
                mint.mint(&member, &FEE);
                s.client.register_member(&member);
                prop_assert_eq!(s.client.get_active_members(), 1);
                prop_assert_eq!(s.client.get_total_members(), 1);

                s.env.as_contract(&s.client.address, || {
                    let record =
                        crate::storage::get_member(&s.env, &member)
                            .unwrap();
                    prop_assert_eq!(
                        record.status,
                        MemberStatus::ActiveMember,
                    );
                });
            }
        }

        s.env.as_contract(&s.client.address, || {
            let addresses =
                crate::storage::get_members(&s.env);

            let mut active_count = 0u32;
            let mut record_count = 0u32;

            for address in addresses.iter() {
                if let Some(record) =
                    crate::storage::get_member(&s.env, &address)
                {
                    record_count += 1;
                    if record.status == MemberStatus::ActiveMember {
                        active_count += 1;
                    }
                }
            }

            prop_assert_eq!(
                crate::storage::get_total_members(&s.env),
                record_count,
            );
            prop_assert_eq!(
                crate::storage::get_active_members(&s.env),
                active_count,
            );
        });
    }
}
