# Implement report: TUI shared-Hub keyboard claim via Available sessions

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `trybotster/botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Ticket | `ticket_1786529885_807584` |
| Run | `run_1786546300_948152` |
| Code commit (live run) | `692138b206e5e8b678eed094511b79791daa89f6` |
| Evidence commit | (this follow-up commit) |
| Runtime-teardown class | does not apply |

## Playbooks / notes applied

- implementer-playbook, botster-implementer-playbook
- botster-tui-playbook (target charter)
- Approved plan: `docs/plans/tui-shared-hub-keyboard-claim-available-sessions-plan.md`

## Open Review findings addressed (`review_1786552038_370345`)

| Finding | Severity | Resolution |
| --- | --- | --- |
| `finding_1786552038_976623` parent entrypoint requires manual receipt repair | high | Added `script/write-claim-build-receipt` which extracts the **exact** Hub `Cargo.lock` botster-core SHA into `core_rev` (rejects placeholders). README parent entrypoint calls this script unchanged — no manual edit. `script/test-live-hub workspaces claim-driver` uses the same writer. Smoke: parent receipt writer produced `core_rev=2c5171a6…` against Hub `de6b099…`; live claim-driver green on clean HEAD `692138b` with path-neutral evidence. |

## Prior findings (still resolved)

| Finding | Resolution summary |
| --- | --- |
| `finding_1786550992_978118` fabricates build proof without receipt | Strict mandatory typed receipt |
| `finding_1786550992_151207` machine-local paths in evidence | Path-neutral labels + sanitize fail-closed + PII scan |
| `finding_1786550159_*` stale bins / missing binaries / report drift | Fresh locked builds, README bins, report rewritten |
| Earlier UUID oracle / surface-render / pin HEAD / lifecycle_class | Remain in product path |

## Files changed (this Implement revisit)

- `script/write-claim-build-receipt` — executable parent/harness receipt writer (auto core_rev)
- `script/test-live-hub` — claim-driver uses shared writer
- `README.md` — documented parent entrypoint calls `script/write-claim-build-receipt` (no placeholder)
- `docs/reports/tui-shared-hub-keyboard-claim-live-evidence.jsonl` — live campaign evidence
- this report

(Prior stack also owns strict receipt validation and path-neutral pin ledger in `crates/botster-tui/src/acceptance.rs`.)

## Ownership boundaries preserved

Edits only in `botster-tui`. No Hub/Workspaces product patches.

## Cross-repo routing

| Pin floor | SHA |
| --- | --- |
| Hub | `de6b09982e72fd5efd04a5258f5fc645f611adbc` |
| Workspaces | `7ab4d1334214b3ea3c8b02e9ea665a27e70c0916` |
| TUI minimum | `abc804e19bc3e01465cd308c11de5f4292331c3d` |

## Deviations from plan

None material. Shared receipt writer is the durable form of the parent entrypoint requirement.

## Live evidence (final)

Source: `docs/reports/tui-shared-hub-keyboard-claim-live-evidence.jsonl`

| Field | Value |
| --- | --- |
| `tui_rev` | `692138b206e5e8b678eed094511b79791daa89f6` |
| `hub_rev` | `de6b09982e72fd5efd04a5258f5fc645f611adbc` |
| `workspaces_rev` | `7ab4d1334214b3ea3c8b02e9ea665a27e70c0916` |
| `core_rev` / `session_worker_rev` | `2c5171a6cb3b073c53620a9838d8b08480dd215c` |
| path labels | `$HUB_SOURCE`, `$HUB_BUILD_TARGET`, `$TUI_SOURCE`, `$WORKSPACES_PACKAGE` |
| `hub_build_command` | path-neutral locked hub build via labels |
| baseline session | `00000000-0000-4000-8000-000000008f89` |
| `lifecycle_class` | `current` / `running` |
| add_session values | exact `session_id` only |
| membership_join | exact W+S |
| option_excluded | option_count 0, reopened true |
| machine-local paths | none |

## Parent entrypoint smoke

Documented README step 3 (build into fresh target + `script/write-claim-build-receipt`) executed against Hub `de6b099…`:

- Receipt `core_rev` auto-extracted: `2c5171a6cb3b073c53620a9838d8b08480dd215c`
- No placeholder tokens; file ready for strict claim-mode validation
- Full claim path re-proven via harness that shares the same writer

## Tests and downstream proof

- Parent receipt writer smoke (exact Cargo.lock SHA)
- `script/test-live-hub workspaces claim-driver` green on clean HEAD `692138b…`
- Prior unit coverage: strict receipt negatives, path sanitize, committed PII scan

## Unverified behavior / residual risk

- Parent dual-browser claim-stack campaign remains parent C2.
- Full dual-browser Web+TUI campaign not re-run in this TUI ticket.

## Missing vault guidance

None discovered that blocked this implement revisit.

## Merge

`merge_policy: direct`. Code at `692138b`; evidence + this report in the follow-up commit.
