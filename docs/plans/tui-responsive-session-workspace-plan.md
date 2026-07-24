# Responsive Session Workspace Plan

Ticket: `ticket_1784754422_878069`
Run: `run_1784782236_659399`
Step: Plan

## Target And Routing

- Target repository: `botster-tui` (`trybotster/botster-tui`).
- Target ID: `tgt_c3d470bab78549df920a41e8fb0e58d8`.
- Repository charter: [[botster-tui-playbook]].
- Botster layer: first-party Rust terminal client application policy over
  `botster-hub-client` and `botster-tui-kit`.
- Assigned-worktree rule: implementation and verification operate only in the
  run-assigned target worktree. The process working directory was not used to
  infer repository ownership.

## Context Loaded

- Pipeline context: ticket, run, current step, gate, reviews, findings,
  artifacts, dependencies, questions/answers, events, and checklist state from
  `project_pipelines_current_context`.
- Required role playbooks, in order: [[planner-playbook]] and
  [[botster-planner-playbook]].
- Repository ownership charter: [[botster-tui-playbook]].
- Consumed-substrate and runtime overlays: [[botster-tui-kit-playbook]],
  [[botster-runtime-reviewer-playbook]], and
  [[botster-runtime-verifier-playbook]].
- Architecture maps: [[botster-architecture]], [[cli-patterns]], and
  [[spa-patterns]].
- Ownership and client-boundary notes:
  [[tui and browser are equal clients]],
  [[botster tui consumes tui kit through a thin app policy adapter]],
  [[tui adapter maps shared primitives onto existing rust render tree without flag day rewrite]],
  [[tui client attach uses hub protocol not session protocol]],
  [[tui and socket terminal streams use clientworker transport adapters]],
  [[botster terminal clients share one sessionio data plane subscription path]],
  [[botster tui uinode event routing captures hit regions during draw]], and
  [[tui error dedup tests must drive real input handlers]].
- Layout and interaction notes:
  [[adaptive ui should use shared semantic viewport classes not raw breakpoints]],
  [[priority-based content collapse uses three tiers instead of ad-hoc hiding]],
  [[botster toolbar actions use declaration order plus fixed overflow intent]],
  [[focused mouse mode terminal passthrough needs complete sgr reports]],
  [[narrowing a shared dispatch path silently changes its other callers]], and
  [[renderer acceptance tests must drive real frame backend]].
- Lifecycle and migration notes:
  [[embedded hub tui guards exited session attach before daemon attach]],
  [[botster clients restore visible terminal state from readscreen before buffered live output]],
  [[coredaemon attached follows initial snapshots before live terminal output]],
  and [[cold turkey migrations eliminate dual code paths and version suffixes]].
- Workflow/evidence notes: [[plan steps need reviewable plan artifacts]],
  [[project pipelines checklist worker timeouts require artifact evidence fallback]],
  [[test script required for rust tests not cargo test]],
  [[rust repo strict lints must be verified before dismissing warnings]],
  [[workspace struct field changes require workspace cargo gates]], and
  [[a regression test must be shown to go red with the fix reverted]].
- [[project-pipelines-playbook]] was loaded as a workflow-policy overlay because
  this run explicitly requires Project Pipelines checklists, artifacts, and
  gate evidence. No Project Pipelines package/plugin implementation is in
  scope.
- Repository evidence inspected: `README.md`, workspace and crate Cargo
  manifests, `crates/botster-tui/src/app.rs`,
  `crates/botster-tui/src/renderer.rs`, the package manifest, repository
  scripts, prior plans under `docs/plans/`, current `origin/main`, both closed
  prerequisite tickets and merged PRs, and the current TUI-kit public API,
  README, coverage guide, application-shell plan, widget explorer, layout
  helpers, renderer, and input router.
- Current production path:
  `main` -> `app::run` -> `run_loop` -> `TuiApp::surface` ->
  kit `render_node` -> draw-derived `HitMap` -> Crossterm event ->
  kit `InputRouter::dispatch_event` -> `TuiApp::handle_dispatch` ->
  public `botster-hub-client` request or terminal data-plane input.

## Current Baseline And Prerequisite Reconciliation

- The assigned branch currently starts at `09fca91`. Its remote-tracking
  `origin/main` is `aae0bb6`, four commits ahead, and contains the merged
  push-lifecycle prerequisite from botster-tui PR #32.
- Implementation must integrate `origin/main` before editing. Planning against
  the assigned branch's older `ListSessions` polling path would recreate a
  superseded synchronization model.
- The merged TUI-kit prerequisite is PR #18, merge commit `19508f6`. The
  authoritative TUI-kit `main` was independently verified at
  `fb0fdcb87d102232cb015b6da782a971903b4190`, which includes the application
  shell plus subsequent mainline scrollbar fixes and polish.
- Current botster-tui `origin/main` still pins TUI-kit `16c6035` and core
  `978c436`. The current TUI-kit requires core
  `7d52fb78024b45764d6830cf4c6b131f13a83e62`. The consumer must update its
  TUI-kit, core, core-test-support, and lockfile together so `UiNode`, viewport,
  toolbar, render-state, and test-support types remain aligned.
- The hub-client/test-support pin on current botster-tui main is
  `02bffebd0e29cb69a8e1e639e01f704f6dfffe48`, which carries the pushed session
  entity subscription used by the merged prerequisite. This ticket does not
  replace that pin unless a concrete compatibility failure requires a
  separately justified update.

## Scope

In scope:

- Replace the diagnostic-first root with a session-first application workspace:
  compact connection/system status, session navigation, focused-session
  details and terminal, and a contextual action toolbar.
- Make the default screen task-oriented while retaining existing
  package/app/plugin diagnostics and controls behind an explicitly selected,
  scrollable system-details surface. Do not leave the current giant status
  panel as the primary information hierarchy.
- Adopt the current kit's state-aware production renderer:
  `InputRouter::render_state`, `render_node_with_state`, post-draw
  `InputRouter::reconcile`, semantic viewport conditions, bounded scroll
  areas, toolbar overflow, and real visible hit geometry.
- Compose wide, medium, and narrow layouts from the shared
  compact/regular/expanded viewport vocabulary. The renderer owns terminal
  thresholds; the app owns which session-workspace regions appear in each
  class.
- Keep selected session and attached terminal stream as separate app states.
  Row selection changes the focused session only. Attach is a separate
  contextual action, and terminal input remains gated on the authoritative
  attached subscription.
- Give pending spawn, authoritative running, selected, attached, exited/failed,
  unavailable hub, empty session list, and actionable errors distinct copy,
  badges/tones, affordances, and tests. The authoritative entity reducer
  remains the lifecycle source; pending rows remain client-local only until
  replaced or removed by pushed state.
- Render owner selection through the core selection vocabulary and combine it
  with kit-local focus state so selected, focused, and selected+focused rows
  stay visually distinct.
- Add contextual actions using existing public hub-client requests:
  attach/detach, spawn, reconnect, active-session shutdown, and exited-session
  removal. Actions that cannot run remain visible when that improves
  discoverability, but are schema-valid disabled actions with an adjacent
  reason; they must not dispatch.
- Require confirmation before `ShutdownSession` or `RemoveSession`. Use the
  existing core `Dialog` primitive and kit modal replacement-root behavior;
  danger tone and disabled state come from existing core semantics. Cancel,
  Esc, confirm, keyboard focus, and mouse activation must be covered.
- Use toolbar declaration order plus `toolbar_overflow` intent. Primary
  session actions stay direct when possible; secondary/system actions move
  into overflow; impossible widths retain a reachable overflow affordance and
  no hidden hit regions.
- Put the session navigator and secondary details in kit `ScrollArea`
  viewports so long lists and diagnostics remain usable without copying
  clipping, scrollbar, offset, or hit-map logic into this repository.
- Preserve terminal attach, ReadScreen hydration, buffered live output,
  resize, keyboard forwarding, authoritative mouse-mode reapplication, and
  full SGR mouse passthrough.
- Replace user-visible and root application "live-runtime scaffold" terminology
  with session-workspace terminology where touched. Keep `--headless-live-runtime`
  naming only for the existing live-hub test mode whose purpose is still
  live-runtime verification; do not perform an unrelated repository-wide rename.
- Update repository documentation to describe the daily-use workspace,
  responsive behavior, keyboard/mouse shortcuts, state distinctions, system
  details, and destructive confirmation.

## Non-Scope

- No renderer, hit-map, focus, toolbar-overflow, scroll clipping, scrollbar,
  viewport-threshold, mouse encoder, or generic widget implementation in
  `botster-tui`.
- No reusable product-neutral widget library, shell framework, service object,
  state-management framework, third-party dependency, or alternate rendering
  path.
- No change to core UiNode schema, shared action meaning, TUI-kit public
  semantics, browser rendering, or browser/TUI parity contract.
- No private socket protocol, session-worker frame, polling lifecycle fallback,
  parallel session authority, hub policy, terminal truth, workflow policy, or
  plugin policy.
- No rewrite of the pushed entity reducer, terminal hydration algorithm,
  package lifecycle/configuration implementation, or plugin surface renderer.
  Those paths may be adapted to the new presentation but retain their current
  request and state semantics.
- No automatic terminal attachment after spawn, selection, reconnect, or
  authoritative session appearance.
- No horizontal scrolling, terminal scrollback ownership, large-data
  virtualization, or speculative user configurability.
- No edits in botster-core, botster-hub, botster-hub-client,
  botster-tui-kit, botster-web, or Project Pipelines repositories.

## Repository Ownership And Cross-Repository Dependencies

- `botster-tui` owns product terminology, state-to-presentation mapping,
  responsive composition, selected-versus-attached policy, contextual action
  priority, confirmation state, request dispatch, and operator diagnostics.
- `botster-tui-kit` owns real-frame UiNode rendering, semantic viewport
  resolution, state composition, pane geometry helpers, toolbar measurement
  and overflow, focus reconciliation, scroll viewport/scrollbar behavior,
  draw-derived hit maps, and terminal forwarding encoders.
- `botster-core` owns portable UiNode validation and the selection, tone,
  disabled action, dialog, viewport, and toolbar-overflow vocabulary.
- `botster-hub-client` owns the public daemon requests, compatibility
  requirement, session entity frames, attach/terminal DTOs, and transport
  helpers. Hub/SessionIo/ClientWorker remain authoritative for lifecycle and
  terminal data.
- Closed registered dependency `ticket_1784754421_254409` targets
  `botster-tui-kit` (`tgt_3dfae49c02454037bf13554f552baf7f`) and supplied the
  application-shell mechanics.
- Closed registered dependency `ticket_1784752212_275852` targets
  `botster-tui` (`tgt_c3d470bab78549df920a41e8fb0e58d8`) and supplied pushed
  lifecycle reconciliation.
- No open cross-repository prerequisite remains. If implementation finds that
  a required state, disabled-reason field, dialog behavior, or action contract
  is absent from the consumed revisions, stop and register a ticket against
  the owning repository target instead of adding a private local substitute.

## Assumptions And Unknowns

- "Current TUI-kit mechanics" means the exact latest remote `main` revision
  verified during planning (`fb0fdcb...`), not merely the earlier dependency
  merge. At implementation start, verify remote main again. If it advanced,
  inspect the delta and use the later compatible revision only when it remains
  within this ticket; otherwise retain the reviewed revision or ask the human.
- The current core vocabulary can express the required selection, disabled,
  danger, dialog, empty-state, responsive, and toolbar intent without new
  schema work. Disabled reasons remain app-authored explanatory text because
  no new universal reason field is proposed.
- Wide/medium/narrow are semantic classes, with current kit thresholds:
  compact `<80`, regular `80..119`, expanded `>=120`; height classes use the
  kit's current short/regular/tall thresholds.
- The session list is the primary navigator. System/package/plugin material
  remains supported but secondary; removing that functionality would be an
  unintended regression.
- Active-session shutdown and exited-session removal are the ticket's
  destructive session actions because both public requests already exist.
  Confirmation is local presentation state; the hub remains action authority.
- Confirmation replaces the workspace root while open so background focus,
  click, and scroll targets are absent. Returning from the dialog reconciles
  focus against the newly drawn workspace rather than reviving stale geometry.
- Existing headless live-hub shutdown at the end of its owned smoke session is
  harness cleanup, not a user confirmation path.
- Package/app/plugin system-detail tests should continue to prove their public
  DTO and action mapping, but they need not assert the old diagnostic-first
  visual ordering.
- No browser downstream proof is required because this plan consumes, but does
  not alter, shared UiNode/action semantics. TUI consumer proof against the
  pinned kit/core revisions is required.

## Product Decision Ledger

- Defaults: session workspace opens first; selection never attaches; primary
  attach/detach/spawn action stays visible when space permits; secondary
  refresh/system actions overflow; system details are explicit and scrollable.
- Non-goals: hub/package policy changes, private UI props, custom widget
  mechanics, automatic attachment, and browser work.
- Follow-up acceptable: richer human session labels or metadata when the
  authoritative session entity contract supplies them.
- Ask-human threshold: removing existing package/app/plugin functionality,
  weakening confirmation, retaining polling as fallback, adding private
  renderer props, changing shared contracts, or waiving live-hub/real-handler
  proof.

## Affected Surfaces And Files

Expected:

- `Cargo.toml` / `Cargo.lock`
  - Align botster-core and test-support with current TUI-kit; pin the verified
    TUI-kit revision; preserve the merged lifecycle-capable hub-client pin.
- `crates/botster-tui/src/renderer.rs`
  - Keep the thin adapter; re-export only the kit state-aware rendering/layout
    APIs actually used by the app and add state-aware real-frame test helpers.
- `crates/botster-tui/src/app.rs`
  - Integrate the merged pushed-session base.
  - Replace the diagnostic-first surface hierarchy with responsive workspace,
    status, navigation, focused session, terminal, contextual toolbar,
    system-details, empty/error, and confirmation compositions.
  - Pass router render state into every production/headless draw, reconcile
    focus after the current hit map is drawn, and continue reapplying
    attachment-scoped terminal mouse mode.
  - Add local workspace/system/confirmation presentation state and map
    confirmed actions to existing public daemon requests.
  - Extend real handler, headless render, state transition, and live-hub tests.
- `README.md`
  - Replace scaffold-first description with the shipped workspace hierarchy,
    responsive/state behavior, shortcuts, destructive confirmation, system
    details, and exact dependency pins.
- `docs/plans/tui-responsive-session-workspace-plan.md`
  - This durable plan; resynchronize it if approved implementation findings
    alter scope, files, decisions, or executable checks.

Conditional:

- `crates/botster-tui/src/main.rs` only if a deterministic existing headless
  mode needs a small argument handoff. Do not add a new CLI mode solely for
  tests when app-level real-backend fixtures provide the required output.
- `crates/botster-tui/tests/package_manifest_test.rs` and
  `botster-package.json` only if renamed user-facing command/help text or an
  entrypoint assertion must change. The runnable contract itself should remain
  unchanged.

Not expected:

- New source modules, new crates, Project Pipelines plugin files, or edits in
  any dependency repository.

## Implementation Sequence

1. Integrate current `origin/main`; verify the pushed entity subscription,
   pending-row reconciliation, fresh reconnect generation, and no-`ListSessions`
   normal path are present before changing UI code.
2. Verify current remote TUI-kit main and pin the chosen revision. Align
   botster-core/core-test-support and refresh the lockfile. Compile before UI
   edits to expose dependency/type mismatches early.
3. Update the thin renderer adapter and production loop to use the kit's
   state-aware renderer and post-draw focus reconciliation. Pin existing
   terminal, field, list, table, plugin-surface, and mouse routes with tests
   before changing shared call sites.
4. Replace the primary root with compact status plus responsive session
   workspace variants. Use semantic conditional branches and kit `ScrollArea`
   / toolbar mechanics; keep node IDs unique across the validated tree and
   keep zero-area/hidden controls out of the hit map.
5. Map authoritative session and client-local pending/selection/attachment
   state to distinct rows, badges, empty state, focused detail, terminal copy,
   and disabled reasons. Do not derive lifecycle from terminal state.
6. Add contextual toolbar ordering/overflow and explicit system-details
   navigation. Move the existing package/app/plugin diagnostics and controls
   behind that secondary surface without rewriting their request logic.
7. Add shutdown/remove confirmation as local modal state. Route real
   keyboard/mouse actions through `InputRouter`, dispatch the public daemon
   request only after confirmation, and let pushed lifecycle frames update the
   workspace.
8. Add real-backend wide/medium/narrow and state-matrix fixtures, real routed
   key/mouse tests, overflow/scroll/focus negative cases, and live-hub
   assertions against the new production surface.
9. Update README and this plan if necessary, run all exact gates and negative
   controls, then inspect the final diff for ticket-only traceability and stale
   scaffold terminology.

## Risks

- Stale-base risk: editing before integrating `origin/main` could silently
  restore list polling or lose the lifecycle subscription cleanup.
- Dependency risk: pinning TUI-kit without aligning core creates duplicate
  incompatible UiNode types or unavailable toolbar/viewport vocabulary.
- Hierarchy risk: preserving every diagnostic in the default root would meet
  code-level reuse but fail the daily-use workspace intent; deleting those
  paths would regress current package/app/plugin functionality.
- Responsive-tree risk: duplicate IDs across conditional branches can fail
  core validation or create ambiguous focus state. Every rendered branch must
  have stable unique IDs.
- Focus risk: resize, overflow, scrolling, view switches, and modal replacement
  can leave a stale node focused unless the router reconciles against the
  newest hit map.
- Overflow risk: hidden controls must not remain in keyboard traversal or
  mouse geometry, especially destructive actions.
- Scroll risk: selected/focused row visibility, app-owned selection, and
  router-local offset can fight. The kit owns clamping/ensure-visible; the app
  must not maintain a second offset.
- Lifecycle risk: selection, attach, pending, exited, and confirmation state
  can be conflated. Only authoritative session entities own lifecycle; only
  the active attachment generation owns terminal I/O.
- Destructive-action risk: a danger style without a modal is insufficient;
  a modal that leaves background hits/focus active is also insufficient.
- Input regression risk: changing the production draw/router path can break
  forms, plugin surfaces, terminal keyboard/mouse forwarding, or action
  payloads even when workspace-specific tests pass.
- Test-fixture risk: source-level or semantic-tree assertions can pass while
  Ratatui clips content or records stale hits. Acceptance requires the real
  frame/backend and real input router.
- Live-hub artifact risk: reused target directories can retain stale
  same-version hub-client schemas. Use fresh target directories for final
  live proof.
- Large-file risk: `app.rs` already combines app state, protocol adaptation,
  presentation, and tests. Keep changes local to existing model/presentation
  methods rather than introducing speculative abstractions or unrelated
  cleanup.

## Acceptance Checks And Tests

Required repository commands:

- `script/fmt`
- `script/test`
- `script/clippy`
- `CARGO_TARGET_DIR=<fresh-dir> BOTSTER_LIVE_HUB_TARGET_DIR=<fresh-dir> script/test-live-hub`
- `cargo run -p botster-tui -- --smoke`
- `git diff --check origin/main...HEAD`

Required focused real-frame/headless evidence:

- Render the production workspace through
  `ratatui::backend::TestBackend` plus `render_node_with_state` at representative
  expanded (`140x42`), regular (`96x30`), and compact (`72x24`) sizes.
- Capture/assert output for connected, unavailable/disconnected, empty,
  pending spawn, authoritative running, selected-not-attached, attached, and
  exited/failed sessions. Each state must have distinct visible copy or
  semantic presentation, not only different internal fields.
- Wide output shows navigator and focused terminal/details without large unused
  regions; regular and narrow outputs remain readable with no overlap,
  zero-area focus targets, or unreachable primary action.
- Session and system-detail overflow uses kit `ScrollArea`; indicators appear
  only on overflow, selected/focused content stays visible, and clipped rows
  have no hit regions.
- Toolbar boundary-width tests prove declaration-order priority,
  `never`/`auto`/`always` intent, reachable overflow, stable visible order, and
  no hit/focus/action dispatch for hidden controls.
- Cell and glyph assertions distinguish selected, focused, and
  selected+focused session rows. Resize preserves focus by stable ID when
  visible and chooses deterministic visible fallback when hidden.
- Empty and unavailable states keep the recovery/spawn action discoverable;
  disabled actions display a reason and produce `InputDispatch::Ignored`.

Required production input/action evidence:

- Drive Tab/Shift-Tab, arrows, Enter/Space, Esc, PageUp/PageDown, wheel, click,
  press/release, and resize through the real `InputRouter` and draw-derived
  `HitMap`.
- Row selection changes `selected_session` without producing Attach; attach
  produces the public request only for an authoritative running row; terminal
  input remains rejected until the matching subscription is attached.
- Reconnect and pushed snapshots preserve/reset selection according to the
  existing reducer tests and never auto-attach.
- Shutdown is offered only for the applicable active session and removal only
  for the applicable exited session. Keyboard and mouse activation open a
  danger confirmation; cancel/Esc dispatch nothing; confirm dispatches exactly
  one `ShutdownSession` or `RemoveSession`.
- While confirmation is open, background workspace nodes are absent from
  focus traversal, click routing, and scroll routing. Returning reconciles
  focus against the workspace.
- Existing form editing, plugin surface actions, package controls, terminal
  key forwarding, SGR mouse reports, resize, hydration, and deduplicated error
  routes stay green through their real handlers.

Required live-hub/downstream proof:

- The documented isolated-hub smoke runs the production package entrypoint and
  renders the new session workspace, not a test-only tree.
- Prove empty snapshot, client-local pending spawn, authoritative pushed
  appearance, an externally created session becoming visible without
  `ListSessions`, running-to-exited lifecycle update, exited removal,
  disconnect, and fresh reconnect snapshot.
- Prove selection remains separate from attachment throughout the sequence.
- Preserve and visibly prove attach -> ReadScreen restoration -> buffered live
  output -> input echo/readback -> resize using the same public
  `botster-hub-client` and SessionIo/ClientWorker path.
- Assert the compatibility requirement still includes session entity
  subscriptions and revision-16 lifecycle conformance. Source inspection or
  fixture-only tests are not live proof.
- Install/enable/open this checkout as the first-party terminal app through the
  hub-owned runnable entrypoint contract, as `script/test-live-hub` currently
  does.
- No browser parity run is needed unless implementation changes a shared
  UiNode/action contract. If it does, stop this run and register the owning
  cross-repository work rather than widening scope.

Regression discipline:

- For new workspace layout, selection-vs-attach, disabled dispatch, overflow
  visibility, scroll clipping, confirmation isolation, and lifecycle
  presentation tests, record a negative control by temporarily disabling or
  reverting the behavior and showing the targeted test fails, then restore the
  implementation.
- Run exact wrapper commands and record raw exits. Do not substitute
  package-scoped `cargo test` or a lighter Clippy command for repository gates.
- If a pre-existing failure appears, record the exact command, failing test or
  file, and proof that it is unchanged and unrelated; do not use a blanket
  baseline waiver.

## Pipeline Checklists, Gates, And Artifacts

- Workflow checklist: `checklist_1784782501_103205`.
- Vault checklist: `checklist_1784782506_619242`.
- Both checklist creation responses timed out after persistence; listing the
  run's checklists confirmed exactly one of each, so no retry was issued.
- Plan gate: `botster_stack_plan_gate`.
- Durable repo plan: this document.
- Required Project Pipelines artifact: `kind=plan`, pointing to this document
  and carrying the explicit base/prerequisite/dependency assumptions.
- Advancement occurs only after checklist evidence is updated, the plan
  artifact is attached, and every required gate field is submitted.

## Vault Gaps Worth Capturing

- Existing notes already cover the app/kit boundary, semantic viewport
  classes, priority collapse, toolbar overflow, draw-derived hit maps,
  real-frame proof, lifecycle authority, attach/readback order, real input
  handlers, stale dependency worktrees, and checklist timeout reconciliation.
- No vault note should be created from this plan alone.
- After implementation, capture one atomic note only if the shipped workspace
  establishes a reusable rule for session selection/attachment presentation
  or modal destructive-action focus isolation that is not already represented.
- The repeated mismatch between assigned run worktrees and newly merged
  dependency commits is already captured by
  `[[stale project pipeline worktrees can miss merged dependency apis]]`; this
  run adds evidence but not a new claim.
