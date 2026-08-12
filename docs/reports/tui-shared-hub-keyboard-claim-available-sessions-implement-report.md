# Implement report: TUI shared-Hub keyboard claim via Available sessions

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `trybotster/botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Ticket | `ticket_1786529885_807584` |
| Run | `run_1786546300_948152` |
| Code commit (live run) | `3d1ac8b1645c369fe5c1403a10026e0b59e954a4` |
| Evidence commit | (this follow-up commit) |
| Runtime-teardown class | does not apply |

## Playbooks / notes applied

- implementer-playbook, botster-implementer-playbook
- botster-tui-playbook (target charter)
- Approved plan: `docs/plans/tui-shared-hub-keyboard-claim-available-sessions-plan.md`

## Open Review findings addressed (`review_1786550992_578475`)

| Finding | Severity | Resolution |
| --- | --- | --- |
| `finding_1786550992_978118` pin ledger fabricates build proof when receipt absent/incomplete | high | Claim mode **requires** `BOTSTER_TUI_CLAIM_BUILD_RECEIPT`. Strict typed `ClaimBuildReceipt` with `deny_unknown_fields`, all fields required, exact equality for hub source/target/revs/bin realpaths, and locked build command content checks. No defaults or synthesis when absent/empty/incomplete. Parent README creates and exports the receipt. Negative unit tests cover missing, empty, incomplete, unknown-field, and mismatched-target receipts. |
| `finding_1786550992_151207` committed live evidence contains PII/machine-local paths | high | Pin ledger serializes only path-neutral labels (`$HUB_SOURCE`, `$HUB_BUILD_TARGET`, `$TUI_SOURCE`, `$WORKSPACES_PACKAGE`). Build-command sanitization rewrites mktemp/`//` and `/private/var` variants and **fails closed** if residual machine paths remain. Harness canonicalizes claim build target (`pwd -P`) before recording commands. Committed-artifact PII scan (known-positive control) rejects `/Users/`, `/var/folders/`, `/private/var/`, `/tmp/`, and local username. Live evidence regenerated natively path-neutral. |

## Prior findings (still resolved)

| Finding | Resolution summary |
| --- | --- |
| `finding_1786550159_744686` stale Hub/worker binaries | Fresh locked Hub + session-worker builds into isolated target + receipt binding |
| `finding_1786550159_801238` parent entrypoint omits binaries | README documents bins, build target, receipt, locked builds |
| `finding_1786550159_125581` report ≠ evidence | This report rewritten from final evidence |
| Earlier UUID oracle / surface-render budget / pin HEAD / lifecycle_class | Remain in product path |

## Files changed (this Implement revisit)

- `crates/botster-tui/src/acceptance.rs` — strict receipt; path-neutral pin ledger; sanitize variants; fail-closed residual path check; negative + PII tests
- `crates/botster-tui/src/app.rs` — live claim driver path-neutral command assertions
- `script/test-live-hub` — claim-driver fresh locked builds, receipt, canonicalized target
- `README.md` — parent entrypoint with receipt create/export (prior commit in stack)
- `docs/reports/tui-shared-hub-keyboard-claim-live-evidence.jsonl` — live campaign evidence
- this report

## Ownership boundaries preserved

Edits only in `botster-tui`. No Hub/Workspaces product patches. Cross-repo work is pin floors + package path consumption only.

## Cross-repo routing

| Pin floor | SHA |
| --- | --- |
| Hub | `de6b09982e72fd5efd04a5258f5fc645f611adbc` |
| Workspaces | `7ab4d1334214b3ea3c8b02e9ea665a27e70c0916` |
| TUI minimum | `abc804e19bc3e01465cd308c11de5f4292331c3d` |

Dependencies (membership publish, available sessions picker, entity-backed select, hub fanout, contract entity options) are closed.

## Deviations from plan

None material. Build-receipt provenance and path-neutral evidence labels were hardened beyond the original plan in response to Review findings.

## Live evidence (final)

Source: `docs/reports/tui-shared-hub-keyboard-claim-live-evidence.jsonl`

| Field | Value |
| --- | --- |
| `tui_rev` | `3d1ac8b1645c369fe5c1403a10026e0b59e954a4` |
| `hub_rev` | `de6b09982e72fd5efd04a5258f5fc645f611adbc` |
| `workspaces_rev` | `7ab4d1334214b3ea3c8b02e9ea665a27e70c0916` |
| `core_rev` / `session_worker_rev` | `2c5171a6cb3b073c53620a9838d8b08480dd215c` |
| path labels | `$HUB_SOURCE`, `$HUB_BUILD_TARGET`, `$TUI_SOURCE`, `$WORKSPACES_PACKAGE` |
| `hub_build_command` | `cargo build --locked --release -p botster-hub --manifest-path $HUB_SOURCE/Cargo.toml --target-dir $HUB_BUILD_TARGET` |
| `session_worker_build_command` | `cargo build --locked --release -p botster-core-daemon --bin botster-session-worker --manifest-path $HUB_SOURCE/Cargo.toml --target-dir $HUB_BUILD_TARGET` |
| `build_receipt_path` | `$HUB_BUILD_TARGET/claim-build-receipt.json` |
| baseline session | `00000000-0000-4000-8000-00000000d930` |
| `lifecycle_class` | `current` / `running` |
| add_session values | exact `session_id` only |
| membership_join | exact W+S on `botster-workspaces.membership` |
| option_excluded | option_count 0, reopened true |
| request_summary | no `list_sessions`; surface-action path only |
| machine-local paths in committed evidence | none |

## Tests and downstream proof

- `cargo fmt` / `cargo clippy -D warnings` on touched crate
- Unit: strict receipt negatives; sanitize mktemp/private variants; committed evidence PII scan with known-positive control
- `script/test-live-hub workspaces claim-driver` green on clean HEAD `3d1ac8b…` with path-neutral evidence out

## Parent entrypoint (exact)

```sh
export BOTSTER_HUB_CONNECTION='…'   # or apps open inject
export BOTSTER_HUB_DATA_DIR=/path/to/shared-hub-data
export BOTSTER_WORKSPACES_PACKAGE_PATH=/path/to/botster-workspaces   # ≥ 7ab4d133…
export BOTSTER_HUB_SOURCE_PATH=/path/to/botster-hub                  # ≥ de6b099… clean

export BOTSTER_HUB_BUILD_TARGET_DIR="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/botster-hub-claim-build.XXXXXX")" && pwd -P)"
cargo build --locked --release -p botster-hub \
  --manifest-path "$BOTSTER_HUB_SOURCE_PATH/Cargo.toml" \
  --target-dir "$BOTSTER_HUB_BUILD_TARGET_DIR"
cargo build --locked --release -p botster-core-daemon --bin botster-session-worker \
  --manifest-path "$BOTSTER_HUB_SOURCE_PATH/Cargo.toml" \
  --target-dir "$BOTSTER_HUB_BUILD_TARGET_DIR"
export BOTSTER_HUB_BIN="$BOTSTER_HUB_BUILD_TARGET_DIR/release/botster-hub"
export BOTSTER_SESSION_WORKER_BIN="$BOTSTER_HUB_BUILD_TARGET_DIR/release/botster-session-worker"
export BOTSTER_TUI_CLAIM_BUILD_RECEIPT="$BOTSTER_HUB_BUILD_TARGET_DIR/claim-build-receipt.json"
# Write typed receipt (all fields required) — see README parent entrypoint block.

BOTSTER_TUI_ACCEPTANCE_SCENARIO=claim.scenario.json \
BOTSTER_TUI_ACCEPTANCE_EVIDENCE=/path/to/new-claim.evidence.jsonl \
  "$BOTSTER_HUB_BIN" apps open --data-dir "$BOTSTER_HUB_DATA_DIR" botster-tui
```

Repository live harness (rebuilds Hub for you):

```sh
BOTSTER_WORKSPACES_PACKAGE_PATH=… \
BOTSTER_HUB_SOURCE_PATH=… \
BOTSTER_TUI_CLAIM_EVIDENCE_OUT=/tmp/claim-evidence.jsonl \
  script/test-live-hub workspaces claim-driver
```

## Unverified behavior / residual risk

- Parent dual-browser claim-stack campaign remains parent C2.
- Advanced historical UUID field remains out of path.
- Live proof uses isolated claim-build target under the harness TMPDIR; production parent must export the same receipt + binary binding.

## Missing vault guidance

None discovered that blocked this implement revisit.

## Merge

`merge_policy: direct`. Code at `3d1ac8b`; evidence + this report in the follow-up commit.
