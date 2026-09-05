# Plan: roll Hub and Core pins to the integration cold cut

Ticket: `ticket_1788460430_647093`
Run: `run_1788570301_694931`
Pipeline: Botster Stack Delivery (`botster_stack_delivery`)
Plan base: `origin/main` at `b051c67` (the run worktree was 21 commits behind main at spawn; the branch was reset to `origin/main` before planning)
Revision 2: resolves Plan Review `review_1788571199_153928` findings `finding_1788571199_226441`, `finding_1788571199_658184`, and `finding_1788571199_293708`.

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
- `[[project-pipelines-playbook]]` -- for the dependency and hold policy in the Sequencing section only.

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
- `[[cross repo dependency registration must use dependency repo target]]` -- the Hub barrier registers against the Hub target id.
- `[[dependency closure must requeue the blocked parent step]]` -- Hub ticket closure resumes the held TUI step.

`[[botster runtime teardown lenses]]` was not loaded. The ticket removes one deleted request variant and changes one helper signature. It does not change peer, session, or adapter ownership. Plan Review confirmed that the teardown class does not apply.

Pipeline context:

- Human sequencing decision `question_1788570185_464058` (Hub run) froze Hub `205cadf6f8dab9dc990537c2c00ef3d27edb31dd` and Core `93acae3f98adbc21dc981d113c4eb2f31ead4ad0`.
- Coordinator message `msg_plugin-w_1788570541_5a4ccf` under the operator's authority set the final order. The ticket description now carries it under "Coordinated candidate barrier". The Sequencing section below follows the ticket text exactly.
- Failed Hub integration evidence `artifact_1788569974_983676` names TUI `app.rs` lines 4714, 19694, 19710, and 20920 as the four compile failures against Hub `205cadf`.
- Human answer `question_1788570499_542658` (this run) chose option A: the Hub integration agent pushed the existing branch `project-pipelines/ticket_1787600679_990088` at exact commit `205cadf` to origin, with no rewrite. Hub evidence `artifact_1788570656_582032`.
- Hub integration ticket `ticket_1787600679_990088` (target `tgt_7e208a0c76a44980a83b63af976b1f22`) owns the complete matrix. Its post-Core correction requires `script/test-live-hub ghostty` and `script/prove-north-star-shared-session` in the final matrix before Hub merge.
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
5. README pin prose. Every active sentence that states the revision this crate pins changes to the new pins:
   - Every sentence that names `bb1a330...` or `48a4370...` (Foundation table, Live hub verification paragraph, the Ghostty live proof build sentence and export block, the session-types `Pins` list and its comment). Short-form SHAs count.
   - The Workspaces lanes sentence at README line 308, "the revision this crate pins, currently `4f30d6952f9a29541ab3a670a54bf5e136b8eb8e`". It is an active current-pin claim and changes to `205cadf...`.
   - Keep genuine minimum-version claims (`7a09292` or later for shared lanes, floor 48, protocol 8) and the historical public package reference `@trybotster/hub-test-support@0.1.39` in the contract-matrix paragraph. Those are not current-pin claims and have no source evidence for a change.
6. One implement report under `docs/reports/` that records the candidate SHA, gate commands, live-lane provenance, and the zero-match search.

Out of scope:

- Adopting the new `botster-hub-client` `poll_terminal` or `next_terminal` helpers. The TUI keeps its own production mux reader (`pending_mux_frames`, `mux_buf`). The ticket does not ask for that change.
- Any adapter, fallback, shim, `#[allow(dead_code)]`, or compatibility branch. The scratch commit's `#[allow(dead_code)]` on `ObservedRequest::Drain` is a hack and is not adopted.
- Compensation for any Hub or Core close defect. If a live lane exposes one, file it against Hub or Core and stop.
- Historical files under `docs/plans/**` and `docs/reports/**`. They keep their original revisions.
- Any re-pin to a Hub revision other than `205cadf`. See Sequencing.
- Merging this ticket before Hub merges exact `205cadf`.

## Sequencing and the candidate barrier

The pins stay exact through final merge: Hub `205cadf6f8dab9dc990537c2c00ef3d27edb31dd` and Core `93acae3f98adbc21dc981d113c4eb2f31ead4ad0`. There is no phase that re-pins to a later Hub main tip. Hub direct-merges the exact frozen candidate, so the merged Hub main contains `205cadf`, and the TUI pin already names the consumed revision.

Order:

1. Implement verifies fetchability before it changes any pin: `git ls-remote https://github.com/trybotster/botster-hub.git refs/heads/project-pipelines/ticket_1787600679_990088` must return `205cadf...`.
2. Implement makes the source migration, pin roll, lock update, defaults, README, and report. It commits one clean candidate on `project-pipelines/ticket_1788460430_647093`, pushes the branch, and runs every repository gate plus the isolated `ghostty` lane (Acceptance section).
3. Review approves that exact candidate SHA independently. At approval, Review registers the durable barrier: a ticket dependency from this ticket to the Hub integration ticket `ticket_1787600679_990088` on Hub target `tgt_7e208a0c76a44980a83b63af976b1f22`, with the approved TUI candidate SHA in the dependency evidence. The barrier is registered at the handoff, not before candidate creation, so it cannot block the candidate. It is not circular: the Hub ticket consumes a TUI commit SHA, not TUI ticket closure.
4. The Hub integration run consumes the exact approved TUI SHA for one complete unspliced matrix, including its `script/test-live-hub ghostty` and `script/prove-north-star-shared-session` legs, and direct-merges exact `205cadf` after the matrix passes.
5. Verify holds until Hub merge evidence exists. Verify gate evidence must include: the Hub matrix artifact id that names the TUI candidate SHA; `git ls-remote https://github.com/trybotster/botster-hub.git refs/heads/main`; and `git merge-base --is-ancestor 205cadf6f8dab9dc990537c2c00ef3d27edb31dd <hub main>` returning success in a fresh Hub fetch. Hub ticket closure requeues the held step per `[[dependency closure must requeue the blocked parent step]]`; if the engine does not requeue, the steward reactivates Verify with the same evidence.
6. Verify confirms the merged Hub pin (step 5 evidence), reruns the repository gates and the isolated `ghostty` lane at the same candidate SHA from a clean tree, records the shared-lane evidence from the Hub matrix, and approves. The run then merges directly to main.

Stop conditions:

- If Hub merges a revision that is not `205cadf`, or the Hub branch is rewritten, or the Core revision consumed by the merged Hub differs from `93acae3`: stop. Ask the human for coordination. Any new candidate on either side renews Review, the complete Hub matrix, and Verify before merge.
- If the TUI candidate changes after Review approval (for example a Review send-back), the Hub matrix must rerun against the new SHA before Hub merges.

## Ownership boundaries and cross-repository dependencies

- botster-tui owns the source migration, pins, live-lane defaults, README claims, the isolated `ghostty` proof, and the TUI-side assertions of the shared lanes.
- botster-hub owns `botster-hub-client`, `botster-hub-test-support`, the Hub binary, the removal of `Drain`, the complete integration matrix, the north-star shared-session harness, and the Hub merge. The TUI does not restore any of it.
- botster-core owns the session worker and the terminal protocol crates. Core `93acae3` is Core main; no Core change is required.
- botster-web owns the Web keep-alive leg that produces `NORTH_STAR_HISTORY` and the browser one-document reconnect proof inside the Hub matrix.
- Cross-repository prerequisite (satisfied at plan time): Hub `205cadf` fetchable on the Hub remote. Fetchability is re-checked as the first Implement gate.
- Cross-repository barrier: the ticket dependency on `ticket_1787600679_990088` (target `tgt_7e208a0c76a44980a83b63af976b1f22`), registered by Review at candidate handoff (Sequencing step 3). Barrier owner: the Hub integration ticket. Barrier proof: Sequencing step 5.

## Assumptions and unknowns

Assumptions:

- The Hub matrix consumes the TUI candidate by Git SHA. It may additionally patch Hub crates to its own worktree; that is Hub's concern and does not change the TUI candidate.
- Hub direct-merges exact `205cadf`. If the merge creates a merge commit, the ancestry check in Sequencing step 5 still passes and the TUI pin stays `205cadf`.
- The three `read_frame_from_reader` sites plus the Drain arm are the only compile failures. The Core API delta adds one method and removes nothing, so no Core-driven source change is expected.
- The engine's dependency registration on an active run holds Verify without blocking Implement or Review. If the engine blocks earlier steps, the steward removes the edge and uses the Verify gate evidence alone as the barrier.

Unknowns for Implement to resolve:

- Whether `cargo test --workspace --all-targets` at the new pins exposes a behavioral change in the shared-connection recovery stubs beyond the signature change. The scratch commit built and ran with a 21-line diff, which suggests no.
- Whether `script/test-live-hub ghostty` at Hub `205cadf` and Core `93acae3` completes with `ghostty-live-complete`. The Hub matrix previously failed at compile, so no live TUI result exists at these pins.

## Affected surfaces and files

| File | Change |
| --- | --- |
| `crates/botster-tui/Cargo.toml` | Six `rev` values (two Hub, four Core) |
| `Cargo.lock` | Hub and Core Git source lines only |
| `crates/botster-tui/src/app.rs` | Drain arm and variant removal, three assertion removals, one test deletion, three `read_frame_from_reader` calls with a persistent buffer, two live-lane revision defaults |
| `README.md` | Every active sentence that names Hub `bb1a330`, Core `48a4370`, or the current-pin claim `4f30d69` at line 308 |
| `docs/reports/tui-roll-hub-and-core-pins-to-the-integration-cold-cut-implement-report.md` | New report |

## Risks

- Branch-only Hub commit can disappear or be rewritten before Hub merges. Mitigation: fetchability gate at Implement start, and the stop condition in Sequencing.
- Production build versus test build divergence (`default-features = false`): green tests can hide a broken binary graph. Mitigation: `cargo build -p botster-tui --locked` as a separate gate.
- Stale README or default revisions. Mitigation: zero-match search for both old SHAs (full and seven-character forms) and for `4f30d69` outside `Cargo.lock` and `docs/`.
- Coverage loss from test deletion: only `poll_hub_does_not_send_terminal_drain` is deleted, and its single assertion cannot be expressed. The two larger tests keep every non-Drain assertion.
- Live-lane fixture mismatch: the isolated lane asserts `Hub fixture Core pin must match the live session worker`. Hub `205cadf` test support pins Core `93acae3`, which matches the new worker default.
- Shared-lane screen state: if the Web keep-alive leg leaves the producer on the alternate screen, `ghostty-shared` fails on `NORTH_STAR_HISTORY` without a TUI defect. Mitigation: the matrix leg contract in Acceptance.
- Environment revision labels are not running-binary proof. Mitigation: the isolated lane records the caller build receipt and binary paths; the shared lanes record the caller's receipt, socket, and session identity.
- Barrier drift: if Verify runs before Hub merge evidence exists, the merge would precede the matrix. Mitigation: the Verify gate fields in Sequencing step 5 are required, and the dependency edge holds the step.

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
grep -rn 'bb1a330543bc06888f894edd5f40a0f867753a12\|48a437032791e678010254708259568ce4ad02bf\|bb1a330\|48a4370\|4f30d6952f9a29541ab3a670a54bf5e136b8eb8e\|4f30d69' . --exclude-dir=target --exclude-dir=.git --exclude-dir=docs --exclude=Cargo.lock
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

### Live proof, isolated lane (TUI-owned, Implement and Verify)

Build Hub and session-worker binaries from a clean Hub checkout at exact `205cadf` into a fresh target directory. Write the build receipt with `script/write-claim-build-receipt` so `hub_rev` comes from `git rev-parse HEAD` of that checkout and `core_rev` comes from the Hub `Cargo.lock` (`93acae3`). Record the receipt path, both binary paths, and the receipt contents in the implement report.

```sh
export CARGO_TARGET_DIR="$TMPDIR/botster-tui-cold-cut-target"
export BOTSTER_HUB_BIN=<receipt hub_bin>
export BOTSTER_SESSION_WORKER_BIN=<receipt worker_bin>
export BOTSTER_HUB_BIN_REV=205cadf6f8dab9dc990537c2c00ef3d27edb31dd
export BOTSTER_SESSION_WORKER_BIN_REV=93acae3f98adbc21dc981d113c4eb2f31ead4ad0
script/test-live-hub ghostty
# must stream: ghostty-live-complete: hub_rev=205cadf… worker_rev=93acae3…
```

Run the lane twice: once with the two `BOTSTER_*_REV` exports, and once with them unset. Both runs must print the same `hub_rev` and `worker_rev`, which proves the new defaults. The printed revisions are labels; the receipt and binary paths are the running-binary proof.

This lane proves terminal input, output, Ghostty install, scrollback, palette, mode-gated input, and isolated-session shutdown.

### Live proof, shared lanes (north-star, Hub-matrix legs)

The caller is the Hub integration run's `script/prove-north-star-shared-session` harness at the same frozen tuple. The caller owns the Hub, the shared session, and the Web producer. The TUI run does not spawn or shut down shared resources.

Caller provenance that the TUI evidence must record verbatim: the caller's build receipt (`hub_rev` = `205cadf…`, `core_rev` = `93acae3…`, binary paths), the Hub socket path inside `BOTSTER_HUB_CONNECTION`, and the session identity `BOTSTER_SHARED_SESSION_ID=north-star-shared`. `BOTSTER_HUB_BIN` and `BOTSTER_SESSION_WORKER_BIN` must be unset for these lanes. Environment revision labels are not accepted as running-binary proof for shared lanes.

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
- Verify gate evidence includes the Hub matrix artifact id, the Hub main SHA from a fresh `ls-remote`, and a successful `git merge-base --is-ancestor 205cadf… <hub main>` (Sequencing step 5).
- Verify reruns the repository gates and the isolated lane at the candidate SHA after Hub merge, from a clean tree.

## Vault gaps worth capturing

- The TUI README carried an active current-pin claim (`4f30d69` in the Workspaces lanes paragraph) that earlier zero-match searches missed because they targeted only the pins being rolled. A capture should require every "the revision this crate pins" sentence to be part of the search, not only the old SHAs.
- A consumer candidate pinned to a frozen branch-only upstream commit, with the upstream merging that exact commit first and the consumer holding Verify on ancestry proof, is a reusable cold-cut barrier pattern worth one convention note.
- The persistent `incomplete` buffer per stream for `read_frame_from_reader` is a new client-side contract at Hub `205cadf`. A short gotcha note should point consumers at one buffer per `BufReader`.
- Shared live lanes take no binary inputs, so their running-binary provenance must come from the caller's receipt, socket, and session identity. Worth folding into the live Ghostty profile note.
