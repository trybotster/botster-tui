# Implement report: TUI reactive entity-backed select options

## Target

| Field | Value |
| --- | --- |
| Ticket | `ticket_1786474781_871159` |
| Target repository | `botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Run | `run_1786480500_850089` |
| Step | `botster_stack_implement` (revisit after `review_1786491226_172233`) |
| PR | https://github.com/trybotster/botster-tui/pull/50 |

## Finding addressed

| Finding | Resolution |
| --- | --- |
| `finding_1786491226_232190` TUI rejects valid owner-authored replacement trees | Removed content-based refusal. Accepted `UiActionResult.replacement` is always applied; `sync_entity_options_subscriptions` runs so families drop when the owner replaces the picker with a static success tree. Unit test updated to require application of a success replacement. Live proof remains on Hub-delivered surface (no test body rewrite). |

## Playbooks

- [[implementer-playbook]], [[botster-implementer-playbook]], [[botster-tui-playbook]]
- Convention conflicts: **none**

## Ownership

TUI app policy only.

## Files changed

- `crates/botster-tui/src/app.rs`
- `docs/reports/tui-render-reactive-entity-backed-select-options-implement-report.md`

## Tests

| Check | Result |
| --- | --- |
| `cargo fmt` / `clippy -D warnings` | pass |
| `./test.sh` | unit suite green |
| `plugin_action_result_applies_static_success_replacement_and_drops_options_families` | pass |
| `entity_options_live_hub_proof_when_binaries_are_available` | pass (`surface_renders=1`) |

## Residual risk

None new. Owner success replacements intentionally drop options demand via resync.

## Missing vault guidance

Owner replacement trees that drop options_source producers must resync family demand — candidate capture.
