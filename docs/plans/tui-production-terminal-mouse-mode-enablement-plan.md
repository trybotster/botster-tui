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
- Plan Review `review_1784564219_724937` returned changes required after proving
  that the authoritative producer substrate is absent, not merely unwired.
  Human decision `question_1784563519_602413` selected route A2: run a cheap
  libghostty feasibility spike before committing to a multi-repository
  implementation.
- Canonical spike `ticket_1784564069_340152`, target botster-core
  `tgt_1f7bce66eb304881980f9b4a2a5ae3fe`, run
  `run_1784564384_511975`, produced report artifact
  `artifact_1784566225_914155` and PR
  `https://github.com/trybotster/botster-core/pull/105`. Native proof classified
  outcome (a): the pinned Ghostty revision already exports an authoritative
  synchronous mode query, so no Ghostty-repository change is required.
- The mandatory native command
  `cargo test -p botster-terminal-ghostty --features libghostty-vt mouse_mode`
  passed with mise Zig 0.15.2 and pinned Ghostty `76853b3`: one test passed,
  zero failed. Its proven path is DECSET/DECRST bytes ->
  `TerminalScreenRuntime::write_output` -> `ghostty_terminal_vt_write` ->
  Ghostty durable mode state -> `ghostty_terminal_mode_get`.
- A duplicate spike ordering edge created during a worker-timeout retry was
  removed. The duplicate ticket record remains for operator cleanup, but only
  `ticket_1784564069_340152` blocks this ticket.
- Planning authority: [[planner-playbook]], [[botster-planner-playbook]],
  [[botster-architecture]], [[cli-patterns]], and [[spa-patterns]].
- Ticket-specific vault constraints:
  [[terminal view prop contract is closed in botster core]],
  [[nested rich tuis lose scrolling when botster consumes mouse reports or control keys]],
  [[focused mouse mode terminal passthrough needs complete sgr reports]],
  [[synced state types are allowed while pushed event variants are forbidden]],
  [[terminal accessory reattach must restore nested tui input passthrough]],
  [[ghostty shadow terminal integration belongs outside botster core]],
  [[cross repo dependency registration must use dependency repo target]],
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
  - core has `SessionIoRequest::GetModeFlags`, `ModeFlagsReady`, and the
    `ModeFlags.mouse_mode` vocabulary, but no authoritative producer:
    `TerminalScreenRuntime` has no mode-flags method,
    `TerminalScreenState::new` hardcodes defaults,
    botster-terminal-ghostty exposes no mouse-mode state, production adapters
    return defaults, and `managed_session_runtime` explicitly rejects
    `GetModeFlags` as unsupported;
  - the spike proved the missing piece is Botster's handwritten binding and
    production runtime wiring, not Ghostty itself.

## Product decisions and assumptions

- The mode source is authoritative session/runtime state. botster-tui consumes
  it; it does not parse terminal output to recreate emulator state.
- Route A2 returned verified outcome (a). Extend the backend-neutral core seam
  and existing targeted `GetModeFlags` vocabulary. Concrete Ghostty queries
  stay behind botster-terminal-ghostty per
  [[ghostty shadow terminal integration belongs outside botster core]]. Do not
  add a server-pushed mode event variant.
- The external hub-client result should carry `session_id` and the existing
  `mouse_mode: u8` value. Preserve the established mapping
  1000/1003/1002/1006 -> 1/2/4/8. botster-tui enables SGR passthrough only when
  bit 8 and at least one tracking bit (1, 2, or 4) are set; bit 8 alone is not
  tracking. The kit hit-map may retain a derived boolean, but the client-owned
  session shadow remains the canonical `u8`.
- Probe on attach/focus and reattach hydration, suppress duplicate in-flight
  probes, and rate-limit output-triggered refreshes to at most one per second
  per attached session. Unbounded per-drain probing is forbidden.
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
- Verified assumption: pinned Ghostty `76853b3` exposes
  `ghostty_terminal_mode_get`; the production core ticket needs only the narrow
  Botster adapter/runtime read and must not substitute default flags or infer
  state from botster-tui output bytes. A future Ghostty pin change requires ABI
  re-verification.
- Cross-repo targets are resolved now, not deferred to Implement:
  - botster-core: `tgt_1f7bce66eb304881980f9b4a2a5ae3fe`
  - Ghostty: `tgt_fa66a1b4bd92472c8b000fb031a1fd0b`
  - botster-hub: `tgt_7e208a0c76a44980a83b63af976b1f22`
  - botster-tui-kit: `tgt_3dfae49c02454037bf13554f552baf7f`
  - botster-web: `tgt_40abcf71ccf049f4ac0c99953a799869`
  - botster-tui: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Unknown: whether exposing the existing query through CoreDaemon requires a
  core public-facade addition or can reuse an existing readback result. Choose
  the narrowest facade path parallel to `ReadScreen`; do not expose raw actor
  routers to hub code.
- Registered delivery tickets:
  - core producer: `ticket_1784566294_599639`
  - hub/hub-client probe: `ticket_1784566294_391171`
  - kit schema-valid hook: `ticket_1784566295_993056`
  - botster-web protocol regeneration: `ticket_1784566295_552369`

## Scope

1. Backend-neutral core trait/runtime/CoreDaemon reporting, including removal
   or narrowing of the explicit `get_mode_flags` unsupported rejection.
2. Concrete botster-terminal-ghostty mode reporting using the already-proven
   FFI; no separate Ghostty-repository change.
3. Hub/hub-client request-response plus generated TypeScript surface and
   downstream botster-web drift handling.
4. Kit hit-map mutation hook and migration away from invalid renderer props.
5. botster-tui canonical `u8` session shadow, bounded probes, SGR+tracking
   enable predicate, production routing, immutable pin updates, tests, and
   docs.

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
- No implementation work in this ticket before the registered upstream
  dependency tickets merge and immutable pins can point to those revisions.

## Affected surfaces and files

Current branch:

- `docs/plans/tui-production-terminal-mouse-mode-enablement-plan.md`
  - records the reviewed spike evidence, registered ticket DAG, exact ownership
    boundaries, and botster-tui acceptance path.
- No production source files are changed during Plan.

Registered botster-core ticket `ticket_1784566294_599639`:

- `crates/botster-core/src/engine/terminal_screen.rs`: add required
  `TerminalScreenRuntime::mode_flags() -> ModeFlags` and its boxed forwarder,
  with explicit plain/fake implementations and no silent trait default.
- `crates/botster-core/src/engine/managed_session_runtime.rs` and facade files
  such as `engine/botster.rs`: serve the existing `GetModeFlags` /
  `ModeFlagsReady` path parallel to `read_screen`.
- `crates/botster-core-daemon/src/...`: add the CoreDaemon request/result path
  if the daemon facade does not already surface `GetModeFlags`.
- `crates/botster-core/src/runtime/...`,
  `crates/botster-core/src/bin/botster-session-worker.rs`, and the production
  runtime adapters: stop fabricating default mode state.
- `crates/botster-core/src/engine/managed_session_runtime.rs`: remove or narrow
  the explicit `unsupported("get_mode_flags")` rejection and prove the targeted
  request reaches the backend-neutral seam.
- `crates/botster-terminal-ghostty/src/sys.rs` and `src/lib.rs`: consume the
  proven packed mode ABI/query from spike PR #105 and expose authoritative
  `ModeFlags`, preserving query failure instead of collapsing it to zero.
- Core unit, worker-process, daemon integration, and downstream conformance
  tests for enable, disable, reattach, and unknown/error behavior.

Registered botster-hub ticket `ticket_1784566294_391171`:

- `src/runtime.rs` and `src/client_api.rs`: route the targeted CoreDaemon
  mode-flags probe through the existing hub client facade.
- `src/daemon_transport.rs`: map one daemon request to one response and preserve
  session identity/error semantics.
- `crates/botster-hub-client/src/lib.rs`: add the serializable request,
  response kind, `session_id` plus `mouse_mode` DTO, and request/response
  wire-name maps.
- `crates/botster-hub-client/src/typescript.rs`: add the generated TypeScript
  request, response, DTO, and discriminator mappings.
- Hub runtime/client API/daemon transport tests and
  botster-hub-test-support fixtures for real on/off values and failures.

Registered botster-web ticket `ticket_1784566295_552369`:

- Generated protocol artifacts and drift tests on target
  `tgt_40abcf71ccf049f4ac0c99953a799869`, generated from the merged hub-client
  source. No hand-written parallel browser type or UI behavior.

Registered botster-tui-kit ticket `ticket_1784566295_993056`:

- `crates/botster-tui-kit/src/hit_map.rs`: add and test a narrow
  `set_terminal_mouse_mode(node_id, enabled)`-style API.
- `crates/botster-tui-kit/src/renderer.rs`: terminal regions default off and no
  longer read `mouse_mode` from UiNode props.
- `crates/botster-tui-kit/src/input.rs`: expected behavior unchanged; retain
  exact SGR full-stream and non-mouse-mode negative tests.
- `crates/botster-tui-kit/examples/widget_library.rs` and kit docs/tests:
  replace invalid prop fixtures with direct hit-map setup and document that
  client adapters own mode application.

This botster-tui ticket after upstream dependencies merge:

- `crates/botster-tui/src/app.rs`
  - store the attached session's canonical `mouse_mode: u8` shadow;
  - probe on focus/attach and after reattach hydration;
  - refresh after non-empty terminal drain results;
  - clear on every ownership/error boundary;
  - derive enabled from SGR bit 8 plus any tracking bit and apply it to the
    rendered terminal hit region;
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
  - this evidence-backed Plan artifact.

## Implementation sequence

1. Merge/close spike `ticket_1784564069_340152` and its PR #105 so the proven
   handwritten FFI declaration is durable.
2. Deliver core producer ticket `ticket_1784566294_599639`, which depends on
   the spike. It adds the required runtime method, real Ghostty reporting,
   request handling, and core/daemon tests.
3. Deliver hub/client ticket `ticket_1784566294_391171`, which depends on core.
   It adds the correlated request/response, Rust wire maps, generated
   TypeScript, and transport/conformance tests.
4. After hub lands:
   - deliver kit ticket `ticket_1784566295_993056`, replacing the invalid
     prop-derived bool with a client-applied hit-map boundary;
   - deliver web ticket `ticket_1784566295_552369`, regenerating protocol
     artifacts from merged hub-client source.
5. This parent remains blocked on all four delivery tickets. Once merged, enter
   Implement in this worktree and update immutable pins. Migrate the test-local
   `mouse_mode` harness to the explicit hit-map API; production is already
   schema-valid, so its validation assertion is a regression guard, not a fix.
6. Implement the client shadow and bounded probes, apply it after render, prove
   exact bit enable/reset and reattach through the real path, update docs, and
   run all repository gates.

## Risks

- ABI drift: outcome (a) is verified only for pinned Ghostty `76853b3`.
  Mitigation: consume spike PR #105 unchanged and re-run the native mode test
  whenever the Ghostty pin changes.
- False authority: existing production adapters return default mode flags, so
  plumbing the query without backend truth would silently ship permanent off.
  The explicit `managed_session_runtime` unsupported path also proves that
  vocabulary alone is not a usable substrate. Mitigation: require the positive
  spike's native proof, remove or narrow that rejection, preserve query failure
  as error, and prove a real child/backend enable/disable path rather than only
  fake DTO fixtures.
- Cross-repo ordering: botster-tui cannot consume unmerged core/hub/kit APIs.
  Mitigation: use registered tickets `ticket_1784566294_599639`,
  `ticket_1784566294_391171`, `ticket_1784566295_993056`, and
  `ticket_1784566295_552369`, use immutable merged pins, and verify each
  consumer revision.
- Reattach drift: visible screen restoration can succeed while mouse mode stays
  stale or off. Mitigation: probe after hydration and test detach/reattach with
  mode already enabled.
- Stale-session race: an old probe response could enable routing for a newly
  selected session. Mitigation: bind results to session id and clear state at
  detach/switch/failure boundaries.
- Disable lag and probe pressure: mode can change after initial attach, while a
  request after every non-empty drain can degenerate into per-frame traffic
  under sustained output. Mitigation: attach/focus and reattach probes,
  one-in-flight suppression, and an output-triggered rate limit of at most one
  request per second per attached session, revised only from measured spike
  cost.
- Schema regression: a convenient prop could reappear in production or kit
  examples. Mitigation: renderer defaults off, explicit hit-map API, core
  validation tests, and repository scans for `mouse_mode` in UiNode fixtures.
- Routing regression: enabled mode may steal semantic-control releases or
  disabled mode may still forward. Mitigation: retain kit precedence tests and
  add consumer tests for tracking+SGR on, tracking-only off, SGR-only off, reset,
  and failure through the real rendered `HitMap`.
- Scope growth: exposing all terminal capabilities or adding events would turn
  a boolean routing source into a broad protocol project. Mitigation: one
  targeted request/response carrying existing `mouse_mode`, no general
  registry or pushed stream.
- Error behavior: transport/query failure could leave a stale true shadow.
  Mitigation: all unknown/error/disconnect paths synchronously clear to off.

## Acceptance checks and tests

Plan/spike evidence:

- Artifact `artifact_1784566225_914155` verifies classification (a), exact
  Ghostty revision `76853b3`, native runtime path, established bit mapping, and
  no Ghostty-repository change.
- The native mouse-mode test executed one test and passed; source inspection
  alone is not the basis for the classification.
- Core, hub, kit, and web tickets are registered on their dependency-repository
  targets with ordering edges, and this parent depends on all four.
- Plan-only branch validation passes; no botster-tui production source changes
  are part of this step.

Required core/substrate evidence:

- A production terminal-backend test feeds the proven DECSET/DECRST sequences,
  observes the exact 1/2/4/8 mapping, and observes zero after reset.
- The same state is available after the supported snapshot/reattach path.
- CoreDaemon/facade tests prove targeted request-response behavior and error
  attribution without a pushed mode event.
- A test proves the former `managed_session_runtime` unsupported
  `GetModeFlags` path now reaches the authoritative backend seam, or documents
  the narrowly removed obsolete branch.

Required hub/client evidence:

- Hub client serialization/TypeScript drift tests include the new request,
  response kind, session id, `mouse_mode`, and Rust wire-name mappings.
- Runtime/client API and daemon transport tests prove real on/off values and
  unknown-session/error behavior through the public client boundary.
- Hub test-support fixture/conformance tests can drive deterministic on/off
  mode without teaching clients an event stream.
- botster-web regeneration/drift checks either consume the generated protocol
  change or prove from generator output that no repository change is needed.

Required kit evidence:

- A schema-valid terminal render creates a hit region with mouse mode off.
- The explicit hit-map hook turns the matching terminal on and off without
  changing the UiNode.
- Existing exact SGR press/release/drag/move/wheel/clamping tests remain green.
- The negative path proves a focused terminal with mode off does not receive
  mouse bytes, and semantic-control release precedence remains green.
- A scan confirms kit production renderer/examples no longer rely on a
  `mouse_mode` UiNode prop.

Required botster-tui focused evidence:

- `DogfoodApp::surface().validate()` passes with only `session_id` and `title`
  on `dogfood-terminal`.
- An authoritative response sets only the attached session's canonical `u8`
  shadow. Tracking bit plus SGR bit enables the rendered hit-region flag;
  tracking-only, SGR-only, zero, and unknown all leave it off.
- The real production `HitMap -> InputRouter -> DogfoodApp::handle_dispatch`
  path produces exact kit SGR bytes and one `DaemonRequest::SendInput`.
- A later authoritative off response clears the hit region and restores
  non-passthrough/chrome routing.
- Unknown session, stale response, detach, process exit, transport error, and
  probe error all leave mode off.
- Attach and reattach/hydration each trigger a probe; reattach with an already
  mouse-enabled child restores passthrough.
- Under sustained non-empty terminal drains, instrumentation or a deterministic
  clock test proves no more than one mode request per second per attached
  session and never more than one in-flight request; attach/reattach probes
  remain prompt.
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

- Plan artifact: this file, revised after Plan Review
  `review_1784564219_724937`.
- Feasibility artifact: `artifact_1784566225_914155`, commit
  `caae5b863ac4e15a6f535928b33cf568b5bcb33f`, PR #105.
- Plan gate may advance this artifact to Plan Review because the outcome-
  specific DAG is now registered and all conditional details have been
  replaced. Advancement does not authorize botster-tui implementation while
  upstream dependency tickets remain open.
- Product decision ledger: human answer `question_1784561631_624748` binds the
  authoritative targeted-probe design, safe-off default, and rejected parser/
  pushed-event/schema-expansion alternatives.
- Feasibility decision ledger: human answer
  `question_1784563519_602413` binds route A2 and forbids implementation before
  the spike result.
- Plan Review must verify:
  - verified outcome (a), exact ticket ordering, and target ownership are
    explicit;
  - the selected core/backend design can return real mode state rather than
    defaults or an unsupported result;
  - the hub surface is request/response only;
  - hub-client TypeScript/wire-name and botster-web drift are accounted for;
  - kit mode application is outside UiNode props;
  - on/off, bounded-probe, and reattach tests cross the production path.
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
  core adapters return default `ModeFlags`, one path explicitly rejects the
  request, the Ghostty adapter exposes no mode query, and the public hub-client
  has no probe. The spike captured verified feasibility at
  `Vault inbox/2026-07-20-libghostty-mouse-mode-feasibility.md`; separately
  capture the shipped request/response path after implementation.
- The plan confirms [[ghostty shadow terminal integration belongs outside
  botster core]] constrains ownership: core owns a backend-neutral terminal
  runtime contract while concrete Ghostty FFI belongs in
  botster-terminal-ghostty. The spike verified that no trybotster/ghostty
  change is needed at the current pin; promote that fact through the vault
  pipeline after spike review/merge.
- Reconcile [[terminal accessory reattach must restore nested tui input passthrough]]
  after implementation: its `ModeChanged` wording conflicts with the newer
  [[synced state types are allowed while pushed event variants are forbidden]]
  rule. Update it to name the targeted probe/snapshot behavior actually shipped.
- Capture no plan-only implementation claim. Route any new durable knowledge
  through the vault inbox/document/connect/verify pipeline after runtime and
  test evidence exists.
