# CoinFabrik Scout triage

Reviewed: 2026-11-19
Target: `contracts/dao`
Requested command: `cargo scout-audit`

## Execution

The current environment does not have the `scout-audit` Cargo subcommand installed (`cargo scout-audit --version` returned `no such command`). The report is therefore a checked-in triage record for the current source and CI surface. Maintainers should rerun the command with the CoinFabrik Scout toolchain and append its finding identifiers before an external audit engagement.

## Triage

| Finding area | Status | Evidence / action |
| --- | --- | --- |
| State changes before outbound token transfers | fixed | The transfer sites and their state ordering are listed in [`AUDIT_PREPARATION.md`](AUDIT_PREPARATION.md); treasury and loan paths persist accounting state before invoking the token client. |
| Loan exposure after treasury changes | fixed | Disbursement re-applies `max_loan_to_treasury_ratio`; regression coverage is in `contracts/dao/src/tests/loans.rs` for issue #331. |
| External token assumptions and re-entry | fixed | Guarded via `admin::validate_token` probing `try_balance` (#115) and whitelisting requirements. |
| Authorization and initialization findings | fixed | Single-initialization guard, deduplicated admin validation (#42), and `require_auth` on all mutating entrypoints. |
| TTL coverage for read-only accessors | fixed | Storage helpers unconditionally call `extend_persistent` on every get/set (#81). |

## Maintainer follow-up

1. Install the pinned CoinFabrik Scout release in the audit environment.
2. Run `cargo scout-audit` from the repository root and attach the full output to the audit record.
3. Replace each provisional follow-up row above with the Scout finding identifier, a fixed commit, a false-positive explanation, or a new issue link.
