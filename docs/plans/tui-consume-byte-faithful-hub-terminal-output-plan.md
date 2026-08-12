# Plan: TUI consume byte-faithful Hub terminal output

Ticket: `ticket_1786562566_712634`
Run: `run_1786568426_623487`
Step: `botster_stack_plan` (visit 4 after Plan Review `review_1786572802_594390`)
Plan revision 4

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`trybotster/botster-tui`) |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Resolved from | `list_spawn_targets` (`name=botster-tui`, `repo_name=trybotster/botster-tui`) |
| Ambient worktree | pipeline worktree for this run; routing is the ticket target, not the process cwd |
| Base | refreshed `origin/main` **`f2bc11fc8c0b14b57ebcf9b6ace4f1d80565720f`** |
| Run HEAD | **`f2bc11fc8c0b14b57ebcf9b6ace4f1d80565720f`** after recut (`git reset --hard origin/main`). `git log origin/main..HEAD` is empty. Prior HEAD `fbe6cbc` had an identical tree but two pre-squash commits; those are gone. |
| Branch | `project-pipelines/ticket_1786562566_712634` |
| `teardown_class_applies` | **false** |
| Session-type eligibility consumer | **false** |

## Plan Review corrections (rev 1 → rev 2)

| Finding | Class | Fix |
| --- | --- | --- |
| `finding_1786569520_932106` live worker `-p botster-core` and no fail-closed command | product | Worker is `-p botster-core-daemon`. Live execution is `script/test-live-hub ghostty` (rev 4). |
| `finding_1786569520_512596` kit prerequisite not in the ticket dependency list | product | Re-register `ticket_1786568835_840471` on kit `tgt_3dfae49c02454037bf13554f552baf7f` as a ticket dependency. Implement must not activate while it is open. |
| `finding_1786569520_300828` `script/fmt` fails on current-main claim-lifecycle wrapping | infra | Surgical rustfmt of the current-main drift around `app.rs` claim C2.5 (`lifecycle_live_update`). Not a product waiver. Rerun all three repo gates after the repair. |
| `finding_1786569520_788174` stale base SHA and omitted planner Must Load titles | process | Base is `f2bc11f`. Load [[spa-patterns]] plus the orchestration target/worktree notes; they do not change TUI ownership. |
| Inbox `msg_device-2_1786572576_6ec223` | product | Kit ticket closed. Pin `botster-tui-kit` to merge **`c07f793fb9ac46c24dcf1688881cd08be18ebc27`**. |
| `finding_1786572802_513483` `script/test` does not forward args | product | Live gate is **`script/test-live-hub ghostty`**, not `script/test -- --exact`. Implement adds that mode. |
| `finding_1786572803_650423` run branch not based on `f2bc11f` | process | Recut this worktree onto `f2bc11f`. `HEAD == origin/main`. |

## Repository playbook loaded

- [[botster-tui-playbook]]

## Other playbooks and notes loaded

Role / stack:

- [[planner-playbook]]
- [[botster-planner-playbook]]
- [[botster-architecture]]
- [[cli-patterns]]
- [[spa-patterns]] — planner Must Load. This ticket is TUI-only; it does not change Web/Restty. The equal-client rule still applies: do not invent a TUI-only live-output shape.
- [[botster orchestration should spawn agents with explicit target ids]] — planner Must Load. This run is bound to `tgt_c3d470bab78549df920a41e8fb0e58d8`. The kit prerequisite is registered against kit `tgt_3dfae49c02454037bf13554f552baf7f`, not this TUI target ([[cross repo dependency registration must use dependency repo target]]).
- [[botster orchestration prompts must bind agents to explicit worktrees]] — planner Must Load. Implement edits this run's TUI worktree only. Kit work stays on the kit ticket's worktree.

Consumed / adjacent charters (not edit surfaces):

- [[botster-tui-kit-playbook]]
- [[botster-hub-client-playbook]]
- [[botster-terminal-ghostty-playbook]]

TUI charter must-load notes:

- [[tui and browser are equal clients]]
- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[tui client attach uses hub protocol not session protocol]]
- [[tui and socket terminal streams use clientworker transport adapters]]
- [[botster tui uinode event routing captures hit regions during draw]]
- [[tui error dedup tests must drive real input handlers]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]
- [[incomplete repo local session types drop the hub client connection]]
- [[deleting a waiver proof test can drop unrelated coverage in its tail]]

Task-surface notes:

- [[live terminal output base64 envelopes carry renderable bytes]]
- [[botster rust consumers that share ui contract must pin one hub revision]]
- [[botster hub client crate is the external client boundary]]
- [[cold turkey migrations eliminate dual code paths and version suffixes]]
- [[external client hub tests use subprocess spawned hub test support]]
- [[test script required for rust tests not cargo test]]
- [[live hub proof records distinct hub and locked core binary provenance]]
- [[live hub target dirs can cache stale same version client schema]]
- [[renderer acceptance tests must drive real frame backend]]
- [[coredaemon attached follows initial snapshots before live terminal output]]
- [[botster clients restore visible terminal state from readscreen before buffered live output]]
- [[hub replays full history on every attach so clients must clear per cycle]]
- [[plan agents must author vault context as wikilinks not home paths]]
- [[pipeline vault checklists must cite exact resolvable note titles]]

Not loaded (out of class / out of repo):

- [[botster runtime teardown lenses]] — this ticket is byte-faithful live-output consumption, not WebRTC/peer lifecycle, SessionIo/ClientWorker teardown, multi-peer ownership, CPU/FD spin, or terminal-file vs live-runtime divergence
- [[project-pipelines-playbook]] — no Project Pipelines package/plugin paths
- [[botster-hub-playbook]] / [[botster-web-playbook]] / [[botster-core-playbook]] — adjacent ownership only; do not treat as this run's charter

## Context loaded

### Ticket intent

Consume Hub protocol 7 live `TerminalOutput` as exact bytes through the already-shipped thin Ghostty client. Do not decode live PTY frames with `String::from_utf8_lossy`. Do not invent a TUI-specific terminal protocol or dual `data` fallback. Pin the merged Hub client contract. Prove split multi-byte UTF-8, arbitrary bytes, NUL, escape sequences, GHOSTSNP-then-Attached-then-live order, reconnect, and later live output.

### Closed Hub parent (correct target)

| Field | Value |
| --- | --- |
| Ticket | `ticket_1786562565_286591` — Hub: preserve exact terminal output bytes through the client contract |
| Target | `botster-hub` / `tgt_7e208a0c76a44980a83b63af976b1f22` |
| Merge | `7499c1615078069ba391489b20c6f39c55c2d4c6` (`trybotster/botster-hub#209`) |
| Protocol | **7** (handshake flag day; exact equality) |
| Conformance | **36** |
| Wire | flattened `payload_base64` + `payload_encoding: "base64"` + `bytes` |
| Retired | JSON key `data` is rejected even on an otherwise valid envelope |
| Rust DTO | `DaemonEvent::TerminalOutput { session_id, subscription_id, payload: DaemonLiveOutputPayload }` |
| Decode API | `DaemonLiveOutputPayload::from_bytes` / `decoded_bytes()` |
| Residual | `@trybotster/hub-test-support@0.1.31` is **published and verified**. TUI still consumes the **git** `botster-hub-test-support` pin at Hub `7499c161`, not npm. |

Hub already measured that current TUI source fails to compile against `TerminalOutput.data`. Web consumer `ticket_1786562565_267926` stays separately routed.

### Current TUI baseline (`fbe6cbc`)

Pins:

| Crate | Pin |
| --- | --- |
| `botster-hub-client` / `botster-ui-contract` / `botster-hub-test-support` | Hub `89dae7e15a844bcb7411b83b32581121720e23eb` |
| `botster-tui-kit` | `32d804e3bbcb982e77113d5df12374baa8e9a2fa` (ui-contract → Hub `89dae7e`) — **superseded** by `c07f793` |
| `botster-core` / `botster-terminal-ghostty` | Core `4d0d1d8832d19352454a0789419a3e31e67d50df` |

Handshake: protocol **6** exact, conformance floor **34**.

Production live path is already Ghostty-backed:

```text
Attach/Drain (botster-hub-client)
  Snapshot.history.decoded_bytes() → GhosttyClientProjection::install_ghostsnp
  TerminalOutput.data: String     → apply_live_terminal_output(&str)
                                  → projection.apply_terminal_output(data.as_bytes())
  H1 buffer: AttachHydration.buffered_live_output: String
```

`botster-terminal-ghostty::GhosttyClientProjection::apply_terminal_output` already takes `&[u8]`. The lossy cut is **this client**: `DaemonEvent::TerminalOutput { data: String }`, `buffered_live_output: String`, `apply_live_terminal_output(&str)`, and `append_terminal_output` on the hydration-ended path.

### Registered cross-repo prerequisite (new)

Kit pin ticket `ticket_1786568835_840471` / `tgt_3dfae49c02454037bf13554f552baf7f` is **closed**. Merge **`c07f793fb9ac46c24dcf1688881cd08be18ebc27`** is `origin/main` and pins `botster-ui-contract` to Hub `7499c1615078069ba391489b20c6f39c55c2d4c6`.

`crates/botster-ui-contract` is **byte-identical** between Hub `89dae7e` and `7499c161`. Cargo still treats those git revisions as different crate identities. A TUI Hub repin without a matching kit pin splits `UiNode` and breaks the kit adapter ([[botster rust consumers that share ui contract must pin one hub revision]]).

This TUI run must **not** edit kit. Implement waits for the kit merge SHA whose `botster-ui-contract` pin is `7499c1615078069ba391489b20c6f39c55c2d4c6`.

## Scope

1. Pin as one set after the kit merge exists:
   - `botster-hub-client` = Hub **`7499c1615078069ba391489b20c6f39c55c2d4c6`**
   - `botster-ui-contract` = same Hub rev
   - `botster-hub-test-support` = same Hub rev
   - `botster-tui-kit` = **`c07f793fb9ac46c24dcf1688881cd08be18ebc27`**
   - Keep Core / `botster-terminal-ghostty` at **`4d0d1d8832d19352454a0789419a3e31e67d50df`** unless live GHOSTSNP install fails against the Hub-locked worker (see Risks)
2. Handshake: exact protocol **7**, conformance floor **36**. Reject protocol 6 as firmly as any other mismatch.
3. Consume `DaemonEvent::TerminalOutput { payload }` only. Call `payload.decoded_bytes()`. Apply the returned `Vec<u8>` to `GhosttyClientProjection::apply_terminal_output`. Never `from_utf8`, `from_utf8_lossy`, or `str` on live PTY frames.
4. Change `AttachHydration.buffered_live_output` to `Vec<u8>`. Flush those exact bytes on H5. On hydration lifecycle-end, do **not** `append_terminal_output` live bytes into the diagnostic `String`.
5. Cold turkey: delete every `TerminalOutput { data: String }` constructor and match arm. Do not accept retired `data`. Do not add a TUI envelope decoder; hub-client already validates base64 / length / retired `data`.
6. Preserve shipped H0–H5 Ghostty policy: Snapshot GHOSTSNP is visible restore authority; Scrollback is never install input; ReadScreen stays optional diagnostic text only.
7. Update README pin table, protocol/conformance narrative, and live-hub pin comments.
8. Tests listed under Acceptance. Ablate the lossy conversion and require the split-UTF-8 / invalid-byte cases to go red.

## Non-scope

- Hub, Core, Ghostty backend, kit renderer/input, or Web/Restty work
- Replacing GHOSTSNP restore with ReadScreen-first hydration
- A TUI-specific terminal protocol, dual `data` fallback, or local base64 decoder
- Publishing `@trybotster/hub-test-support`
- Session-type eligibility parent pins / `list_session_types_for_target`
- Workspaces live lanes, contract-matrix, or plugin UI work
- Runtime-teardown class work
- Speculative buffer/cap redesign of the diagnostic `terminal_output: String` used only for ReadScreen / no-projection chrome

## Repository ownership boundaries and cross-repo dependencies

| Layer | Owner | This run |
| --- | --- | --- |
| Live output wire / `DaemonLiveOutputPayload` | `botster-hub-client` @ `7499c161` | consume only |
| GHOSTSNP / `apply_terminal_output(&[u8])` | `botster-terminal-ghostty` @ `4d0d1d88` | consume only |
| TerminalView hit / chrome / SGR encode | `botster-tui-kit` @ `c07f793` | pin only |
| Install policy, byte apply, handshake, paint | **botster-tui** | **edit** |
| Web Restty byte path | `botster-web` `ticket_1786562565_267926` | separately routed |
| SessionIo / ClientWorker / PTY producer | Core / Hub | already closed parent |

Dependencies:

| Ticket | Target | Status | Role |
| --- | --- | --- | --- |
| `ticket_1786562565_286591` | hub `tgt_7e208a0c76a44980a83b63af976b1f22` | closed / merged `7499c161` | parent contract |
| `ticket_1786568835_840471` | kit `tgt_3dfae49c02454037bf13554f552baf7f` | **closed** / merged `c07f793fb9ac46c24dcf1688881cd08be18ebc27` | ui-contract identity pin |
| `ticket_1786562565_267926` | web `tgt_40abcf71ccf049f4ac0c99953a799869` | open | sibling; do not implement |

**Authoritative registration (rev 3):** `ticket_1786568835_840471` remains a ticket-level `depends_on` (`dependency_1786569661_633193`) against kit `tgt_3dfae49c02454037bf13554f552baf7f` and is now **closed**. Implement pins `botster-tui-kit` to **`c07f793fb9ac46c24dcf1688881cd08be18ebc27`** together with Hub `7499c161`. This TUI run still must not edit kit.

Do not `[patch]` ui-contract in this workspace to dodge the kit pin.

## Assumptions and unknowns

- Shipped TUI Ghostty restore stays GHOSTSNP-first. [[botster clients restore visible terminal state from readscreen before buffered live output]] still describes the generic client hydration sentence (buffer decoded live bytes, do not render opaque Snapshot/Scrollback as text). This ticket does **not** revert the closed thin-Ghostty product decision that ReadScreen is diagnostic only.
- Implement uses `payload.decoded_bytes()` and treats decode failure as fail-closed (`self.error`), not repair.
- Handshake remains exact protocol equality from hub-client. Floor 36 still accepts a future same-protocol higher fixture revision.
- Live Hub binary comes from checkout `7499c161`. Session worker comes from that checkout's locked Core **`5a9938377b492ee1fa3acfb31365ebbebccc2a96`** (Hub `89dae7e` locked `2c5171a6…`; do not attribute the Hub SHA to the worker).
- Local Ghostty projection stays on TUI's Core pin `4d0d1d88`. Unknown: whether worker `5a99383` GHOSTSNP remains installable by that pin. If live install fails, stop and register a Core pin ticket; do not silently retarget Core inside this run.
- `@trybotster/hub-test-support@0.1.31` is published (protocol 7 / conformance 36). That does not replace the kit pin. TUI rust proof stays on the git `botster-hub-test-support` pin at Hub `7499c161`.
- Worktree path has no `:`; colon-free `CARGO_TARGET_DIR` is only required if a later worktree path contains `:`.
- Tracked `.gitignore` is present and non-empty (73 bytes, matches HEAD).
- `script/fmt` currently fails on refreshed `origin/main` at the claim C2.5 `lifecycle_live_update` wrapping in `app.rs` (~6949). Independent of this ticket. Implement rustfmt-repairs that drift only, then reruns `script/fmt`, `script/test`, and `script/clippy`. That is not a waiver of the live-proof or kit-pin findings.

## Affected surfaces / files

- `crates/botster-tui/Cargo.toml` — Hub + kit pins
- `Cargo.lock` — regenerate; prove one `botster-ui-contract` source
- `crates/botster-tui/src/app.rs`
  - `MINIMUM_CONFORMANCE_FIXTURE_REVISION = 36`
  - `AttachHydration.buffered_live_output: Vec<u8>`
  - `apply_response_state` `TerminalOutput` arm
  - `apply_live_terminal_output(&[u8])`
  - remove live-byte `append_terminal_output` on hydration end
  - handshake tests (`tui_requires_protocol_6_…` → protocol 7 / revision 36)
  - every `DaemonEvent::TerminalOutput { data }` fixture (13 sites)
  - shared `late_attach_history_conformance_scenario` live-field matches (`data` → `payload.decoded_bytes()`)
  - new byte-faithful unit + live proofs
  - update live-test fallback revs from `89dae7e` / `2c5171a6` to `7499c161` / `5a993837`
  - surgical rustfmt of current-main claim C2.5 wrapping (hygiene only)
- `README.md` — Foundation pin table, protocol 7 / floor 36, live-hub SHA, `script/test-live-hub ghostty`
- `script/test-live-hub` — add `ghostty` mode (exact filter + `ghostty-live-complete` grep)
- `docs/plans/tui-consume-byte-faithful-hub-terminal-output-plan.md` — this plan
- Implement report under `docs/reports/` (repo prior art)

Production entry points that must use the new behavior:

1. `HubConnection::connect` → `tui_compatibility_requirement()` (protocol 7 / floor 36)
2. `run_loop` → `poll_hub` / `request_and_apply` → public `HubConnection::request` → `TuiApp::apply_response` → `apply_response_state`
3. `DaemonEvent::TerminalOutput` → `payload.decoded_bytes()` → H1 `Vec<u8>` buffer or H5 `GhosttyClientProjection::apply_terminal_output`
4. `open_attach_live_path` flushes buffered bytes into the same apply path
5. `draw_workspace_shell` continues to paint `ProjectionWidget` from the projection (already the production paint seam)

Code existence of `decoded_bytes` is not enough. Review must see the production `apply_response_state` arm call it and pass those bytes to the projection.

## Risks

| Risk | Mitigation |
| --- | --- |
| Dual `botster-ui-contract` after Hub pin | Pin kit `c07f793` + Hub `7499c161` as one set; `cargo tree -i botster-ui-contract` must be one source |
| Silent `[patch]` / local kit edit | Forbidden; kit work stays on `ticket_1786568835_840471` |
| Reintroducing ReadScreen-as-authority | Keep H0–H5; ReadScreen diagnostic assertions stay |
| Per-frame UTF-8 repair | `Vec<u8>` buffer + `apply_terminal_output(&[u8])`; ablation of `from_utf8_lossy` must fail split / `0xFF` cases |
| Weak split-UTF-8 proof (sleep / concatenated equality) | Deterministic producer barrier: write `[0xE2]`, observe that exact applied payload, then `[0x82, 0xAC]` |
| NUL / invalid UTF-8 only proven as DTO compile | Live path must apply those bytes; invalid `0xFF` must not become U+FFFD |
| Stale live-hub artifacts after same-version client schema change | Fresh `BOTSTER_LIVE_HUB_TARGET_DIR` when rebuilding Hub `7499c161` |
| Worker / Ghostty pin skew (`5a99383` vs `4d0d1d88`) | Record both SHAs; if `install_ghostsnp` fails, escalate a Core pin ticket |
| Soft residual instead of live proof | `script/test-live-hub ghostty` fail-closes; plain `script/test` is not the live gate |
| Wrong worker package (`-p botster-core`) | Hub/README command is `-p botster-core-daemon --bin botster-session-worker` |
| Current-main `script/fmt` red | Surgical rustfmt of claim C2.5 wrapping; rerun all three repo gates |
| Shared fixture field rename | After pin, consume `payload`, not `data`; do not fork the fixture |
| Deleting a large handshake test wholesale | Rename/update in place; keep untouched neighboring assertions ([[deleting a waiver proof test can drop unrelated coverage in its tail]]) |

## Acceptance checks / tests

### Local workspace gates

```sh
script/fmt
script/test
script/clippy
```

`script/test` is the wrapper (`BOTSTER_ENV=test`). Do not call bare `cargo test` as the gate.

After pin:

```sh
cargo tree -i botster-ui-contract --locked
```

Require exactly one `botster-ui-contract` git source at `7499c1615078069ba391489b20c6f39c55c2d4c6`. An ambiguous unqualified tree is duplicate-source evidence.

### Required unit / fixture proofs (production `apply_response`)

Drive `apply_response` with `DaemonEvent::TerminalOutput { payload: DaemonLiveOutputPayload::from_bytes(...) }`. Do not mutate `terminal_output: String` as the live-byte proof.

| Case | Assertion |
| --- | --- |
| Split UTF-8 | `[0xE2]` then `[0x82, 0xAC]`: after first payload, no U+FFFD / `EF BF BD`; after second, euro `U+20AC` is in the projection/viewport; concatenated applied bytes equal `[0xE2, 0x82, 0xAC]` |
| Arbitrary / invalid | `[0x00, 0x1b, 0xff, 0xc0]` reach `apply_terminal_output` unchanged; `0xff` is not U+FFFD |
| NUL + later marker | NUL does not drop later live bytes |
| ESC | existing styled/OSC path still applies as bytes |
| GHOSTSNP order | attaching → Snapshot (GHOSTSNP or opaque) → attached → live payload; live bytes apply only after H2/H5 rules already shipped |
| Retired `data` | constructing/decoding a live event with `data` is a hub-client reject; TUI has no fallback arm |
| Stale subscription | unchanged: stale `subscription_id` must not apply |
| Shared late-attach fixture | still waits empty drain, then applies decoded live bytes; ReadScreen text is not projection authority |
| Handshake | `PROTOCOL_VERSION == 7`, floor `36`; reject protocol 6 and 8; accept same-protocol revision > 36 |

Ablation: temporarily apply `String::from_utf8_lossy` before `apply_terminal_output`. Split-first-frame and `0xff` assertions must go red. Restore bytes path to green.

Paint proof for visible cases (euro after both fragments, later live marker) uses the existing real-frame helper (`render_app_painted` / `TestBackend`), not only viewport-cache text.

### Live Hub proof (required, not residual)

`script/test` is `cargo test --workspace --all-targets` and **does not forward arguments**. `script/test -- --exact …` is invalid and runs the full suite (Plan Review: 216 tests plus unrelated Workspaces failures when `BOTSTER_TUI_REQUIRE_HUB_TEST=1`). That is not the Ghostty live lane.

`script/test` also **soft-skips** `headless_live_runtime_ghostty_install_scrollback_palette_and_mode_gated_input` when binaries are absent.

Hub `7499c161` and this repo's README build the worker from package **`botster-core-daemon`**, not `botster-core`.

Implement adds a `ghostty` mode to `script/test-live-hub`, matching the existing session-types / workspaces wrapper:

- usage: `script/test-live-hub ghostty`
- `test_filter=app::tests::headless_live_runtime_ghostty_install_scrollback_palette_and_mode_gated_input`
- `resolve_binary` for `BOTSTER_HUB_BIN` / `BOTSTER_SESSION_WORKER_BIN` (fail closed if missing)
- wrapper already sets `BOTSTER_TUI_REQUIRE_HUB_TEST=1` and `BOTSTER_ENV=test`, runs `cargo test -p botster-tui "$test_filter" -- --exact --nocapture`, and greps `test $test_filter ... ok`
- also grep `ghostty-live-complete` (existing test println)
- update the usage line; do not change other modes

Exact fail-closed command:

```sh
HUB_SRC="<clean checkout of trybotster/botster-hub at 7499c1615078069ba391489b20c6f39c55c2d4c6>"
HUB_TGT="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/botster-hub-bytefaithful.XXXXXX")" && pwd -P)"

cargo build --locked --bin botster-hub \
  --manifest-path "$HUB_SRC/Cargo.toml" \
  --target-dir "$HUB_TGT"

cargo build --locked -p botster-core-daemon --bin botster-session-worker \
  --manifest-path "$HUB_SRC/Cargo.toml" \
  --target-dir "$HUB_TGT"

# Provenance: Hub SHA 7499c1615078069ba391489b20c6f39c55c2d4c6
# Worker source is Hub lockfile Core 5a9938377b492ee1fa3acfb31365ebbebccc2a96
# Resolve both realpaths under $HUB_TGT; do not attribute the Hub SHA to the worker.

export BOTSTER_HUB_BIN="$HUB_TGT/debug/botster-hub"
export BOTSTER_SESSION_WORKER_BIN="$HUB_TGT/debug/botster-session-worker"
export BOTSTER_HUB_BIN_REV=7499c1615078069ba391489b20c6f39c55c2d4c6
export BOTSTER_SESSION_WORKER_BIN_REV=5a9938377b492ee1fa3acfb31365ebbebccc2a96
# Optional; the wrapper already creates a colon-free temp CARGO_TARGET_DIR when unset.
# export CARGO_TARGET_DIR="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/botster-tui-bytefaithful.XXXXXX")" && pwd -P)"

script/test-live-hub ghostty
```

Missing binaries must fail this command via `resolve_binary` and `BOTSTER_TUI_REQUIRE_HUB_TEST=1`. Soft skip is a failed gate. Do not use `script/test` as the live gate. A documented wrapper exception (`BOTSTER_ENV=test cargo test -p botster-tui -- --exact …`) is only allowed if the `ghostty` mode cannot land; prefer the wrapper.

Extend that same production test (IsolatedHubBuilder, public Attach/Drain, `TuiApp::poll_hub`) rather than inventing a private protocol. Add a file-token / barrier producer:

1. Attach; prove GHOSTSNP Snapshot installed before live apply (existing assert stays).
2. Write `[0xE2]`; drain until TUI applied that exact payload (not merely “some later output”).
3. Release `[0x82, 0xAC]`; prove euro in projection / painted frame; no U+FFFD.
4. Write NUL + ESC + later ASCII marker; marker appears; no lossy repair of the invalid/NUL prefix.
5. Reconnect: full H0–H5; GHOSTSNP reinstall; later live marker after reconnect appears in the painted frame.

Do not filter live sessions by client `target_id` equality (not this ticket’s parent, and forbidden if anyone cargo-cults the session-type rule).

### Independent Plan Review base re-verification

From a clean worktree at the planned pins, Review re-runs `script/fmt`, `script/test`, `script/clippy`, and **`script/test-live-hub ghostty`** with the provenance-pinned binaries. Confirm `git rev-parse HEAD` equals `f2bc11f` plus Implement commits, and `git merge-base --is-ancestor f2bc11fc8c0b14b57ebcf9b6ace4f1d80565720f HEAD`. Soft “code exists” or a skipped live test is not enough.

## Vault gaps worth capturing

- None new required to start Implement. The live-output envelope is already [[live terminal output base64 envelopes carry renderable bytes]].
- Optional later capture if Implement discovers GHOSTSNP incompatibility between TUI Core `4d0d1d88` and Hub-locked worker `5a99383`.
- Convention tension already recorded above: generic ReadScreen-first sentence vs shipped TUI GHOSTSNP-first client. Do not write a new note unless product changes that decision.

## Implement sequencing

1. Kit `ticket_1786568835_840471` is closed. Pin `botster-tui-kit` to **`c07f793fb9ac46c24dcf1688881cd08be18ebc27`**.
2. Confirm `git merge-base --is-ancestor f2bc11fc8c0b14b57ebcf9b6ace4f1d80565720f HEAD`. Restore `.gitignore` from HEAD if wiped.
3. Surgical rustfmt of the current-main claim C2.5 wrapping so `script/fmt` is green.
4. Add `script/test-live-hub ghostty` as specified above. Update README live-hub usage.
5. Apply the pin set; regenerate lock; prove one ui-contract source.
6. Switch live-output types and handshake constants. Update live-test fallback revs.
7. Update fixtures/tests; add barrier split-UTF-8 + invalid-byte proofs to the production ghostty live test.
8. Run `script/fmt`, `script/test`, `script/clippy`, then `script/test-live-hub ghostty` with the provenance-pinned binaries, then the lossy-decode ablation.
9. Update README pins / protocol 7 / floor 36.
10. Write `docs/reports/` implement report with pins, the exact commands, ablation, and distinct Hub/Core provenance.
