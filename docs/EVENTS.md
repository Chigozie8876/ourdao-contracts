# Adding events for a new proposal kind

`ProposalKind` is used by shared proposal storage such as attached documents,
commitments, and vote records. Adding a variant is an ABI and indexer change,
not just a Rust enum edit. Use this checklist whenever a future proposal kind
is introduced:

1. Add the variant to `contracts/dao/src/storage.rs` without renaming or
   reordering existing variants. The encoded names are already persisted on
   chain.
2. Add the state-changing entrypoints and emit a new, uniquely named event for
   each state transition. Keep topics short, lowercase, and stable; never
   rename an event that has shipped.
3. Add the event and its exact tuple payload to `README.md` and
   `docs/events.schema.json`. Update the event schema version when the schema
   changes.
4. Add tests that assert the topic and every payload field, plus a test that
   the new `ProposalKind` is accepted by the shared document/commit paths.
5. Update `ourdao-backend`'s event decoder and indexer in the same rollout.
   Replay an event from a fresh database before deployment to verify that the
   indexed state matches the contract views.
6. Mention the new event names and the coordinated backend change in the pull
   request description. Run `make ci` before requesting review.

The existing variants are append-only. A future variant must therefore be
added at the end, with migrations and indexer support shipped together.
