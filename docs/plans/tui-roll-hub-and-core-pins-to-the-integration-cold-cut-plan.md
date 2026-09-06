# Plan: roll Hub and Core pins to the integration cold cut

Ticket: `ticket_1788460430_647093`
Run: `run_1788570301_694931`
Pipeline: Botster Stack Delivery (`botster_stack_delivery`)
Plan base: `origin/main` at `b051c67` (the run worktree was 21 commits behind main at spawn; the branch was reset to `origin/main` before planning)
Revision 3 history: applies steward correction `msg_plugin-w_1788571659_a664fb` to the gate barrier and resolves Plan Review `review_1788571199_153928` findings `finding_1788571199_226441`, `finding_1788571199_658184`, and `finding_1788571199_293708`.

Historical revision 4: human answer `question_1788573184_913439` authorizes Hub `9a02e55f06ac269188a7d81604eda6efd9584a13` after one documentation-only commit on `205cadf6f8dab9dc990537c2c00ef3d27edb31dd`. This decision supersedes the prior frozen Hub revision. Core stays at `93acae3f98adbc21dc981d113c4eb2f31ead4ad0`. The Hub-first matrix and merge barrier remains unchanged.

Historical revision 5: Verify finding C in `review_1788577387_456888` returns the isolated Ghostty pressure fixture to Implement. Human answer `question_1788577472_219314` authorizes a test-owned producer release signal and a disposable Hub source ablation. The ablation must suppress actual Core close enforcement or delivery for the negative control. It must leave no tracked Hub change. No permanent Hub or Core export is authorized.

Revision 6 preparation: Verify finding D in `review_1788657073_301842` and Root message `msg_plugin-w_1788657168_1f7475` select Hub `1a0df65230a476cfea362fdc5131e035d303a928` and Core `bf6e7d996bca2786ad4142c870a13c57a490e241`. This tuple supersedes revision 5. Root authorizes a clean preparation commit before publication approval. The lockfile remains at the prior tuple during preparation. This checkpoint is unvalidated and cannot advance to Review.

Root controls the build window. Do not resolve the lock before publication approval. Do not start tests before the lock is valid and Root assigns the window. Use the existing assigned TUI worktree for every phase. Preserve the Ghostty close fixture from `812b200` exactly. Root message `msg_plugin-w_1788657222_a67b12` preserves historical and minimum-version references. README retains `9a02e55 or later` as a minimum-version claim. Classify that match separately from active pin claims.

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
- `[[botster-runtime-reviewer-playbook]]` -- Review overlay for the terminal, transport, and live-lane surfaces this roll touches.
- `[[botster-runtime-verifier-playbook]]` -- Verify overlay; live evidence must come from exact commands, a clean tree, and production paths.
- `[[project-pipelines-playbook]]` -- for the gate barrier and hold policy in the Sequencing section only.

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
- `[[TUI live Ghostty has IsolatedHub ghostty plus attach-only ghostty-shared and ghostty-shared-exit]]` -- the three live profiles and their lifecycle oracles.
- `[[web shared session keep alive leaves the producer on the alternate screen]]` -- the Web keep-alive leg must end on the primary screen before the TUI shared attach.
- `[[live hub proof records distinct hub and locked core binary provenance]]` -- the isolated lane records separate Hub and worker source identities.
- `[[verification evidence is scoped to a stable commit and clean tree]]` -- every gate names the candidate SHA.
- `[[deleting a waiver proof test can drop unrelated coverage in its tail]]` -- remove only the Drain assertion inside larger tests.
- `[[colon worktree paths break cargo dyld library paths]]` -- colon-free `CARGO_TARGET_DIR` for gates.
- `[[test script required for rust tests not cargo test]]` -- use the repository wrappers.
- `[[cross repo dependency registration must use dependency repo target]]` -- identifies the Hub owner; the operator forbids formal dependency edges for this candidate barrier.
- `[[dependency closure must requeue the blocked parent step]]` -- describes a proposed workflow invariant, not proof of automatic resumption; the steward resumes Verify.

`[[botster runtime teardown lenses]]` was not loaded. The ticket removes one deleted request variant and changes one helper signature. It does not change peer, session, or adapter ownership. Plan Review confirmed that the teardown class does not apply.

Pipeline context:

- Historical human sequencing decision `question_1788570185_464058` (Hub run) initially froze Hub `205cadf6f8dab9dc990537c2c00ef3d27edb31dd` and Core `93acae3f98adbc21dc981d113c4eb2f31ead4ad0`.
- Coordinator message `msg_plugin-w_1788570541_5a4ccf` under the operator's authority set the final order. The ticket description now carries it under "Coordinated candidate barrier". The Sequencing section preserves that order and applies the later human revision decision.
- Failed Hub integration evidence `artifact_1788569974_983676` names TUI `app.rs` lines 4714, 19694, 19710, and 20920 as the four compile failures against Hub `205cadf`.
- Human answer `question_1788570499_542658` (this run) chose option A: the Hub integration agent pushed the existing branch `project-pipelines/ticket_1787600679_990088` at exact commit `205cadf` to origin, with no rewrite. Hub evidence `artifact_1788570656_582032`.
- Hub integration ticket `ticket_1787600679_990088` (target `tgt_7e208a0c76a44980a83b63af976b1f22`) owns the complete matrix. Its post-Core correction requires `script/test-live-hub ghostty` and `script/prove-north-star-shared-session` in the final matrix before Hub merge.
- Scratch commit `eeadb33` (parent `38e5717`, branch `proof/ticket_1787600679-e50e0f0`) is an unreviewed handoff reference. This plan reimplements it in the run worktree. It does not cherry-pick it.

Historical facts verified for revision 3 (before the authorized Hub documentation correction):

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
| `@trybotster/hub-test-support` package version at `205cadf` | `0.1.43` (unchanged from `bb1a330`; README rows that name it keep the version and change only the Hub revision) |
| Core public API delta `48a4370..93acae3` in `botster-terminal-protocol-client`, `botster-terminal-ghostty`, `botster-core-test-support` | One added method (`shared_owner_count`). No removals. |
| TUI use of removed Hub client API | Only `DaemonRequest::Drain` at `app.rs:4714`. No `drain_session` or `drain_subscription` calls. |
| TUI `read_frame_from_reader` call sites | `app.rs:19694`, `app.rs:19710`, `app.rs:20920` (all inside `mod tests`) |
| `Cargo.lock` old-pin source lines | Core `48a4370`: 5 lines. Hub `bb1a330`: 2 lines. |
| README active current-pin sentences that name a Hub revision | Foundation table rows (lines 34, 36), Live hub verification paragraph (171), Ghostty export block (199, 205), session-types `Pins` list (275) and comment (284), and the Workspaces lanes sentence "the revision this crate pins, currently `4f30d6952f9a29541ab3a670a54bf5e136b8eb8e`" (308). |
| Shared lane provenance sources | `script/test-live-hub ghostty-shared` and `ghostty-shared-exit` skip binary resolution and must not receive `BOTSTER_HUB_BIN` or `BOTSTER_SESSION_WORKER_BIN`. Their only inputs are `BOTSTER_HUB_CONNECTION` and `BOTSTER_SHARED_SESSION_ID`. Running-binary identity comes from the caller's build receipt, not from environment labels. |
| Live lane oracles in `app.rs` | isolated `ghostty` prints `ghostty-live-complete: hub_rev=… worker_rev=…` and asserts `Hub fixture Core pin must match the live session worker`; `ghostty-shared` asserts `NORTH_STAR_HISTORY` after late attach and after reconnect, echoes `NORTH_STAR_TUI_<suffix>`, requires `attach_occupancy` in Hello, and prints `ghostty-shared-complete`; `ghostty-shared-exit` prints `ghostty-shared-exit-attached` and ends on `process exited` status or the exact session entity in `exited` or `failed`. |

## Scope

In scope:

1. Source migration in `crates/botster-tui/src/app.rs` against Hub `1a0df65`:
   - Remove the `DaemonRequest::Drain { .. }` match arm at line 4714.
   - Remove the `ObservedRequest::Drain(String)` variant at line 7420. The variant has no producer after the arm is gone.
   - Remove the three assertions that state no `ObservedRequest::Drain` was observed (lines 20167, 26551, 26821). The type can no longer express a Drain request, so the assertions become unrepresentable.
   - Delete the test `poll_hub_does_not_send_terminal_drain` (line 20157). Its only assertion is the Drain check. The two larger tests at 26551 and 26821 keep every other assertion.
   - Give each `read_frame_from_reader` stream one persistent `incomplete: String` that lives as long as the `BufReader` for that stream. The recovery stub thread (lines 19694 and 19710) uses one buffer for Hello and the frame loop. The second stub (line 20920) uses one buffer for its loop.
2. Durable pin roll in `crates/botster-tui/Cargo.toml`:
   - `botster-hub-client` and `botster-hub-test-support`: `rev = "1a0df65230a476cfea362fdc5131e035d303a928"`.
   - `botster-core`, `botster-terminal-ghostty`, `botster-terminal-protocol-client`, `botster-core-test-support`: `rev = "bf6e7d996bca2786ad4142c870a13c57a490e241"`.
   - Keep the `https://github.com/trybotster/botster-core.git` and `https://github.com/trybotster/botster-hub.git` URL forms and the `rev =` selector.
   - Keep `default-features = false` on `botster-core` and `botster-core-test-support`. Keep `features = ["libghostty-vt"]` on `botster-terminal-ghostty`.
   - Do not touch `botster-tui-kit` or `botster-ui-contract`.
3. `Cargo.lock`: update only the Hub and Core Git sources. Allow only dependency changes required by the new tuple. Document each registry dependency change after resolution.
4. Ghostty live-lane defaults in `app.rs` (main lines 23532 and 23534): `BOTSTER_HUB_BIN_REV` default `1a0df65...`, `BOTSTER_SESSION_WORKER_BIN_REV` default `bf6e7d9...`.
5. README pin prose. Every active sentence that states the revision this crate pins changes to the new pins:
   - Every sentence that names `bb1a330...` or `48a4370...` (Foundation table, Live hub verification paragraph, the Ghostty live proof build sentence and export block, the session-types `Pins` list and its comment). Short-form SHAs count.
   - The Workspaces lanes sentence at README line 308, "the revision this crate pins, currently `4f30d6952f9a29541ab3a670a54bf5e136b8eb8e`". It is an active current-pin claim and changes to `1a0df65...`.
   - Keep genuine minimum-version claims (`7a09292` or later for shared lanes, floor 48, protocol 8) and the historical public package reference `@trybotster/hub-test-support@0.1.39` in the contract-matrix paragraph. Those are not current-pin claims and have no source evidence for a change.
6. Update this active plan for the authorized Hub revision and keep historical context labeled.
7. Repair the isolated Ghostty pressure fixture in `app.rs`, including one focused regression test that uses the same proof helper:
   - Configure the existing `IsolatedHubBuilder::env` pressure hook for the exact flood session. Disable inherited global pressure controls.
   - Keep the real producer. Release its output only after real Attached, pre-close Status, and the pressure marker.
   - Require Core-generated `core_adapter_closed` through the real Unix mux. Preserve the exact reason, host-close exclusion, one recovery, retired subscription and generation, and sibling progress.
   - Keep the 30-second close deadline and Core budgets unchanged.
   - Run the focused proof and the full isolated Ghostty lane with debug Hub and worker binaries.
   - Temporarily suppress actual close delivery for the exact flood session in a disposable Hub checkout. Require the focused proof to fail for missing close evidence. Record the diff, command, failure, restoration, and clean tracked Hub status.
   - Restore and rebuild Hub before final positive proof. Do not overlap source ablation with another suite.
8. One implement report under `docs/reports/` that records the candidate SHA, gate commands, live-lane provenance, and the zero-match search.

Out of scope:

- Adopting the new `botster-hub-client` `poll_terminal` or `next_terminal` helpers. The TUI keeps its own production mux reader (`pending_mux_frames`, `mux_buf`). The ticket does not ask for that change.
- Any adapter, fallback, shim, `#[allow(dead_code)]`, or compatibility branch. The scratch commit's `#[allow(dead_code)]` on `ObservedRequest::Drain` is a hack and is not adopted.
- Compensation for any Hub or Core close defect. If a live lane exposes one, file it against Hub or Core and stop.
- Unrelated historical files under `docs/plans/**` and `docs/reports/**`. They keep their original revisions. This ticket's active plan and report follow revision 6.
- Any re-pin to a Hub revision other than `1a0df65`. See Sequencing.
- Merging this ticket before Hub merges exact `1a0df65`.

## Sequencing and the candidate barrier

The pins stay exact through final merge: Hub `1a0df65230a476cfea362fdc5131e035d303a928` and Core `bf6e7d996bca2786ad4142c870a13c57a490e241`. Root coordination and Verify finding D select this exact candidate. No later Hub main tip is selected automatically. Hub direct-merges the exact frozen candidate, so the merged Hub main contains `1a0df65`, and the TUI pin already names the consumed revision.

Order:

1. Implement verifies fetchability before it changes any pin: `git ls-remote https://github.com/trybotster/botster-hub.git refs/heads/project-pipelines/ticket_1787600679_990088` must return `1a0df65...`.
2. Implement makes the source migration, pin roll, lock update, defaults, README, and report. It commits one clean candidate on `project-pipelines/ticket_1788460430_647093`, pushes the branch, and runs every repository gate plus the isolated `ghostty` lane (Acceptance section).
3. Review approves that exact candidate SHA independently. Review records the SHA for the Hub integration ticket `ticket_1787600679_990088` on target `tgt_7e208a0c76a44980a83b63af976b1f22`. Use the operator-approved Verify gate barrier. Do not register a formal ticket dependency in either direction. Coordinator decision `msg_plugin-w_1788570541_5a4ccf`, clarified by steward message `msg_plugin-w_1788571659_a664fb`, controls this exception.
4. The Hub integration run consumes the exact approved TUI SHA for one complete unspliced matrix, including its `script/test-live-hub ghostty` and `script/prove-north-star-shared-session` legs, and direct-merges exact `1a0df65` after the matrix passes.
5. Verify holds until Hub merge evidence exists. Verify gate evidence must include: the Hub matrix artifact id that names the TUI candidate SHA; `git ls-remote https://github.com/trybotster/botster-hub.git refs/heads/main`; and `git merge-base --is-ancestor 1a0df65230a476cfea362fdc5131e035d303a928 <hub main>` returning success in a fresh Hub fetch. Verify must not submit a passed gate or request advancement before this evidence exists. The steward resumes Verify when the Hub matrix and merge evidence are available.
6. Verify confirms the merged Hub pin (step 5 evidence), reruns the repository gates and the isolated `ghostty` lane at the same candidate SHA from a clean tree, records the shared-lane evidence from the Hub matrix, and approves. The run then merges directly to main.

Stop conditions:

- If Hub merges a revision that is not `1a0df65`, or the Hub branch is rewritten, or the Core revision consumed by the merged Hub differs from `bf6e7d9`: stop. Ask the human for coordination. Any new candidate on either side renews Review, the complete Hub matrix, and Verify before merge.
- If the TUI candidate changes after Review approval (for example a Review send-back), the Hub matrix must rerun against the new SHA before Hub merges.

## Ownership boundaries and cross-repository dependencies

- botster-tui owns the source migration, pins, live-lane defaults, README claims, the isolated `ghostty` proof, and the TUI-side assertions of the shared lanes.
- botster-hub owns `botster-hub-client`, `botster-hub-test-support`, the Hub binary, the removal of `Drain`, the complete integration matrix, the north-star shared-session harness, and the Hub merge. The TUI does not restore any of it.
- botster-core owns the session worker and the terminal protocol crates. Core `bf6e7d9` is published on `foundation/stale-mode-contract`; no Core change is required.
- botster-web owns the Web keep-alive leg that produces `NORTH_STAR_HISTORY` and the browser one-document reconnect proof inside the Hub matrix.
- Cross-repository prerequisite (rechecked for revision 4): Hub `1a0df65` fetchable on the Hub remote. Fetchability is re-checked as the first Implement gate.
- Cross-repository barrier: the operator-approved Verify gate holds completion until the Hub matrix and merge evidence exist. The Hub integration ticket `ticket_1787600679_990088` owns that evidence on target `tgt_7e208a0c76a44980a83b63af976b1f22`. No formal ticket dependency is registered. Barrier proof: Sequencing step 5.

## Assumptions and unknowns

Assumptions:

- The Hub matrix consumes the TUI candidate by Git SHA. It may additionally patch Hub crates to its own worktree; that is Hub's concern and does not change the TUI candidate.
- Hub direct-merges exact `1a0df65`. If the merge creates a merge commit, the ancestry check in Sequencing step 5 still passes and the TUI pin stays `1a0df65`.
- The three `read_frame_from_reader` sites plus the Drain arm are the only compile failures. The Core API delta adds one method and removes nothing, so no Core-driven source change is expected.
- Formal dependencies cannot represent this candidate barrier under the controlling operator decision. The steward coordinates resumption. Final Verify still requires every gate field in Sequencing step 5.

Unknowns for Implement to resolve:

- Whether `cargo test --workspace --all-targets` at the new pins exposes a behavioral change in the shared-connection recovery stubs beyond the signature change. The scratch commit built and ran with a 21-line diff, which suggests no.
- Whether `script/test-live-hub ghostty` at Hub `1a0df65` and Core `bf6e7d9` completes with `ghostty-live-complete`. Prior isolated proof passed at the old candidate. Renew isolated proof and the complete matrix at the new candidate.

## Affected surfaces and files

| File | Change |
| --- | --- |
| `crates/botster-tui/Cargo.toml` | Six `rev` values (two Hub, four Core) |
| `Cargo.lock` | Hub and Core Git source lines only |
| This plan | Revision 6 records the tuple decision and updates active acceptance commands |
| `crates/botster-tui/src/app.rs` | Drain arm and variant removal, three assertion removals, one test deletion, three `read_frame_from_reader` calls with a persistent buffer, two live-lane revision defaults, exact-session pressure fixture and focused proof |
| `README.md` | Every active sentence that names Hub `bb1a330`, Core `48a4370`, or the current-pin claim `4f30d69` at line 308 |
| `docs/reports/tui-roll-hub-and-core-pins-to-the-integration-cold-cut-implement-report.md` | New report |

## Risks

- Branch-only Hub commit can disappear or be rewritten before Hub merges. Mitigation: fetchability gate at Implement start, and the stop condition in Sequencing.
- Production build versus test build divergence (`default-features = false`): green tests can hide a broken binary graph. Mitigation: `cargo build -p botster-tui --locked` as a separate gate.
- Stale README or default revisions. Mitigation: zero-match search for both old SHAs (full and seven-character forms) and for `4f30d69` outside `Cargo.lock` and `docs/`.
- Coverage loss from test deletion: only `poll_hub_does_not_send_terminal_drain` is deleted, and its single assertion cannot be expressed. The two larger tests keep every non-Drain assertion.
- Live-lane fixture mismatch: the isolated lane asserts `Hub fixture Core pin must match the live session worker`. Hub `1a0df65` test support pins Core `bf6e7d9`, which matches the new worker default.
- Shared-lane screen state: if the Web keep-alive leg leaves the producer on the alternate screen, `ghostty-shared` fails on `NORTH_STAR_HISTORY` without a TUI defect. Mitigation: the matrix leg contract in Acceptance.
- Environment revision labels are not running-binary proof. Mitigation: the isolated lane records the caller build receipt and binary paths; the shared lanes record the caller's receipt, socket, and session identity.
- Barrier drift: completing Verify before Hub merge would permit an early TUI merge. Mitigation: Verify must not pass or advance without every evidence field in Sequencing step 5.

## Acceptance checks and tests

All commands run from the run worktree at the candidate SHA with a clean tracked tree. Each gate records `git rev-parse HEAD` and `git status --porcelain` before and after.

Pre-change gate:

```sh
git ls-remote https://github.com/trybotster/botster-hub.git refs/heads/project-pipelines/ticket_1787600679_990088
# must print 1a0df65230a476cfea362fdc5131e035d303a928
git ls-remote https://github.com/trybotster/botster-core.git refs/heads/foundation/stale-mode-contract
# must print bf6e7d996bca2786ad4142c870a13c57a490e241
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
grep -rn '205cadf\|bb1a330543bc06888f894edd5f40a0f867753a12\|48a437032791e678010254708259568ce4ad02bf\|bb1a330\|48a4370\|4f30d6952f9a29541ab3a670a54bf5e136b8eb8e\|4f30d69' . --exclude-dir=target --exclude-dir=.git --exclude-dir=docs --exclude=Cargo.lock
# must return zero active-pin matches; README minimum-version references are classified separately
grep -c 'botster-core.git?rev=bf6e7d996bca2786ad4142c870a13c57a490e241' Cargo.lock   # expected 5
grep -c 'botster-hub.git?rev=1a0df65230a476cfea362fdc5131e035d303a928' Cargo.lock    # expected 2
grep -c 'rev=205cadf\|rev=bb1a330\|rev=48a4370' Cargo.lock                                          # expected 0
grep -n 'DaemonRequest::Drain\|ObservedRequest::Drain\|Drain(String)\|allow(dead_code)' crates/botster-tui/src/app.rs
# must return zero matches
git diff origin/main -- Cargo.lock | grep '^[-+]source' | grep -v 'botster-hub.git\|botster-core.git'
# inspect any registry changes; permit only changes required by the new tuple
```

Source-migration proof:

- `grep -n 'read_frame_from_reader' crates/botster-tui/src/app.rs` shows three calls, each passing a `&mut String` declared once per `BufReader` and reused across every read on that stream.
- The deleted test and the three removed assertions appear in the diff. No other migration assertion is removed. The pressure repair replaces the permissive Attached-or-close check with strict Attached before producer release.

### Live proof, isolated lane (TUI-owned, Implement and Verify)

Build debug Hub and session-worker binaries from a clean Hub checkout at exact `1a0df65` into a fresh target directory. Use `cargo build --locked -p botster-hub -p botster-core-daemon --bin botster-hub --bin botster-session-worker`. Record the actual command, source status, binary paths and hashes, Hub HEAD, and Core lock source in a runtime receipt. The existing receipt writer emits release commands, so it cannot describe this required debug build accurately. Do not change that writer in this ticket.

Run the focused test through `./test.sh -p botster-tui app::tests::ghostty_live_core_close_uses_session_scoped_pressure -- --exact --nocapture`. Supply both debug binary paths and `BOTSTER_TUI_REQUIRE_HUB_TEST=1`. Require one selected test. The authorized negative control must reach the pressure marker and then fail because close evidence is missing. Restore and rebuild the clean Hub source before the final focused test and both full isolated runs.

```sh
export CARGO_TARGET_DIR="$TMPDIR/botster-tui-cold-cut-target"
export BOTSTER_HUB_BIN=<receipt hub_bin>
export BOTSTER_SESSION_WORKER_BIN=<receipt worker_bin>
export BOTSTER_HUB_BIN_REV=1a0df65230a476cfea362fdc5131e035d303a928
export BOTSTER_SESSION_WORKER_BIN_REV=bf6e7d996bca2786ad4142c870a13c57a490e241
script/test-live-hub ghostty
# must stream: ghostty-live-complete: hub_rev=1a0df65… worker_rev=bf6e7d9…
```

Run the lane twice: once with the two `BOTSTER_*_REV` exports, and once with them unset. Both runs must print the same `hub_rev` and `worker_rev`, which proves the new defaults. The printed revisions are labels; the receipt and binary paths are the running-binary proof.

This lane proves terminal input, output, Ghostty install, scrollback, palette, mode-gated input, and isolated-session shutdown.

### Live proof, shared lanes (north-star, Hub-matrix legs)

The caller is the Hub integration run's `script/prove-north-star-shared-session` harness at the same frozen tuple. The caller owns the Hub, the shared session, and the Web producer. The TUI run does not spawn or shut down shared resources.

Caller provenance that the TUI evidence must record verbatim: the caller's build receipt (`hub_rev` = `1a0df65…`, `core_rev` = `bf6e7d9…`, binary paths), the Hub socket path inside `BOTSTER_HUB_CONNECTION`, and the session identity `BOTSTER_SHARED_SESSION_ID=north-star-shared`. `BOTSTER_HUB_BIN` and `BOTSTER_SESSION_WORKER_BIN` must be unset for these lanes. Environment revision labels are not accepted as running-binary proof for shared lanes.

Named matrix legs, in order, at the same frozen tuple:

| Leg | Owner | Contract | Oracle |
| --- | --- | --- | --- |
| Web keep-alive producer | botster-web (Hub matrix) | Writes `NORTH_STAR_HISTORY` and ends on the primary screen before the TUI attaches | Caller asserts primary screen; see `[[web shared session keep alive leaves the producer on the alternate screen]]` |
| `script/test-live-hub ghostty-shared` | botster-tui, run by the Hub matrix | Late attach, `NORTH_STAR_HISTORY` visible, `NORTH_STAR_TUI_<suffix>` echo, cancel, socket-cut occupancy release through a sibling Status with `attach_occupancy` advertised in Hello, then reconnect with `NORTH_STAR_HISTORY` still visible (one-document reconnect over the caller-owned session) | Streams `ghostty-shared-complete`; session must still be running afterward |
| `script/test-live-hub ghostty-shared-exit` | botster-tui, run by the Hub matrix | Stays attached until the caller ends the session from the Hub control plane, then observes close | Streams `ghostty-shared-exit-attached` before the caller ends the session; ends on `process exited` status or the exact session entity in `exited` or `failed` |
| Browser one-document reconnect | botster-web (Hub matrix) | Browser reconnect over the same session identity | Hub matrix evidence; not a TUI oracle |

The TUI report and Verify evidence cite the Hub matrix artifact id and each leg's marker line. Close and one-document reconnect are proven by the `ghostty-shared-exit` and `ghostty-shared` legs respectively, not by the isolated lane.

### Downstream and barrier proof

- The Hub matrix runs against the exact approved TUI candidate SHA, not TUI main.
- Verify gate evidence includes the Hub matrix artifact id, the Hub main SHA from a fresh `ls-remote`, and a successful `git merge-base --is-ancestor 1a0df65… <hub main>` (Sequencing step 5).
- Verify reruns the repository gates and the isolated lane at the candidate SHA after Hub merge, from a clean tree.

## Vault gaps worth capturing

- The TUI README carried an active current-pin claim (`4f30d69` in the Workspaces lanes paragraph) that earlier zero-match searches missed because they targeted only the pins being rolled. A capture should require every "the revision this crate pins" sentence to be part of the search, not only the old SHAs.
- A consumer candidate pinned to a frozen branch-only upstream commit, with the upstream merging that exact commit first and the consumer holding Verify on ancestry proof, is a reusable cold-cut barrier pattern worth one convention note.
- The persistent `incomplete` buffer per stream for `read_frame_from_reader` is a new client-side contract at the prior Hub `205cadf`. A short gotcha note should point consumers at one buffer per `BufReader`.
- Shared live lanes take no binary inputs, so their running-binary provenance must come from the caller's receipt, socket, and session identity. Worth folding into the live Ghostty profile note.
