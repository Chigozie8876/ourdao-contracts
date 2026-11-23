extern crate std;

use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::xdr::{ContractEventBody, ScVal};
use soroban_sdk::{Address, Env, String, Vec};

use super::common::*;
use crate::types::{LoanStatus, ProposalStatus, TreasuryAccounting};
use crate::{Error, Loan, OurDao, OurDaoClient};

#[test]
fn treasury_withdrawal_open_vote() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let dest = Address::generate(&s.env);

    let reason = String::from_str(&s.env, "grant");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &600, &dest, &reason, &false);

    s.client.vote_on_treasury_proposal(&v1, &pid, &true);
    let mid = s.client.get_treasury_proposal(&pid).unwrap();
    assert_eq!(mid.status, ProposalStatus::Pending); // 1 vote not enough (needs 2)

    s.client.vote_on_treasury_proposal(&v2, &pid, &true);
    let done = s.client.get_treasury_proposal(&pid).unwrap();
    assert_eq!(done.status, ProposalStatus::Executed);
    assert_eq!(s.token.balance(&dest), 600);
}

#[test]
fn approved_but_unfundable_treasury_withdrawal_waits_then_executes_after_refill() {
    // 4 members => treasury 4000, 3 of 4 votes required.
    let s = setup(4);
    let p = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");

    // Two withdrawals that each fit today but not both together.
    let first = s
        .client
        .propose_treasury_withdrawal(&p, &2_000, &dest, &reason, &false);
    let second = s
        .client
        .propose_treasury_withdrawal(&p, &2_500, &dest, &reason, &false);

    for voter in [&p, &v1, &v2] {
        s.client.vote_on_treasury_proposal(voter, &first, &true);
    }
    assert_eq!(s.client.get_treasury_balance(), 2_000);
    assert_eq!(s.token.balance(&dest), 2_000);

    s.client.vote_on_treasury_proposal(&p, &second, &true);
    s.client.vote_on_treasury_proposal(&v1, &second, &true);
    s.client.vote_on_treasury_proposal(&v2, &second, &true); // approved, but only 2000 left
    assert!(emitted(&s.env, "tre_wait"));

    let prop = s.client.get_treasury_proposal(&second).unwrap();
    assert_eq!(prop.status, ProposalStatus::ApprovedPendingDisbursement);
    assert_eq!(s.token.balance(&dest), 2_000); // nothing more moved
    assert_eq!(s.client.get_treasury_balance(), 2_000);
    assert_eq!(
        s.client.try_execute_treasury_proposal(&second),
        Err(Ok(Error::InsufficientTreasury))
    );

    refill_treasury(&s);
    refill_treasury(&s);
    s.client.execute_treasury_proposal(&second);

    let prop = s.client.get_treasury_proposal(&second).unwrap();
    assert_eq!(prop.status, ProposalStatus::Executed);
    assert_eq!(s.token.balance(&dest), 4_500);
}

#[test]
fn rejected_treasury_transfer_rolls_back_approval_vote_and_execution_state() {
    let s = rejecting_setup(3);
    let proposer = s.members.get(0).unwrap();
    let voter_one = s.members.get(1).unwrap();
    let voter_two = s.members.get(2).unwrap();
    let destination = Address::generate(&s.env);

    let proposal_id = s.client.propose_treasury_withdrawal(
        &proposer,
        &500,
        &destination,
        &String::from_str(&s.env, "rollback test"),
        &false,
    );

    s.client
        .vote_on_treasury_proposal(&voter_one, &proposal_id, &true);
    let before = s.client.get_treasury_proposal(&proposal_id).unwrap();
    assert_eq!(before.status, ProposalStatus::Pending);
    assert_eq!(before.for_votes, 1);
    assert_eq!(before.votes_cast, 1);

    s.token.set_reject_transfers(&true);
    let result = s
        .client
        .try_vote_on_treasury_proposal(&voter_two, &proposal_id, &true);
    assert!(result.is_err());

    let after = s.client.get_treasury_proposal(&proposal_id).unwrap();
    assert_eq!(after, before, "proposal changes must roll back");
    let has_vote = s.env.as_contract(&s.client.address, || {
        crate::storage::has_treasury_voted(&s.env, proposal_id, &voter_two)
    });
    assert!(
        !has_vote,
        "vote marker must not survive a rejected execution transfer"
    );
    assert_eq!(s.token.balance(&destination), 0);
}

// Issue #172: Treasury withdrawal must account for reserved loan commitments
#[test]
fn treasury_withdrawal_blocked_by_approved_pending_loan() {
    // 4 members => treasury 4000.
    let s = setup(4);
    let borrower = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let v3 = s.members.get(3).unwrap();
    let dest = Address::generate(&s.env);

    // Drain treasury so it can't cover the loan immediately, forcing it into
    // ApprovedPendingDisbursement when votes pass.
    let drain_dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "drain");
    let drain_id = s
        .client
        .propose_treasury_withdrawal(&v1, &2_500, &drain_dest, &reason, &false);
    s.client.vote_on_treasury_proposal(&v1, &drain_id, &true);
    s.client.vote_on_treasury_proposal(&v2, &drain_id, &true);
    s.client.vote_on_treasury_proposal(&v3, &drain_id, &true);
    // treasury is now 1500

    // Refill so the loan request passes the treasury-ratio check.
    refill_treasury(&s); // +1000 => treasury 2500
    let pid = s.client.request_loan(&borrower, &1_000, &None);
    advance(&s.env, EDITING + 1);

    // Drain again so the loan approval parks in ApprovedPendingDisbursement.
    let drain2_dest = Address::generate(&s.env);
    let drain2_id =
        s.client
            .propose_treasury_withdrawal(&v1, &2_000, &drain2_dest, &reason, &false);
    s.client.vote_on_treasury_proposal(&v1, &drain2_id, &true);
    s.client.vote_on_treasury_proposal(&v2, &drain2_id, &true);
    s.client.vote_on_treasury_proposal(&v3, &drain2_id, &true);
    // treasury is now 500

    s.client.vote_on_loan_proposal(&v1, &pid, &true);
    s.client.vote_on_loan_proposal(&v2, &pid, &true);
    s.client.vote_on_loan_proposal(&v3, &pid, &true);
    let prop = s.client.get_loan_proposal(&pid).unwrap();
    assert_eq!(prop.status, ProposalStatus::ApprovedPendingDisbursement);

    // treasury=500, committed=1000 => available=-500: any withdrawal is rejected.
    let reason2 = String::from_str(&s.env, "withdrawal attempt");
    let blocked = s
        .client
        .try_propose_treasury_withdrawal(&v1, &1, &dest, &reason2, &false);
    assert_eq!(blocked, Err(Ok(Error::InsufficientTreasury)));

    // Refill enough to cover the committed loan with room to spare.
    refill_treasury(&s); // +1000 => treasury 1500, committed 1000, available 500
    refill_treasury(&s); // +1000 => treasury 2500, committed 1000, available 1500

    // A withdrawal within available amount succeeds.
    let ok_pid = s
        .client
        .propose_treasury_withdrawal(&v1, &500, &dest, &reason2, &false);
    assert!(s.client.get_treasury_proposal(&ok_pid).is_some());

    // A withdrawal that would eat into the committed amount is still rejected.
    let too_much = s
        .client
        .try_propose_treasury_withdrawal(&v2, &1_600, &dest, &reason2, &false);
    assert_eq!(too_much, Err(Ok(Error::InsufficientTreasury)));
}

// ==================== issue #236: withdrawals in a second asset ====================

#[test]
fn withdrawal_in_second_asset_pays_out_that_asset() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");
    let asset = whitelist_second_asset(&s, 5_000);
    let base_treasury = s.client.get_treasury_balance();

    // More than the base treasury holds, but covered by the second asset.
    let pid = s.client.propose_withdrawal_in_asset(
        &proposer,
        &4_000,
        &asset.address,
        &dest,
        &reason,
        &false,
    );
    s.client.vote_on_treasury_proposal(&v1, &pid, &true);
    s.client.vote_on_treasury_proposal(&v2, &pid, &true);

    let done = s.client.get_treasury_proposal(&pid).unwrap();
    assert_eq!(done.status, ProposalStatus::Executed);
    assert_eq!(asset.balance(&dest), 4_000);
    assert_eq!(s.token.balance(&dest), 0);
    assert_eq!(
        s.client.get_treasury_balance_in_asset(&asset.address),
        1_000
    );
    assert_eq!(s.client.get_treasury_balance(), base_treasury);
}

#[test]
fn withdrawal_in_asset_rejects_unlisted_or_unfunded_asset() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");
    let issuer = Address::generate(&s.env);
    let unknown = s.env.register_stellar_asset_contract_v2(issuer).address();

    let res = s
        .client
        .try_propose_withdrawal_in_asset(&proposer, &100, &unknown, &dest, &reason, &false);
    assert_eq!(res, Err(Ok(Error::TokenNotWhitelisted)));

    let asset = whitelist_second_asset(&s, 500);
    let res = s.client.try_propose_withdrawal_in_asset(
        &proposer,
        &501,
        &asset.address,
        &dest,
        &reason,
        &false,
    );
    assert_eq!(res, Err(Ok(Error::InsufficientTreasury)));
}

// ==================== issue #185: cancel during editing (migrated from test.rs) ====================

#[test]
fn cancel_treasury_proposal_emits_event_and_sets_cancelled_status() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");

    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &600, &dest, &reason, &false);
    let prop = s.client.get_treasury_proposal(&pid).unwrap();
    assert_eq!(prop.status, ProposalStatus::Pending);

    s.client.cancel_treasury_proposal(&proposer, &pid);
    // Events only cover the latest invocation, so check before reading state.
    assert_prop_cancelled_event(&s.env, pid);

    let prop = s.client.get_treasury_proposal(&pid).unwrap();
    assert_eq!(prop.status, ProposalStatus::Cancelled);
}

// `tally` writes the proposal once, after execution is attempted (#357): the
// final status and vote counts must still be persisted on both outcomes.
#[test]
fn tally_persists_status_and_votes_whether_executed_or_waiting() {
    let s = setup(4); // treasury 4000, 3 of 4 votes required
    let p = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");

    let paid = s
        .client
        .propose_treasury_withdrawal(&p, &2_000, &dest, &reason, &false);
    let parked = s
        .client
        .propose_treasury_withdrawal(&p, &2_500, &dest, &reason, &false);
    for voter in [&p, &v1, &v2] {
        s.client.vote_on_treasury_proposal(voter, &paid, &true);
        s.client.vote_on_treasury_proposal(voter, &parked, &true);
    }

    let executed = s.client.get_treasury_proposal(&paid).unwrap();
    assert_eq!(executed.status, ProposalStatus::Executed);
    assert_eq!((executed.for_votes, executed.votes_cast), (3, 3));

    let waiting = s.client.get_treasury_proposal(&parked).unwrap();
    assert_eq!(waiting.status, ProposalStatus::ApprovedPendingDisbursement);
    assert_eq!((waiting.for_votes, waiting.votes_cast), (3, 3));
    assert_eq!(s.token.balance(&dest), 2_000);
}

#[test]
fn propose_treasury_withdrawal_rejects_self_destination() {
    let s = setup(2);
    let proposer = s.members.get(0).unwrap();
    let self_dest = s.client.address.clone();
    let reason = String::from_str(&s.env, "grant to self");

    let res = s
        .client
        .try_propose_treasury_withdrawal(&proposer, &500, &self_dest, &reason, &false);
    assert_eq!(res, Err(Ok(Error::InvalidProposal)));

    let res_asset = s.client.try_propose_withdrawal_in_asset(
        &proposer,
        &500,
        &s.token.address,
        &self_dest,
        &reason,
        &false,
    );
    assert_eq!(res_asset, Err(Ok(Error::InvalidProposal)));
}

// ==================== issue #285: single-token withdrawal entrypoint ====================

/// Propose → vote → vote (executes) through one entrypoint family, observing
/// after every call.
fn run_withdrawal_path(path: EntryPath) -> std::vec::Vec<Observation> {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");
    let mut steps = std::vec::Vec::new();

    let pid = match path {
        EntryPath::Legacy => s
            .client
            .propose_treasury_withdrawal(&proposer, &600, &dest, &reason, &false),
        EntryPath::InBaseAsset => s.client.propose_withdrawal_in_asset(
            &proposer,
            &600,
            &s.token.address,
            &dest,
            &reason,
            &false,
        ),
    };
    steps.push(observe(&s.env));
    s.client.vote_on_treasury_proposal(&v1, &pid, &true);
    steps.push(observe(&s.env));
    s.client.vote_on_treasury_proposal(&v2, &pid, &true);
    steps.push(observe(&s.env));
    assert_eq!(s.token.balance(&dest), 600);
    steps
}

#[test]
fn single_token_withdrawal_entrypoint_matches_base_asset_counterpart() {
    let legacy = run_withdrawal_path(EntryPath::Legacy);
    let in_base = run_withdrawal_path(EntryPath::InBaseAsset);

    // Every step did real work, so the comparison below isn't vacuous.
    let topics: std::vec::Vec<_> = legacy.iter().map(|o| event_topics(&o.0)).collect();
    for (step, expected) in ["tre_prop", "tre_vote", "tre_exec"].iter().enumerate() {
        assert!(
            topics[step].iter().any(|t| t == expected),
            "step {step} emitted {:?}, expected {expected}",
            topics[step]
        );
    }
    // Naming the base token records no proposal asset, like the legacy path.
    assert!(!topics[0].iter().any(|t| t == "tre_asset"));

    assert_runs_identical(&legacy, &in_base);
}

// ==================== issue #288: treasury reconciliation view ====================

#[test]
fn treasury_accounting_matches_manual_calculation() {
    let s = setup(3);
    let staker = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let base = s.token.address.clone();
    let accounting = |asset: &Address| s.client.get_treasury_accounting(asset);

    // Base token: the balance is what the token reports, and member stake is
    // reported apart from it. 50 is below one weight unit, so the votes
    // below are unaffected.
    s.client.stake(&staker, &50);
    let held = s.token.balance(&s.client.address);
    let acct = accounting(&base);
    assert_eq!(
        acct,
        TreasuryAccounting {
            token_balance: held,
            staked: 50,
            outstanding_principal: 0,
        }
    );
    assert_eq!(
        acct.token_balance - acct.staked,
        s.client.get_treasury_balance()
    );

    // A base-token loan moves its principal out of the balance into
    // outstanding, and full repayment brings it back with interest.
    let pid = s.client.request_loan(&v2, &1_000, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&staker, &pid, &true);
    s.client.vote_on_loan_proposal(&v1, &pid, &true);
    let loan = s.client.get_loan(&pid).unwrap();
    assert_eq!(accounting(&base).token_balance, held - 1_000);
    assert_eq!(accounting(&base).outstanding_principal, 1_000);
    s.client.repay_loan(&v2, &pid);
    assert_eq!(
        accounting(&base),
        TreasuryAccounting {
            token_balance: held - loan.principal + loan.total_repayment,
            staked: 50,
            outstanding_principal: 0,
        }
    );
    let base_after = accounting(&base);

    // Asset B: nothing staked, nothing lent yet.
    let b = whitelist_second_asset(&s, 10_000);
    assert_eq!(
        accounting(&b.address),
        TreasuryAccounting {
            token_balance: 10_000,
            staked: 0,
            outstanding_principal: 0,
        }
    );

    let pid = s
        .client
        .request_loan_in_asset(&v1, &4_000, &b.address, &None);
    advance(&s.env, EDITING + 1);
    s.client.vote_on_loan_proposal(&staker, &pid, &true);
    s.client.vote_on_loan_proposal(&v2, &pid, &true);
    let loan = s.client.get_loan(&pid).unwrap();
    let interest = loan.total_repayment - loan.principal;
    assert_eq!(
        accounting(&b.address),
        TreasuryAccounting {
            token_balance: 6_000,
            staked: 0,
            outstanding_principal: 4_000,
        }
    );

    // Partial repayment: only the part beyond the interest repays principal.
    mint_asset(&s, &b.address, &v1, interest);
    s.client.repay_loan_partial(&v1, &pid, &(interest + 1_500));
    assert_eq!(
        accounting(&b.address),
        TreasuryAccounting {
            token_balance: 6_000 + interest + 1_500,
            staked: 0,
            outstanding_principal: 2_500,
        }
    );

    // A direct transfer raises the balance without any accounted flow: the
    // drift this view exists to expose.
    mint_asset(&s, &b.address, &s.client.address, 777);
    assert_eq!(
        accounting(&b.address).token_balance,
        6_000 + interest + 1_500 + 777
    );
    assert_eq!(accounting(&b.address).outstanding_principal, 2_500);

    // Overdue: the penalty (20% of the 2_500 still owed = 500) is booked as
    // principal, then this all-principal payment of 1_000 reduces it.
    advance(&s.env, LOAN_DURATION + 1);
    s.client.repay_loan_partial(&v1, &pid, &1_000);
    assert_eq!(
        accounting(&b.address).outstanding_principal,
        2_500 + 500 - 1_000
    );

    // Default writes off whatever principal is still owed.
    s.client.mark_loan_defaulted(&pid);
    assert_eq!(
        accounting(&b.address),
        TreasuryAccounting {
            token_balance: 6_000 + interest + 1_500 + 777 + 1_000,
            staked: 0,
            outstanding_principal: 0,
        }
    );
    // None of this touched the base token.
    assert_eq!(accounting(&base), base_after);
}

#[test]
fn treasury_accounting_requires_initialization() {
    let env = Env::default();
    let client = OurDaoClient::new(&env, &env.register(OurDao, ()));
    let asset = Address::generate(&env);
    assert_eq!(
        client.try_get_treasury_accounting(&asset),
        Err(Ok(Error::NotInitialized))
    );
}

#[test]
fn seeded_loans_count_towards_outstanding_principal() {
    let s = setup(1);
    let borrower = s.members.get(0).unwrap();
    let base = s.token.address.clone();
    let now = s.env.ledger().timestamp();

    // 100 already repaid: the 50 of interest, then 50 of principal.
    let active = Loan {
        id: 7,
        borrower: borrower.clone(),
        principal: 1_000,
        interest_rate: 500,
        total_repayment: 1_050,
        start_time: now,
        due_time: now + LOAN_DURATION,
        status: LoanStatus::Active,
        amount_repaid: 100,
        default_grace_period: 0,
        default_penalty_bps: 0,
    };
    let mut batch = Vec::new(&s.env);
    batch.push_back(active.clone());
    s.client.seed_loans(&s.admin, &batch);
    let outstanding = || {
        s.client
            .get_treasury_accounting(&base)
            .outstanding_principal
    };
    assert_eq!(outstanding(), 950);

    // Re-seeding an id replaces what it contributed instead of adding to it,
    // and a loan that is no longer active contributes nothing.
    let mut repaid = active.clone();
    repaid.id = 8;
    repaid.status = LoanStatus::Repaid;
    repaid.amount_repaid = repaid.total_repayment;
    batch.push_back(repaid);
    s.client.seed_loans(&s.admin, &batch);
    assert_eq!(outstanding(), 950);

    // Repaying the seeded loan settles it exactly.
    s.client.repay_loan(&borrower, &7);
    assert_eq!(outstanding(), 0);
}

// Issue #279: an approved treasury proposal remains bound to its original asset.
#[test]
fn treasury_execution_uses_proposal_asset_after_whitelist_removal() {
    let s = setup(3);
    let proposer = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "asset-specific grant");
    let asset = whitelist_second_asset(&s, 5_000);
    let base_before = s.token.balance(&dest);

    let pid = s.client.propose_withdrawal_in_asset(
        &proposer,
        &1_000,
        &asset.address,
        &dest,
        &reason,
        &false,
    );

    // Whitelist membership controls new proposals, not the asset already
    // committed into an existing proposal.
    s.client.remove_token(&s.admin, &asset.address);
    assert!(!s.client.is_token_whitelisted(&asset.address));

    s.client.vote_on_treasury_proposal(&v1, &pid, &true);
    s.client.vote_on_treasury_proposal(&v2, &pid, &true);

    let done = s.client.get_treasury_proposal(&pid).unwrap();
    assert_eq!(done.status, ProposalStatus::Executed);
    assert_eq!(asset.balance(&dest), 1_000);
    assert_eq!(s.token.balance(&dest), base_before);
}

// ==================== issues #477 and #478: expiry observability and idempotency ====================

fn count_topic(env: &Env, name: &str) -> usize {
    let events = env.events().all().events().to_vec();
    event_topics(&events)
        .iter()
        .filter(|topic| topic.as_str() == name)
        .count()
}

#[test]
fn expire_treasury_proposal_is_idempotent_and_emits_tre_exp_once() {
    let s = setup(2);
    let proposer = s.members.get(0).unwrap();
    let destination = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "expiring grant");
    let pid = s
        .client
        .propose_treasury_withdrawal(&proposer, &300, &destination, &reason, &false);

    advance(&s.env, VOTING_PERIOD + 1);
    s.client.expire_treasury_proposal(&pid);

    assert_eq!(count_topic(&s.env, "tre_exp"), 1);
    let events = s.env.events().all().events().to_vec();
    let event = events
        .iter()
        .rev()
        .find(|event| {
            let ContractEventBody::V0(body) = &event.body;
            matches!(body.topics.first(), Some(ScVal::Symbol(sym)) if sym.0.to_utf8_string_lossy() == "tre_exp")
        })
        .expect("tre_exp event not found");
    let ContractEventBody::V0(body) = &event.body;
    assert_eq!(body.topics.len(), 1);
    let ScVal::Vec(Some(data)) = &body.data else {
        panic!("unexpected tre_exp data: {:?}", body.data);
    };
    assert_eq!(data.len(), 2);
    assert_eq!(data[0], ScVal::U32(pid));
    assert_eq!(data[1], ScVal::Address(proposer.into()));

    s.client.expire_treasury_proposal(&pid);
    assert_eq!(count_topic(&s.env, "tre_exp"), 0);
    assert_eq!(
        s.client.get_treasury_proposal(&pid).unwrap().status,
        ProposalStatus::Expired,
    );
}

// #71: `execute_treasury_proposal` is a deliberate permissionless keeper call.
// This pins that intent: a non-member with no auth at all can drive it, so
// adding a `require_auth` to the entrypoint makes this test fail.
#[test]
fn non_member_can_execute_approved_treasury_proposal_without_auth() {
    // 4 members => treasury 4000, 3 of 4 votes required.
    let s = setup(4);
    let p = s.members.get(0).unwrap();
    let v1 = s.members.get(1).unwrap();
    let v2 = s.members.get(2).unwrap();
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");

    let first = s
        .client
        .propose_treasury_withdrawal(&p, &2_000, &dest, &reason, &false);
    let second = s
        .client
        .propose_treasury_withdrawal(&p, &2_500, &dest, &reason, &false);
    for voter in [&p, &v1, &v2] {
        s.client.vote_on_treasury_proposal(voter, &first, &true);
    }
    for voter in [&p, &v1, &v2] {
        s.client.vote_on_treasury_proposal(voter, &second, &true);
    }
    assert_eq!(
        s.client.get_treasury_proposal(&second).unwrap().status,
        ProposalStatus::ApprovedPendingDisbursement
    );
    refill_treasury(&s);
    refill_treasury(&s);

    let keeper = Address::generate(&s.env);
    assert!(s.client.get_member(&keeper).is_none());
    let keeper_bal = s.token.balance(&keeper);
    let dest_bal = s.token.balance(&dest);

    // No mocked auths from here on: any `require_auth` in the path would fail.
    s.env.set_auths(&[]);
    s.client.execute_treasury_proposal(&second);
    assert!(s.env.auths().is_empty());

    assert_eq!(
        s.client.get_treasury_proposal(&second).unwrap().status,
        ProposalStatus::Executed
    );
    // Funds go to the destination fixed on the proposal; the keeper gets nothing.
    assert_eq!(s.token.balance(&dest), dest_bal + 2_500);
    assert_eq!(s.token.balance(&keeper), keeper_bal);

    // Repeated calls are rejected by the status guard: no double payout.
    assert_eq!(
        s.client.try_execute_treasury_proposal(&second),
        Err(Ok(Error::NotInVotingPhase))
    );
    assert_eq!(s.token.balance(&dest), dest_bal + 2_500);
}

// ── #73: quorum is fixed at proposal creation ────────────────────────────────

#[test]
fn members_exiting_mid_vote_do_not_lower_treasury_quorum() {
    // 10 members at creation, 51% => 6 votes required.
    let s = setup(10);
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");
    let tid = s.client.propose_treasury_withdrawal(
        &s.members.get(0).unwrap(),
        &100,
        &dest,
        &reason,
        &false,
    );
    assert_eq!(
        s.client.get_treasury_proposal(&tid).unwrap().active_members,
        10
    );

    for i in 0..5 {
        s.client
            .vote_on_treasury_proposal(&s.members.get(i).unwrap(), &tid, &true);
    }
    s.client.exit_dao(&s.members.get(8).unwrap());
    s.client.exit_dao(&s.members.get(9).unwrap());

    // With a live count of 8 the bar would be 5 and this against-vote would
    // execute the withdrawal on five in-favour votes.
    s.client
        .vote_on_treasury_proposal(&s.members.get(5).unwrap(), &tid, &false);
    assert_eq!(
        s.client.get_treasury_proposal(&tid).unwrap().status,
        ProposalStatus::Pending
    );
    assert_eq!(s.token.balance(&dest), 0);

    s.client
        .vote_on_treasury_proposal(&s.members.get(6).unwrap(), &tid, &true);
    assert_eq!(
        s.client.get_treasury_proposal(&tid).unwrap().status,
        ProposalStatus::Executed
    );
    assert_eq!(s.token.balance(&dest), 100);
}

#[test]
fn members_joining_mid_vote_cannot_vote_on_treasury_proposal() {
    let s = setup(3);
    let dest = Address::generate(&s.env);
    let reason = String::from_str(&s.env, "grant");
    let tid = s.client.propose_treasury_withdrawal(
        &s.members.get(0).unwrap(),
        &100,
        &dest,
        &reason,
        &false,
    );
    s.client
        .vote_on_treasury_proposal(&s.members.get(0).unwrap(), &tid, &true);

    advance(&s.env, 1);
    let late = Address::generate(&s.env);
    soroban_sdk::token::StellarAssetClient::new(&s.env, &s.token.address).mint(&late, &MINT);
    s.client.register_member(&late);
    refill_treasury(&s);
    assert_eq!(s.client.get_active_members(), 5); // a live bar would now be 3

    // The late joiners are outside the electorate (checked in the shared
    // tally path, so commit-reveal reveals get the same rule).
    assert_eq!(
        s.client.try_vote_on_treasury_proposal(&late, &tid, &true),
        Err(Ok(Error::JoinedAfterProposal))
    );

    // The second original vote reaches the snapshotted bar of 2.
    s.client
        .vote_on_treasury_proposal(&s.members.get(1).unwrap(), &tid, &true);
    assert_eq!(
        s.client.get_treasury_proposal(&tid).unwrap().status,
        ProposalStatus::Executed
    );
}
