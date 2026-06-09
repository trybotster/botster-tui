# TUI Dogfood UiNode Hub Surface Plan

Ticket: Dogfood one botster-tui surface through UiNode and hub client contracts
Run: run_1780968484_560034
Step: botster_plan
Target: tgt_c3d470bab78549df920a41e8fb0e58d8
Worktree: current assigned pipeline worktree

## Context Loaded

- Pipeline context from `project_pipelines_current_context`: ticket, run, active Plan step, gate prompt, three closed dependency tickets, no prior artifacts, no open findings, no questions, and no prior answers.
- Role playbooks: [[planner-playbook]], [[botster-planner-playbook]].
- Required Botster vault context: [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]].
- Plan Review follow-up context loaded after `review_1780968989_901837`: [[botster local client api lives over hubruntime not raw core routers]], [[botster tui attach must explicitly pull core entities after subscribing]], [[botster data plane bypasses the hub through session and client actors]], [[botster hub client state sync is entity frame only]], [[botster-core local process runtime is feature-gated from contract-only embeds]], [[nested rich tuis lose scrolling when botster consumes mouse reports or control keys]], [[botster hub daemon startup requires explicit data dir]], [[botster hub socket liveness requires a protocol handshake]].
- Identity/current goals: [[identity]], [[goals]].
- TUI skill context: `botster-customize-tui`, especially the shared UI contract forms and semantic renderer-neutral action guidance.
- Repo evidence: `crates/botster-tui` already consumes `botster-core`, renders shared `UiNode` primitives, dispatches semantic `UiActionRequest` values, has terminal-view input forwarding tests, and still runs the interactive app against `renderer::demo_ui_node()`.
- Pinned dependency evidence from `botster-core` rev `8f2f4acf`: `botster-core` with `default-features = false` exports `TransportIngress`, `TransportEgress`, `ClientStreamHarness`, `ClientStreamOutcome`, `ClientStreamObservation`, `EntityFrame`, `EntityStore`, `EntityStores`, `EntityKind`, `EntityId`, `ClientId`, `SessionId`, `SubscriptionId`, `RequestId`, `ResizePayload`, and session protocol hello/frame helpers. It does not export `HubClientApi`, `HubRuntime`, `HubDaemon`, `DefaultBotsterEngine`, or the local PTY process runtime under this feature set.
- Baseline command evidence: plain `script/test` fails before tests run because the Botster session worktree path contains `:` and Rust rejects it in `DYLD_FALLBACK_LIBRARY_PATH`; `CARGO_TARGET_DIR=/tmp/botster-tui-plan-target script/test` passes 24 tests; `CARGO_TARGET_DIR=/tmp/botster-tui-plan-target cargo run -p botster-tui -- --smoke` prints `botster-tui smoke ok`.
- Checklist evidence: `project_pipelines_create_vault_checklist` timed out in the plugin worker. A fallback custom checklist attempt failed on `UNIQUE constraint failed: checklist_items.id`. Following [[project pipelines checklist worker timeouts require artifact evidence fallback]], this plan records vault provenance, convention conflict status, verification evidence, and capture decision in this artifact and gate evidence.
- Plan Review disposition: this revision addresses findings `finding_1780968989_399045`, `finding_1780968989_629881`, `finding_1780968989_463805`, `finding_1780968989_104244`, and `finding_1780968989_249621` by naming the contract-only socket framing path, resolving hub provisioning, loading the missing authority notes, and requiring real socket round-trip evidence.

## Scope

Implement one real TUI dogfood surface that runs through the existing shared `UiNode` renderer, semantic action router, entity-frame/client boundary, and `terminal_view` path.

Chosen surface: session spawn/attach. This is the preferred ticket path because it exercises a form-like request, semantic submit, success state, validation/error state, and terminal output attachment in one operator flow.

Planned work:

- Add a small socket transport client boundary in `botster-tui` that connects to a running local Botster hub socket and frames the public `botster-core` contract types over that socket. The TUI must not pretend `HubClientApi` or `HubRuntime` exists in this repo's current dependency set.
- Use the concrete contract-only surfaces available from `botster-core`: `TransportIngress::{SubscribeSession, TerminalInput, Resize, RequestSnapshot, Focus, Ping, BoundaryPayload}`, `TransportEgress::{TerminalOutput, Snapshot, Scrollback, ProcessExit, AttachState, FocusChanged, BoundaryPayload, Pong, Close}`, `ClientStreamHarness`/`ClientStreamOutcome` for local routing semantics tests, and `EntityFrame::{Snapshot, ScopedSnapshot, Upsert, Patch, Remove}` for read-model hydration.
- Connect path: open a Unix stream to a caller-supplied or documented hub socket path, prove Botster identity with the protocol `hello` / `hello_ack` handshake, then send/receive framed contract payloads. Implement must inspect the existing hub socket framing before choosing JSON line, binary frame, or boundary-payload envelope encoding; do not invent a parallel wire format if the daemon already defines one.
- Hub provisioning path: this standalone repo does not ship a hub daemon. Acceptance must document that the operator starts an installed Botster hub daemon separately with an explicit data dir, using the established `botster-hub start --data-dir <path>` or equivalent installed `botster` command. The TUI connects to that daemon's socket and rejects mere pathname/connect success without a protocol handshake.
- Replace the demo-only runtime surface in `crates/botster-tui/src/app.rs` with a connected session spawn/attach surface that renders a `UiNode` tree from client state instead of hardcoded demo content.
- Keep the renderer generic. Product/workflow policy stays outside the renderer; the app layer adapts hub/session/entity state into shared `UiNode` nodes and entity-backed view state.
- Model the surface with explicit states: disconnected/local hub unavailable, connected/empty form, local validation error, hub/action error, spawn pending, spawn success, terminal attached, and terminal unavailable.
- Route keyboard and mouse interaction through the existing `InputRouter` and `HitMap`. Submit/validate/reset should emit semantic `UiActionRequest` values and then be handled by the app/client layer.
- Wire `terminal_view` focus, resize, keyboard bytes, paste-compatible text where available, and mouse passthrough to the local client terminal subscription/input contract. The terminal panel must not become authoritative terminal truth.
- After every fresh hub socket subscribe and every reconnect subscribe, explicitly request the entity snapshots required by the surface, at minimum the reserved core families needed for this flow: `session`, `workspace`, `spawn_target`, `worktree`, `hub`, and `connection_code` as applicable. Subscribe is transport readiness only; it must not be treated as global hydration.
- Preserve the existing `--smoke` path, existing renderer tests, and the core conformance fixture test harness.
- Document local hub commands in the README or a focused docs file: starting/using a local hub, running `botster-tui`, performing a successful spawn/attach action, triggering validation/error state, and observing terminal output.
- Add a review-visible runtime proof path with a real socket round trip against a running hub: protocol handshake, subscribe or attach, explicit entity snapshot pull, one successful semantic action, one visible validation/error branch, terminal subscribe/snapshot or output bytes, terminal input, and resize.
- If implementation concludes the real socket path is infeasible without adding `botster-hub` as a dependency or changing hub policy, ask a human before widening scope or falling back to package/plugin configuration.

## Non-Scope

- Do not add new shared UI primitives, renderer-specific form controls, or a product-specific TUI widget system.
- Do not move Project Pipelines workflow policy into core or into `botster-tui`.
- Do not change Lua plugin orchestration, Rails relay, React SPA, MCP tools, or hub runtime policy unless the public local client contract is missing and a human approves expanding scope.
- Do not implement authentication, cloud federation, QR pairing, or browser WebRTC paths.
- Do not make the TUI own terminal scrollback, authoritative terminal state, or session lifecycle policy beyond invoking existing local hub/client contracts.
- Do not add `botster-hub` or an in-process hub runtime dependency without explicit human approval; this plan commits to the socket/framed contract path for the current dependency set.

## Assumptions And Unknowns

- Assumption: this Plan agent is operating in the assigned worktree for target `tgt_c3d470bab78549df920a41e8fb0e58d8`.
- Assumption: the closed dependency tickets mean shared `UiNode` rendering, semantic input routing, and `terminal_view` pass-through primitives are available and should be reused.
- Assumption: the operator can provide a running installed Botster hub daemon and socket path for runtime verification; this repo will not build or own the hub daemon.
- Assumption: `botster-core` revision `8f2f4acf` exposes the contract-only frame types needed for a socket adapter, but not an in-process local hub API.
- Assumption: session spawn/attach is the correct dogfood surface unless implementation proves the local client contract is not available in this repo.
- Known contract surfaces: `TransportIngress` provides subscribe, terminal input, resize, snapshot request, focus, heartbeat/ping, client-state, and boundary payload ingress; `TransportEgress` provides terminal output, snapshot, scrollback, process exit, attach state, focus changed, boundary payload, pong, and close egress; `EntityFrame` provides snapshot, scoped snapshot, upsert, patch, and remove; `EntityStore`/`EntityStores` apply those frames; `session_protocol` provides hello/frame helpers and resize/session metadata shapes.
- Unknown: exact hub daemon socket discovery convention and existing on-wire encoding for these contract payloads. Implement must inspect the installed hub or upstream hub source and reuse the existing socket protocol.
- Unknown: exact request shape for session spawn and entity snapshot pull if those are currently carried as `TransportIngress::BoundaryPayload` rather than a typed variant. Implement must name the route ids and payload schema used by the running hub before coding.
- Unknown: whether package/plugin configuration is a more stable dogfood surface for the currently installed hub. Do not switch to it silently; ask a human if session spawn/attach cannot be proven.

## Affected Surfaces And Files

Expected files/surfaces:

- `crates/botster-tui/src/app.rs`: replace demo fixture runtime state with connected dogfood app state and action handling.
- `crates/botster-tui/src/main.rs`: likely unchanged except argument handling if a hub socket/path flag already exists in Botster conventions.
- `crates/botster-tui/src/renderer.rs`: keep generic; only touch if production app needs a missing existing dispatch/result hook, not for product policy.
- New focused app/client modules if the code shape warrants it: `client.rs`, `surface.rs`, `entities.rs`, or `terminal.rs`. Keep them small and boundary-named.
- `crates/botster-tui/Cargo.toml` and `Cargo.lock`: no new hub dependency expected for the socket contract path; add a small serialization or socket helper dependency only if the existing standard library plus current deps are insufficient.
- `README.md`: update included/not-included scope and add local hub dogfood commands.
- `docs/adr/0001-ratatui-crossterm-tui-renderer-foundation.md`: update only if implementation changes renderer/client ownership boundaries.
- New or updated plan/docs artifact with the command transcript expected by acceptance.

Botster layers touched:

- TUI/client app layer, TUI renderer integration surface, local hub client boundary, docs.
- No plugin policy, Lua core policy, Rust hub policy, React SPA, Rails relay, MCP, or cloud transport changes expected.

Pipeline gates/artifacts:

- Plan gate: this artifact plus structured evidence.
- Implement gate should require committed code, command evidence, docs commands, and runtime proof that `botster-tui` connects to a local hub and drives the chosen surface.
- Review/Verify must reject an implementation that only adds code or fixtures without changing the production runtime path.

## Risks

- Socket protocol risk: this repo has core contract types but not hub runtime APIs. Implementation must write a socket adapter against the existing hub protocol, or ask a human before adding `botster-hub`.
- Hub provisioning risk: this standalone repo cannot prove acceptance by itself. Verification requires a separately running installed hub daemon with explicit data dir and a discoverable socket path.
- Socket liveness risk: pathname or Unix connect success is not enough. The TUI must perform the Botster protocol hello/ack handshake before treating the hub as connected.
- Unwired implementation risk: tests could pass while `app.rs` still renders `demo_ui_node()`. Mitigation: acceptance requires the production app path to render the dogfood surface from client state.
- Entity-store risk: rendering only `ui_tree_snapshot` without explicit entity pulls can produce connected-empty UI. Mitigation: after subscribe/reconnect, request the required core entity families and apply `EntityFrame` snapshots/upserts/patches/removes into a TUI read model.
- Terminal passthrough risk: outer TUI routing can swallow nested TUI mouse reports or control keys. Mitigation: preserve current terminal-key/mouse forwarding tests and add client-boundary tests for subscribe/input/resize dispatch.
- Validation/error-state risk: successful action demos often skip failure UX. Mitigation: require local validation and hub/action error states with tests and documented manual commands.
- Runtime environment risk: macOS Rust test execution fails under this colon-containing Botster session worktree unless `CARGO_TARGET_DIR` is colon-free. Mitigation: use a colon-free target dir in Plan/Verify evidence.
- Scope creep risk: dogfooding Project Pipelines or package configuration could pull in plugin policy. Mitigation: use session spawn/attach only unless human-approved fallback is needed.

## Acceptance Checks And Tests

Minimum commands:

- `CARGO_TARGET_DIR=/tmp/botster-tui-target script/fmt`
- `CARGO_TARGET_DIR=/tmp/botster-tui-target script/test`
- `CARGO_TARGET_DIR=/tmp/botster-tui-target script/clippy`
- `CARGO_TARGET_DIR=/tmp/botster-tui-target cargo run -p botster-tui -- --smoke`
- Documented local hub command to start or connect to the hub.
- Documented interactive command to run `botster-tui` against that local hub.

Required automated coverage:

- App-state tests build the chosen session spawn/attach surface from client/entity state and render it through `render_node`.
- Validation tests prove invalid spawn input renders an error state without calling the hub.
- Action handling tests prove semantic `UiActionRequest` submit/validate/reset maps to the app/client operation, not DOM/event vocabulary.
- Entity-store tests cover snapshot, upsert, patch, remove, and reconnect baseline behavior for the surface state used by the TUI.
- Terminal client-boundary tests cover subscribe/attach success, terminal bytes displayed in `terminal_view`, input forwarding, resize dispatch, and action/error branches.
- Socket adapter tests cover handshake failure, connect failure, stale/closed egress, and framed `TransportIngress`/`TransportEgress` encode/decode using the same wire shape as the hub.
- Existing renderer conformance, hit-map, form, action, and terminal passthrough tests continue to pass.

Required runtime/manual evidence:

- Local commands show a separately started Botster hub daemon with explicit data dir, socket path, and protocol handshake evidence.
- `botster-tui` connects to that running local hub over the socket path.
- Real socket round trip evidence: subscribe or attach frame sent, matching hub response received, explicit entity snapshot pull performed, one successful action crosses the socket, and terminal snapshot/output bytes are received from the hub.
- The chosen surface renders in the TUI from hub/client state, not `demo_ui_node()`.
- A successful spawn/attach action completes and updates success/attached state.
- A validation or hub/action error state is visible.
- Terminal output from the attached session is displayed if the session-based path is implemented.
- Fake-client unit tests are not sufficient for Review or Verify. They can support edge cases, but Verify must reject fake-only evidence.

## Vault Gaps Worth Capturing

- Capture the exact local hub client API shape used by `botster-tui` after implementation proves it.
- Capture the exact hub socket protocol framing and socket discovery command used by `botster-tui`.
- Capture the TUI entity-store read model rule once snapshot/upsert/patch/remove behavior is verified.
- Capture any terminal subscribe/input/resize gotcha discovered while wiring `terminal_view` to the real client boundary.
- Capture the Project Pipelines checklist persistence failure if it recurs outside this run: default checklist timeout followed by duplicate item-id insert failure.
