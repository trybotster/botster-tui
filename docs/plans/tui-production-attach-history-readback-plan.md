# TUI Production Attach History Readback Plan

## Context Loaded

- Pipeline context: ticket `ticket_1783552998_357315`, run `run_1783636083_459002`, Plan step `botster_plan`, gate `botster_plan_gate`, target `tgt_c3d470bab78549df920a41e8fb0e58d8`. The only dependency, `ticket_1783552997_403516` (hub terminal readback), is closed; there were no prior artifacts, reviews, findings, or answers when planning began.
- Repo context: branch `project-pipelines/ticket_1783552998_357315` at `77a3605`; the worktree was clean before this plan was added. `botster-tui` currently pins `botster-hub-client` and `botster-hub-test-support` to `196d56825a93c9fe8f754e1aa8e8ce18943041b1`.
- Existing TUI path: `DogfoodApp::attach_selected_or_first` sends public `DaemonRequest::Attach`; `poll_hub` sends `DaemonRequest::Drain`; `apply_response` appends ordered `TerminalOutput`, `Snapshot`, and `Scrollback` data to `terminal_output`; `surface` renders that buffer through the existing `TerminalView`. Reconnect refreshes session rows and calls the same attach method.
- Closed hub dependency: merged hub PR #128 at `333b75fc66de7eda521e05bea5dcc5eb91b8884c` adds public `DaemonRequest::{ReadScreen,CaptureSnapshot}`, `DaemonResponseKind::{ReadScreen,CaptureSnapshot}`, `DaemonResponse.{read_screen,capture_snapshot}`, `DaemonReadScreen`, `DaemonCaptureSnapshot`, and the `terminal_readback` compatibility feature. `ReadScreen` carries renderable current-screen text; `CaptureSnapshot` carries rows, columns, payload format, and payload byte count, but not opaque snapshot bytes. Attach/drain remains the source of renderable ordered history.
- Shared conformance context: the pinned test-support revision already exposes `late_attach_history_conformance_scenario`; the hub merge retains that fixture and adds terminal-readback support to the first-party matrix. The shared scenario contains both history-then-live and no-history-then-live cases.
- Vault/playbook context: [[identity]], [[goals]], [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[botster hub client crate is the external client boundary]], [[tui client attach uses hub protocol not session protocol]], [[botster initial terminal scrollback is delivered by sessionio directly to clientworker]], [[initial terminal history replay targets one active subscription]], [[coredaemon must expose terminal truth used by the production hub path]], [[retention without a reachable flush is data loss]], [[lifecycle guards evaluated before the reconciling drain are one call stale]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], [[plan agents must author vault context as wikilinks not home paths]], [[plan steps need reviewable plan artifacts]], and [[project pipelines checklist worker timeouts require artifact evidence fallback]].
- Human decision `question_1783636289_200630`: ordered attach/drain history is authoritative. Use `ReadScreen` only as a fallback when initial attach/drain yields no renderable history; never let it replace successful history. Treat `CaptureSnapshot` as metadata unless a future protocol exposes a usable payload, and prefer shared hub fixtures over a TUI-only truth cache.

## Scope

- Keep the change in the standalone `botster-tui` client and its public hub-client dependency boundary.
- Bump `botster-hub-client` and `botster-hub-test-support` together to merged hub revision `333b75fc66de7eda521e05bea5dcc5eb91b8884c` (or a later verified main revision only if required at implementation time), updating the lockfile narrowly.
- Add the public terminal-readback feature to the TUI compatibility expectation and consume only the new typed request/response DTOs.
- Model one bounded attach/reconnect restoration cycle in `DogfoodApp`:
  - begin the cycle when the production attach method sends `DaemonRequest::Attach`;
  - preserve non-empty `Snapshot`/`Scrollback` events in arrival order and mark ordered history as restored;
  - allow the initial attach response and first drain response to satisfy history restoration before choosing a fallback;
  - if that bootstrap produces no renderable history, issue one `DaemonRequest::ReadScreen` and seed/replace the visible terminal buffer from its non-empty text only while the cycle still has no restored history;
  - never apply a later `ReadScreen` body over a cycle that already received ordered history;
  - issue one `DaemonRequest::CaptureSnapshot` for the attached session and retain its typed metadata for observable status/diagnostic rendering without inventing access to opaque payload bytes;
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
- Assumption: non-empty `Snapshot` or `Scrollback` data is the signal that ordered historical restoration succeeded. `AttachState` and `ProcessExit` remain control metadata, and later `TerminalOutput` remains live data.
- Assumption: one initial drain is the bounded bootstrap point after attach. Waiting indefinitely to decide whether history exists would leave the terminal blank; falling back before any drain would race the normal history replay.
- Assumption: `ReadScreen.text` is already renderable terminal text. The fallback replaces/seeds the current visible buffer for that session; it is not appended as another history event.
- Assumption: snapshot metadata is useful observability but not renderable restoration data until the public protocol exposes a payload.
- Unknown: the fresh hub pin may add required fields to local `DaemonResponse` fixtures. Mechanical compile fixes are allowed only where caused by the pin.
- Unknown: exact snapshot metadata presentation should use the smallest existing status/action-feedback row that remains visible and testable; it must not create a new panel or screen.
- Unknown: a live attach can legitimately choose `Snapshot` or `Scrollback`, and timing may vary. Live acceptance should assert prior text and later live text in order, not require one specific history variant; the shared fixture provides deterministic variant coverage.
- Convention conflict: none after the human decision. The plan preserves the external client boundary, the shared actor-owned attach path, the assigned target/worktree, path-neutral artifact references, and the ticket's explicit no-embedded-TUI constraint.

## Botster Layers And Affected Surfaces/Files

- TUI/client state and production path — `crates/botster-tui/src/app.rs`
  - import terminal-readback public DTOs/feature;
  - add bounded restoration and snapshot-metadata state;
  - route attach, first drain, read-screen fallback, and capture-snapshot through the existing persistent `HubConnection` request path;
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

- History-clobber risk: applying `ReadScreen` after ordered replay would discard scrollback. Mitigation: record history success per restoration cycle and make fallback conditional and one-shot.
- Duplicate-output risk: appending screen text or firing fallback before the first drain could show the same text twice. Mitigation: wait through the initial attach/first-drain bootstrap and replace only when no history event was restored.
- Session-mixing risk: attach state for one session could affect another selected or reconnected session. Mitigation: bind restoration state to session/subscription identity and reset it on every terminal lifecycle boundary.
- Blank-terminal risk: waiting forever for a history variant would defeat the fallback. Mitigation: use the first drain as the explicit decision point and test the no-history fixture.
- Snapshot-overclaim risk: `payload_bytes` is not a payload. Mitigation: retain/render metadata only and never synthesize terminal content from the count or format.
- Readback/drain ordering risk: hub readback internally reconciles daemon state and retained egress. Mitigation: keep normal drain reachable, continue polling after readback, and add later-live-output plus process-exit coverage.
- Compatibility risk: the hub adds `terminal_readback` without a protocol-version bump. Mitigation: update the TUI's current feature/conformance requirement and assert the feature in live status evidence.
- Dependency drift risk: a newer hub main could include unrelated changes. Mitigation: start from the exact merged PR revision and move only for a demonstrated build/runtime requirement.
- False live-proof risk: tests that apply synthetic responses do not prove production wiring. Mitigation: combine shared typed fixture tests with request-observation tests and an isolated real-hub pre-output/late-attach/later-output test.
- Embedded-TUI regression risk: reaching into hub internals could appear easier. Mitigation: retain and extend the existing public-client boundary guard and dependency inspection.

## Acceptance Checks And Tests

- Focused Rust tests in `crates/botster-tui/src/app.rs` should prove:
  - the shared `late_attach_history_conformance_scenario().history_then_live` flows through `apply_response`, renders history before live output through the real `TerminalView` backend, and does not request/apply `ReadScreen` as authority;
  - the shared no-history scenario reaches the bounded fallback after initial drain, sends exactly one typed `ReadScreen`, and non-empty screen text seeds the terminal buffer;
  - empty `ReadScreen.text` is non-fatal and does not erase existing output;
  - a late `ReadScreen` response cannot replace history already restored in the same session/subscription cycle;
  - snapshot metadata is retained from `DaemonResponseKind::CaptureSnapshot`, rendered through the chosen existing status surface, and never treated as terminal bytes;
  - detach, process exit, session switch, transport failure, and reconnect reset restoration state so stale history decisions do not leak between cycles;
  - attach and first drain preserve request ordering and the same `subscription_id`, with `ReadScreen`/`CaptureSnapshot` using the attached session id;
  - the compatibility descriptor includes `terminal_readback`, and the public-boundary guard still rejects private protocol plumbing.
- Real runtime proof in the existing isolated-hub test:
  - start a real hub and session worker through `botster-hub-test-support`;
  - create a session and produce a unique prior-output marker before constructing/attaching the TUI client;
  - attach through the production `DogfoodApp` path and poll until the rendered terminal contains the prior marker;
  - send a distinct later-live marker and prove it renders after the restored marker;
  - assert typed read-screen/snapshot responses or their observable TUI state where the restoration branch invokes them, plus `terminal_readback` compatibility;
  - detach/shutdown cleanly so the harness does not leak a session.
- Run repo-approved verification:
  - `script/fmt`
  - `cargo test -p botster-tui`
  - `script/test`
  - `script/clippy`
  - `script/test-live-hub`
  - `git diff --check`
- Review must inspect the real path: `run_loop` -> `DogfoodApp::attach_selected_or_first` -> public `HubConnection::request(Attach)` -> initial `Drain` in `poll_hub` -> history-first/fallback decision -> optional typed `ReadScreen` and `CaptureSnapshot` -> `apply_response`/restoration helper -> `terminal_output`/snapshot metadata -> `surface` -> ratatui renderer. Code-existence or direct field mutation alone is insufficient.

## Pipeline Gates, Checklist Evidence, And Artifacts

- Plan artifact: this document, attached to the run as a durable `plan` artifact.
- Plan gate: submit all required fields with this artifact URI, the answered human decision, affected runtime path, risks, and test commands before requesting `botster_plan_review`.
- Vault checklist creation was attempted and returned `plugin worker invoke timeout`. Per [[project pipelines checklist worker timeouts require artifact evidence fallback]], this plan and gate evidence record:
  - notes/playbooks read: listed under Context Loaded;
  - convention conflicts: none;
  - planning verification: current pipeline context, dependency state, branch/worktree, current attach/drain/apply/render code, current pins/fixtures, merged hub PR #128 public DTOs/docs/tests, and repo scripts were inspected;
  - implementation verification required: commands listed under Acceptance Checks And Tests;
  - capture disposition: no new durable vault note from planning; the history-authority decision is explicitly preserved in the pipeline answer and this repo plan.
- Worktree/target: all later steps must stay on the pipeline-assigned target and ticket worktree; no ambient checkout is authorized.

## Vault Gaps Worth Capturing

- No mandatory new note. Existing notes already cover the public client boundary, SessionIo/ClientWorker history ownership, readback production seam, readback/drain retention, lifecycle reconciliation, shared fixture discipline, and artifact fallback.
- Capture only if implementation demonstrates a repeatable new rule not already covered: for example, a general first-drain boundary required by all terminal clients before `ReadScreen` fallback, or a snapshot payload becoming renderable in a later public protocol revision.
