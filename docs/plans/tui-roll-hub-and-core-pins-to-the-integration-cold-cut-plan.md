# Plan: roll Hub and Core pins to the integration cold cut

Ticket: `ticket_1788460430_647093`
Run: `run_1788570301_694931`
Pipeline: Botster Stack Delivery (`botster_stack_delivery`)
Plan base: `origin/main` at `b051c67` (the run worktree was 21 commits behind main at spawn; the branch was reset to `origin/main` before planning)

## Target repository

- Target repository: `botster-tui` (`https://github.com/trybotster/botster-tui`)
- Target id: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Repository charter: `[[botster-tui-playbook]]`
- Botster layer touched: TUI client over the Hub Unix control and terminal planes. No plugin, Lua, Hub, Core, SPA, or Rails surface changes.

## Context loaded

Role and stack playbooks:

- `[[planner-playbook]]`
- `[[botster-planner-playbook]]`
- `[[botster-tui-playbook]]`

Targeted atomic notes:

- `[[pin rolls update live lane provenance defaults and README pin prose]]` -- the four-site update rule and the zero-old-revision search.
- `[[Hub Core pin rolls update eleven literal sites and six lock sources]]` -- zero-old-revision invariant and stable lock-source count.
- `[[Git-consumed Hub members pin Core protocol by exact revision]]` -- TUI and Hub share one exact Core revision.
- `[[Cargo Git URL and selector form are part of crate identity]]` -- keep the `.git` URL and `rev =` selector form.
- `[[TUI bin only Core 8fce204 builds require local runtime feature unification]]` -- `cargo build -p botster-tui --locked` is a separate production gate after a pin roll.
- `[[downstream proof targets the consumer branch that exposed the failure]]` -- the Hub matrix must consume the exact TUI candidate SHA.
- `[[a downstream reproduction ticket can be overtaken by a pin roll]]` -- recheck pin ancestry before the roll.
- `[[Unix mux polling returns bounded complete-frame batches while input stays readable]]` -- one persistent incomplete-line buffer per stream.
- `[[first-party Unix attach clients use split Hello and subscription close events]]` -- the attach contracts this roll must preserve.
- `[[TUI live Ghostty has IsolatedHub ghostty plus attach-only ghostty-shared and ghostty-shared-exit]]` -- the live lanes this roll must renew.
- `[[deleting a waiver proof test can drop unrelated coverage in its tail]]` -- remove only the Drain assertion inside larger tests.
- `[[colon worktree paths break cargo dyld library paths]]` -- colon-free `CARGO_TARGET_DIR` for gates.
- `[[verification evidence is scoped to a stable commit and clean tree]]` -- every gate names the candidate SHA.
- `[[test script required for rust tests not cargo test]]` -- use the repository wrappers.

`[[botster runtime teardown lenses]]` was not loaded. The ticket removes one deleted request variant and changes one helper signature. It does not change peer, session, or adapter ownership.

Pipeline context:

- Human sequencing decision `question_1788570185_464058` (Hub run) freezes Hub `205cadf6f8dab9dc990537c2c00ef3d27edb31dd` and Core `93acae3f98adbc21dc981d113c4eb2f31ead4ad0`.
- Failed Hub integration evidence `artifact_1788569974_983676` names TUI `app.rs` lines 4714, 19694, 19710, and 20920 as the four compile failures against Hub `205cadf`.
- Human answer `question_1788570499_542658` (this run) chose option A: the Hub integration agent pushed the existing branch `project-pipelines/ticket_1787600679_990088` at exact commit `205cadf` to origin, with no rewrite. The Hub ticket stays unmerged.
- Project orchestrator message `msg_plugin-w_1788570541_5a4ccf` and steward message `msg_plugin-w_1788570603_7b8ca6` supersede the ticket sentence "Merge the TUI ticket directly to main before the Hub integration ticket runs its new complete matrix". The new order is in the Sequencing section below.
- Scratch commit `eeadb33` (parent `38e5717`, branch `proof/ticket_1787600679-e50e0f0`) is an unreviewed handoff reference. This plan reimplements it in the run worktree. It does not cherry-pick it.

Verified facts at plan time:

| Fact | Result |
| --- | --- |
| Current TUI main Hub pin | `bb1a330543bc06888f894edd5f40a0f867753a12` |
| Current TUI main Core pin | `48a437032791e678010254708259568ce4ad02bf` |
| Hub `205cadf` fetchable from `https://github.com/trybotster/botster-hub.git` | Yes. `git ls-remote origin refs/heads/project-pipelines/ticket_1787600679_990088` returns `205cadf...`. A depth-1 fetch of the SHA succeeded. |
| Core `93acae3` on remote | Yes. It is `refs/heads/main` of `botster-core`. |
| Hub `205cadf` contains Hub main `ae6a0b1` and old pin `bb1a330` | Yes. 54 commits ahead of Hub main. |
| Hub `205cadf` Core pins (root, hub-client, hub-test-support, `PROTOCOL_REV`) | All `93acae3` |
| `botster-hub-client` at `205cadf`: `read_frame_from_reader` signature | `(reader: &mut BufReader<UnixStream>, incomplete: &mut String)` |
| `botster-hub-client` at `205cadf`: `DaemonRequest::Drain`, `drain_session`, `drain_subscription` | Removed |
| `botster-hub-client` at `205cadf`: `CONFORMANCE_FIXTURE_REVISION`, `PROTOCOL_VERSION` | 48, 8 (unchanged; TUI `MINIMUM_CONFORMANCE_FIXTURE_REVISION = 48` stays valid) |
| `@trybotster/hub-test-support` package version at `205cadf` | `0.1.43` (unchanged) |
| Core public API delta `48a4370..93acae3` in `botster-terminal-protocol-client`, `botster-terminal-ghostty`, `botster-core-test-support` | One added method (`shared_owner_count`). No removals. |
| TUI use of removed Hub client API | Only `DaemonRequest::Drain` at `app.rs:4714`. No `drain_session` or `drain_subscription` calls. |
| TUI `read_frame_from_reader` call sites | `app.rs:19694`, `app.rs:19710`, `app.rs:20920` (all inside `mod tests`) |
| `Cargo.lock` old-pin source lines | Core `48a4370`: 5 lines. Hub `bb1a330`: 2 lines. |

## Scope

In scope:

1. Source migration in `crates/botster-tui/src/app.rs` against Hub `205cadf`:
   - Remove the `DaemonRequest::Drain { .. }` match arm at line 4714.
   - Remove the `ObservedRequest::Drain(String)` variant at line 7420. The variant has no producer after the arm is gone.
   - Remove the three assertions that state no `ObservedRequest::Drain` was observed (lines 20167, 26551, 26821). The type can no longer express a Drain request, so the assertions become unrepresentable.
   - Delete the test `poll_hub_does_not_send_terminal_drain` (line 20157). Its only assertion is the Drain check. The two larger tests at 26551 and 26821 keep every other assertion.
   - Give each `read_frame_from_reader` stream one persistent `incomplete: String` that lives as long as the `BufReader` for that stream. The recovery stub thread (lines 19694 and 19710) uses one buffer for Hello and the frame loop. The second stub (line 20920) uses one buffer for its loop.
2. Durable pin roll in `crates/botster-tui/Cargo.toml`:
   - `botster-hub-client` and `botster-hub-test-support`: `rev = "205cadf6f8dab9dc990537c2c00ef3d27edb31dd"`.
   - `botster-core`, `botster-terminal-ghostty`, `botster-terminal-protocol-client`, `botster-core-test-support`: `rev = "93acae3f98adbc21dc981d113c4eb2f31ead4ad0"`.
   - Keep the `https://github.com/trybotster/botster-core.git` and `https://github.com/trybotster/botster-hub.git` URL forms and the `rev =` selector.
   - Keep `default-features = false` on `botster-core` and `botster-core-test-support`. Keep `features = ["libghostty-vt"]` on `botster-terminal-ghostty`.
   - Do not touch `botster-tui-kit` or `botster-ui-contract`.
3. `Cargo.lock`: update only the Hub and Core Git sources. No registry crate churn.
4. Ghostty live-lane defaults in `app.rs` (main lines 23532 and 23534): `BOTSTER_HUB_BIN_REV` default `205cadf...`, `BOTSTER_SESSION_WORKER_BIN_REV` default `93acae3...`.
5. README pin prose: every sentence that names `bb1a330...` or `48a4370...` (Foundation table rows for hub-client, hub-test-support, and Core; Live hub verification paragraph; the Ghostty live proof export block; the session-types `Pins` list; the session-types comment `# Use Hub bb1a330 and Core 48a4370 binaries`). Short-form SHAs count.
6. One implement report under `docs/reports/` that records the candidate SHA, gate commands, live-lane provenance, and the zero-match search.

Out of scope:

- Adopting the new `botster-hub-client` `poll_terminal` or `next_terminal` helpers. The TUI keeps its own production mux reader (`pending_mux_frames`, `mux_buf`). The ticket does not ask for that change.
- Any adapter, fallback, shim, `#[allow(dead_code)]`, or compatibility branch. The scratch commit's `#[allow(dead_code)]` on `ObservedRequest::Drain` is a hack and is not adopted.
- Compensation for any Hub or Core close defect. If a live lane exposes one, file it against Hub or Core and stop.
- Stale README claims that predate this roll and name other revisions (`4f30d69...` in the Workspaces lanes paragraph, `@trybotster/hub-test-support@0.1.39` in the Workspaces section). They are not the old pins of this roll. Record them as a vault gap.
- Historical files under `docs/plans/**` and `docs/reports/**`. They keep their original revisions.
- Merging this ticket before Hub merges. See Sequencing.

## Sequencing (supersedes the ticket's TUI-first merge sentence)

Phase 1 (this run's Implement, Review, Verify):

1. Implement verifies fetchability before it changes any pin: `git ls-remote https://github.com/trybotster/botster-hub.git refs/heads/project-pipelines/ticket_1787600679_990088` must return `205cadf...`.
2. Implement makes the source migration, pin roll, lock update, defaults, README, and report. It commits one clean candidate on `project-pipelines/ticket_1788460430_647093` and pushes the branch.
3. Review and Verify approve that exact candidate SHA independently.
4. The run does not request merge. The candidate SHA is handed to the Hub integration run (`ticket_1787600679_990088`). Hub runs its complete unspliced matrix against that SHA. Hub merges first.

Phase 2 (after Hub merges, same run, returned Implement visit):

5. Implement re-pins `botster-hub-client` and `botster-hub-test-support` to the merged Hub main SHA. It updates `Cargo.lock`, the `BOTSTER_HUB_BIN_REV` default, and every README Hub revision sentence. Core stays `93acae3` unless the merged Hub SHA pins a different Core; then the Core family follows in lockstep.
6. Implement reruns every repository gate and the live lanes in the Acceptance section against the merged Hub SHA.
7. Review and Verify renew approval for the new candidate SHA. Then the run merges directly to main.

If Hub source changes between phase 1 and the Hub merge, phase 2 reconverges the pin to the actual merged SHA and renews all proof.

## Ownership boundaries and cross-repository dependencies

- botster-tui owns the source migration, pins, live-lane defaults, README claims, and TUI proof.
- botster-hub owns `botster-hub-client`, `botster-hub-test-support`, the Hub binary, and the removal of `Drain`. The TUI does not restore any of it.
- botster-core owns the session worker and the terminal protocol crates. Core `93acae3` is Core main; no Core change is required.
- Cross-repository prerequisite (satisfied at plan time): Hub `205cadf` fetchable on the Hub remote. No dependency ticket is registered because the Hub integration ticket depends on this ticket's candidate, and a ticket dependency in either direction would deadlock. The prerequisite is enforced as the first Implement gate instead.
- Cross-repository handoff: the Hub integration run must consume the exact TUI candidate SHA that Review and Verify approved. The Implement report and gate evidence name that SHA.

## Assumptions and unknowns

Assumptions:

- The Hub matrix consumes the TUI candidate by Git SHA. It may additionally patch Hub crates to its own worktree; that is Hub's concern and does not change the TUI candidate.
- Hub `205cadf` will be merged without a rewrite, or the merged SHA will be given to phase 2. Either way phase 2 re-pins to the actual merged SHA.
- The three `read_frame_from_reader` call sites are the only compile failures. The Core API delta adds one method and removes nothing, so no Core-driven source change is expected.

Unknowns for Implement to resolve:

- Whether `cargo test --workspace --all-targets` at the new pins exposes a behavioral change in the shared-connection recovery stubs beyond the signature change. The scratch commit built and ran with a 21-line diff, which suggests no.
- Whether `script/test-live-hub ghostty` at Hub `205cadf` and Core `93acae3` completes with `ghostty-live-complete`. The Hub matrix previously failed at compile, so no live TUI result exists at these pins.

## Affected surfaces and files

| File | Change |
| --- | --- |
| `crates/botster-tui/Cargo.toml` | Six `rev` values (two Hub, four Core) |
| `Cargo.lock` | Hub and Core Git source lines only |
| `crates/botster-tui/src/app.rs` | Drain arm and variant removal, three assertion removals, one test deletion, three `read_frame_from_reader` calls with a persistent buffer, two live-lane revision defaults |
| `README.md` | Every sentence that names Hub `bb1a330` or Core `48a4370` |
| `docs/reports/tui-roll-hub-and-core-pins-to-the-integration-cold-cut-implement-report.md` | New report |

## Risks

- Cargo resolution against a branch-only Hub commit: if the Hub branch is force-pushed or deleted before Hub merges, `cargo build --locked` fails. Mitigation: fetchability gate at Implement start, and phase 2 re-pins to the merged SHA.
- Production build versus test build divergence (`default-features = false`): green tests can hide a broken binary graph. Mitigation: `cargo build -p botster-tui --locked` as a separate gate.
- Stale README or default revisions: mitigation is the zero-match search for both old SHAs outside `Cargo.lock` and `docs/`, including the seven-character short forms.
- Coverage loss from test deletion: only `poll_hub_does_not_send_terminal_drain` is deleted, and its single assertion cannot be expressed. The two larger tests keep every non-Drain assertion.
- Live-lane fixture mismatch: the Ghostty lane asserts `Hub fixture Core pin must match the live session worker`. Hub `205cadf` test support pins Core `93acae3`, which matches the new worker default.
- Colon in the worktree path: the run worktree path contains no colon (`git-github.com-trybotster-...`). Gates still set a colon-free `CARGO_TARGET_DIR` under `$TMPDIR` for the live lanes that build Hub binaries into a fresh target.
- Double-pin churn: phase 2 changes the Hub pin again. Review and Verify must renew, not carry, their approvals.

## Acceptance checks and tests

All commands run from the run worktree at the candidate SHA with a clean tracked tree. Each gate records `git rev-parse HEAD` and `git status --porcelain` before and after.

Pre-change gate:

```sh
git ls-remote https://github.com/trybotster/botster-hub.git refs/heads/project-pipelines/ticket_1787600679_990088
# must print 205cadf6f8dab9dc990537c2c00ef3d27edb31dd
git ls-remote https://github.com/trybotster/botster-core.git refs/heads/main
# must print 93acae3f98adbc21dc981d113c4eb2f31ead4ad0
```

Repository gates (all must exit 0):

```sh
script/fmt
script/clippy
./test.sh --workspace --all-targets
cargo build -p botster-tui --locked
cargo build --locked
```

Pin invariants:

```sh
grep -rn 'bb1a330543bc06888f894edd5f40a0f867753a12\|48a437032791e678010254708259568ce4ad02bf\|bb1a330\|48a4370' . --exclude-dir=target --exclude-dir=.git --exclude-dir=docs --exclude=Cargo.lock
# must return zero matches
grep -c 'botster-core.git?rev=93acae3f98adbc21dc981d113c4eb2f31ead4ad0' Cargo.lock   # expected 5
grep -c 'botster-hub.git?rev=205cadf6f8dab9dc990537c2c00ef3d27edb31dd' Cargo.lock    # expected 2
grep -c 'rev=bb1a330\|rev=48a4370' Cargo.lock                                          # expected 0
grep -n 'DaemonRequest::Drain\|ObservedRequest::Drain\|Drain(String)\|allow(dead_code)' crates/botster-tui/src/app.rs
# must return zero matches
git diff origin/main -- Cargo.lock | grep '^[-+]source' | grep -v 'botster-hub.git\|botster-core.git'
# must return zero lines (no registry churn)
```

Source-migration proof:

- `grep -n 'read_frame_from_reader' crates/botster-tui/src/app.rs` shows three calls, each passing a `&mut String` declared once per `BufReader` and reused across every read on that stream.
- The deleted test and the three removed assertions appear in the diff. No other test assertion is removed.

Live proof against exact Hub `205cadf` and Core `93acae3` (Hub and session-worker binaries built from those revisions into a fresh target directory):

```sh
export CARGO_TARGET_DIR="$TMPDIR/botster-tui-cold-cut-target"
export BOTSTER_HUB_BIN=/path/to/205cadf-target/debug/botster-hub
export BOTSTER_SESSION_WORKER_BIN=/path/to/205cadf-target/debug/botster-session-worker
export BOTSTER_HUB_BIN_REV=205cadf6f8dab9dc990537c2c00ef3d27edb31dd
export BOTSTER_SESSION_WORKER_BIN_REV=93acae3f98adbc21dc981d113c4eb2f31ead4ad0
script/test-live-hub ghostty          # must stream ghostty-live-complete
```

North-star shared Ghostty against the same binaries, with a caller-owned Hub connection and shared session per the README `north-star-shared` section:

```sh
export BOTSTER_HUB_CONNECTION='{...}'
export BOTSTER_SHARED_SESSION_ID=north-star-shared
script/test-live-hub ghostty-shared   # attach, input echo, exact-pair release proof
```

The live lanes must record the Hub and worker provenance (`hub_rev`, `worker_rev`) equal to the pinned revisions. A run with defaults (no `BOTSTER_*_REV` exported) must resolve to the same revisions, which proves the new defaults.

Contracts to preserve (proven by the existing test suite plus the live lanes): terminal input, output, close, reconnect, Ghostty install/scrollback/palette/mode-gated input, and one-document reconnect.

Downstream proof (charter): the Hub integration run must build and run its matrix against the exact approved TUI candidate SHA, not TUI main. The Implement report names that SHA.

Phase 2 renewal: every gate and live lane above reruns against the merged Hub SHA before the run requests merge.

## Vault gaps worth capturing

- The TUI README still names Hub `4f30d69...` and `@trybotster/hub-test-support@0.1.39` in Workspaces-lane prose. Those claims escaped earlier zero-match searches because the search only targets the pins being rolled. A capture should decide whether the search must cover every seven-character or full SHA in README, not only the old pins.
- A consumer pin to an unmerged, branch-only upstream commit is valid only while that branch exists unchanged. The two-phase sequence (candidate pin to the frozen branch SHA, then re-pin to the merged SHA) is worth one convention note for future cold cuts.
- The persistent `incomplete` buffer per stream for `read_frame_from_reader` is a new client-side contract at Hub `205cadf`. A short gotcha note should point consumers at one buffer per `BufReader`.
