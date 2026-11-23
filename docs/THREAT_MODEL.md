# Threat Model and Trust Assumptions

Status: testnet-stage, **not externally audited**. This document states what the contract assumes and what it does not defend against. It complements [`SECURITY.md`](../SECURITY.md) (reporting) and [`docs/INVARIANTS.md`](./INVARIANTS.md) (what must always hold).

## Assets

- Treasury funds held by the contract in the DAO token.
- Member stakes and accrued yield.
- Governance state: proposals, votes, loans, membership records.

## Actors and trust

| Actor | Trusted to | Not trusted to |
| --- | --- | --- |
| Admin set | Perform admin-only entrypoints (policy, pause/unpause, migration seeding) and stay online | Move member funds outside the entrypoints' rules |
| Members | Vote and act only for their own address (`require_auth` per call) | Act on another address's behalf |
| Anyone | Call permissionless entrypoints such as `mark_loan_defaulted`, which is a pure function of ledger time and loan state | Anything else |
| DAO token contract | Behave as a standard Soroban token | Nothing beyond that; a malicious token is out of scope, the DAO token is chosen at initialization |
| Off-chain backend and frontend | Index and display state | They are not a source of truth; the contract is |

## Trust assumptions

1. **Admins are honest and available.** Admin keys can pause the contract and set policy within bounds. A compromised admin quorum is the largest single risk. The consensus threshold is set at initialization.
2. **The DAO token is well behaved.** Balance-changing operations follow checks-effects-interactions, but a token that lies about balances or transfers breaks accounting.
3. **Migration seeding is trusted.** A migrated deployment relies on admins seeding state truthfully; see [`docs/MIGRATION.md`](./MIGRATION.md#trust-assumption).
4. **The contract is immutable.** There is no upgrade path, so a bug means migration to a fresh deployment.
5. **Ledger time is the only clock.** Voting periods, expiry and defaults use ledger timestamps.

## Threats considered

| Threat | Mitigation |
| --- | --- |
| Acting on another member's behalf | `require_auth()` on the member address in every state-changing entrypoint |
| Double or miscounted votes | Per-member vote tracking (`has_voted`), proposal phase guards, voting-period expiry |
| Private proposal stalling or reveal manipulation | Commit-reveal phase guards; see the open reveal-deadline and membership-fee issues below |
| Reentrancy | Soroban has no mid-call re-entry; state is still updated before token transfers |
| Rounding drift in yield | `YieldRemainder` carried forward, property tests |
| Unbounded storage / cost growth | Document hash size cap, documented membership ceiling and resource costs (#138) |
| Sybil / vote buying via cheap memberships | See [Sybil and Cheap Membership Attacks](#sybil-and-cheap-membership-attacks) below |

## Sybil and Cheap Membership Attacks

### Attack Mechanics

Every member in the DAO receives a base voting weight of 1 upon paying the one-time `membership_fee` ($F$). When $F$ is set too low relative to treasury assets or available loan sizes, an attacker can register multiple Sybil accounts to purchase voting power and push through malicious loan proposals or treasury withdrawals.

### Cost-to-Attack Formula

Let:
- $M$ = number of existing honest active members
- $S$ = number of Sybil accounts created by the attacker
- $T$ = consensus/quorum threshold in basis points (e.g., $5\,100$ for 51%, $6\,000$ for 60%)
- $F$ = `membership_fee` (in base DAO tokens)
- $W$ = amount extracted via malicious loan or treasury withdrawal

To pass a proposal over honest opposition (all $M$ honest members voting against), the attacker needs their Sybil votes to satisfy:
$$\frac{S}{M + S} \ge \frac{T}{10\,000} \implies S \ge \left\lceil M \cdot \frac{T}{10\,000 - T} \right\rceil$$

For example, at a 51% threshold ($T = 5\,100$), $S \ge \lceil M \cdot \frac{5\,100}{4\,900} \rceil \approx 1.041 \times M$ accounts.

The upfront capital cost to execute the attack is:
$$\text{Cost}_{\text{attack}} = S \times F$$

The attack is economically rational whenever the extractable value exceeds the cost:
$$\text{Net Profit} = W - (S \times F) > 0$$

If $F$ is nominal (e.g., 1 token) and treasury balance or loan cap is large, $\text{Cost}_{\text{attack}} \ll W$, allowing trivial drained funds.

### Recommended Mitigations and Policy Ranges

1. **Adequate Membership Fee ($F$)**:
   - $F$ must scale with the DAO's treasury size.
   - **Recommended range**: Set $F$ such that acquiring $S$ accounts exceeds the maximum single loan limit ($W_{\max} = \text{Treasury} \times \text{max\_loan\_to\_treasury\_ratio}$). Specifically, ensure $F \ge \frac{W_{\max}}{S_{\min}}$.
2. **Minimum Membership Duration (`min_membership_duration`)**:
   - Prevent flash-joining to vote on active proposals.
   - **Recommended range**: $7 \text{ to } 30 \text{ days}$ ($604\,800$ to $2\,592\,000$ seconds). New members must wait this duration before voting or requesting loans.
3. **Voting Quorum (`quorum_bps`)**:
   - Set quorum thresholds high enough to prevent an attacker from exploiting low honest participation.
   - **Recommended range**: $5\,000 \text{ to } 6\,667 \text{ bps}$ (50%–66.67%).
4. **Staking Weight Incentives**:
   - Rely on stake-weighted voting (`util::voting_weight`) to dilute flat 1-member-1-vote power by rewarding long-term capital lockup.

## Known gaps

- No external audit.
- Fixed membership fee and no reveal deadline on private proposals are tracked as separate issues (#47, #48).
- Testnet-only deploy tooling.
- Admin-key custody and rotation are operational concerns outside the contract.

## Admin-Destination Treasury Withdrawals (#471)

### Decision: no special-casing required (matches #118)

`propose_withdrawal_in_asset` already rejects the contract's own address as destination (#323) and flags the proposer as self-paying when `destination == caller` (closed #118). An admin whose address is the *destination* (but not the proposer) is treated by the same member vote as any other destination.

This matches the documented resolution of #118: a treasury proposal paying any address — including an admin's — is subject to the same consensus threshold as every other proposal. Because admins are individually authenticated (`require_auth`), are not automatically a quorum, and each member's vote is public on-chain, the existing mechanism provides sufficient disclosure without an extra flag.

**Why no conflict-of-interest flag is added:** Adding a flag creates a surface for social-engineering (e.g., framing a legitimate payment to an admin-run entity as suspicious) without changing the security boundary: the security boundary is the consensus vote, not any metadata on the proposal. If governance concludes an admin-destination proposal is improper it votes it down.

**What to watch for operationally:** If a future governance model grants admins elevated voting weight, re-evaluate whether a destination-is-admin flag becomes necessary to maintain accountability.

---

## Asset Removal vs. Pending Treasury Withdrawals (#472)

### Decision: execution is intentionally permitted after asset removal

When `remove_token` removes an asset that has an already-approved, unexecuted treasury withdrawal, `execute_treasury_proposal` / `execute_approved` will still disburse the earmarked funds.

**Rationale:** The funds were locked into the proposal at approval time and are reflected in the treasury balance. Blocking execution would trap approved funds indefinitely — a worse outcome than allowing the disbursement to complete. This is intentionally different from the loan side, where `remove_token` blocks new disbursements via the max-loan-to-treasury-ratio re-check, because a loan is not yet earned at approval time, while a treasury withdrawal represents an already-approved governance decision.

**Mitigating factors:**
- A subsequent admin can re-whitelist the asset long enough to let the disbursement complete, then remove it again.
- The member who proposed and the approving voters are on-chain; accountability is preserved.
- `treasury_balance_in_asset` is still checked at execution to prevent overdraft.

**Test added:** `test_execute_withdrawal_after_asset_removal` in `contracts/dao/src/tests/treasury.rs` documents and locks in this behavior.

---

## Unbounded Single-Member Stake Concentration (#473)

### Analysis

`staking::stake` enforces only `amount > 0`; there is no per-member or global-per-depositor ceiling.

**What is affected by an unbounded stake:**

| Effect | Bounded? | Mechanism |
|--------|----------|-----------|
| Governance voting weight | **Yes — capped at 6** | `util::voting_weight` caps at 5 bonus votes + 1 base = 6 max per member |
| `total_staked` counter | **No** | Accumulates every deposit; no ceiling |
| Token balance vs. treasury accounting gap | **No** | The raw token balance grows with `total_staked`; lending math uses `treasury_balance_in_asset`, which excludes staked tokens, so the gap between balance and lendable treasury widens with stake concentration |
| Exit-share payout | **Proportional to stake** | A whale's stake claim at exit is proportionally larger but bounded by their own deposited tokens |
| Yield distribution | **Proportional to stake** | A very large stake earns a larger share of yield; this is economic dilution for other stakers, not a security threat |

**What a whale depositor cannot do:**
- Buy more than 6 votes regardless of stake size.
- Drain other members' stakes.
- Affect the lendable treasury (stake is excluded from `treasury_balance_in_asset`).

**What a whale depositor can do:**
- Accumulate a disproportionate share of yield.
- Distort exit-share ratios if they unstake a large position quickly.
- Widen the gap between raw token balance and lendable treasury, which could be confusing to off-chain monitoring tools that compare `token.balance(contract)` against `treasury_balance_in_asset`.

### Conclusion

The governance-weight cap makes the most harmful attack (using stake to buy votes) impossible. The economic concentration risk (yield dilution, exit-share distortion) is real but is a **policy concern, not a security vulnerability**. A per-member stake cap is **not implemented here**; if governance determines one is warranted, it should be filed as a separate follow-up. See `docs/GLOSSARY.md` for the definitions of `total_staked` and `treasury_balance_in_asset`.

---

## Stale Private-Vote Commitments on Resolved Proposals (#474)

### Behavior: resolved proposals do not block unstaking

`staking::unstake` scans treasury proposals and blocks unstaking while `p.status == ProposalStatus::Pending && now <= voting_end && (has_treasury_voted || has_commitment)`.

A member who committed a private vote and never revealed it on a proposal that was subsequently resolved (via early rejection, execution, or expiry) has `status != Pending` for that proposal. The unstake guard therefore does **not** block them, even though a stale commitment exists.

**This is correct behavior:** once a proposal is terminal its commitment is inert. There is no value in keeping the member locked — the reveal period is over, the proposal is decided, and the commitment cannot be replayed on a future proposal.

**Test coverage added:** Two tests in `contracts/dao/src/tests/staking.rs` lock in this behavior:
1. `test_stale_commitment_does_not_block_unstake_after_rejection` — commits a private vote, drives proposal to `Rejected`, confirms unstake succeeds.
2. `test_pending_commitment_blocks_unstake_during_reveal_window` — same setup but proposal remains `Pending` within the reveal window; confirms unstake is correctly blocked.
