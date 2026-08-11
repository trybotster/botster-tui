# Implement report: TUI reactive entity-backed select options

## Target

| Field | Value |
| --- | --- |
| Ticket | `ticket_1786474781_871159` |
| Target repository | `botster-tui` / `trybotster/botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Run | `run_1786480500_850089` |
| Step | `botster_stack_implement` (revisit after Review `changes_required`) |
| Plan | `docs/plans/tui-render-reactive-entity-backed-select-options-plan.md` |
| PR | https://github.com/trybotster/botster-tui/pull/50 |
| Runtime-teardown class | Does not apply |

## Review findings addressed (`review_1786489500_723691`)

| Finding | Resolution |
| --- | --- |
| Gap-recovery branch cannot observe a sequence gap | Production reducer returns `Err` for matching-generation pre-snapshot deltas and sequence holes (`current+1` only accepted). Drain resubscribes on `Err`. Unit tests cover pre-snapshot, hole, and recovery snapshot. |
| Action-result surface replacement leaves stale families | `apply_plugin_action_result` calls `sync_entity_options_subscriptions` after body replacement. Unit test proves old family generation dropped. |
| Live proof bypasses keyboard submit and ordered changes | Live fixture uses `/session`. Full InputRouter keyboard select + form submit; post-baseline ordered lifecycle patch from ShutdownSession; no manual resubscribe cheat; `surface_renders=1`. |
| Format / strict lint gates fail | `cargo fmt --all`; clippy `-D warnings` clean; fixture helpers under `#[cfg(test)]`. |

## Repository playbook and notes applied

- [[implementer-playbook]], [[botster-implementer-playbook]], [[botster-tui-playbook]]
- Targeted notes from prior implement visit (kit adapter, hit-map routing, pin identity, entity binding, real input handlers, implement gate artifacts)
- Convention conflicts: **none**

## Ownership boundaries preserved

TUI app policy only. Shared projector from contract. Kit pin only. No Hub/Web/Workspaces product edits.

## Files changed (this revisit + prior implement)

| Path | Change |
| --- | --- |
| `crates/botster-tui/src/entity_options.rs` | Gap dispositions; empty projection keeps `options_source`; cfg(test) fixture helpers; generation/gap tests |
| `crates/botster-tui/src/app.rs` | Action-result resync; live keyboard/session path; replacement unit test; collapsible-if lint fixes |
| `crates/botster-tui/fixtures/entity-options-reactive/*` | Live package: `/session` options_source producer |
| pins / README / lock / report | Prior implement + this report update |

## Cross-repo

Hub `891cc796…` and kit `9d4a566…` pins unchanged. No new dependencies.

## Deviations

1. Live fixture uses process-wide `/session` (not dual plugin item+exclude). Exclude matrix remains unit-covered by shared fixture. Ordered live changes use Hub session lifecycle frames on the active subscription.
2. Empty projected option sets keep `options_source` so realized xor validation passes (empty options slot is treated as missing).
3. After live form submit, authored picker body is restored if the host action result stripped the producer prop (host re-emit residual documented).

## Tests / proof

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy -p botster-tui --all-targets --all-features -- -D warnings` | pass |
| `./test.sh` (no live env) | 188 unit + 1 package green |
| `entity_options` unit filters | green (gap recovery matrix included) |
| `entity_options_live_hub_proof_when_binaries_are_available` | green — keyboard exact submit + ordered lifecycle drop, `surface_renders=1` |

## Residual risk

1. Consecutive seq acceptance (`current+1` only) is stricter than session-entity “any greater”; if Hub jumps seq on a plugin family, recovery resubscribes. Acceptable for generation safety.
2. Host action results that replace plugin bodies without `options_source` require client restore in live proof; production surfaces should keep producer bodies.
3. Live hub binary was Projects debug build (contract pin still enforced by Cargo).

## Missing vault guidance

Same candidates as prior visit: TUI entity-options generation seam; process-wide family dedupe; realized empty options xor rule for producers.
