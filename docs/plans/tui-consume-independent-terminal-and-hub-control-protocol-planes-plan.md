# Plan: TUI consume independent terminal and Hub control protocol planes

Ticket: `ticket_1786661009_551067`
Run: `run_1786704127_656781`
Step: `botster_stack_plan`
Plan revision 4

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`trybotster/botster-tui`) |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Resolved from | `list_spawn_targets` (`name=botster-tui`, `repo_name=trybotster/botster-tui`) |
| Ambient worktree | pipeline worktree for this run; routing is the ticket target, not the process cwd |
| Base | `origin/main` `5d2af28e92eef94d51d8f59c45ab94b8e9a58c7c` |
| Branch | `project-pipelines/ticket_1786661009_551067` |
| `teardown_class_applies` | **yes** |
| Session-type eligibility consumer | **false** |
| Implement blocked on | none. Identity parent `ticket_1786716545_950076` closed at `3bee3a57`. Slow-client parent `ticket_1786716545_417854` closed at `4f30d6952f9a29541ab3a670a54bf5e136b8eb8e`. Hello parent `ticket_1786705502_228757` closed at `aafd6c2`. |

## Plan Review corrections

| Finding | Class | Fix |
| --- | --- | --- |
| `finding_1786705308_744616` no live terminal handshake | product / blocker | Human 1B. Hub merge `aafd6c2` ships `DaemonHello.terminal_compatibility` and `DaemonHelloAck.terminal_compatibility`. TUI uses `connect_and_hello_with_terminal_requirement` plus Core `ensure_terminal_compatible`. |
| `finding_1786705308_512406` no bounded close trigger | product / high | Hub merge emits `DaemonEvent::TerminalSubscriptionClosed` on mux `Event`. Reasons: `core_adapter_closed`, `host_adapter_closed`. TUI treats that as the authoritative incomplete-stream signal. |
| `finding_1786712667_764862` TUI cannot match the active Core generation | product / high | Hub `aafd6c2` puts `generation` only on `TerminalSubscriptionClosed`. Attach, AttachState, and the mux envelope omit it. TUI owner key is the unique `(session_id, subscription_id)` it mints. Treat `generation` as close-event evidence only. Every recovery mints a new `subscription_id`. Do not register a Hub generation-on-attach ticket. |

## Implement revisit after Review `changes_required`

| Finding | TUI action | Formal owner |
| --- | --- | --- |
| `finding_1786715974_149013` dual `botster-terminal-protocol` identities | Keep decoder pin on Core `f4f6bf5`. Do not `[patch]` the same git source; Cargo rejects branch-to-rev retarget. | Hub `ticket_1786716545_950076` |
| `finding_1786715974_898936` mux poll can drop partial or concatenated frames | Persistent `mux_buf`, `Read::read`, keep partial lines, `parse_unix_mux_value` per JSON value. | TUI |
| `finding_1786715974_797854` Ghostty sequence failures must fresh-attach | Unexpected progress and apply `Err` call `recover_current_subscription`. Keep `SnapshotHistoryIncomplete` separate. | TUI |
| `finding_1786715974_781287` live test does not prove Core hard-stop or sibling isolation | Live Ghostty keeps a second Unix connection readable during a stall. Do not widen the oracle to `host_adapter_closed`. | Hub `ticket_1786716545_417854` |

## Repository playbook loaded

- [[botster-tui-playbook]]

## Other playbooks and notes loaded

Role / stack: [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[botster runtime teardown lenses]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], [[cross repo dependency registration must use dependency repo target]], [[plan agents must author vault context as wikilinks not home paths]], [[pipeline vault checklists must cite exact resolvable note titles]], [[vault example paths are not repository placement conventions]].

Adjacent charters: [[botster-hub-playbook]], [[botster-hub-client-playbook]], [[botster-tui-kit-playbook]], [[botster-terminal-ghostty-playbook]].

TUI charter must-load notes: [[kit UI contract pin proof uses an already split TUI consumer]], [[first-party Rust consumers pin the UI contract Git tag not a Hub rev]], [[compatibility fixtures advertise every required optional feature]], [[pre READY attach failed ends client hydration]], [[canceling incremental attach aborts the decoder and sends Detach]], [[Core terminal protocol separates Hub-safe envelopes from client semantic bodies]], [[tui and browser are equal clients]], [[botster tui consumes tui kit through a thin app policy adapter]], [[tui client attach uses hub protocol not session protocol]], [[tui and socket terminal streams use clientworker transport adapters]], [[botster tui uinode event routing captures hit regions during draw]], [[tui error dedup tests must drive real input handlers]], [[acceptance readiness requires the exact expected entity not any authoritative snapshot]], [[incomplete repo local session types drop the hub client connection]], [[deleting a waiver proof test can drop unrelated coverage in its tail]].

Task-surface: [[public protocol versions host control and Core terminal planes independently]], [[botster terminal v1 starts at protocol 1 and conformance revision 1]], [[incremental GHOSTSNP uses one decoder per subscription]], [[incremental GHOSTSNP attach streams READY history pages and FINISH]], [[incremental GHOSTSNP clients defer resize and input until FINISH and attached]], [[post READY history failure releases the decoder but keeps the terminal]], [[terminal subscription lifecycle is Core owned while host session policy is Hub owned]], [[Core terminal subscription ownership is session, subscription, and generation]], [[Core ClientWorker bind requires a live attach generation]], [[Core subscription hard-stop is synchronous close and drop on the host tick]], [[cold turkey migrations eliminate dual code paths and version suffixes]], [[botster hub client crate is the external client boundary]], [[external client hub tests use subprocess spawned hub test support]], [[live hub proof records distinct hub and locked core binary provenance]], [[test script required for rust tests not cargo test]], [[live hub target dirs can cache stale same version client schema]].

Not loaded: [[project-pipelines-playbook]].

## Context loaded

Shipped Hub contract at `aafd6c2cde430804f1bb54094c568fc88c15944b` (closed `ticket_1786705502_228757`):

- `DaemonHello.compatibility` remains host `DaemonCompatibilityRequirement`.
- `DaemonHello.terminal_compatibility: Option<TerminalCompatibilityRequirement>` is the live terminal requirement field.
- `DaemonHelloAck.terminal_compatibility: Option<TerminalCompatibility>` is the live terminal advertisement.
- Production connect helper: `connect_and_hello_with_terminal_requirement`. The older `connect_and_hello_with_requirement` still sends `terminal_compatibility: None` and must not be the TUI attach path.
- Client must call `ensure_terminal_compatible` on the ack. The helper checks host compatibility only.
- Hub Hello mismatch returns ack diagnostics and `UnixTerminalAdmission::Rejected { code: "terminal_compatibility" }`. Attach then returns operator error `terminal_compatibility`. TUI must fail closed after Hello and never Attach.
- Mux lines: `DaemonUnixMuxFrame::{Response, Terminal, Event}`. `type=terminal_subscription_closed` parses as `Event`.
- `DaemonEvent::TerminalSubscriptionClosed { session_id, subscription_id, generation, reason }`.
- `generation` exists only on that close event. `DaemonRequest::Attach`, `AttachState`, and `DaemonUnixTerminalEnvelope` omit it. The TUI cannot know the Core generation before close.
- Reasons: `TERMINAL_SUBSCRIPTION_CLOSED_CORE_ADAPTER` (`core_adapter_closed`) and `TERMINAL_SUBSCRIPTION_CLOSED_HOST_ADAPTER` (`host_adapter_closed`).
- Host feature `terminal_subscription_closed`. Host conformance 40. `@trybotster/hub-test-support@0.1.35`.
- Hub lockfile Core remains `f4f6bf5babe92dfb9241a760c414187f711c2c42`.

Current TUI `5d2af28` still pins Hub `f9f0d8df`, sends Drain, and applies `DaemonEvent::Snapshot`.

Closed parents: Hub Unix adapters, TUI Kit identity, Hub Hello split.

## Scope

Surgical TUI consumer cutover against Hub `aafd6c2`. Implement may edit now.

1. Pin identities
   - `botster-ui-contract = { git = "https://github.com/trybotster/botster-hub.git", tag = "botster-ui-contract-v0.3.2" }`
   - `botster-hub-client` and `botster-hub-test-support` to `4f30d6952f9a29541ab3a670a54bf5e136b8eb8e`
   - `botster-tui-kit` to `c83ba6c518e2324e34ce24c7abe5a8a05e56293c`
   - `botster-core`, `botster-terminal-ghostty`, `botster-core-test-support`, `botster-terminal-protocol-client` to Core `f4f6bf5babe92dfb9241a760c414187f711c2c42`
   - Prove one UI-contract source with `cargo tree -i botster-ui-contract`

2. Compose two live Hello handshakes
   - Host: protocol 7, floor 40, existing host features plus `unix_terminal_adapter` and `terminal_subscription_closed`.
   - Terminal: `TerminalCompatibilityRequirement::for_ready_then_history_attach()` with `client_name = "botster-tui"` on `DaemonHello.terminal_compatibility`.
   - Connect with `connect_and_hello_with_terminal_requirement`.
   - Require `ack.terminal_compatibility` and `ensure_terminal_compatible`. Missing ack field or mismatch: do not Attach.
   - Do not use host `FEATURE_TERMINAL_STREAMING` / `FEATURE_RESIZE` / `FEATURE_SNAPSHOT_DELIVERY_READY_THEN_HISTORY` as terminal-plane authority.

3. Consume mux planes
   - Read with `read_unix_mux_frame_from_reader`.
   - `Response` is host request/response.
   - `Event` carries `TerminalSubscriptionClosed`.
   - `Terminal` payload is opaque: `TerminalFrame::from_bytes` then `TerminalEvent::from_frame`.
   - Apply Core `Snapshot.phase`, `AttachStateKind`, `TerminalOutput`, `ProcessExit`.
   - Delete production terminal Drain. Keep entity pumps on the host control plane.
   - Keep `DaemonRequest::{Attach,Detach,SendInput,Resize}`.

4. One Ghostty decoder per subscription
   - One `GhosttyClientProjection` for `(session_id, subscription_id)` at attach start.
   - Same decoder consumes READY, every PAGE, and FINISH.
   - Paint at READY. Queue input and keep only the latest resize until FINISH and `Attached`, or until post-READY `SnapshotHistoryIncomplete` plus `Attached`.

5. Fail-closed recovery
   - `AttachFailed` before READY: abort decoder, drop hydration, do not open live.
   - Cancel / Detach: abort decoder and send `Detach`.
   - Decode or phase gap: `Detach`, drop leftover frames, one fresh Attach with a new subscription_id and decoder. Never replay frames.
   - `TerminalSubscriptionClosed` matching the current unique `(session_id, subscription_id)`: abort decoder, one fresh Attach that mints a new `subscription_id`. If that new pair also closes, fail closed. This is the authoritative slow-client / adapter-close signal. Do not use a 20s timer.
   - Record `generation` and `reason` as close-event evidence. Do not require generation equality to accept the event. The TUI never learned the live generation from Attach.
   - Ignore a close event whose `subscription_id` is not the current pair. A late close for a retired subscription must not abort the replacement.
   - Post-READY `SnapshotHistoryIncomplete`: keep READY terminal, release barriers after `Attached`.
   - `ProcessExit`: close that pair only.
   - Reconnect: new subscription_id, new decoder.

6. Docs and live proof
   - Update README pins, handshake, and Ghostty provenance to Hub `4f30d695` / Core `f4f6bf5`.
   - Hermetic tests for Hello split, mux `Event`, Core events, one recovery then fail closed.
   - `script/test-live-hub ghostty` against Hub `4f30d695` and Core worker `f4f6bf5`: 12,000 history lines, READY before FINISH, PAGE, live output, detach, reconnect, ProcessExited, host Status while the flood connection keeps reading, exact `core_adapter_closed`, and sibling frames after that close.
   - Do not treat a whole-mux stall `host_adapter_closed` as Core write-budget proof. Authentic `core_adapter_closed` while host frames continue is Hub `ticket_1786716545_417854`.
   - Merge directly into main. Do not create a PR.

## Non-scope

- No Hub, Core, Kit, or Web edits.
- Do not wait for Hub cold-cut, golden deletion, Web, or integration tickets.
- Do not keep Drain + mux dual production path.
- Do not use `connect_and_hello_with_requirement` for the attach connection.
- Do not invent a 20s authoritative hydration timer.
- Do not register a Hub ticket to expose generation on Attach. Unique `subscription_id` is enough for late-close isolation on this consume pin.

## Repository ownership boundaries and cross-repo dependencies

| Layer | Owner | This run |
| --- | --- | --- |
| TUI Hello/mux/hydration/decoder/live proof | `botster-tui` | edit |
| Hello split, mux Event, close event | Hub `aafd6c2` | consume |
| UI contract | tag `botster-ui-contract-v0.3.2` | pin by tag |
| Terminal semantic types | `botster-terminal-protocol-client` @ Core `f4f6bf5` | consume |
| Ghostty | `botster-terminal-ghostty` | consume |
| Kit | `c83ba6c` | pin only |

Dependencies, all closed, on their repository targets:

- `ticket_1786661008_634435` Hub Unix adapters
- `ticket_1786661009_576857` TUI Kit identity
- `ticket_1786705502_228757` Hub Hello split + `TerminalSubscriptionClosed`

Closed Hub identity follow-up:

- `ticket_1786716545_950076` pin hub-client `botster-terminal-protocol` to Core `f4f6bf5` at Hub `3bee3a57`

Open Hub follow-up:

- `ticket_1786716545_417854` emit `core_adapter_closed` while the Unix host mux stays readable

## Product decision ledger

Defaults: human 1B; require both Hello planes; mux `Terminal` + `Event`; owner key is unique `(session_id, subscription_id)`; `generation` is close-event evidence only; one recovery attach with a new `subscription_id`; decode/phase gap is immediate.

Non-goals: Drain fallback; TUI-owned grants; 20s timer as oracle.

Ask-human only if live Hub `aafd6c2` omits `ack.terminal_compatibility` when the TUI sent a requirement, or omits `TerminalSubscriptionClosed` after a write-budget close.

## Assumptions and unknowns

Assumptions: Hub `aafd6c2` is the consume pin. Attach stays session_id + subscription_id. TUI mints a unique `subscription_id` per Attach and never reuses it, so late close events for a retired subscription cannot hit the replacement. Entity pumps stay host control. Not a session-type eligibility consumer.

Unknowns Implement must verify on the live pin: mux Response/Event/Terminal interleaving on one socket; whether `third_party/botster-ui-contract` is still referenced.

## Affected surfaces / files

- `crates/botster-tui/Cargo.toml`, `Cargo.lock`
- `crates/botster-tui/src/app.rs` — `connect_and_hello_with_terminal_requirement`, host floor 40, mux read, hydration, `TerminalSubscriptionClosed`, decoder
- `README.md`
- Hermetic attach tests and `script/test-live-hub` Ghostty provenance

## Runtime-teardown answers

| Field | Answer |
| --- | --- |
| `teardown_class_applies` | yes |
| `teardown_isolation` | One unique `(session_id, subscription_id)` owns one decoder/hydration. Sibling sessions and entity pumps survive. Socket loss tears down this client. |
| `teardown_bounds` | Authoritative incomplete-stream trigger is `TerminalSubscriptionClosed` for the current pair. One recovery Attach with a new `subscription_id`. Second close or second decode/phase gap fails closed. No `block_on` Hub close. No 20s timer. |
| `late_message_matrix` | See table. |
| `production_path_proof` | `connect_and_hello_with_terminal_requirement` → `ensure_terminal_compatible` → `Attach` → mux `Terminal` → `TerminalEvent::from_frame` → one decoder → FINISH+Attached. Close event or decode/phase gap → one fresh Attach with a new `subscription_id`. Live oracle: `script/test-live-hub ghostty` on Hub `4f30d695` / Core `f4f6bf5`, including keep-reading write-budget → exact `core_adapter_closed` and sibling frames after close. |
| `ownership_identity` | TUI mints a unique `subscription_id` per Attach and never reuses it. Match close events on `session_id` + `subscription_id` only. Record `generation` as evidence. Do not compare generation against an unknown live value. |
| `sibling_fail_closed_policy` | Successful close keeps other sessions. Second recovery close fails that subscription. Prove sequential attach B survives closed A. |

### Late-message matrix

| Message | Tag | Reject after failure | Sweep |
| --- | --- | --- | --- |
| Hello `terminal_compatibility` | connection | Do not Attach after mismatch or missing ack field | No subscription yet |
| Host `Attach` | new `subscription_id` | Do not send after Hello rejection or closed socket | Drop hydration if connect is dead |
| Mux `Snapshot` READY/PAGE/FINISH | session + subscription | Ignore if not current pair; never feed a new decoder | Drop leftover frames after Detach, close, decode gap, or cancel |
| Mux `TerminalOutput` | same | Ignore unless live path is open | Drop buffered live bytes with hydration |
| Mux `AttachState` | Core kinds only | `AttachFailed` ends hydration | Clear that pair |
| Mux `ProcessExit` | same | Close only that pair | Clear decoder |
| Mux `Event` `TerminalSubscriptionClosed` | session + subscription | Ignore if `subscription_id` is not the current pair. `generation` is evidence only. | Abort decoder; mint a new `subscription_id`; one recovery; second close fails closed |
| Host `Detach` | retiring pair | Reject later mux frames and close events for that pair | Abort decoder |
| Host `SendInput` / `Resize` | current attached pair | Drop if barriers closed or pair retired | Keep latest resize while barriers hold |
| Entity subscribe | entity generation | Unrelated to terminal decoder | Do not sweep entity pumps |
| Socket EOF | connection | Fail closed all hydrations on this client | Abort every decoder |

## Risks

- Using the old Hello helper omits the terminal requirement. Mitigation: production attach path must call `connect_and_hello_with_terminal_requirement`.
- Treating HelloAck success as terminal admission. Mitigation: require ack field and `ensure_terminal_compatible` before Attach.
- Mux Event stall if TUI only reads Responses. Mitigation: one mux reader that classifies every line.
- UI-contract identity split. Mitigation: tag pin plus `cargo tree`.
- Recovery loop. Mitigation: second close or second decode gap fails closed.
- Reusing a `subscription_id` would make a late close indistinguishable from the replacement. Mitigation: mint a new `subscription_id` on every Attach and prove a late close for A cannot abort B.

## Acceptance checks / tests

- `script/fmt`, `script/clippy`, `script/test`
- `cargo tree -i botster-ui-contract` shows one tag identity
- Hello test: missing `snapshot_delivery=ready_then_history` on terminal ack fails before Attach
- Hermetic mux tests: 12,000-line READY-before-FINISH, barriers, cancel+Detach, `AttachFailed`, decode/phase-gap fresh attach with no replay, `TerminalSubscriptionClosed` one recovery then fail closed, ProcessExited isolation, sibling attach survival
- Hermetic late-close test: after recovery to subscription B, a `TerminalSubscriptionClosed` for retired subscription A must leave B's decoder and hydration intact, even if the event carries A's generation
- Mux decoder keeps a partial line across polls and emits every concatenated JSON value on one line
- Ghostty unexpected progress and post-READY apply errors call `recover_current_subscription`; `SnapshotHistoryIncomplete` stays separate
- `script/test-live-hub ghostty` with Hub bin from `4f30d695` and worker from Core `f4f6bf5`: history, PAGE, live output, detach, reconnect, ProcessExited, host Status on the flood connection, exact `core_adapter_closed`, sibling frames after close
- Do not accept `host_adapter_closed` as the Core write-budget oracle
- `cargo tree -p botster-tui -e normal -i botster-terminal-protocol` shows one identity at Core `f4f6bf5` after Hub `3bee3a57`
- Fresh `BOTSTER_LIVE_HUB_TARGET_DIR`
- Distinct Hub and Core binary realpaths
- Production entry point: Hello split + mux read; `poll_hub` does not send terminal Drain

## Vault gaps

Capture after Implement proves the path: first-party Unix clients must call `connect_and_hello_with_terminal_requirement` and treat `TerminalSubscriptionClosed` as the adapter-close signal. No capture this Plan visit.

## Worktree hygiene

- Tracked `.gitignore` exists and has 7 lines.
- Worktree path has no `:`.
- Vault checklist `checklist_1786704545_122156` already exists. This visit did not create another.
