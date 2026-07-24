# TUI Production Attach History Readback Plan

## Context Loaded

- Pipeline context: ticket `ticket_1783552998_357315`, run `run_1783636083_459002`, Plan step `botster_plan`, gate `botster_plan_gate`, target `tgt_c3d470bab78549df920a41e8fb0e58d8`. The only dependency, `ticket_1783552997_403516` (hub terminal readback), is closed; there were no prior artifacts, reviews, findings, or answers when planning began.
- Repo context: branch `project-pipelines/ticket_1783552998_357315` at `77a3605`; the worktree was clean before this plan was added. `botster-tui` currently pins `botster-hub-client` and `botster-hub-test-support` to `196d56825a93c9fe8f754e1aa8e8ce18943041b1`.
- Existing TUI path: `TuiApp::attach_selected_or_first` sends public `DaemonRequest::Attach`; `poll_hub` sends `DaemonRequest::Drain`; `apply_response` appends ordered `TerminalOutput`, `Snapshot`, and `Scrollback` data to `terminal_output`; `surface` renders that buffer through the existing `TerminalView`. Reconnect refreshes session rows and calls the same attach method.
- Closed hub dependency: merged hub PR #128 at `333b75fc66de7eda521e05bea5dcc5eb91b8884c` adds public `DaemonRequest::{ReadScreen,CaptureSnapshot}`, `DaemonResponseKind::{ReadScreen,CaptureSnapshot}`, `DaemonResponse.{read_screen,capture_snapshot}`, `DaemonReadScreen`, `DaemonCaptureSnapshot`, and the `terminal_readback` compatibility feature. `ReadScreen` carries renderable current-screen text; `CaptureSnapshot` carries `rows`, `cols`, optional `payload_format`, and `payload_bytes`, but not opaque snapshot bytes. Attach/drain remains the source of renderable ordered history.
- Compatibility delta verified by Plan Review: `CONFORMANCE_FIXTURE_REVISION` moves from 8 to 10, and `FEATURE_TERMINAL_READBACK` is present in both required and supported first-party feature lists. This is a required first-party capability at the new pin, not optional configurability.
- Shared conformance context: the pinned test-support revision already exposes `late_attach_history_conformance_scenario`; the hub merge retains that fixture and adds terminal-readback support to the first-party matrix. The shared scenario contains both history-then-live and no-history-then-live cases, but it is a single event vector and does not define daemon drain boundaries.
- Readiness/ordering evidence after two review passes: [[terminal subscribe readiness gates on sessionio initial snapshot delivery]] and [[initial terminal snapshots must precede live output activation]] describe the stronger intended core contract, but [[adoption restart evidence must come from real protocol primitives not defaults]] prevents treating it as current daemon evidence. At the merged dependency, no production producer or real socket test proves `TerminalAttachState::Attached`; the daemon vocabulary is only `attaching | attached | detached`; and the hub shared fixture orders `attached` before `Snapshot` while core's regression fixture orders `Attaching`, `Snapshot`, `Attached`, `TerminalOutput`. Therefore this TUI plan treats `AttachState` only as lifecycle acknowledgement, not hydration readiness.
- Vault/playbook context: [[identity]], [[goals]], [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[botster hub client crate is the external client boundary]], [[tui client attach uses hub protocol not session protocol]], [[botster initial terminal scrollback is delivered by sessionio directly to clientworker]], [[initial terminal history replay targets one active subscription]], [[terminal subscribe readiness gates on sessionio initial snapshot delivery]], [[initial terminal snapshots must precede live output activation]], [[adoption restart evidence must come from real protocol primitives not defaults]], [[coredaemon must expose terminal truth used by the production hub path]], [[retention without a reachable flush is data loss]], [[lifecycle guards evaluated before the reconciling drain are one call stale]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], [[plan agents must author vault context as wikilinks not home paths]], [[plan steps need reviewable plan artifacts]], and [[project pipelines checklist worker timeouts require artifact evidence fallback]].
- Human decision `question_1783636289_200630`: ordered attach/drain history is authoritative. Use `ReadScreen` only as a fallback when initial attach/drain yields no renderable history; never let it replace successful history. Treat `CaptureSnapshot` as metadata unless a future protocol exposes a usable payload, and prefer shared hub fixtures over a TUI-only truth cache.
- Human decision `question_1783637877_215898`: choose a bounded TUI hydration window, not an `AttachState` gate and not a new blocking hub dependency. Drain until history arrives or a deadline expires; then use `ReadScreen` only if the terminal remains empty. A separate non-blocking core/hub follow-up will restore and prove the stronger attached-after-snapshot contract and align fixtures.

## Scope

- Keep the change in the standalone `botster-tui` client and its public hub-client dependency boundary.
- Bump `botster-hub-client` and `botster-hub-test-support` together to merged hub revision `333b75fc66de7eda521e05bea5dcc5eb91b8884c` (or a later verified main revision only if required at implementation time), updating the lockfile narrowly.
- Add the public terminal-readback feature to the TUI compatibility expectation and consume only the new typed request/response DTOs.
- Model one bounded attach/reconnect restoration cycle in `TuiApp`:
  - begin the cycle with the `session_id` already resolved by `attach_selected_or_first`, the current `subscription_id`, and a monotonic `Instant` deadline five seconds after that method sends `DaemonRequest::Attach`; never source hydration/readback identity from `self.attached_session`, because the production daemon path does not emit the `AttachState` event that sets it;
  - track which session owns the displayed terminal buffer and clear `terminal_output`, the fallback slot, and session-scoped snapshot metadata before every attach hydration cycle. Each `DaemonRequest::Attach` creates a new subscription whose full history replay repopulates the buffer; preserving same-session presentation would duplicate that replay on reconnect;
  - keep issuing normal `DaemonRequest::Drain` requests at the TUI run-loop cadence while hydration is pending; the live harness should use 30 ms polling to mirror the upstream regression, and an empty drain never completes the cycle by itself;
  - process every response completely and preserve `Snapshot`, `Scrollback`, and `TerminalOutput` in event order; non-empty `Snapshot`/`Scrollback` completes hydration immediately, while live `TerminalOutput` remains rendered and suppresses fallback but does not shorten the history-wait window;
  - ignore `AttachState` as a hydration boundary. `attaching` explicitly means initial data has not arrived, `attached` is only an unproven lifecycle acknowledgement in this daemon path, and `detached` ends/reset the attachment. Do not cite or add `subscribed`, `ready`, `closed`, or `unsubscribed`; the existing comparisons are known-dead and should either be removed as cleanup made necessary by the touched match or left untouched and out of scope;
  - when the five-second deadline expires, finish processing that drain response before deciding on fallback; issue one `DaemonRequest::ReadScreen` only if the visible terminal buffer is still empty and the cycle received no terminal bytes;
  - store non-empty fallback screen text in a dedicated `read_screen_fallback: Option<String>` display slot rather than appending it to the flat `terminal_output` buffer. `TerminalView` renders ordered `terminal_output` when non-empty, otherwise the fallback slot. Any later history/live bytes clear the fallback slot before normal append, so authoritative bytes supersede fallback without byte-range surgery or duplication;
  - address `ReadScreen`, `CaptureSnapshot`, and hydration-cycle drains with the cycle's attach-time `session_id`. Issue one `DaemonRequest::CaptureSnapshot` when hydration completes by ordered history or deadline, and retain its typed metadata (`rows`, `cols`, optional `payload_format`, and `payload_bytes`) for observable status/diagnostic rendering without inventing access to opaque payload bytes;
  - handle optional `ReadScreen`/`CaptureSnapshot` operator errors as non-fatal readback failures: preserve terminal bytes and attached state, advance/finish the restoration cycle, and surface compact feedback/diagnostics instead of routing these responses through the generic fatal `apply_response` error branch;
  - if `ProcessExit` or another terminal lifecycle end appears in a response, process all terminal bytes in that response, finish the cycle, and issue neither `ReadScreen` nor `CaptureSnapshot` against the now-unreadable session. Lifecycle end takes precedence over history/deadline readback decisions from the same response;
  - guarantee the cycle terminates once on ordered history, deadline, or terminal lifecycle end, and reset its deadline state on detach, session exit, transport failure, and a new reconnect attach. Every new attach cycle resets session-owned terminal/readback presentation before the new subscription replay.
- Reuse `append_terminal_output` and the existing `TerminalView`; add only the minimal state/helper methods needed to keep the fallback one-shot and history-first.
- Consume `botster_hub_test_support::late_attach_history_conformance_scenario` directly in Rust tests instead of copying event JSON or inventing TUI-specific fixtures.
- Extend the isolated live-hub live-runtime so an already-running session produces output before a new TUI attachment, then prove the real socket attach/readback path renders prior output and remains usable for later live output.

## Non-Scope

- No embedded hub TUI resurrection and no dependency on `botster-hub` internals, raw core routers, session-worker frames, or hand-written protocol DTOs.
- No changes to botster-hub, botster-core, botster-web, Rails, Lua plugins, Project Pipelines policy, or the shared TUI kit.
- No TUI-owned scrollback database, terminal truth cache, VT parser, alternate socket, or parallel attach data plane.
- No attempt to decode or reconstruct snapshot payload bytes: the merged public DTO exposes metadata only.
- No terminal visual redesign, new screen, navigation change, or tui-kit extraction.
- No repair of the pre-existing real-daemon terminal-input path that depends on never-produced `attached_session`; live verification uses direct daemon `SendInput`. That gap and the missing AttachState producer/fixture divergence are tracked separately by non-blocking follow-up `ticket_1783639801_771329`.
- No broad dependency refresh, unrelated fixture cleanup, or adjacent state-machine refactor.

## Assumptions And Unknowns

- Assumption: the merged hub revision is the authoritative dependency surface because the registered blocking ticket is closed and PR #128 is on hub `main`.
- Assumption: non-empty `Snapshot` or `Scrollback` data is the signal that ordered historical restoration succeeded. `AttachState` and `ProcessExit` remain control metadata, and `TerminalOutput` remains live data that fallback may never overwrite.
- Assumption: neither drain count nor `AttachState` proves hydration. The only successful early boundary is receipt of non-empty `Snapshot` or `Scrollback`; live `TerminalOutput` is preserved but history polling continues until the monotonic five-second deadline.
- Assumption: `ReadScreen.text` is already renderable terminal text. It may only populate the separate fallback slot after the deadline while `terminal_output` is empty. It is never appended as history and never replaces restored or live bytes.
- Assumption: `ReadScreen` restores at most the current terminal screen, not scrollback. It is a blank-terminal mitigation only; history parity with web must come from ordered attach/drain events.
- Assumption: snapshot metadata is useful observability but not renderable restoration data until the public protocol exposes a payload. `payload_format` is optional.
- Assumption: the hydration cycle owns its attach-time session identity independently of `attached_session`. `attached_session` remains existing UI lifecycle state and is not a prerequisite for this ticket's production readback requests.
- Assumption: a fresh/no-history attach remains pending for the full five-second window by design even while live output renders normally; only hydration completion and snapshot metadata are deferred, not the visible terminal stream.
- Assumption: lifecycle-ended sessions are not read back. `ProcessExit` finishes hydration without `ReadScreen`/`CaptureSnapshot`; non-fatal operator-error handling covers races where a session exits between a valid completion decision and the optional request.
- Unknown: the fresh hub pin may add required fields to local `DaemonResponse` fixtures. Mechanical compile fixes are allowed only where caused by the pin.
- Unknown: exact snapshot metadata presentation should use the smallest existing status/action-feedback row that remains visible and testable; it must not create a new panel or screen.
- Unknown: a live attach can legitimately choose `Snapshot` or `Scrollback`, and timing may vary. Live acceptance should assert prior text and later live text in order, not require one specific history variant; the shared fixture provides deterministic variant coverage.
- Convention conflict: none after the human decision. The plan preserves the external client boundary, the shared actor-owned attach path, the assigned target/worktree, path-neutral artifact references, and the ticket's explicit no-embedded-TUI constraint.

## Botster Layers And Affected Surfaces/Files

- TUI/client state and production path — `crates/botster-tui/src/app.rs`
  - import terminal-readback public DTOs/feature;
  - update the live-hub required-feature assertion near the headless live-runtime compatibility checks to include `FEATURE_TERMINAL_READBACK`;
  - keep the production minimum conformance requirement derived from the client crate's bumped revision (8 -> 10), and update the hard-coded low-revision compatibility test fixture near the existing compatibility tests so it still tests the intended mismatch rather than accidentally passing;
  - add hydration-cycle `session_id`/`subscription_id`, the five-second deadline/completion state, terminal-buffer owner session id, separate read-screen fallback slot, and snapshot-metadata state;
  - route attach, bounded drains, deadline-only read-screen fallback, and capture-snapshot through the existing persistent `HubConnection` request path;
  - apply typed response bodies without clobbering ordered history;
  - reset restoration state on lifecycle/transport boundaries;
  - add focused state-machine, renderer, compatibility, request-observation, and shared-fixture tests;
  - strengthen the existing isolated-hub headless live-runtime with a real late-attach/reconnect history proof.
- Public dependency boundary — `crates/botster-tui/Cargo.toml`
  - update `botster-hub-client` and `botster-hub-test-support` to the same merged revision.
- Dependency resolution — `Cargo.lock`
  - accept only lockfile changes caused by those pin updates and their already-merged transitive revisions.
- Runtime harness — `script/test-live-hub`
  - change only if the existing environment cannot invoke the added late-attach proof; prefer keeping the proof inside the existing test selected by this script.
- Plan handoff — `docs/plans/tui-production-attach-history-readback-plan.md`.
- No user documentation update is expected: this completes already-promised attach behavior rather than adding a new command or configuration surface.

## Risks

- History-clobber risk: applying `ReadScreen` after ordered replay or live output would discard newer terminal truth. Mitigation: decide only after the full deadline-crossing response and populate a separate fallback slot only while `terminal_output` is empty.
- Async-history duplication risk: an empty early drain does not mean history is absent, and history can arrive after fallback in a slow path. Mitigation: wait five seconds, never append fallback into `terminal_output`, and clear the fallback slot before appending any later authoritative terminal event.
- Live-output clobber risk: the shared no-history fixture includes live output after `attached`. Mitigation: ignore `AttachState` as a boundary, process the complete response, preserve live bytes through the deadline, and skip fallback when the flat buffer is non-empty.
- Session-mixing risk: hydration/readback identity could accidentally depend on never-set `attached_session` or stale selection. Mitigation: bind every cycle/request to the session/subscription ids captured directly at attach and clear presentation before each subscription's authoritative replay.
- Reconnect duplication risk: every attach creates a new subscription and the hub replays full history, so preserving the prior flat buffer duplicates terminal content. Mitigation: clear terminal/fallback/snapshot state before every attach cycle and prove the second same-session attach renders each marker exactly once.
- Blank-terminal risk: an unproven `attached` signal or unbounded drain loop would make fallback unreachable. Mitigation: a monotonic five-second deadline always completes the cycle and attempts one screen read when the buffer is empty.
- Deadline tradeoff: history delayed beyond five seconds may be briefly preceded by screen fallback. Mitigation: the value matches upstream runtime evidence, the fallback is isolated from the terminal buffer, and late authoritative bytes supersede it without duplication.
- Fresh-session latency tradeoff: a no-history attach keeps hydration pending for five seconds even while live bytes render. This is accepted because the current protocol cannot distinguish absent history from delayed history; the UI remains live and only optional readback completion is delayed.
- Snapshot-overclaim risk: `payload_bytes` is not a payload and `payload_format` can be absent. Mitigation: retain/render typed metadata only and never synthesize terminal content from count or format.
- Optional-readback error risk: unknown/exited sessions can return operator errors from `ReadScreen` or `CaptureSnapshot`. Mitigation: handle these optional request responses separately from fatal attach/transport errors, preserve the terminal/attachment, finish the cycle, and test both negative paths.
- Readback/drain ordering risk: hub readback internally reconciles daemon state and retained egress. Mitigation: keep normal drain reachable, continue polling after readback, and add later-live-output plus process-exit coverage.
- Compatibility risk: the hub adds required `terminal_readback` and raises conformance revision 8 -> 10 without a protocol-version bump. Mitigation: update the three known TUI expectation/test call sites, continue deriving the production requirement from `botster-hub-client`, and assert the feature in live status evidence.
- Dependency drift risk: a newer hub main could include unrelated changes. Mitigation: start from the exact merged PR revision and move only for a demonstrated build/runtime requirement.
- False live-proof risk: tests that apply synthetic responses do not prove production wiring. Mitigation: combine shared typed fixture tests with request-observation tests and an isolated real-hub pre-output/late-attach/later-output test.
- Embedded-TUI regression risk: reaching into hub internals could appear easier. Mitigation: retain and extend the existing public-client boundary guard and dependency inspection.

## Acceptance Checks And Tests

- Focused Rust tests in `crates/botster-tui/src/app.rs` should prove:
  - slice the shared `late_attach_history_conformance_scenario().history_then_live` across explicit synthetic attach/drain responses, including at least one empty early drain and its fixture-native `attached` before `Snapshot`; prove `AttachState` does not trigger fallback, history completes hydration before deadline, renders before live output through the real `TerminalView`, and the prior marker appears exactly once;
  - slice the shared no-history scenario so `attached` and live output share a response; prove the whole response is applied, live bytes survive exactly once while hydration remains pending until deadline, and no `ReadScreen` is issued when the deadline completes the non-empty buffer;
  - set the hydration start/deadline state so tests cross the five-second boundary without sleeping; with no history/live bytes and an empty buffer, prove exactly one typed `ReadScreen` is requested, non-empty screen text visibly fills the separate fallback slot, `terminal_output` remains empty, and repeated drains do not repeat it;
  - empty `ReadScreen.text` is non-fatal and does not erase existing output;
  - a late `ReadScreen` response cannot replace history already restored in the same session/subscription cycle;
  - if history/live arrives after a fallback, it clears the fallback slot before appending authoritative bytes, so prior and later markers remain ordered and exactly once;
  - snapshot metadata is retained from `DaemonResponseKind::CaptureSnapshot`, rendered through the chosen existing status surface, and never treated as terminal bytes;
  - `ReadScreen` and `CaptureSnapshot` operator-error responses are non-fatal, do not clear attached state or terminal output, do not leave restoration pending, and render bounded failure feedback/diagnostics;
  - hydration-cycle requests always use the `session_id` captured directly by `attach_selected_or_first`, even when `attached_session` remains `None`; tests must assert exact ReadScreen/CaptureSnapshot/Drain target ids;
  - every attach cycle clears terminal output, fallback, and snapshot metadata before hydration; a second same-session attach must replay each prior marker exactly once rather than append it to preserved content;
  - `ProcessExit` before deadline (including in the same response as terminal bytes/history) finishes hydration after applying those bytes and issues no optional readbacks; a separate race test covers the non-fatal operator error when exit occurs after the completion decision;
  - detach, process exit, session switch, transport failure, and reconnect reset the specified cycle/session-owned state so stale history decisions do not leak between cycles;
  - attach and repeated drains preserve request ordering and the same `subscription_id`, with `ReadScreen` only after deadline and `CaptureSnapshot` after either ordered-history or deadline completion for a still-readable session;
  - the compatibility descriptor includes `terminal_readback`, and the public-boundary guard still rejects private protocol plumbing.
- Real runtime proof in the existing isolated-hub test:
  - start a real hub and session worker through `botster-hub-test-support`;
  - create a session and produce a unique prior-output marker before constructing/attaching the TUI client;
  - attach through the production `TuiApp` path and poll drains at 30 ms intervals until prior history is rendered or the five-second deadline expires;
  - assert the prior marker appears exactly once and the observed request log contains no `ReadScreen` on this history-present path;
  - send a distinct later-live marker through a direct daemon `DaemonRequest::SendInput`, as the hub regression does, and prove it renders exactly once after the restored marker. Do not use `InputDispatch::TerminalForward`; its pre-existing dependency on never-set `attached_session` is out of scope and tracked separately;
  - reconnect the same `TuiApp` to the same session through the production reconnect path and prove both prior and later markers are still rendered in order exactly once after the hub's full replay;
  - use a fresh `TuiApp` (or an explicitly proven different-session clear) for a second real-daemon attachment whose terminal buffer remains empty through the bounded window; prove the cycle reaches its finished state, exactly one `ReadScreen` and one `CaptureSnapshot` target that cycle's attach-time session id, and non-empty returned screen text (when present) is shown by `renderer::render_to_lines`; if the real empty screen is empty, assert the honest empty fallback plus finished/request state and retain the synthetic non-empty render proof rather than fabricating text;
  - require both request and rendered-state evidence, not either/or: observed requests must show typed `CaptureSnapshot` on both completion paths and typed `ReadScreen` only on the expired empty-buffer path, while `renderer::render_to_lines` must show resulting snapshot metadata and any screen fallback through production surface state;
  - assert `terminal_readback` compatibility;
  - detach/shutdown cleanly so the harness does not leak a session.
- Run repo-approved verification:
  - `script/fmt`
  - `cargo test -p botster-tui`
  - `script/test`
  - `script/clippy`
  - `script/test-live-hub`
  - `git diff --check`
- Review must inspect the real path: `run_loop` -> `TuiApp::attach_selected_or_first` -> public `HubConnection::request(Attach)` plus five-second monotonic deadline -> repeated `Drain` in `poll_hub` -> complete-on-ordered-history or deadline while preserving live bytes -> optional typed `ReadScreen` plus typed `CaptureSnapshot` -> non-fatal optional-response/restoration helper -> authoritative `terminal_output` or separate fallback slot plus snapshot metadata -> `surface` -> ratatui renderer. `AttachState` is not a hydration boundary. Code-existence or direct field mutation alone is insufficient.

## Pipeline Gates, Checklist Evidence, And Artifacts

- Plan artifact: this document, attached to the run as a durable `plan` artifact.
- Plan gate: submit all required fields with this artifact URI, the answered human decision, affected runtime path, risks, and test commands before requesting `botster_plan_review`.
- Vault checklist creation initially returned `plugin worker invoke timeout`, but the checklist had persisted and was subsequently loaded and completed as `checklist_1783636148_774449`. Its evidence records notes read, convention conflicts (`none`), planning verification, implementation commands, and no-capture disposition. Plan Review's informational finding is therefore resolved for Plan; Implement should update or create its own checklist evidence rather than inherit Plan evidence.
- Worktree/target: all later steps must stay on the pipeline-assigned target and ticket worktree; no ambient checkout is authorized.

## Vault Gaps Worth Capturing

- No mandatory new note. Existing notes cover the public client boundary, SessionIo/ClientWorker history ownership, intended readiness/snapshot ordering, real-protocol evidence discipline, readback production seam, readback/drain retention, lifecycle reconciliation, shared fixture discipline, and artifact fallback. The human-routed fixture/producer contradiction is being captured by a separate non-blocking core/hub follow-up.
- Capture only if implementation demonstrates a repeatable new rule not already covered, such as a future renderable snapshot payload. Neither the rejected first-drain heuristic nor the unproven `AttachState` gate should become a convention.
