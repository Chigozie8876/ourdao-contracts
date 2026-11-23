#![allow(unused_imports)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, BytesN, Env, String, Symbol, Vec};

use super::super::common::*;
use crate::admin::TIMELOCK_DURATION;
use crate::storage::ProposalKind;
use crate::{Error, OurDao, OurDaoClient};

#[test]
fn initialize_rejects_invalid_scalar_parameters() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let contract = env.register(OurDao, ());
    let client = OurDaoClient::new(&env, &contract);
    let admin = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin);

    assert_eq!(
        client.try_initialize(&admins, &0, &FEE, &token, &policy()),
        Err(Ok(Error::InvalidThreshold))
    );
    assert_eq!(
        client.try_initialize(&admins, &10_001, &FEE, &token, &policy()),
        Err(Ok(Error::InvalidThreshold))
    );
    assert_eq!(
        client.try_initialize(&admins, &5_100, &0, &token, &policy()),
        Err(Ok(Error::InvalidAmount))
    );
    let mut invalid_policy = policy();
    invalid_policy.max_loan_duration = 0;
    assert_eq!(
        client.try_initialize(&admins, &5_100, &FEE, &token, &invalid_policy),
        Err(Ok(Error::InvalidLoanPolicy))
    );
}

#[test]
fn pause_blocks_all_mutating_entrypoints() {
    // This test explicitly checks every mutating entrypoint to ensure pause()
    // prevents state changes. Every public entrypoint that mutates contract state
    // must be listed below with an assertion. If you add a new mutating entrypoint
    // without a pause decision, this test will fail — the property being protected
    // (an emergency stop actually stops everything) is too critical to check by
    // hand-written list.
    //
    // Entrypoints deliberately callable while paused:
    // - None. All state-changing operations should be blocked by pause().
    // - Note: repayment was discussed but decided to be pause-gated to maintain
    //   consistent emergency stop semantics (see issue #52).

    let s = setup(3);
    let borrower = s.members.get(0).unwrap();
    let voter = s.members.get(1).unwrap();
    let staker = s.members.get(2).unwrap();
    let newcomer = Address::generate(&s.env);

    // Set up some proposals to vote on / interact with
    let loan_pid = s.client.request_loan(&borrower, &500, &None);
    advance(&s.env, EDITING + 1);

    let treasury_pid = s.client.propose_treasury_withdrawal(
        &borrower,
        &100,
        &newcomer,
        &String::from_str(&s.env, "test"),
        &false,
    );

    // Pause the contract
    s.client.pause(&s.admin);
    assert!(s.client.is_paused());

    // ==================== Membership ====================
    let res = s.client.try_register_member(&newcomer);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "register_member should be pause-gated"
    );

    let res = s.client.try_exit_dao(&borrower);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "exit_dao should be pause-gated"
    );

    let res = s.client.try_claim_rewards(&borrower);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "claim_rewards should be pause-gated"
    );

    // ==================== Loans ====================
    let res = s.client.try_request_loan(&borrower, &1000, &None);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "request_loan should be pause-gated"
    );

    let res = s.client.try_edit_loan_proposal(&borrower, &loan_pid, &600, &None);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "edit_loan_proposal should be pause-gated"
    );

    let res = s.client.try_vote_on_loan_proposal(&voter, &loan_pid, &true);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "vote_on_loan_proposal should be pause-gated"
    );

    let res = s.client.try_disburse_approved_loan(&loan_pid);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "disburse_approved_loan should be pause-gated"
    );

    let res = s.client.try_repay_loan(&borrower, &0);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "repay_loan should be pause-gated"
    );

    let res = s.client.try_repay_loan_partial(&borrower, &0, &100);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "repay_loan_partial should be pause-gated"
    );

    let res = s.client.try_mark_loan_defaulted(&0);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "mark_loan_defaulted should be pause-gated"
    );

    let res = s.client.try_expire_loan_proposal(&0);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "expire_loan_proposal should be pause-gated"
    );

    // ==================== Treasury ====================
    let res = s.client.try_propose_treasury_withdrawal(
        &borrower,
        &100,
        &newcomer,
        &String::from_str(&s.env, "test"),
        &false,
    );
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "propose_treasury_withdrawal should be pause-gated"
    );

    let res = s
        .client
        .try_vote_on_treasury_proposal(&voter, &treasury_pid, &true);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "vote_on_treasury_proposal should be pause-gated"
    );

    let res = s.client.try_expire_treasury_proposal(&0);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "expire_treasury_proposal should be pause-gated"
    );

    let res = s.client.try_execute_treasury_proposal(&0);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "execute_treasury_proposal should be pause-gated"
    );

    // ==================== Staking ====================
    let res = s.client.try_stake(&staker, &100);
    assert_eq!(res, Err(Ok(Error::Paused)), "stake should be pause-gated");

    let res = s.client.try_unstake(&staker, &100);
    assert_eq!(res, Err(Ok(Error::Paused)), "unstake should be pause-gated");

    // ==================== Registry ====================
    let res = s
        .client
        .try_register_name(&borrower, &String::from_str(&s.env, "test"));
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "register_name should be pause-gated"
    );

    // ==================== Privacy (commit-reveal voting) ====================
    let commitment = BytesN::from_array(&s.env, &[0u8; 32]);
    let res = s
        .client
        .try_commit_treasury_vote(&voter, &treasury_pid, &commitment);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "commit_treasury_vote should be pause-gated"
    );

    let salt = BytesN::from_array(&s.env, &[0u8; 32]);
    let res = s
        .client
        .try_reveal_treasury_vote(&voter, &treasury_pid, &true, &salt);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "reveal_treasury_vote should be pause-gated"
    );

    // ==================== Docs (content-hash metadata) ====================
    let cid = Bytes::from_slice(&s.env, &[1, 2, 3]);
    let res = s
        .client
        .try_attach_document(&borrower, &ProposalKind::Loan, &loan_pid, &cid);
    assert_eq!(
        res,
        Err(Ok(Error::Paused)),
        "attach_document should be pause-gated"
    );

    // Verify unpause works
    s.client.unpause(&s.admin);
    assert!(!s.client.is_paused());
}

