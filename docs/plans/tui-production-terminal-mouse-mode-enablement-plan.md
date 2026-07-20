# Production Terminal Mouse-Mode Enablement Plan

Ticket: Production terminal mouse-mode enablement (client-owned, schema-valid)

## Context loaded

- Pipeline run `run_1784561404_957810`, Plan step `botster_plan`, run step
  `run_step_1784561404_317572`, and required gate `botster_plan_gate`.
- The run started without prior artifacts, reviews, findings, questions, or
  answers. Its registered dependency, ticket `ticket_1784521840_795217`
  ("Bump botster-tui kit pin for full SGR mouse passthrough"), is closed and
  merged into the current `origin/main`.
- Human decision `question_1784561631_624748` selected authoritative session
  `ModeFlags` via a targeted request/response probe. The decision explicitly
  rejects a pushed `ModeChanged` stream, a TUI-local DECSET/DECRST parser, and
  any expansion of the core `terminal_view` UiNode schema. Probe failure or
  unknown state means mouse mode is off.
- Planning authority: [[planner-playbook]], [[botster-planner-playbook]],
  [[botster-architecture]], [[cli-patterns]], and [[spa-patterns]].
- Ticket-specific vault constraints:
  [[terminal view prop contract is closed in botster core]],
  [[nested rich tuis lose scrolling when botster consumes mouse reports or control keys]],
  [[focused mouse mode terminal passthrough needs complete sgr reports]],
  [[synced state types are allowed while pushed event variants are forbidden]],
  [[terminal accessory reattach must restore nested tui input passthrough]],
  [[terminal-capability-propagation-alacritty-eventlistener-is-integration-point]],
  [[botster tui consumes tui kit through a thin app policy adapter]], and
  [[botster tui uinode event routing captures hit regions during draw]].
- Worktree/target: this run is bound to the botster-tui target
  `tgt_c3d470bab78549df920a41e8fb0e58d8` and the ticket worktree. At planning
  time the worktree was clean and both `HEAD` and `origin/main` were at
  `6fa4671`, the merge of the SGR kit-pin dependency.
- Current production path:
  `run_loop` polls the hub, renders `DogfoodApp::surface()` into the frame and
  `HitMap`, dispatches Crossterm events through kit `InputRouter`, then passes
  `TerminalForward` bytes to `DogfoodApp::handle_dispatch`, which issues
  `DaemonRequest::SendInput` for the attached session/subscription.
- Current schema proof: `DogfoodApp::surface()` calls `UiNode::validate()` before
  rendering. Its production `terminal_view` contains only `session_id` and
  `title`; no production surface currently emits `mouse_mode`.
- Current missing seam:
  - kit `render_terminal_view` sets `HitRegion.terminal_mouse_mode` by reading
    renderer-local `mouse_mode` from the UiNode, but core rejects that prop;
  - `HitMap` exposes immutable regions and has no client-owned setter;
  - botster-tui has no mouse-mode shadow;
  - the pinned hub-client exposes `ReadScreen` and `CaptureSnapshot`, but no
    `ModeFlags` probe;
  - core already defines targeted `SessionIoRequest::GetModeFlags` and
    `ModeFlagsReady`, but the inspected production adapters currently return
    `ModeFlags::default()` rather than emulator truth.

## Product decisions and assumptions

- The mode source is authoritative session/runtime state. botster-tui consumes
  it; it does not parse terminal output to recreate emulator state.
- Extend the existing targeted `GetModeFlags` request/response path. Do not add
  a server-pushed mode event variant.
- The external hub-client result should carry `session_id` and the existing
  `mouse_mode: u8` value. botster-tui needs only `mouse_mode != 0`; preserving
  the existing value avoids inventing a second mode encoding while keeping the
  ticket's routing behavior boolean.
- Query after attach/focus and reattach hydration. Also refresh after a
  non-empty terminal drain batch so a child that later enables or disables
  tracking updates the client shadow without byte parsing or an unconditional
  high-frequency request loop.
- Store mode against the attached session identity and ignore stale responses.
  Detach, process exit, session switch, transport failure, query failure, and
  unknown state clear the shadow to off.
- Apply the shadow after the schema-valid tree has rendered, by mutating only
  the matching terminal hit region before `InputRouter::dispatch_event`.
- Remove kit renderer dependence on the invalid `mouse_mode` prop once the
  explicit hit-map hook exists. Kit tests/examples may set `HitRegion` mode
  directly; the invalid prop is not retained as a compatibility path.
- The existing kit owns SGR encoding and focus-first routing. The TUI must not
  reimplement mouse report encoding.
- Assumption: the terminal backend can expose current mouse tracking through
  its existing screen/mode state without a UiNode contract change. If the
  production backend has no truthful mode query at its pinned revision, the
  substrate work must add that narrow backend read. It must not substitute
  default flags or infer state from botster-tui output bytes.
- Assumption: coordinated upstream work will land in botster-core,
  botster-hub, and botster-tui-kit before this repo updates immutable git pins.
  Because this run's worktree is botster-tui-only, Implement must register
  explicit dependency tickets/targets or obtain pipeline coordination for
  those repositories rather than editing ambient sibling checkouts.
- Unknown: the exact production terminal backend API that exposes mouse mode.
  Implement must trace the backend used by the session worker and prove its
  enable/disable values. A fake-only `ModeFlagsReady` result does not satisfy
  this plan.
- Unknown: whether exposing the existing query through CoreDaemon requires a
  core public-facade addition or can reuse an existing readback result. Choose
  the narrowest facade path parallel to `ReadScreen`; do not expose raw actor
  routers to hub code.

## Scope

Botster layers touched: terminal backend/session worker and CoreDaemon
readback substrate, Rust hub/client protocol, reusable TUI-kit hit-map
mechanics, botster-tui client policy/state, tests, dependency pins, and docs.

1. Make the existing targeted core mode-flags request return authoritative
   production terminal state, including mouse tracking enable/disable.
2. Expose one hub/client request-response operation for reading a session's
   mode flags, parallel to the existing read-screen/capture-snapshot path.
3. Add one narrow kit API that sets `terminal_mouse_mode` on a rendered
   terminal hit region by node id. Default remains off.
4. Remove kit rendering/tests/examples that treat `mouse_mode` as a UiNode prop;
   test harnesses set the hit-region flag through the explicit kit API.
5. Add botster-tui attached-session mode shadow and lifecycle probes. Apply the
   shadow to the production `dogfood-terminal` hit region after render and
   before input dispatch.
6. Update immutable core, hub-client/test-support, and kit pins only after their
   upstream changes merge; audit `Cargo.lock` for explained movement.
7. Document ownership and failure behavior in the botster-tui README and
   affected upstream contract docs.

## Non-scope

- No `mouse_mode` or mode enum prop on core `terminal_view`; its allowed props
  remain `session_id` and `title`.
- No pushed `ModeChanged` daemon/client event stream.
- No DECSET/DECRST or other partial terminal parser in botster-tui.
- No xterm 1002-versus-1003 product routing distinction; the kit continues to
  consume one boolean.
- No text selection engine, multi-click UI, scroll-normalizer integration,
  terminal-output refactor, or adjacent mouse cleanup.
- No change to terminal byte-path ownership:
  `TerminalForward -> DaemonRequest::SendInput -> ClientWorker/SessionIo`
  remains intact.
- No speculative general capability registry, observer framework, service
  object, feature flag, optional configuration, or compatibility dual path.
- No unrelated dependency upgrades or broad core/hub/TUI refactors.

## Affected surfaces and files

Expected upstream botster-core surfaces (exact filenames may narrow after the
backend trace):

- `crates/botster-core/src/engine/managed_session_runtime.rs` and facade files
  such as `engine/botster.rs`: expose targeted mode-flags readback parallel to
  `read_screen`.
- `crates/botster-core-daemon/src/...`: add the CoreDaemon request/result path
  if the daemon facade does not already surface `GetModeFlags`.
- `crates/botster-core/src/runtime/...`,
  `crates/botster-core/src/bin/botster-session-worker.rs`, and the production
  terminal backend (currently under `crates/botster-terminal-ghostty/...` at
  the inspected pin): return real `TerminalScreenState.mode_flags` rather than
  defaults and preserve it across snapshot/reattach.
- Core unit, worker-process, daemon integration, and downstream conformance
  tests for enable, disable, reattach, and unknown/error behavior.

Expected upstream botster-hub surfaces:

- `src/runtime.rs` and `src/client_api.rs`: route the targeted CoreDaemon
  mode-flags probe through the existing hub client facade.
- `src/daemon_transport.rs`: map one daemon request to one response and preserve
  session identity/error semantics.
- `crates/botster-hub-client/src/lib.rs`: add the serializable request,
  response kind, and `session_id` plus `mouse_mode` DTO.
- Hub runtime/client API/daemon transport tests and
  botster-hub-test-support fixtures for real on/off values and failures.

Expected upstream botster-tui-kit surfaces:

- `crates/botster-tui-kit/src/hit_map.rs`: add and test a narrow
  `set_terminal_mouse_mode(node_id, enabled)`-style API.
- `crates/botster-tui-kit/src/renderer.rs`: terminal regions default off and no
  longer read `mouse_mode` from UiNode props.
- `crates/botster-tui-kit/src/input.rs`: expected behavior unchanged; retain
  exact SGR full-stream and non-mouse-mode negative tests.
- `crates/botster-tui-kit/examples/widget_library.rs` and kit docs/tests:
  replace invalid prop fixtures with direct hit-map setup and document that
  client adapters own mode application.

This botster-tui repository:

- `crates/botster-tui/src/app.rs`
  - store the attached session's mouse-mode shadow;
  - probe on focus/attach and after reattach hydration;
  - refresh after non-empty terminal drain results;
  - clear on every ownership/error boundary;
  - apply the shadow to the rendered terminal hit region;
  - add schema, on/off, stale-session, failure-default, reattach, and real
    router-to-`SendInput` tests.
- `crates/botster-tui/src/renderer.rs`
  - remain a thin adapter; re-export only the narrow kit hook if needed.
- `crates/botster-tui/Cargo.toml` and `Cargo.lock`
  - update exact merged revisions for core, hub client/test support, and kit,
    with no unexplained package movement.
- `README.md`
  - document session/runtime authority, client shadow, kit hit-region
    application, closed UiNode contract, probe points, and safe-off failures.
- `docs/plans/tui-production-terminal-mouse-mode-enablement-plan.md`
  - this reviewable Plan-stage artifact.

## Implementation sequence

1. Register/confirm explicit cross-repo dependency work for botster-core,
   botster-hub, and botster-tui-kit so each change lands in its own target and
   worktree. Do not work in ambient sibling checkouts.
2. In core, trace the production session worker's terminal backend and make the
   existing `GetModeFlags -> ModeFlagsReady` request return actual mouse-mode
   state. Add a facade readback parallel to `ReadScreen`, including reattach
   state restoration and a real enable/disable backend test.
3. In hub/hub-client, expose one targeted mode-flags request/response. Prove the
   daemon transport uses the production runtime facade, returns the requested
   session id/value, propagates errors, and introduces no pushed event.
4. In kit, add the explicit hit-map mode setter, make terminal render default
   to off, remove `mouse_mode` prop consumption, and migrate fixtures/examples
   to direct hit-map state.
5. Pin the merged upstream revisions in botster-tui and inspect the lockfile
   before adding client behavior.
6. In `DogfoodApp`, add the smallest attached-session mode shadow. Probe at the
   decided lifecycle points, refresh after non-empty drain batches, reject
   stale-session results, and fail closed to off.
7. After every production draw, apply the shadow to `dogfood-terminal` in the
   rendered `HitMap` before dispatching input.
8. Replace the current invalid-prop app test with a schema-valid production
   surface test that drives:
   authoritative on -> hit-region on -> exact kit SGR `TerminalForward` ->
   app `SendInput`, then authoritative off -> hit-region off -> no terminal
   mouse forwarding. Cover attach/reattach and failure clearing.
9. Update docs, run focused upstream and consumer tests, then run every
   repo-approved full gate. Inspect diffs and dependency revisions before
   handing off.

## Risks

- False authority: existing production adapters return default mode flags, so
  plumbing the query without backend truth would silently ship permanent off.
  Mitigation: require a real child/backend enable/disable test, not only fake
  DTO fixtures.
- Cross-repo ordering: botster-tui cannot consume unmerged core/hub/kit APIs.
  Mitigation: explicit dependency tickets/targets, immutable merged pins, and
  per-consumer revision verification.
- Reattach drift: visible screen restoration can succeed while mouse mode stays
  stale or off. Mitigation: probe after hydration and test detach/reattach with
  mode already enabled.
- Stale-session race: an old probe response could enable routing for a newly
  selected session. Mitigation: bind results to session id and clear state at
  detach/switch/failure boundaries.
- Disable lag: mode can change after initial attach. Mitigation: refresh after
  non-empty drain batches so child-generated mode changes are observed without
  parsing bytes or adding an unconditional request loop.
- Schema regression: a convenient prop could reappear in production or kit
  examples. Mitigation: renderer defaults off, explicit hit-map API, core
  validation tests, and repository scans for `mouse_mode` in UiNode fixtures.
- Routing regression: enabled mode may steal semantic-control releases or
  disabled mode may still forward. Mitigation: retain kit precedence tests and
  add consumer on/off tests through the real rendered `HitMap`.
- Scope growth: exposing all terminal capabilities or adding events would turn
  a boolean routing source into a broad protocol project. Mitigation: one
  targeted request/response carrying existing `mouse_mode`, no general
  registry or pushed stream.
- Error behavior: transport/query failure could leave a stale true shadow.
  Mitigation: all unknown/error/disconnect paths synchronously clear to off.

## Acceptance checks and tests

Core/substrate evidence:

- A production terminal-backend test starts or feeds a child that enables mouse
  tracking, observes nonzero authoritative `mouse_mode`, disables it, and
  observes zero.
- The same state is available after the supported snapshot/reattach path.
- CoreDaemon/facade tests prove targeted request-response behavior and error
  attribution without a pushed mode event.

Hub/client evidence:

- Hub client serialization/TypeScript drift tests include the new request,
  response kind, session id, and `mouse_mode`.
- Runtime/client API and daemon transport tests prove real on/off values and
  unknown-session/error behavior through the public client boundary.
- Hub test-support fixture/conformance tests can drive deterministic on/off
  mode without teaching clients an event stream.

Kit evidence:

- A schema-valid terminal render creates a hit region with mouse mode off.
- The explicit hit-map hook turns the matching terminal on and off without
  changing the UiNode.
- Existing exact SGR press/release/drag/move/wheel/clamping tests remain green.
- The negative path proves a focused terminal with mode off does not receive
  mouse bytes, and semantic-control release precedence remains green.
- A scan confirms kit production renderer/examples no longer rely on a
  `mouse_mode` UiNode prop.

botster-tui focused evidence:

- `DogfoodApp::surface().validate()` passes with only `session_id` and `title`
  on `dogfood-terminal`.
- An authoritative on response for the attached session sets only the
  client-owned shadow and rendered hit-region flag.
- The real production `HitMap -> InputRouter -> DogfoodApp::handle_dispatch`
  path produces exact kit SGR bytes and one `DaemonRequest::SendInput`.
- A later authoritative off response clears the hit region and restores
  non-passthrough/chrome routing.
- Unknown session, stale response, detach, process exit, transport error, and
  probe error all leave mode off.
- Attach and reattach/hydration each trigger a probe; reattach with an already
  mouse-enabled child restores passthrough.
- Focused terminal key forwarding and one-attach mouse focus behavior remain
  green.

Repo-approved botster-tui commands:

```sh
./test.sh terminal_mouse_mode
./test.sh focused_terminal
./test.sh attach
script/fmt
script/test
script/clippy
git diff --check origin/main...HEAD
```

Use actual Rust test-function substrings after implementation and record test
counts so a zero-test filter cannot pass unnoticed. Each upstream repository
must also run its own approved formatting, full test, strict clippy, and
contract/conformance commands; Implementation and Verify reports must name the
exact commands and results rather than substituting botster-tui-only evidence.

Manual/live acceptance, if the core backend harness cannot already prove the
full production chain:

1. Attach botster-tui to a minimal child that enables mouse tracking.
2. Confirm a focused terminal forwards wheel/press/drag/release as SGR bytes to
   the child.
3. Have the child disable tracking and confirm Botster chrome regains mouse
   routing.
4. Detach and reattach while the child mode is enabled and repeat the forwarding
   proof.

Code existence, fake DTO serialization, or test-local `HitRegion` mutation
alone is insufficient; evidence must cross the actual production session mode
source and botster-tui input path.

## Pipeline gates and artifacts

- Plan artifact: this file.
- Plan gate: attach every required section from this artifact to
  `botster_plan_gate`.
- Product decision ledger: human answer `question_1784561631_624748` binds the
  authoritative targeted-probe design, safe-off default, and rejected parser/
  pushed-event/schema-expansion alternatives.
- Plan Review must verify:
  - cross-repo dependencies and target/worktree ownership are explicit;
  - the selected core backend returns real mode state rather than defaults;
  - the hub surface is request/response only;
  - kit mode application is outside UiNode props;
  - on/off and reattach tests cross the production path.
- Implement must attach:
  - upstream PR/revision links and dependency registration;
  - exact files and contracts changed per repo;
  - lockfile/pin audit;
  - focused and full command results with test counts;
  - runtime-path evidence for authoritative on, authoritative off, and
    reattach;
  - confirmation that no pushed mode event, TUI parser, or UiNode prop was
    introduced.
- Review and Verify must re-run the strict repo gates, scan all committed
  artifacts for local paths/PII, inspect dependency revisions, and reject
  fake-only or unwired mode evidence.

## Vault gaps worth capturing

- Durable gap confirmed: the vault says targeted synced terminal-mode state is
  allowed and reattach must restore it, but the inspected current production
  core adapters return default `ModeFlags` and the public hub-client has no
  probe. After implementation, capture an atomic note describing the shipped
  authoritative request/response path and its safe-off client semantics.
- Reconcile [[terminal accessory reattach must restore nested tui input passthrough]]
  after implementation: its `ModeChanged` wording conflicts with the newer
  [[synced state types are allowed while pushed event variants are forbidden]]
  rule. Update it to name the targeted probe/snapshot behavior actually shipped.
- Capture no plan-only implementation claim. Route any new durable knowledge
  through the vault inbox/document/connect/verify pipeline after runtime and
  test evidence exists.
