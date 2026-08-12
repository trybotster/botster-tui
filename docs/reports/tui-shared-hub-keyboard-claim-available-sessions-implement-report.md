# Implement report: TUI shared-Hub keyboard claim via Available sessions

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `trybotster/botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Ticket | `ticket_1786529885_807584` |
| Run | `run_1786546300_948152` |
| Code commit (live run) | `da36322129f20a6cf2e1f5d14c3090ad9e385f5b` |
| Evidence commit | (this follow-up commit) |
| Runtime-teardown class | does not apply |

## Open Review findings addressed (`review_1786550159_999903`)

| Finding | Severity | Resolution |
| --- | --- | --- |
| `finding_1786550159_744686` stale Hub/worker binaries | high | `script/test-live-hub workspaces claim-driver` rebuilds Hub + session-worker with `--locked` into a **fresh** target dir, writes a build receipt, and pin ledger requires binaries under `BOTSTER_HUB_BUILD_TARGET_DIR` (rejects shared `target/release` caches). Records build commands + receipt path. |
| `finding_1786550159_801238` parent entrypoint omits binaries | high | README parent entrypoint documents `BOTSTER_HUB_BIN`, `BOTSTER_SESSION_WORKER_BIN`, `BOTSTER_HUB_BUILD_TARGET_DIR`, and locked build commands. |
| `finding_1786550159_125581` report ≠ evidence | low | This report rewritten from the final evidence file (session `…29d1`). |

## Prior findings

Earlier Review findings on UUID oracle, surface-render budget, pin HEAD derivation, baseline `lifecycle_class=current`, and live campaign execution remain resolved in code.

## Live evidence (final)

Source: `docs/reports/tui-shared-hub-keyboard-claim-live-evidence.jsonl`

| Field | Value |
| --- | --- |
| `tui_rev` | `da36322129f20a6cf2e1f5d14c3090ad9e385f5b` |
| `hub_rev` | `de6b09982e72fd5efd04a5258f5fc645f611adbc` |
| `workspaces_rev` | `7ab4d1334214b3ea3c8b02e9ea665a27e70c0916` |
| `core_rev` / `session_worker_rev` | `2c5171a6cb3b073c53620a9838d8b08480dd215c` |
| `hub_bin_path` | under fresh claim-build target dir |
| `session_worker_bin_path` | under same fresh target dir |
| `hub_bin_under_build_target` | true |
| `hub_build_command` | `cargo build --locked --release -p botster-hub …` |
| `session_worker_build_command` | `cargo build --locked --release -p botster-core-daemon --bin botster-session-worker …` |
| baseline session | `00000000-0000-4000-8000-0000000029d1` |
| `lifecycle_class` | `current` / `running` |
| add_session values | exact session_id |
| membership_join | exact W+S |
| option_excluded | option_count 0, reopened true |

## Parent entrypoint (exact)

```sh
export BOTSTER_HUB_CONNECTION='…'   # or apps open inject
export BOTSTER_HUB_DATA_DIR=/path/to/shared-hub-data
export BOTSTER_WORKSPACES_PACKAGE_PATH=/path/to/botster-workspaces   # ≥ 7ab4d133…
export BOTSTER_HUB_SOURCE_PATH=/path/to/botster-hub                  # ≥ de6b099… clean

export BOTSTER_HUB_BUILD_TARGET_DIR="$(mktemp -d "${TMPDIR:-/tmp}/botster-hub-claim-build.XXXXXX")"
cargo build --locked --release -p botster-hub \
  --manifest-path "$BOTSTER_HUB_SOURCE_PATH/Cargo.toml" \
  --target-dir "$BOTSTER_HUB_BUILD_TARGET_DIR"
cargo build --locked --release -p botster-core-daemon --bin botster-session-worker \
  --manifest-path "$BOTSTER_HUB_SOURCE_PATH/Cargo.toml" \
  --target-dir "$BOTSTER_HUB_BUILD_TARGET_DIR"
export BOTSTER_HUB_BIN="$BOTSTER_HUB_BUILD_TARGET_DIR/release/botster-hub"
export BOTSTER_SESSION_WORKER_BIN="$BOTSTER_HUB_BUILD_TARGET_DIR/release/botster-session-worker"

# Scenario + evidence paths, then:
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

## Ownership

Edits only in `botster-tui`. No Hub/Workspaces product patches.

## Residual risk

Parent dual-browser claim-stack campaign remains parent C2. Advanced historical UUID field remains out of path.

## Merge

`merge_policy: direct`. Code at `da36322`; evidence + this report in the follow-up commit.
