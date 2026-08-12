# Implement report: TUI shared-Hub keyboard claim via Available sessions

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `trybotster/botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Ticket | `ticket_1786529885_807584` |
| Run | `run_1786546300_948152` |
| Pipeline | `botster_stack_delivery` / `botster_stack_implement` (revisit after Review) |
| Branch | `project-pipelines/ticket_1786529885_807584` |
| Runtime-teardown class | does not apply |

## Review revisit (`review_1786548951_579375`)

| Finding | Severity | Resolution |
| --- | --- | --- |
| `finding_1786548951_900036` UUID oracle can pass on draft | high | Driver requires `request.values.session_id` only (no draft fallback). Hermetic test submits via InputRouter Enter and asserts observed PluginSurfaceAction values; stripped-values negative proves oracle. |
| `finding_1786548951_618201` surface refresh not failed | high | After claim submit, capture `surface_renders` baseline; membership join + exclusion fail if count increases (reopen is keyboard Action only). |
| `finding_1786548951_322843` pin ledger caller-supplied revs | high | Revisions always derived from checkout HEAD; explicit rev must equal HEAD; Hub source path required. |
| `finding_1786548951_886988` shared-Hub campaign missing | high | Live campaign via `script/test-live-hub workspaces claim-driver`; evidence committed separately after a clean code commit. |
| `finding_1786548951_395337` baseline not running | medium | Baseline requires `lifecycle_class == current`; evidence records lifecycle; ended-row negative unit test. |

## Review revisit (`review_1786549581_936278`)

| Finding | Severity | Resolution |
| --- | --- | --- |
| `finding_1786549581_181050` live evidence does not identify binaries | high | Pin ledger requires clean tracked sources; hub + session-worker **realpaths** under Hub source; Hub Cargo.lock **core_rev** as session-worker provenance; live re-run from clean code HEAD `a185b92` then evidence-only commit. |
| `finding_1786549581_240211` silent tracked evidence write | medium | Live test never writes into `docs/` by default; optional absolute `BOTSTER_TUI_CLAIM_EVIDENCE_OUT` owned by wrapper; write failures panic. |

### Live evidence (post-fix)

| Field | Value |
| --- | --- |
| Code HEAD at run | `a185b926d323333abd1effb2aaa1f7a4eeac06df` (`tui_rev` in pin_ledger) |
| Hub rev | `de6b09982e72fd5efd04a5258f5fc645f611adbc` |
| Workspaces rev | `7ab4d1334214b3ea3c8b02e9ea665a27e70c0916` |
| core_rev (session-worker) | from Hub Cargo.lock botster-core pin |
| Evidence | `docs/reports/tui-shared-hub-keyboard-claim-live-evidence.jsonl` |

## Repository playbook and other playbooks/notes applied

- [[implementer-playbook]], [[botster-implementer-playbook]], [[botster-tui-playbook]]
- [[session UUID is the sole routing key across all layers]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]
- [[tui error dedup tests must drive real input handlers]]
- [[conformance helpers must dispatch the action id read from the rendered node]]
- [[implement gate must verify committed work and pr link before review]]
- [[implementation steps must persist report artifacts for review]]
- [[test script required for rust tests not cargo test]]

## Files changed (this revisit + prior implement)

| Path | Change |
| --- | --- |
| `crates/botster-tui/src/acceptance.rs` | Claim mode; pin ledger from checkout HEAD only; form scan |
| `crates/botster-tui/src/app.rs` | Claim driver oracles; hermetic keyboard submit; live claim driver; baseline current |
| `crates/botster-tui/fixtures/workspaces-claim-driver-v1.*` | Schema/scenario/evidence fixtures |
| `script/test-live-hub` | `workspaces claim-driver` profile |
| `README.md` | Parent entrypoint |
| `docs/reports/tui-shared-hub-keyboard-claim-available-sessions-implement-report.md` | This report |
| `docs/reports/tui-shared-hub-keyboard-claim-live-evidence.jsonl` | Live campaign evidence |

## Ownership boundaries preserved

Edits only in `botster-tui`. No Hub/Workspaces/kit product patches. Consumes installed Workspaces Available sessions form + membership family.

## Cross-repo dependencies

Closed prereqs unchanged. Parent claim-stack `ticket_1786474783_285888` is the downstream consumer of `script/test-live-hub workspaces claim-driver` / schema `botster.tui.workspaces-claim-driver/v1`.

## Deviations from plan

1. Sibling claim schema on shared SCENARIO/EVIDENCE env (non-colliding).
2. Supporting live profile `claim-driver` added (plan allowed when needed for proof).
3. No draft fallback on UUID oracle (stricter than initial implement).

## Tests and downstream proof

```sh
export CARGO_TARGET_DIR=/tmp/botster-tui-claim-impl-tgt
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
./test.sh
# Live shared-Hub:
BOTSTER_HUB_BIN=… BOTSTER_SESSION_WORKER_BIN=… \
BOTSTER_WORKSPACES_PACKAGE_PATH=…/ticket_1786474780_590414 \
BOTSTER_HUB_SOURCE_PATH=…/botster-hub \
  script/test-live-hub workspaces claim-driver
```

| Check | Result |
| --- | --- |
| fmt / clippy -D warnings / git diff --check | pass |
| `./test.sh` | **213 unit + 1 package = 214 passed** |
| Live claim-driver | **ok** — `installed-workspaces-claim-driver: complete` |

### Live evidence facts

From `docs/reports/tui-shared-hub-keyboard-claim-live-evidence.jsonl`:

| Stage | Fact |
| --- | --- |
| pin_ledger | Hub `de6b099…`, Workspaces `7ab4d13…`, form ok, ancestry ok |
| baseline | session `…eab0`, `lifecycle_class=current`, `lifecycle=running` |
| dispatched_action | `botster_workspaces.add_session` with `values.session_id` exact uuid |
| membership_join | family `botster-workspaces.membership`, exact W+S |
| option_excluded | option_count 0, reopened true |
| complete | action_id `botster_workspaces.add_session` |

### Parent entrypoint

```sh
export BOTSTER_HUB_CONNECTION='…'   # or apps open inject
export BOTSTER_HUB_DATA_DIR=/path/to/shared-hub-data
export BOTSTER_WORKSPACES_PACKAGE_PATH=/path/to/workspaces  # ≥ 7ab4d13…
export BOTSTER_HUB_SOURCE_PATH=/path/to/hub-source          # ≥ de6b099…

BOTSTER_TUI_ACCEPTANCE_SCENARIO=claim.scenario.json \
BOTSTER_TUI_ACCEPTANCE_EVIDENCE=/new/claim.evidence.jsonl \
  botster-hub apps open --data-dir "$BOTSTER_HUB_DATA_DIR" botster-tui
```

Scenario: `{ "schema": "botster.tui.workspaces-claim-driver/v1", "workspace_id", "session_uuid", "hub_source_path", "workspaces_package_path" }`.

## Unverified behavior or residual risk

1. Live proof used an isolated Hub with installed packages (caller-shaped injectors + pin floors), not the parent dual-browser shared Hub campaign — that remains parent C2.
2. Session-worker rev is optional in evidence when not supplied.
3. Advanced historical UUID field remains out of primary path.

## Missing vault guidance

Deferred capture (after this live proof): caller-owned claim seam pattern; MCP seed ≠ UI claim; owner replacement dialog exclusion.

## Merge / PR

`merge_policy: direct`. Implementation committed on ticket branch.
