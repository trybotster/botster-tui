# TUI Production Attach History Readback Plan

## Context Loaded

- Pipeline context: ticket `ticket_1783552998_357315`, run `run_1783636083_459002`, Plan step `botster_plan`, gate `botster_plan_gate`, target `tgt_c3d470bab78549df920a41e8fb0e58d8`. The only dependency, `ticket_1783552997_403516` (hub terminal readback), is closed; there were no prior artifacts, reviews, findings, or answers when planning began.
- Repo context: branch `project-pipelines/ticket_1783552998_357315` at `77a3605`; the worktree was clean before this plan was added. `botster-tui` currently pins `botster-hub-client` and `botster-hub-test-support` to `196d56825a93c9fe8f754e1aa8e8ce18943041b1`.
- Existing TUI path: `DogfoodApp::attach_selected_or_first` sends public `DaemonRequest::Attach`; `poll_hub` sends `DaemonRequest::Drain`; `apply_response` appends ordered `TerminalOutput`, `Snapshot`, and `Scrollback` data to `terminal_output`; `surface` renders that buffer through the existing `TerminalView`. Reconnect refreshes session rows and calls the same attach method.
- Closed hub dependency: merged hub PR #128 at `333b75fc66de7eda521e05bea5dcc5eb91b8884c` adds public `DaemonRequest::{ReadScreen,CaptureSnapshot}`, `DaemonResponseKind::{ReadScreen,CaptureSnapshot}`, `DaemonResponse.{read_screen,capture_snapshot}`, `DaemonReadScreen`, `DaemonCaptureSnapshot`, and the `terminal_readback` compatibility feature. `ReadScreen` carries renderable current-screen text; `CaptureSnapshot` carries `rows`, `cols`, optional `payload_format`, and `payload_bytes`, but not opaque snapshot bytes. Attach/drain remains the source of renderable ordered history.
- Compatibility delta verified by Plan Review: `CONFORMANCE_FIXTURE_REVISION` moves from 8 to 10, and `FEATURE_TERMINAL_READBACK` is present in both required and supported first-party feature lists. This is a required first-party capability at the new pin, not optional configurability.
- Shared conformance context: the pinned test-support revision already exposes `late_attach_history_conformance_scenario`; the hub merge retains that fixture and adds terminal-readback support to the first-party matrix. The shared scenario contains both history-then-live and no-history-then-live cases, but it is a single event vector and does not define daemon drain boundaries.
- Readiness/ordering context added after Plan Review: [[terminal subscribe readiness gates on sessionio initial snapshot delivery]] establishes that attached/subscribed readiness is emitted only after SessionIo delivers the initial snapshot to ClientWorker. [[initial terminal snapshots must precede live output activation]] establishes snapshot -> ready control -> live output ordering. The merged hub runtime test waits across repeated drains for this asynchronous delivery; an empty first drain is not a no-history signal.
- Vault/playbook context: [[identity]], [[goals]], [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[botster hub client crate is the external client boundary]], [[tui client attach uses hub protocol not session protocol]], [[botster initial terminal scrollback is delivered by sessionio directly to clientworker]], [[initial terminal history replay targets one active subscription]], [[terminal subscribe readiness gates on sessionio initial snapshot delivery]], [[initial terminal snapshots must precede live output activation]], [[coredaemon must expose terminal truth used by the production hub path]], [[retention without a reachable flush is data loss]], [[lifecycle guards evaluated before the reconciling drain are one call stale]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], [[plan agents must author vault context as wikilinks not home paths]], [[plan steps need reviewable plan artifacts]], and [[project pipelines checklist worker timeouts require artifact evidence fallback]].
- Human decision `question_1783636289_200630`: ordered attach/drain history is authoritative. Use `ReadScreen` only as a fallback when initial attach/drain yields no renderable history; never let it replace successful history. Treat `CaptureSnapshot` as metadata unless a future protocol exposes a usable payload, and prefer shared hub fixtures over a TUI-only truth cache.

## Scope

- Keep the change in the standalone `botster-tui` client and its public hub-client dependency boundary.
- Bump `botster-hub-client` and `botster-hub-test-support` together to merged hub revision `333b75fc66de7eda521e05bea5dcc5eb91b8884c` (or a later verified main revision only if required at implementation time), updating the lockfile narrowly.
- Add the public terminal-readback feature to the TUI compatibility expectation and consume only the new typed request/response DTOs.
- Model one bounded attach/reconnect restoration cycle in `DogfoodApp`:
  - begin the cycle when the production attach method sends `DaemonRequest::Attach`;
  - keep polling normal `DaemonRequest::Drain` responses while the subscription bootstrap is pending; an empty drain does not complete the cycle;
  - preserve non-empty `Snapshot`/`Scrollback` events in arrival order and mark ordered history as restored;
  - treat the subscription-scoped ready `AttachState` (`attached`, `subscribed`, or `ready`, matching the public DTO states already accepted by the TUI) as the delivery-gated decision boundary because hub emits readiness after initial SessionIo snapshot delivery, not after queueing attach work;
  - finish processing the entire response containing readiness before deciding on fallback, so any history or live `TerminalOutput` in that response is visible to the decision;
  - after readiness, issue one `DaemonRequest::ReadScreen` only when the cycle received neither renderable history nor live terminal bytes and the visible terminal buffer is empty; non-empty screen text may seed that empty buffer, but it must never replace or append to a buffer containing history, live bytes, or preserved reconnect content;
  - defensively, if ordered history arrives after a screen fallback despite the hub readiness contract, remove the fallback-owned seed and replace it with the authoritative history before appending later events, preventing duplication in both arrival directions;
  - issue one `DaemonRequest::CaptureSnapshot` after readiness for the attached session and retain its typed metadata (`rows`, `cols`, optional `payload_format`, and `payload_bytes`) for observable status/diagnostic rendering without inventing access to opaque payload bytes;
  - handle optional `ReadScreen`/`CaptureSnapshot` operator errors as non-fatal readback failures: preserve terminal bytes and attached state, advance/finish the restoration cycle, and surface compact feedback/diagnostics instead of routing these responses through the generic fatal `apply_response` error branch;
  - finish/reset restoration state on success, detach, session exit, session switch, transport failure, and a new reconnect attach.
- Reuse `append_terminal_output` and the existing `TerminalView`; add only the minimal state/helper methods needed to keep the fallback one-shot and history-first.
- Consume `botster_hub_test_support::late_attach_history_conformance_scenario` directly in Rust tests instead of copying event JSON or inventing TUI-specific fixtures.
- Extend the isolated live-hub dogfood so an already-running session produces output before a new TUI attachment, then prove the real socket attach/readback path renders prior output and remains usable for later live output.

## Non-Scope

- No embedded hub TUI resurrection and no dependency on `botster-hub` internals, raw core routers, session-worker frames, or hand-written protocol DTOs.
- No changes to botster-hub, botster-core, botster-web, Rails, Lua plugins, Project Pipelines policy, or the shared TUI kit.
- No TUI-owned scrollback database, terminal truth cache, VT parser, alternate socket, or parallel attach data plane.
- No attempt to decode or reconstruct snapshot payload bytes: the merged public DTO exposes metadata only.
- No terminal visual redesign, new screen, navigation change, or tui-kit extraction.
- No broad dependency refresh, unrelated fixture cleanup, or adjacent state-machine refactor.

## Assumptions And Unknowns

- Assumption: the merged hub revision is the authoritative dependency surface because the registered blocking ticket is closed and PR #128 is on hub `main`.
- Assumption: non-empty `Snapshot` or `Scrollback` data is the signal that ordered historical restoration succeeded. `AttachState` and `ProcessExit` remain control metadata, and `TerminalOutput` remains live data that fallback may never overwrite.
- Assumption: subscription readiness, not drain count, is the bootstrap boundary. Hub guarantees initial snapshot delivery before ready control and live activation; the TUI may therefore poll through any number of empty drains without guessing that history is absent.
- Assumption: `ReadScreen.text` is already renderable terminal text. It may only seed an empty buffer after readiness. It is never appended as history and never replaces restored or live bytes.
- Assumption: `ReadScreen` restores at most the current terminal screen, not scrollback. It is a blank-terminal mitigation only; history parity with web must come from ordered attach/drain events.
- Assumption: snapshot metadata is useful observability but not renderable restoration data until the public protocol exposes a payload. `payload_format` is optional.
- Unknown: the fresh hub pin may add required fields to local `DaemonResponse` fixtures. Mechanical compile fixes are allowed only where caused by the pin.
- Unknown: exact snapshot metadata presentation should use the smallest existing status/action-feedback row that remains visible and testable; it must not create a new panel or screen.
- Unknown: a live attach can legitimately choose `Snapshot` or `Scrollback`, and timing may vary. Live acceptance should assert prior text and later live text in order, not require one specific history variant; the shared fixture provides deterministic variant coverage.
- Convention conflict: none after the human decision. The plan preserves the external client boundary, the shared actor-owned attach path, the assigned target/worktree, path-neutral artifact references, and the ticket's explicit no-embedded-TUI constraint.

## Botster Layers And Affected Surfaces/Files

- TUI/client state and production path — `crates/botster-tui/src/app.rs`
  - import terminal-readback public DTOs/feature;
  - update the live-hub required-feature assertion near the headless dogfood compatibility checks to include `FEATURE_TERMINAL_READBACK`;
  - keep the production minimum conformance requirement derived from the client crate's bumped revision (8 -> 10), and update the hard-coded low-revision compatibility test fixture near the existing compatibility tests so it still tests the intended mismatch rather than accidentally passing;
  - add bounded restoration and snapshot-metadata state;
  - route attach, readiness-gated drains, read-screen fallback, and capture-snapshot through the existing persistent `HubConnection` request path;
  - apply typed response bodies without clobbering ordered history;
  - reset restoration state on lifecycle/transport boundaries;
  - add focused state-machine, renderer, compatibility, request-observation, and shared-fixture tests;
  - strengthen the existing isolated-hub headless dogfood with a real late-attach/reconnect history proof.
- Public dependency boundary — `crates/botster-tui/Cargo.toml`
  - update `botster-hub-client` and `botster-hub-test-support` to the same merged revision.
- Dependency resolution — `Cargo.lock`
  - accept only lockfile changes caused by those pin updates and their already-merged transitive revisions.
- Runtime harness — `script/test-live-hub`
  - change only if the existing environment cannot invoke the added late-attach proof; prefer keeping the proof inside the existing test selected by this script.
- Plan handoff — `docs/plans/tui-production-attach-history-readback-plan.md`.
- No user documentation update is expected: this completes already-promised attach behavior rather than adding a new command or configuration surface.

## Risks

- History-clobber risk: applying `ReadScreen` after ordered replay or live output would discard newer terminal truth. Mitigation: wait for subscription readiness, process its entire response, and seed only an empty buffer with no cycle bytes.
- Async-history duplication risk: an empty early drain does not mean history is absent; screen fallback followed by a delayed snapshot would duplicate output. Mitigation: never use drain count as readiness, and defensively replace a fallback-owned seed if authoritative history somehow arrives later.
- Live-output clobber risk: the shared no-history fixture includes live output after readiness. Mitigation: decide fallback only after processing the complete response and skip fallback whenever any cycle `TerminalOutput` or existing visible buffer content is present.
- Session-mixing risk: attach state for one session could affect another selected or reconnected session. Mitigation: bind restoration state to session/subscription identity and reset it on every terminal lifecycle boundary.
- Blank-terminal risk: waiting forever for a history variant would defeat the fallback. Mitigation: use the hub's delivery-gated ready `AttachState`, continuing normal drains until ready rather than inventing a timer or first-drain heuristic.
- Snapshot-overclaim risk: `payload_bytes` is not a payload and `payload_format` can be absent. Mitigation: retain/render typed metadata only and never synthesize terminal content from count or format.
- Optional-readback error risk: unknown/exited sessions can return operator errors from `ReadScreen` or `CaptureSnapshot`. Mitigation: handle these optional request responses separately from fatal attach/transport errors, preserve the terminal/attachment, finish the cycle, and test both negative paths.
- Readback/drain ordering risk: hub readback internally reconciles daemon state and retained egress. Mitigation: keep normal drain reachable, continue polling after readback, and add later-live-output plus process-exit coverage.
- Compatibility risk: the hub adds required `terminal_readback` and raises conformance revision 8 -> 10 without a protocol-version bump. Mitigation: update the three known TUI expectation/test call sites, continue deriving the production requirement from `botster-hub-client`, and assert the feature in live status evidence.
- Dependency drift risk: a newer hub main could include unrelated changes. Mitigation: start from the exact merged PR revision and move only for a demonstrated build/runtime requirement.
- False live-proof risk: tests that apply synthetic responses do not prove production wiring. Mitigation: combine shared typed fixture tests with request-observation tests and an isolated real-hub pre-output/late-attach/later-output test.
- Embedded-TUI regression risk: reaching into hub internals could appear easier. Mitigation: retain and extend the existing public-client boundary guard and dependency inspection.

## Acceptance Checks And Tests

- Focused Rust tests in `crates/botster-tui/src/app.rs` should prove:
  - slice the shared `late_attach_history_conformance_scenario().history_then_live` across explicit synthetic attach/drain responses, including at least one empty early drain; prove no fallback fires before ready, history renders before live output through the real `TerminalView` backend, and the prior marker appears exactly once;
  - slice the shared no-history scenario so ready control and live output share a response; prove the whole response is applied before the fallback decision, live bytes survive exactly once, and no `ReadScreen` is issued because the buffer is non-empty;
  - separately drive ready control with no history/live bytes and an empty buffer; prove exactly one typed `ReadScreen` is requested, non-empty screen text visibly seeds the real rendered terminal, and repeated drains/readiness do not repeat it;
  - empty `ReadScreen.text` is non-fatal and does not erase existing output;
  - a late `ReadScreen` response cannot replace history already restored in the same session/subscription cycle;
  - if a history event is defensively delivered after a fallback seed, it replaces the fallback-owned seed rather than appending duplicate prior text, while later live bytes remain ordered;
  - snapshot metadata is retained from `DaemonResponseKind::CaptureSnapshot`, rendered through the chosen existing status surface, and never treated as terminal bytes;
  - `ReadScreen` and `CaptureSnapshot` operator-error responses are non-fatal, do not clear attached state or terminal output, do not leave restoration pending, and render bounded failure feedback/diagnostics;
  - detach, process exit, session switch, transport failure, and reconnect reset restoration state so stale history decisions do not leak between cycles;
  - attach and repeated drains preserve request ordering and the same `subscription_id`, with `ReadScreen`/`CaptureSnapshot` using the attached session id only after readiness;
  - the compatibility descriptor includes `terminal_readback`, and the public-boundary guard still rejects private protocol plumbing.
- Real runtime proof in the existing isolated-hub test:
  - start a real hub and session worker through `botster-hub-test-support`;
  - create a session and produce a unique prior-output marker before constructing/attaching the TUI client;
  - attach through the production `DogfoodApp` path and poll through asynchronous drains until readiness and prior history are rendered;
  - assert the prior marker appears exactly once and the observed request log contains no `ReadScreen` on this history-present path;
  - send a distinct later-live marker and prove it renders exactly once after the restored marker;
  - require both request and rendered-state evidence, not either/or: observed requests must show typed `CaptureSnapshot` after readiness (and typed `ReadScreen` only in a separately driven empty fallback branch), while `renderer::render_to_lines` must show the resulting snapshot metadata/screen text through production surface state;
  - assert `terminal_readback` compatibility;
  - detach/shutdown cleanly so the harness does not leak a session.
- Run repo-approved verification:
  - `script/fmt`
  - `cargo test -p botster-tui`
  - `script/test`
  - `script/clippy`
  - `script/test-live-hub`
  - `git diff --check`
- Review must inspect the real path: `run_loop` -> `DogfoodApp::attach_selected_or_first` -> public `HubConnection::request(Attach)` -> repeated `Drain` in `poll_hub` -> delivery-gated ready `AttachState` after initial snapshot -> full-response history/live classification -> optional typed `ReadScreen` and `CaptureSnapshot` -> non-fatal optional-response/restoration helper -> `terminal_output`/snapshot metadata -> `surface` -> ratatui renderer. Code-existence or direct field mutation alone is insufficient.

## Pipeline Gates, Checklist Evidence, And Artifacts

- Plan artifact: this document, attached to the run as a durable `plan` artifact.
- Plan gate: submit all required fields with this artifact URI, the answered human decision, affected runtime path, risks, and test commands before requesting `botster_plan_review`.
- Vault checklist creation initially returned `plugin worker invoke timeout`, but the checklist had persisted and was subsequently loaded and completed as `checklist_1783636148_774449`. Its evidence records notes read, convention conflicts (`none`), planning verification, implementation commands, and no-capture disposition. Plan Review's informational finding is therefore resolved for Plan; Implement should update or create its own checklist evidence rather than inherit Plan evidence.
- Worktree/target: all later steps must stay on the pipeline-assigned target and ticket worktree; no ambient checkout is authorized.

## Vault Gaps Worth Capturing

- No mandatory new note. Existing notes already cover the public client boundary, SessionIo/ClientWorker history ownership, readiness after snapshot delivery, snapshot-before-live ordering, readback production seam, readback/drain retention, lifecycle reconciliation, shared fixture discipline, and artifact fallback.
- Capture only if implementation demonstrates a repeatable new rule not already covered, such as a future renderable snapshot payload. The rejected first-drain heuristic is not a capture candidate because existing readiness notes already prohibit it.
