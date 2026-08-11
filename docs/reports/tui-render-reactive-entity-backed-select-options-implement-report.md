# Implement report: TUI reactive entity-backed select options

## Target

| Field | Value |
| --- | --- |
| Ticket | `ticket_1786474781_871159` |
| Target repository | `botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Run | `run_1786480500_850089` |
| Step | `botster_stack_implement` (revisit after `review_1786490665_855473`) |
| PR | https://github.com/trybotster/botster-tui/pull/50 |
| Runtime-teardown | N/A |

## Findings addressed this revisit

| Finding | Resolution |
| --- | --- |
| `finding_1786490665_580144` Gap recovery can leave demanded family unsubscribed | On gap `Err`, drop pump/generation and re-`start`. Start failure sets `force_reconnect` when endpoint/client absent; `heal_entity_options_subscriptions` retries demanded pumps at end of every drain. Production-path tests: successful pump replace + new subscription id + recovery snapshot via `drain_entity_options_subscriptions`; start-failure reconnect signal. |
| `finding_1786490665_250532` Live ordered-change rewrote Hub surface | Removed test-authored body assignment after submit. Production `apply_plugin_action_result` refuses replacements that drop every `options_source` producer while prior body had producers. Live proof keeps Hub-delivered surface; unit test proves strip-refusal. |

Prior findings from `review_1786489500_723691` remain fixed at this commit (gap Err path, action-result resync for real family changes, keyboard live submit, fmt/clippy).

## Playbooks / notes

- [[implementer-playbook]], [[botster-implementer-playbook]], [[botster-tui-playbook]]
- Convention conflicts: **none**

## Ownership

TUI app policy only; shared contract projector; kit pin only.

## Files changed (this revisit)

- `crates/botster-tui/src/app.rs` — gap heal/reconnect, local-pump test harness, action-result producer preservation, drain tests, live proof cleanup
- `docs/reports/tui-render-reactive-entity-backed-select-options-implement-report.md`

## Tests

| Check | Result |
| --- | --- |
| `cargo fmt --check` | pass |
| `cargo clippy -D warnings` | pass |
| `./test.sh` | 191 unit + 1 package |
| `entity_options_drain_gap_recovery_*` | pass |
| `plugin_action_result_keeps_prior_body_*` | pass |
| `entity_options_live_hub_proof_*` | pass (`surface_renders=1`, Hub surface retained) |

## Residual risk

1. Consecutive seq (`current+1`) remains stricter than session-entity any-greater.
2. Refusing replacements that drop all options producers may keep a stale body if a host intentionally removes the select; that product path is rare and should re-render explicitly.
3. Live hub binary is Projects debug build; Cargo pins still enforce contract identity.

## Missing vault guidance

Same capture candidates: generation seam, process-wide family dedupe, realized empty-options xor, host replacement vs producer body preservation.
