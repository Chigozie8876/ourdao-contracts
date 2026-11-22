#![allow(unused_imports)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, BytesN, Env, String, Symbol, Vec};

use super::super::common::*;
use crate::admin::TIMELOCK_DURATION;
use crate::storage::ProposalKind;
use crate::{Error, OurDao, OurDaoClient};

#[test]
fn initialize_rejects_duplicate_admins() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let contract_id = env.register(OurDao, ());
    let client = OurDaoClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin.clone());

    let res = client.try_initialize(&admins, &5_100u32, &FEE, &token_id, &policy());
    assert_eq!(res, Err(Ok(Error::AlreadyAdmin)));
}
#[test]
fn pause_blocks_all_mutating_entrypoints_except_repay() {
    // REVIEW GUARD: adding a public mutating entrypoint to lib.rs requires adding
    // it to this table (and choosing Paused or ExplicitlyAllowed).
    struct EntryPoint<'a> {
        name: &'a str,
        blocked: bool,
    }

    let s = setup(3);
    let borrower = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    let staker = s.members.get(2).unwrap();
    let newcomer = Address::generate(&s.env);

    let loan_id = s.client.request_loan(&borrower, &500, &None);
    advance(&s.env, EDITING + 1);

    let treasury_id = s
        .client
        .propose_treasury_withdrawal(
            &borrower,
            &100,
            &newcomer,
            &String::from_str(&s.env, "test"),
            &false,
        );

    s.client.pause(&s.admin);

    let cases = std::vec![
        EntryPoint {
            name: "initialize",
            blocked: matches!(
                s.client.try_initialize(
                    &Vec::from_array(&s.env, [Address::generate(&s.env)]),
                    &5_100u32,
                    &FEE,
                    &s.client.get_token(),
                    &policy(),
                ),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "add_admin",
            blocked: matches!(
                s.client.try_add_admin(&s.admin, &Address::generate(&s.env)),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "remove_admin",
            blocked: matches!(
                s.client.try_remove_admin(&s.admin, &s.admin),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "set_pauser",
            blocked: matches!(
                s.client.try_set_pauser(&s.admin, &Address::generate(&s.env)),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "revoke_pauser",
            blocked: matches!(
                s.client.try_revoke_pauser(&s.admin),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "set_consensus_threshold",
            blocked: matches!(
                s.client.try_set_consensus_threshold(&s.admin, &6_000),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "propose_policy_update",
            blocked: matches!(
                s.client.try_propose_policy_update(&s.admin, &policy()),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "execute_policy_update",
            blocked: matches!(
                s.client.try_execute_policy_update(&s.admin),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "cancel_policy_update",
            blocked: matches!(
                s.client.try_cancel_policy_update(&s.admin),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "set_loan_policy",
            blocked: matches!(
                s.client.try_set_loan_policy(&s.admin, &policy()),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "pause",
            blocked: matches!(
                s.client.try_pause(&s.admin),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "unpause",
            blocked: matches!(
                s.client.try_unpause(&s.admin),
                Err(Ok(_))
            ),
        },
        EntryPoint {
            name: "register_member",
            blocked: matches!(
                s.client.try_register_member(&newcomer),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "exit_dao",
            blocked: matches!(
                s.client.try_exit_dao(&borrower),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "claim_rewards",
            blocked: matches!(
                s.client.try_claim_rewards(&borrower),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "claim_rewards_in_asset",
            blocked: matches!(
                s.client.try_claim_rewards_in_asset(&borrower, &s.token.address),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "delegate_vote",
            blocked: matches!(
                s.client.try_delegate_vote(&borrower, &voter),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "request_loan",
            blocked: matches!(
                s.client.try_request_loan(&borrower, &100),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "request_loan_in_asset",
            blocked: matches!(
                s.client.try_request_loan_in_asset(
                    &borrower,
                    &100,
                    &s.client.get_token(),
                    &None,
                ),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "edit_loan_proposal",
            blocked: matches!(
                s.client.try_edit_loan_proposal(&borrower, &loan_id, &600, &None),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "cancel_loan_proposal",
            blocked: matches!(
                s.client.try_cancel_loan_proposal(&borrower, &loan_id),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "vote_on_loan_proposal",
            blocked: matches!(
                s.client.try_vote_on_loan_proposal(&voter, &loan_id, &true),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "vote_batch",
            blocked: matches!(
                s.client.try_vote_batch(
                    &voter,
                    &Vec::from_array(
                        &s.env,
                        [crate::types::ProposalVote {
                            proposal_id: loan_id,
                            vote: true,
                        }],
                    ),
                ),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "disburse_approved_loan",
            blocked: matches!(
                s.client.try_disburse_approved_loan(&loan_id),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "repay_loan",
            blocked: false,
        },
        EntryPoint {
            name: "repay_loan_in_asset",
            blocked: false,
        },
        EntryPoint {
            name: "repay_loan_partial",
            blocked: false,
        },
        EntryPoint {
            name: "mark_loan_defaulted",
            blocked: matches!(
                s.client.try_mark_loan_defaulted(&loan_id),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "expire_loan_proposal",
            blocked: matches!(
                s.client.try_expire_loan_proposal(&loan_id),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "propose_treasury_withdrawal",
            blocked: matches!(
                s.client.try_propose_treasury_withdrawal(
                    &borrower,
                    &100,
                    &newcomer,
                    &String::from_str(&s.env, "test"),
                    &false,
                ),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "propose_withdrawal_in_asset",
            blocked: matches!(
                s.client.try_propose_withdrawal_in_asset(
                    &borrower,
                    &100,
                    &s.client.get_token(),
                    &newcomer,
                    &String::from_str(&s.env, "test"),
                    &false,
                ),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "vote_on_treasury_proposal",
            blocked: matches!(
                s.client.try_vote_on_treasury_proposal(&voter, &treasury_id, &true),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "cancel_treasury_proposal",
            blocked: matches!(
                s.client.try_cancel_treasury_proposal(&borrower, &treasury_id),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "expire_treasury_proposal",
            blocked: matches!(
                s.client.try_expire_treasury_proposal(&treasury_id),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "execute_treasury_proposal",
            blocked: matches!(
                s.client.try_execute_treasury_proposal(&treasury_id),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "stake",
            blocked: matches!(
                s.client.try_stake(&staker, &100),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "unstake",
            blocked: matches!(
                s.client.try_unstake(&staker, &100),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "register_name",
            blocked: matches!(
                s.client.try_register_name(
                    &borrower,
                    &String::from_str(&s.env, "member"),
                ),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "commit_treasury_vote",
            blocked: matches!(
                s.client.try_commit_treasury_vote(
                    &voter,
                    &treasury_id,
                    &BytesN::from_array(&s.env, &[0u8; 32]),
                ),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "reveal_treasury_vote",
            blocked: matches!(
                s.client.try_reveal_treasury_vote(
                    &voter,
                    &treasury_id,
                    &true,
                    &BytesN::from_array(&s.env, &[0u8; 32]),
                ),
                Err(Ok(Error::Paused))
            ),
        },
        EntryPoint {
            name: "attach_document",
            blocked: matches!(
                s.client.try_attach_document(
                    &borrower,
                    &ProposalKind::Loan,
                    &loan_id,
                    &Bytes::from_slice(&s.env, &[1, 2, 3]),
                ),
                Err(Ok(Error::Paused))
            ),
        },
    ];

    for case in cases {
        if case.name.starts_with("repay_") {
            assert!(!case.blocked, "{} must remain callable while paused", case.name);
        } else {
            assert!(case.blocked, "{} must be pause-gated", case.name);
        }
    }

    s.client.unpause(&s.admin);
    assert!(!s.client.is_paused());
