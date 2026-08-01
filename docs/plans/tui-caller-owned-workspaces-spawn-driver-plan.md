# TUI caller-owned Workspaces Spawn driver plan

## Outcome

Add one explicit installed-binary acceptance mode to `botster-tui`. The final
Workspaces integration harness will launch the already installed `tui`
`terminal_app` with the current `botster-hub apps open` path and the existing
`BOTSTER_HUB_CONNECTION` / `BOTSTER_HUB_DATA_DIR` injections. When both
caller-owned acceptance paths are present, the binary will attach to that Hub,
open the admitted real Workspaces surface, find controls from the headlessly
realized production render tree and `HitMap`, reach and operate them with
keyboard events through `InputRouter`, and write bounded, correlated JSONL
evidence to the caller's file.

The driver will not start or stop a Hub, install or enable packages, create Git
fixtures, or clean shared state. It is a generic TUI integration seam with a
Workspaces-specific acceptance scenario, not Workspaces product policy in the
interactive client.

## Pipeline identity and routing

- Ticket: `ticket_1785602853_851250`
- Run: `run_1785604407_246317`
- Target repository: `trybotster/botster-tui`
- Target id: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Resolved target: `project_pipelines_current_context` returned the target id;
  the admitted spawn-target registry mapped it to `botster-tui`, and this
  worktree's `origin` is `git@github.com:trybotster/botster-tui.git`.
- Base: `origin/main` / `main` / the ticket branch at `0668435` during Plan.
- Repository charter: `[[botster-tui-playbook]]`.
- Consumed surface charters: `[[botster-tui-kit-playbook]]` and
  `[[botster-workspaces-playbook]]`; neither repository is editable in this run.
- Workflow overlay: `[[project-pipelines-playbook]]` for artifact, checklist,
  gate, and advancement discipline only. Project Pipelines source is not in
  scope.

The target was resolved before repository inspection and was not inferred from
the ambient directory.

## Context loaded

Role and repository playbooks:

- `[[planner-playbook]]`
- `[[botster-planner-playbook]]`
- `[[botster-tui-playbook]]`
- `[[botster-tui-kit-playbook]]`
- `[[botster-workspaces-playbook]]`
- `[[botster-runtime-reviewer-playbook]]`
- `[[botster-runtime-verifier-playbook]]`
- `[[project-pipelines-playbook]]`
- `[[botster-architecture]]`
- `[[cli-patterns]]`
- `[[spa-patterns]]`

Exact targeted notes:

- `[[tui and browser are equal clients]]`
- `[[botster tui consumes tui kit through a thin app policy adapter]]`
- `[[tui client attach uses hub protocol not session protocol]]`
- `[[tui and socket terminal streams use clientworker transport adapters]]`
- `[[botster tui uinode event routing captures hit regions during draw]]`
- `[[tui error dedup tests must drive real input handlers]]`
- `[[acceptance readiness requires the exact expected entity not any authoritative snapshot]]`
- `[[tui adapter maps shared primitives onto existing rust render tree without flag day rewrite]]`
- `[[renderer state accepts only realized literal identity]]`
- `[[post expansion identity uniqueness is scoped to one render not one tree]]`
- `[[plugin authored tui surfaces dispatch via action props not node id literals]]`
- `[[renderer acceptance tests must drive real frame backend]]`
- `[[narrowing a shared dispatch path silently changes its other callers]]`
- `[[botster toolbar actions use declaration order plus fixed overflow intent]]`
- `[[botster runnable entrypoints are hub owned launch contracts]]`
- `[[manifest required injections must be consumed by the launched runtime]]`
- `[[foreground terminal app open conformance belongs in hub test support]]`
- `[[botster client subscriptions should not hydrate global state]]`
- `[[botster hub client state sync is entity frame only]]`
- `[[botster entity snapshots are authoritative reconnect baselines]]`
- `[[plugin surfaces request model state through ui bindings not hub subscribe]]`
- `[[plugin surface requests require a declared id and operation]]`
- `[[plugin surface actions route by explicit metadata]]`
- `[[workspaces are semantic groupings by purpose not by branch]]`
- `[[botster workspace records are plugin owned references not hub authority]]`
- `[[botster plugin entities are canonical for plugin-owned dynamic state]]`
- `[[botster package manifests and lockfiles should declare capabilities and provenance]]`
- `[[botster hub gravity must be watched before it becomes the new monolith]]`
- `[[acceptance harness region oracles must key on node identity not concatenated text]]`
- `[[project pipeline orchestration belongs in a device-level botster plugin]]`
- `[[project pipelines needs an operator workbench not more primitives]]`
- `[[project pipelines ui contract belongs in the plugin readme]]`
- `[[botster orchestration should spawn agents with explicit target ids]]`
- `[[botster orchestration prompts must bind agents to explicit worktrees]]`
- `[[botster pipeline needs continuous product owner between agent steps]]`
- `[[plan agents must author vault context as wikilinks not home paths]]`
- `[[pipeline vault checklists must cite exact resolvable note titles]]`
- `[[vault example paths are not repository placement conventions]]`

Repository context inspected:

- `README.md`, `botster-package.json`, root/crate `Cargo.toml`, `Cargo.lock`,
  `crates/botster-tui/src/{main,app,renderer}.rs`, repository scripts, and the
  package-manifest test;
- the existing Workspaces plumbing/lifecycle harness, request observations,
  session reducer, reconnect flow, binding materialization, production frame,
  `HitMap`, and keyboard/mouse helpers in `app.rs`;
- prior Workspaces lifecycle, materialized-action, and subscription-readiness
  plans under `docs/plans/`;
- current `botster-workspaces` README and `plugin.lua`, especially its
  target-first two-form Spawn flow and accepted result payload containing the
  Hub-returned session/target/branch/worktree facts;
- current Hub `apps open` implementation, which resolves the installed
  foreground launch contract and preserves caller environment while adding the
  Hub-resolved injections;
- current Project Pipelines ticket/run/gates/reviews/artifacts/findings,
  same-project sibling tickets, and the final Workspaces integration ticket.

The durable answer to `question_1785604722_645032` is binding: define one
versioned TUI-owned file contract now. Use
`BOTSTER_TUI_ACCEPTANCE_SCENARIO=<json-file>` and
`BOTSTER_TUI_ACCEPTANCE_EVIDENCE=<jsonl-file>`. Do not use inline JSON or
stdout. The final Workspaces harness imports this repository's schema/fixture.

## Existing path and missing behavior

Already present and to be reused:

- the installed runnable entrypoint and Core-owned decoding of
  `BOTSTER_HUB_CONNECTION` plus `BOTSTER_HUB_DATA_DIR` context;
- one authoritative held `/session` entity subscription with exact-row
  snapshot/delta readiness and reconnect generation replacement;
- admitted package navigation and public `PluginSurfaceRender` /
  `PluginSurfaceAction` requests through `botster-hub-client`;
- Workspaces `ui.bind_list` materialization into literal realized identity;
- the production render tree's headless Ratatui frame, kit `HitMap`, focus
  reconciliation, form drafts, and `InputRouter` keyboard dispatch;
- accepted action result correlation, presentation operations, replacement
  trees, and payload retention.

The current `workspaces_live_acceptance_runs_against_real_package` is not the
requested entrypoint. It runs inside `#[cfg(test)]`, owns an isolated Hub,
installs/enables the package itself, and focuses its claimed keyboard action
with a synthetic mouse-down. The ordinary binary has only interactive,
`--smoke`, and headless-live-runtime modes. It has no caller-owned scenario,
machine-readable evidence file, keyboard-only deterministic focus driver, or
runtime surface-request ledger.

## Scope

1. Parse the two acceptance path variables as one explicit mode. Neither set
   keeps normal behavior unchanged; exactly one set fails before connecting;
   both set require readable strict scenario v1 and a new caller-provided
   evidence path. Do not add another alias, inline format, stdout protocol, or
   automatic mode discovery.
2. Publish the canonical scenario/evidence contract and examples from the TUI
   crate. Scenario v1 contains:
   - schema/version discriminator;
   - the already authoritative `workspace_id`;
   - an ordered non-empty `cases` array;
   - per case: unique `case_id`, admitted `target_id`, requested `branch`,
     resolution class (`existing_worktree`, `existing_branch`, or
     `missing_branch`), and expected returned target/branch/worktree facts.
   Unknown fields, duplicate case ids, empty identifiers, unsupported versions,
   duplicate/missing resolution classes, or unsafe evidence-path reuse fail
   closed. The scenario contains no UiActionRequest payload or node id.
3. Connect only through `AppArgs::daemon_endpoint`, construct the normal
   `TuiApp`, establish the exact authoritative session baseline, pull normal
   package/navigation state, and open the admitted Workspaces surface. Find the
   exact workspace action from rendered metadata and verify the requested
   workspace is present; do not create or select workspace state through MCP or
   direct requests.
4. Render headlessly at a fixed production-sized viewport by calling the
   non-test TUI-kit primitive
   `botster_tui_kit::render_to_lines_with_presentation_state` directly from the
   acceptance module with `&app.surface()`, the viewport dimensions,
   `&router.render_state()`, and `&app.plugin_presentation`. This mirrors
   production `draw()`'s `render_node_with_presentation_state` inputs, so Spawn
   dialogs, router focus/drafts, presentation replacements, and the realized
   `HitMap` are the same state-aware path input routing consumes, without a TTY
   or `CrosstermBackend<Stdout>`. Do not call the crate-local
   `renderer::render_to_lines*` wrappers: they are `#[cfg(test)]` and absent
   from the installed binary, while the plain variant also drops the
   presentation state required for both Workspaces Spawn dialogs. The runtime
   claim is deliberately scoped to the production render tree, realized hit
   map, presentation/focus state, and `InputRouter`, not terminal
   raw-mode/alternate-screen behavior. A small driver helper may inspect
   hit-region action/field metadata to identify the next intended control, but
   it must reach focus only by bounded Tab/Shift-Tab traversal and
   activate/type/select only through `InputRouter::dispatch_event` key events.
   No mouse event, direct router draft mutation, direct `TuiApp` action call,
   synthesized node id, or hand-authored action request/payload is acceptance
   evidence.
5. For each ordered case, keyboard-open the producer-authored Spawn control,
   select the exact rendered target option, submit the target-selection form,
   choose the single eligible rendered session template supplied by the parent
   fixture, type the requested branch, and submit the rendered Spawn form.
   Require the emitted canonical request to retain the focused realized node,
   action, surface, request id, form values, and producer payload identity.
6. Correlate the accepted/rejected `UiActionResult` by request id. On acceptance,
   read the returned session UUID and Hub facts from the producer result payload,
   then wait for that exact session row to reach an authoritative semantic state
   in the TUI entity store and for the active Workspaces surface to materialize
   the membership. Snapshot authority alone is insufficient. The parent
   independently verifies Hub, Git, worktree, package, and session truth.
7. Exercise one sanctioned reconnect after initial open by finding the rendered
   `Reconnect` control in the realized hit map, focusing it with bounded
   keyboard traversal, and activating `botster.tui.connect` through
   `InputRouter`. Record its focused-control and canonical dispatched-action
   identity just like a Spawn action. The driver must not call
   `TuiApp::force_reconnect` or manipulate transport state directly. Require a
   fresh subscription id and authoritative snapshot, refresh admitted
   navigation, and explicitly reopen/reselect the Workspaces surface by the
   same keyboard path. Record the exact initial and reconnect surface render
   requests. After that barrier, session snapshot/upsert/patch/remove
   reconciliation and action-result replacements must update subsequent
   production frames without `ListSessions`, polling requests, list refresh,
   or a synchronization `PluginSurfaceRender`. Bounded local event draining is
   allowed only to consume pushed frames; it may not issue state-read requests
   repeatedly.
8. Add acceptance-only request accounting at the single `TuiApp::request`
   boundary. Record counts and correlated identities for surface render/action
   calls and prove the forbidden legacy/session-list paths stay zero. Keep the
   trace disabled in normal interactive operation; do not turn it into a new
   product diagnostics or transport abstraction.
9. Write evidence with create-new semantics to the caller path, flushing each
   complete JSONL event. Every line carries a schema/version and bounded event
   kind. Required events cover driver readiness, baseline/reconnect, surface
   request count, focused realized control identity, canonical dispatched
   action identity, accepted/rejected result, returned session/entity state,
   case completion, final request-count summary, and terminal completion or
   failure diagnostics. Never write evidence to stdout. Even though acceptance
   mode uses the headless production renderer and emits no escape sequences,
   stdout remains the foreground terminal app's unstructured UI channel and is
   not a stable machine-readable contract.
10. Keep failure evidence bounded and deterministic: case id, phase, active
    subscription/snapshot metadata, expected semantic condition, last relevant
    entity/action/result observation, surface request count, and a capped list
    of focusable realized ids. Deadlines bound failure but do not define
    readiness; exact state/action correlation does.
11. Update the README with the installed `botster-hub apps open` invocation,
    caller ownership, the canonical file contract location, request budget,
    keyboard-only claim, and final downstream proof command shape.

Every changed line must trace to explicit mode selection, schema/evidence IO,
production keyboard driving, request/entity correlation, bounded diagnostics,
or documentation/tests required by this ticket.

## Non-scope

- No Hub start/stop, package install/enable/reload, Git fixture creation,
  spawn-target admission, shared sequencing, or cleanup in the production
  driver. One explicit repository acceptance test may own an isolated ephemeral
  Hub/package/Git fixture solely to execute the installed binary before merge;
  it is not the caller-owned final integration harness or runtime behavior.
- No attach plumbing, direct socket protocol, session-worker/Core access, or
  alternate Hub discovery.
- No Workspaces create/rename/delete/membership/spawn policy in normal TUI code;
  no Git, branch, worktree, target, template, or lifecycle inference.
- No direct Workspaces MCP setup, direct `PluginSurfaceAction`, direct form
  values, mouse-assisted focus, click helper, node-id constant, text-derived
  region parser, or synthesized UiActionRequest payload.
- No `ListSessions`, list refresh, synchronization surface rerender, polling
  request loop, second session store, or timing-only readiness.
- No dogfood mode, sibling checkout discovery, local Hub lifecycle wrapper,
  package reinstall, compatibility alias, optional alternate schema, or stdout
  JSONL.
- No change to `botster-workspaces`, `botster-hub`, `botster-hub-client`,
  `botster-tui-kit`, `botster-web`, `botster-core`, or Project Pipelines.
- No broad `app.rs` decomposition, renderer rewrite, reusable workflow engine,
  or adjacent cleanup.

## Ownership boundaries and cross-repository dependencies

`botster-tui` owns the explicit acceptance mode, scenario/evidence validation,
client request audit, normal Hub-client connection, headless production render
tree/HitMap use, keyboard focus/action routing, bounded local diagnostics, and
one isolated installed-binary pre-merge acceptance profile.

`botster-tui-kit` continues to own reusable rendering, focus traversal, field
editing, select behavior, hit maps, and input dispatch. If keyboard traversal
cannot reach a correctly rendered generic control, route that defect to target
`tgt_3dfae49c02454037bf13554f552baf7f` rather than bypassing the router here.

`botster-workspaces` owns the target-first two-stage Spawn surface, workspace
records, action handlers, atomic-Hub capability composition, accepted result
payload, membership, and lifecycle presentation. The final integration ticket
`ticket_1785192726_335558` at target
`tgt_71266a8d976d4535902ffed09c18a7ba` owns the long-lived fresh Hub, package
installation/enablement, temporary Git matrix, scenario generation, client
sequencing, independent truth verification, cleanup, and final combined proof.
Project Pipelines dependency `dependency_1785602859_859990` records that
`ticket_1785192726_335558` depends on this TUI ticket, enforcing merge and
schema-import ordering rather than leaving it as a prose-only seam.

`botster-hub` owns app-open resolution, the injected connection/data-dir
contract, package/surface admission, spawn targets, managed Git resolution,
session templates, atomic spawn, entity lifecycle truth, and plugin action
execution. Missing or incorrect producer facts are Hub findings, not TUI
fallback scope.

`botster-web` owns its sibling caller-owned browser driver. Browser evidence is
not a substitute for this keyboard path, and TUI evidence is not browser proof.

Same-project TUI ticket `ticket_1785602865_181673` owns canonical BindList
descendant identity after its Hub/TUI-kit prerequisites. It is not a current
dependency for the producer's existing literal Spawn form controls. If the
real pinned Workspaces surface changes those assigned controls to descendant
bound identities before implementation, register that ticket (and its upstream
chain) as a dependency rather than synthesizing local ids or folding its scope.

No other open same-target sibling owns the driver contract, so there is no
scope overlap to fold at Plan time.

## Assumptions and unknowns

- Binding human answer: the TUI defines v1 now and the final Workspaces harness
  consumes it; the pair of path variables is the only interface.
- Assumption: `botster-hub apps open` remains the caller launch path. Its current
  foreground child inherits caller environment and receives the resolved Hub
  injections, so the parent can set both acceptance paths without changing the
  package manifest or rebuilding attach plumbing.
- Assumption: acceptance mode deliberately calls the public, non-test
  `botster_tui_kit::render_to_lines_with_presentation_state` primitive at a
  fixed viewport with the router's `RenderState` and the app's
  `PresentationState`, so neither the in-repo gate nor the downstream parent
  must allocate a PTY. This is the headless equivalent of production `draw()`;
  the proof covers the installed entrypoint, Hub connection, presentation-aware
  production render tree, realized hit map, and `InputRouter`. Interactive
  crossterm terminal setup remains covered by existing TUI tests.
- Assumption: the parent admits exactly one eligible session template per
  assigned target. The scenario intentionally does not duplicate template
  policy; if multiple rendered eligible templates are present, the driver fails
  with bounded option evidence instead of guessing.
- Assumption: the parent gives each accepted Spawn case a unique branch and
  independently checks the returned resolution class and filesystem facts.
- Assumption: action-result replacement is not a new surface request. The
  driver records both so the parent can distinguish owner-authored replacement
  from synchronization rerender.
- Assumption: exact session `current`/`ended`/remove observations can arrive as
  snapshot or ordered delta; readiness examines the converged exact row and
  records the frame-independent semantic state.
- Unknown: implementation may expose a generic TUI-kit focus/edit/select defect.
  Only a generic app adapter correction belongs here; reusable router mechanics
  must become a dependency against the kit target.
- Unknown: a pinned Workspaces/Hub producer may omit or change a required
  structured result fact. That is an owner defect and must be routed rather
  than inferred from text, local Git, or geometry.
- Convention conflict: none. The plan reuses framework/library primitives,
  keeps product policy with Workspaces/Hub, preserves one client entity path,
  and adds no speculative configurability or compatibility branch.

## Affected surfaces and files

Expected changes:

- `crates/botster-tui/src/app.rs`
  - acceptance-mode dispatch using the existing endpoint/data-dir parser;
  - driver orchestration over `TuiApp`, production render, `HitMap`, and
    `InputRouter` key events;
  - acceptance-only request audit and exact entity/action/result waits;
  - focused unit and caller-owned driver tests.
- `crates/botster-tui/src/acceptance.rs` (new)
  - strict v1 scenario types/validation, create-new JSONL evidence writer,
    event records, bounded diagnostics, fixture/schema tests, and the direct
    non-test call to TUI-kit's presentation-aware headless render primitive.
- `crates/botster-tui/src/main.rs`
  - register the small acceptance module; normal entrypoint behavior remains in
    `AppArgs`/`app::run`.
- `crates/botster-tui/Cargo.toml` and `Cargo.lock`
  - add direct `serde` derive support already present transitively; no new
    behavior/library dependency beyond typed JSON contracts.
- `crates/botster-tui/fixtures/workspaces-spawn-driver-v1.schema.json` (new)
  - parent-consumable canonical scenario and evidence event schema. This ticket
    establishes `crates/botster-tui/fixtures/` as a new in-repository canonical
    import location; it is not an existing fixture convention.
- `crates/botster-tui/fixtures/workspaces-spawn-driver-v1.scenario.json` (new)
  - strict three-case example covering each resolution class without node ids
    or action payloads.
- `crates/botster-tui/fixtures/workspaces-spawn-driver-v1.evidence.jsonl` (new)
  - bounded example stream for parent parser tests.
- `README.md`
  - installed shared-Hub invocation, ownership, schema, evidence, request
    budget, downstream proof, and the exact
    `crates/botster-tui/fixtures/` canonical import path for consumers.
- `script/test-live-hub`
  - add one explicit `workspaces installed-driver` profile that owns an
    isolated test Hub/package/Git fixture, launches the installed package via
    `botster-hub apps open` with both caller paths, and validates the resulting
    evidence stream. It must use only explicit binary/package inputs and must
    not discover siblings.
- `docs/plans/tui-caller-owned-workspaces-spawn-driver-plan.md`
  - this durable plan.

Expected unchanged:

- `botster-package.json`: the current runnable entrypoint and required Hub
  injections are already correct; acceptance paths are caller-owned process
  environment, not new normal launch requirements.
- `renderer.rs` and `botster-tui-kit`: the driver consumes existing mechanics.

## Risks and mitigations

- **A direct helper accidentally masquerades as keyboard proof.** Require every
  claimed focus/edit/select/submit transition to originate from a `KeyEvent`
  passed to the production router over the current rendered hit map; add a
  negative test that direct action construction cannot complete the ledger.
- **Hit-map inspection becomes identity synthesis.** Scenario has no node ids.
  Read exact action and field metadata from the realized frame, record the
  focused literal id, and fail on zero/duplicate candidates.
- **Form drafts are injected instead of typed.** Fill fields with character,
  Backspace, arrow, Space, and Enter events; assert the outgoing canonical
  request values equal router-owned drafts.
- **False completion on any snapshot or unrelated result.** Match workspace,
  case, request id, action id, returned session UUID, and exact entity state.
- **Stale evidence from a prior run.** Validate distinct canonical paths and
  create the evidence file with `create_new`; one terminal completion/failure
  event closes the stream.
- **The headless wrapper is unavailable or drops presentation state.** Call
  TUI-kit's public `render_to_lines_with_presentation_state` directly with the
  router `RenderState` and app `PresentationState`, exactly matching production
  `draw()`'s state inputs. Do not call the TUI crate's `#[cfg(test)]` wrappers
  or the presentation-blind variant. This also avoids a PTY requirement for
  local and downstream harnesses. Evidence remains file-only because stdout is
  the foreground app's UI channel, not because this mode emits terminal escapes.
- **Action replacement is confused with a surface rerender.** Count
  `PluginSurfaceRender` at the request boundary and record accepted replacement
  separately. Freeze the render-request count after initial/reconnect pulls.
- **Event draining is mistaken for sanctioned polling.** The bounded loop may
  only consume pushed client frames and inspect local converged state; assert it
  emits no repeat read/list/surface request.
- **Parent/TUI schema drift.** Check in one JSON schema and fixtures, test them
  against Rust decoding/event serialization, and require the parent to import
  those files rather than copy constants.
- **Evidence leaks local paths.** Keep committed fixtures synthetic and scan the
  plan/diff. Runtime worktree facts come from the temporary caller scenario or
  Hub result and are not copied into committed reports without scrubbing.
- **Cross-repo failure is patched locally.** Route missing Workspaces action
  metadata/result facts to Workspaces, Hub spawn/lifecycle facts to Hub, and
  reusable focus/router defects to TUI-kit.
- **Colon-bearing pipeline worktree breaks macOS Cargo.** Use an explicit fresh
  colon-free `CARGO_TARGET_DIR` for all implementation gates.

## Acceptance checks and downstream proof

Clean Plan baseline at `0668435`:

- `script/fmt` — pass.
- plain `script/test` — fails before test execution because the generated
  worktree path contains `:` and Cargo cannot construct
  `DYLD_FALLBACK_LIBRARY_PATH`; this is the known environment gotcha.
- `CARGO_TARGET_DIR=/private/tmp/botster-tui-plan-1785602853 script/test` — pass,
  125 unit tests plus 1 package-manifest test.
- `CARGO_TARGET_DIR=/private/tmp/botster-tui-plan-1785602853 script/clippy` — pass
  with strict `-D warnings`.

Required implementation gates:

1. `script/fmt`.
2. `CARGO_TARGET_DIR=<fresh-colon-free-dir> script/test`.
3. `CARGO_TARGET_DIR=<fresh-colon-free-dir> script/clippy`.
4. Focused strict scenario tests: missing/one-sided paths, unsupported version,
   unknown fields, duplicate/empty cases, invalid resolution matrix,
   scenario/evidence collision, existing evidence, and committed fixture decode.
5. Focused evidence tests: valid JSONL per schema, create-new semantics, flush
   ordering, correlation fields, bounded failure output, one terminal event,
   and no stdout writes.
6. Production-frame keyboard tests over a representative target-first two-form
   surface: Tab/Shift-Tab reaches the exact realized controls, select changes
   by key, text is typed through the router, Enter submits, and canonical
   request node/action/values/payload come from the rendered action. Mouse or a
   direct draft/action ablation must fail the keyboard ledger.
7. Request-ledger tests: exactly the sanctioned initial and reconnect surface
   renders, action-result replacement recorded separately, exact entity
   snapshot/upsert/patch/remove changes the next frame, and no list/surface
   synchronization request is emitted. Ablate the request freeze or exact-row
   predicate and require the focused test to fail.
8. Add a new pre-merge installed-binary gate exposed as
   `script/test-live-hub workspaces installed-driver`, modelled on the existing
   `apps open` live-Hub test. With explicit Hub, worker, and clean Workspaces
   package inputs, the test owns an isolated Hub/package/Git fixture, installs
   and enables this exact TUI package, launches it through
   `botster-hub apps open` with both caller paths, and validates the checked-in
   schema against the resulting JSONL. It must prove the binary consumed the
   Hub-injected connection and both caller paths, reached the real Workspaces
   surface through the presentation-aware headless production render
   tree/HitMap, proved both Spawn dialogs were realized, drove the rendered
   controls, and wrote exactly one schema-valid terminal completion or bounded
   failure event. A successful scenario must exercise the complete
   three-case contract; a bounded-failure fixture is supplemental and cannot be
   the sole entrypoint proof. The test/script owns Hub lifecycle only as test
   infrastructure; the production driver never does.
9. Preserve existing generic runtime evidence:
   - the contract-matrix `script/test-live-hub` mode with explicit Hub/worker
     binaries and canonical fixture;
   - `script/test-live-hub workspaces plumbing` and `workspaces lifecycle`
     against a clean explicit Workspaces checkout when those external inputs
     are available.
   These remain regression/supporting evidence, not the caller-owned proof.
10. `git diff --check` plus raw scans proving no absolute home paths, sibling
   discovery, production-driver Hub lifecycle/package mutation,
   `ListSessions`, mouse helper, direct Workspaces action payload, timing sleep,
   or new production policy. The explicitly named isolated test harness is the
   only allowed Hub/package/Git lifecycle site in this repository.

Required downstream runtime proof is owned by
`ticket_1785192726_335558` after this TUI commit is merged. From one fresh
caller-owned Hub, that harness must install/enable the exact merged TUI,
Workspaces, and Web packages; admit the temporary Git target/template matrix;
import and validate the scenario against the canonical files under
`crates/botster-tui/fixtures/`; and launch the installed TUI without a PTY with:

```sh
BOTSTER_TUI_ACCEPTANCE_SCENARIO=/path/to/caller-scenario.json \
BOTSTER_TUI_ACCEPTANCE_EVIDENCE=/path/to/new-evidence.jsonl \
  botster-hub apps open --data-dir /path/to/shared-hub botster-tui
```

The parent must record exact TUI, Workspaces, Hub, worker/Core, TUI-kit, and UI
contract provenance and prove:

1. the installed binary consumes the Hub-injected connection and caller paths;
2. the real existing workspace and shared session state render;
3. all three ordered target-first cases are reached and submitted by keyboard
   through headlessly realized production render-tree frames/hit maps;
4. each canonical request/result/session UUID is correlated in evidence;
5. the parent independently confirms existing-worktree, existing-branch, and
   missing-branch Hub/Git outcomes and exactly-one-workspace membership;
6. authoritative lifecycle entity changes alter subsequent TUI frames after the
   initial/reconnect surface pulls without `ListSessions`, list refresh,
   polling request, or synchronization surface rerender;
7. rejected collision cases remain non-destructive;
8. the parent—not TUI—stops sessions/Hub and removes all shared fixtures; and
9. the JSONL stream has one complete event and no missing case or request-count
   stage.

Code existence, unit fixtures, the old isolated-Hub Workspaces mode, or direct
payload evidence is insufficient; the new in-repository installed-driver gate
must execute the production entrypoint before merge, and the registered
downstream dependency must then supply shared-runtime proof. Any final producer
defect must be routed to its owning repository rather than waived or repaired
in the integration ticket.

## Vault gaps worth capturing

The vault already covers installed runnable inputs, exact-entity readiness,
real-frame input, and structured action evidence separately. It does not yet
capture the reusable cross-client pattern established by this answered design:
foreground terminal acceptance drivers should use caller-owned scenario and
evidence files because stdout belongs to the terminal, while the parent remains
authoritative for shared runtime truth.

Do not capture that as shipped during Plan. After the merged TUI driver and the
final Workspaces parent proof both pass, capture one atomic pattern through the
inbox/document/connect/verify pipeline. If implementation exposes only a
one-off Workspaces contract, record no durable vault note.

## Convention and workflow checklist

- Target resolved from Project Pipelines before repository inspection: done.
- Exact repository charter and implicated surface charters loaded: done.
- Exact vault note identities validated against filenames: done.
- Repository README, source, package manifest, scripts, prior plans, and
  baseline gates inspected: done.
- Smallest surgical change; no speculative abstraction or adjacent cleanup:
  planned.
- Repository and cross-repository ownership explicit: done.
- Blocking interface ambiguity asked and durable answer incorporated: done.
- Runtime/user entrypoint and downstream proof named: done.
- Convention conflicts: none.
- Verification evidence: Plan baseline recorded above; implementation and final
  downstream shared-Hub commands remain required.
- Durable knowledge captured now: no; one evidence-dependent vault candidate is
  recorded above.
