# Implement report: TUI reactive entity-backed select options

## Target

| Field | Value |
| --- | --- |
| Ticket | `ticket_1786474781_871159` |
| Target repository | `botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Run | `run_1786480500_850089` |
| Step | `botster_stack_implement` (acceptance hygiene after Verify residual) |
| PR | https://github.com/trybotster/botster-tui/pull/50 |
| Prior commit | `f7fa61c` |
| This commit | `7eb6636` (`7eb663671b9b26b14ff46ae982281328144f5f81`) |

## Acceptance residuals addressed

| Residual | Resolution |
| --- | --- |
| Stale live-test comment claimed owner replacements that drop `options_source` are refused | Comment now matches `f7fa61c` product behavior: accepted owner replacements apply (including static success trees that drop every producer). Live fixture still asserts the Hub-delivered surface keeps `options_source` because this host path does not return such a replacement. |
| README dual `botster-core` note named `ff115694…` while lock has `9d41ad4…` | Documented current exact lock value. Drift is a necessary side-effect of this ticket’s Hub pin to `891cc79` (hub-test-support tracks `botster-core?branch=main`); not unrelated noise. Direct core pin remains `16bf08f2…`. |
| PR #50 test plan still listed 186 unit tests | Updated to verified **191** unit + 1 package after `./test.sh`. |

## Playbooks

- [[implementer-playbook]], [[botster-implementer-playbook]], [[botster-tui-playbook]]
- Convention conflicts: **none**
- Teardown class: **does not apply**

## Ownership

TUI docs/comment/PR hygiene only; no product behavior change beyond accurate documentation of existing `f7fa61c` policy.

## Files changed

- `crates/botster-tui/src/app.rs` — live proof comment accuracy
- `README.md` — dual core source pin `9d41ad4c614add7d15ff7e0f88b310a55627cd82`
- `docs/reports/tui-render-reactive-entity-backed-select-options-implement-report.md`
- PR #50 body — test count 191

## Lock assessment

| Source | Value | Action |
| --- | --- | --- |
| `botster-core` direct rev | `16bf08f29ec723c70c290cf995745ccbf79d4f05` | keep |
| `botster-core?branch=main` (via hub-test-support) | `9d41ad4c614add7d15ff7e0f88b310a55627cd82` | keep + document (introduced with Hub/kit pin in `5809f14`) |
| `botster-hub` / ui-contract | `891cc796faeab51ee4bee1a0e8494562b233036e` | keep |
| kit | `9d4a566f309e9d848771b5448764a87f4721468e` | keep |

## Tests

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass (exit 0) |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | pass (exit 0) |
| `git diff --check` | pass (exit 0) |
| `./test.sh` | **191** unit + **1** package passed |
| Focused: `plugin_action_result_*` (2), `entity_options_drain_gap_recovery*` (2), `entity_options::` (4) | 8/8 pass |
| `entity_options_live_hub_proof_when_binaries_are_available` with `BOTSTER_HUB_BIN`/`BOTSTER_SESSION_WORKER_BIN` from Projects/botster-hub@`90d0e1a` and `BOTSTER_TUI_REQUIRE_HUB_TEST=1` | pass, `surface_renders=1` |

## Residual risk

None new. Docs/comment/PR only. Not merged.

## Missing vault guidance

None for this hygiene pass.
