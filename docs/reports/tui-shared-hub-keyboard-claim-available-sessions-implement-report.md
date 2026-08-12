# Implement report: TUI shared-Hub keyboard claim via Available sessions

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `trybotster/botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Ticket | `ticket_1786529885_807584` |
| Run | `run_1786546300_948152` |
| Pipeline | `botster_stack_delivery` / `botster_stack_implement` |
| Branch | `project-pipelines/ticket_1786529885_807584` |
| Base tip | rebased onto `origin/main` `87997861…` (≥ entity-options `abc804e1…`) |
| Runtime-teardown class | does not apply |

## Repository playbook and other playbooks/notes applied

### Role / charter

- [[implementer-playbook]]
- [[botster-implementer-playbook]]
- [[botster-tui-playbook]]
- [[botster-tui-kit-playbook]] (does-not-own boundary only)

### Atomic notes

- [[tui and browser are equal clients]]
- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[botster tui uinode event routing captures hit regions during draw]]
- [[plugin authored tui surfaces dispatch via action props not node id literals]]
- [[conformance helpers must dispatch the action id read from the rendered node]]
- [[session UUID is the sole routing key across all layers]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]
- [[botster entity snapshots are authoritative reconnect baselines]]
- [[acceptance harness region oracles must key on node identity not concatenated text]]
- [[tui error dedup tests must drive real input handlers]]
- [[shared hub workspaces acceptance omits package path without skipping its lane]]
- [[implement gate must verify committed work and pr link before review]]
- [[implementation artifacts must match actual git state]]
- [[implementation steps must persist report artifacts for review]]
- [[test script required for rust tests not cargo test]]

### Not loaded (correctly)

- [[project-pipelines-playbook]] — package/plugin paths not in scope
- [[botster runtime teardown lenses]] — teardown class does not apply

## Files changed

| Path | Change |
| --- | --- |
| `crates/botster-tui/src/acceptance.rs` | Dual-mode acceptance (`spawn` + `claim` schemas); claim scenario/validation; fail-closed pin ledger + Available sessions form scan; evidence schema parameter |
| `crates/botster-tui/src/app.rs` | Claim acceptance driver (production InputRouter path); membership join + option exclusion oracles; `BOTSTER_LIVE_DATA_DIR` alias; hermetic claim keyboard test; drain entity-options in acceptance waits |
| `crates/botster-tui/fixtures/workspaces-claim-driver-v1.scenario.json` | Checked-in claim scenario fixture |
| `crates/botster-tui/fixtures/workspaces-claim-driver-v1.schema.json` | JSON Schema for claim scenario + evidence |
| `crates/botster-tui/fixtures/workspaces-claim-driver-v1.evidence.jsonl` | Example evidence ledger (pin, membership join, exclusion) |
| `README.md` | Parent entrypoint, env contract, pin floors, non-goals |
| `docs/reports/tui-shared-hub-keyboard-claim-available-sessions-implement-report.md` | This report |
| `docs/plans/tui-shared-hub-keyboard-claim-available-sessions-plan.md` | Unchanged plan (already committed) |

## Ownership boundaries preserved

- **Edits only in `botster-tui`.** No Hub, Workspaces, kit, or web product patches.
- Consumes Workspaces Available sessions form identities (`botster-workspaces-add-session-id`, `session_id`, `botster_workspaces.add_session`, membership family `/botster-workspaces.membership`).
- Reuses kit InputRouter + hit map; no kit feature work.

## Cross-repo dependencies or separately routed work

Closed prerequisites (registered on plan): Hub entity_options + fanout, Workspaces Available sessions + membership publish, TUI entity-options.

**Parent consumer** `ticket_1786474783_285888` claim-stack C2 should invoke the documented claim entrypoint on its shared Hub.

**No new cross-repo dependency tickets** opened during Implement.

## Deviations from plan

1. **Activation shape:** sibling schema `botster.tui.workspaces-claim-driver/v1` on the same `BOTSTER_TUI_ACCEPTANCE_SCENARIO` / `EVIDENCE` env pair as spawn (schema-routed), rather than a separate env flag. Non-colliding and smallest dual-mode change.
2. **Live shared-Hub campaign not executed in this Implement visit:** hermetic keyboard + membership path is green; live proof requires parent-seeded Hub with pin-matched Workspaces package ≥ `7ab4d13…` (local Projects checkout main can lag). Residual recorded below — not soft-waived as complete live proof.
3. **No fourth isolated `script/test-live-hub` profile** added (plan optional only when needed for `./test.sh`).

## Tests and downstream proof run

Exact commands (with colon-free target dir):

```sh
export CARGO_TARGET_DIR=/tmp/botster-tui-claim-impl-tgt
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
./test.sh
```

Results:

| Check | Result |
| --- | --- |
| `cargo fmt --all` | clean |
| `cargo clippy … -D warnings` | pass |
| `git diff --check` | pass |
| `./test.sh` | **210 unit + 1 package = 211 passed** |

Key tests:

- `acceptance::tests::*` including claim schema routing, pin form scan, claim evidence stages
- `app::tests::workspaces_claim_keyboard_select_submit_membership_and_exclusion` — real InputRouter keys, exact uuid submit, membership entity join, option exclusion without surface refresh

### Production entrypoint (for parent)

```sh
export BOTSTER_HUB_CONNECTION='…'
export BOTSTER_HUB_DATA_DIR=/path/to/shared-hub-data   # BOTSTER_LIVE_DATA_DIR accepted as alias
export BOTSTER_WORKSPACES_PACKAGE_PATH=/path/to/botster-workspaces  # ≥ 7ab4d133…
export BOTSTER_HUB_SOURCE_PATH=/path/to/botster-hub                  # ≥ de6b099…

BOTSTER_TUI_ACCEPTANCE_SCENARIO=/path/to/claim.scenario.json \
BOTSTER_TUI_ACCEPTANCE_EVIDENCE=/path/to/new-claim.evidence.jsonl \
  botster-hub apps open --data-dir "$BOTSTER_HUB_DATA_DIR" botster-tui
```

Scenario schema `botster.tui.workspaces-claim-driver/v1` with `workspace_id` + `session_uuid`.

Pin floors (fail-closed ancestry):

| Artifact | Minimum SHA |
| --- | --- |
| Hub | `de6b09982e72fd5efd04a5258f5fc645f611adbc` |
| Workspaces | `7ab4d1334214b3ea3c8b02e9ea665a27e70c0916` |
| TUI | `abc804e19bc3e01465cd308c11de5f4292331c3d` |

## Unverified behavior or residual risk

1. **Live shared-Hub claim against a parent-seeded clean Hub** was not run in this Implement session (requires installed Workspaces ≥ Available sessions pin + seeded unclaimed session). Hermetic path proves keyboard/select/submit/membership/exclusion wiring; parent C2 is the intended live consumer.
2. **Dialog-close / reopen race** after owner `replacement` is handled in the driver (re-demand membership family + reopen Add once for exclusion), but timing depends on producer publish latency.
3. **Advanced historical UUID field** remains out of primary path (documented non-goal).
4. **Hub client pin** in this crate (`89dae7e…`) is older than Hub floor `de6b099…` but is an ancestor; live Hub binary/source still must meet the claim pin floor.

## Missing vault guidance discovered

Worth capturing after live parent proof (inbox, not authored here):

1. Caller-owned shared-Hub claim seam pattern (sibling of spawn-driver acceptance).
2. Lifecycle MCP seed ≠ claim UI proof.
3. Owner replacement after claim closes dialogs — exclusion observation strategy.

## Merge / PR note

Pipeline `merge_policy: direct`. Implementation is committed on the ticket branch for Review. PR link not required for direct merge policy; create if Review requests one.
