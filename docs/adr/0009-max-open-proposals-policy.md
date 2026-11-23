# ADR-0009: Evaluation of MAX_OPEN_PROPOSALS_PER_MEMBER in LoanPolicy

## Status

Proposed

## Context

`storage::MAX_OPEN_PROPOSALS_PER_MEMBER` is currently a hard-coded constant set to `3`. In contrast, all other per-member loan parameters — such as `min_membership_duration`, `cooldown_period`, `default_grace_period`, and `quorum_bps` — are configurable fields on the `LoanPolicy` struct, which can be modified by governance via `propose_policy_update` subject to a 48-hour timelock.

Issue #503 asks whether `MAX_OPEN_PROPOSALS_PER_MEMBER` should move into `LoanPolicy` or remain a compiled constant.

## Evaluation

### Arguments for keeping it as a constant

1. **Storage and Gas Bounds**: The open proposal count is calculated by iterating all loan and treasury proposals. Limiting open proposals per member prevents state spam and bounds the work done in `get_open_proposals`. Hard-coding this constant protects the contract from governance mistakenly setting an excessively large limit that makes proposal submission too gas-intensive.
2. **Backward Compatibility**: Adding a field to `LoanPolicy` requires storage migration or schema versioning.
3. **Simplicity**: 3 open proposals per member is generous for normal governance activity without overwhelming keepers or voters.

### Arguments for moving it to LoanPolicy

1. **Flexibility**: Larger DAOs or organizations with higher proposal throughput may want to tune this quota up or down without deploying a new contract.
2. **Consistency**: Aligns the per-member quota with all other operational limits in `LoanPolicy`.

## Decision

We propose keeping `MAX_OPEN_PROPOSALS_PER_MEMBER` as a compiled constant for now, while documenting the migration path: if future DAO operations require adjusting this parameter, it should be introduced as an optional field in `LoanPolicy` along with a corresponding migration and timelock check.
