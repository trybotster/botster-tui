# TUI Workspaces lifecycle acceptance mode plan

## Outcome

Add a production-shaped `botster-tui` live acceptance mode that accepts an
explicit real `botster-workspaces` package checkout, installs and enables that
package in an isolated Hub, opens its owner-authored `workspaces` surface, and
drives the delivered tree through the same `TuiApp`, session entity reducer,
Ratatui frame, `HitMap`, and `InputRouter` used by the interactive client.

The mode is a reusable downstream consumer gate. It must prove that the TUI's
generic `/session` binding path moves a referenced session from current to
ended from entity frames without a list or surface refresh, that reconnect
rebuilds the view through a fresh sanctioned subscription plus explicit route
pull, that ended and absent/deleted references remain legible until an
owner-authored action removes membership, and that keyboard and mouse
activation dispatch the exact rendered Hub action with canonical row identity.

This ticket does not implement the Workspaces lifecycle surface. The durable
human decision recorded as Project Pipelines answer
`question_1785545020_154092` requires this acceptance mode to merge first and
requires downstream Workspaces ticket `ticket_1785296184_677408` to run it
against that ticket's real current/ended/unknown surface. This run therefore
proves the reusable client and harness machinery against current main plus the
Hub-owned canonical session-binding producer, and explicitly leaves the final
real-package lifecycle assertion as a fail-closed downstream gate. It must not
consume the downstream ticket's sibling worktree or claim Workspaces product
behavior before that behavior exists.

## Pipeline identity and routing

- Ticket: `ticket_1785545086_939840`
- Run: `run_1785545102_681755`
- Target repository: `trybotster/botster-tui`
- Target id: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Target resolution: `project_pipelines_current_context` supplied the opaque
  target id, the admitted spawn-target registry mapped it to the `botster-tui`
  target, and this worktree's `origin` is
  `git@github.com:trybotster/botster-tui.git`.
- Base: `main` at `65af502b63503eb6995b2d9cc9c618bc5bd9a618`, equal to
  `origin/main` and the ticket branch at planning time.
- Repository charter: `[[botster-tui-playbook]]`.
- Role playbooks: `[[planner-playbook]]`, then
  `[[botster-planner-playbook]]`.
- Workflow overlay: `[[project-pipelines-playbook]]` for checklist, artifact,
  gate, and advancement policy only. No Project Pipelines source is in scope.

The target was resolved before planning and was not inferred from the ambient
process directory.

## Context loaded

Repository and surface guidance:

- `[[botster-architecture]]`
- `[[cli-patterns]]`
- `[[spa-patterns]]`
- `[[botster-tui-playbook]]`
- `[[botster-tui-kit-playbook]]`
- `[[botster-workspaces-playbook]]` as the consumed package ownership charter,
  not as authority to edit that repository
- `[[tui and browser are equal clients]]`
- `[[botster tui consumes tui kit through a thin app policy adapter]]`
- `[[tui client attach uses hub protocol not session protocol]]`
- `[[tui and socket terminal streams use clientworker transport adapters]]`
- `[[botster tui uinode event routing captures hit regions during draw]]`
- `[[tui error dedup tests must drive real input handlers]]`
- `[[botster-runtime-reviewer-playbook]]`
- `[[botster-runtime-verifier-playbook]]`

Targeted state, binding, identity, and acceptance guidance:

- `[[botster workspace records are plugin owned references not hub authority]]`
- `[[active workspace entity snapshots derive from live sessions before persisted manifests]]`
- `[[list-workspaces now only returns running sessions breaks historical browsing]]`
- `[[plugin-owned dynamic state uses plugin-namespaced entity frames]]`
- `[[botster plugin entities are canonical for plugin-owned dynamic state]]`
- `[[botster hub client state sync is entity frame only]]`
- `[[botster client subscriptions should not hydrate global state]]`
- `[[plugin surfaces request model state through ui bindings not hub subscribe]]`
- `[[botster plugin entity providers must replay after entity broadcast reload]]`
- `[[botster entity broadcast hot reload must re-register built-in providers]]`
- `[[attach mode needs client-side reconnect after hub exec-restart]]`
- `[[botster tui attach must explicitly pull core entities after subscribing]]`
- `[[ui contract row ids can bind before template expansion]]`
- `[[renderer state accepts only realized literal identity]]`
- `[[post expansion identity uniqueness is scoped to one render not one tree]]`
- `[[tui adapter maps shared primitives onto existing rust render tree without flag day rewrite]]`
- `[[plugin authored tui surfaces dispatch via action props not node id literals]]`
- `[[uinode action precedence resolves before disabled filtering]]`
- `[[action precedence scopes to one hit region not one node]]`
- `[[table level row action and activation cannot carry row identity]]`
- `[[fixture driven acceptance smoke tests can prove first party package plumbing]]`
- `[[plugin conformance packages prove shared contracts while examples prove product behavior]]`
- `[[required smoke modes must disable skips and prove execution positively]]`
- `[[adding harness event families changes every mixed family oracle]]`
- `[[a regression test must be shown to go red with the fix reverted]]`

Planner/workflow notes required by the Botster planning overlay were also
loaded: `[[project pipeline orchestration belongs in a device-level botster
plugin]]`, `[[project pipelines needs an operator workbench not more
primitives]]`, `[[project pipelines ui contract belongs in the plugin
readme]]`, `[[botster orchestration should spawn agents with explicit target
ids]]`, `[[botster orchestration prompts must bind agents to explicit
worktrees]]`, `[[botster pipeline needs continuous product owner between agent
steps]]`, `[[plan agents must author vault context as wikilinks not home
paths]]`, `[[pipeline vault checklists must cite exact resolvable note titles]]`,
and `[[vault example paths are not repository placement conventions]]`.

Repository evidence inspected:

- `README.md`, root/crate `Cargo.toml`, `Cargo.lock`,
  `botster-package.json`, and `crates/botster-tui/tests/package_manifest_test.rs`;
- `crates/botster-tui/src/app.rs`, especially `SessionEntityState`, the held
  session subscription, `force_reconnect`, generic plugin action ownership,
  package navigation, `/session` materialization, realized identity checking,
  and the isolated-Hub contract-matrix test;
- `crates/botster-tui/src/renderer.rs` and
  `script/{fmt,test,clippy,test-live-hub}`;
- prior repository plans for push lifecycle reconciliation, canonical session
  binding, canonical BindList row identity, generic plugin action routing, and
  Hub-owned package surfaces;
- current `botster-workspaces` main at `c78f3bf`, including its README,
  manifest, `plugin.lua`, owner-authored detail surface, package smokes, and
  explicit statement that lifecycle grouping is still owned by follow-on
  `ticket_1785296184_677408`;
- the downstream ticket's durable context, dependencies, human answers, and
  Plan artifact, which registers this ticket as an open blocking dependency.

Current TUI main already supplies the generic runtime prerequisites:

- one authoritative held `/session` entity subscription with snapshots,
  ordered deltas, generation invalidation, and reconnect baselines;
- plugin surfaces materialized from that store on every draw, with no
  `ListSessions` or imperative surface refresh;
- item-relative bound row ids resolved to canonical literals before the kit;
- real-frame keyboard/mouse dispatch of exact rendered action metadata; and
- generic owner-routed `PluginSurfaceAction` results through the public Hub
  client contract.

Baseline verification on the clean planning branch passed:

- `env CARGO_TARGET_DIR=/private/tmp/botster-tui-plan-fmt-target script/fmt`
- `env CARGO_TARGET_DIR=/private/tmp/botster-tui-plan-test-target script/test`
  (`119` app tests and `1` package-manifest test)
- `env CARGO_TARGET_DIR=/private/tmp/botster-tui-plan-clippy-target script/clippy`

## Scope

### In scope

1. Add an explicit Workspaces live acceptance mode to the existing isolated-Hub
   harness. The mode takes a caller-supplied real package path, validates that
   it contains the Workspaces manifest and plugin entrypoint, and installs,
   enables, and opens it through public Hub package/navigation requests. It
   never discovers or reads an ambient sibling checkout.
2. Keep all Workspaces knowledge in test/harness code. Production `TuiApp`,
   renderer, input router, session store, and action dispatch remain generic;
   no package name, workspace lifecycle label, workspace node id, or action id
   may enter non-test application behavior.
3. Seed deterministic package state through the real plugin-worker/MCP boundary
   only as test setup. Use canonical UUIDs for a stable current session, a
   session that transitions current to ended, and a deliberate missing/deleted
   reference. Do not hand-author a plugin surface, action request, entity frame,
   or Workspaces response.
4. Open the package from the TUI's rendered package navigation and select the
   owner-authored workspace row through the production frame/hit map/router.
   Derive package/surface/action/node/payload identity from the delivered tree;
   do not reproduce action payloads in the test.
5. While the owner surface remains active, cause the authoritative Hub session
   to end, drain the real entity subscription, and prove the next production
   frame moves the referenced UUID from the owner-authored current region to
   the ended/history region. Assert the transition occurs without
   `ListSessions`, a new `PluginSurfaceRender`, a package reload, or an
   imperative TUI refresh.
6. Prove ended and missing/deleted references remain visible and operable.
   Activate one owner-authored membership action through a second real input
   path (keyboard if selection used mouse, or vice versa), require the exact
   canonical realized row/node identity and delivered payload at the outgoing
   `PluginSurfaceAction` seam, and verify only the explicitly removed reference
   disappears.
7. Force the TUI connection lifecycle through its production reconnect path.
   Require a new session subscription id and authoritative snapshot, reject
   stale-generation deltas, reopen the package through refreshed admitted
   navigation, reselect the workspace through its delivered action, and prove
   current/ended/missing references rehydrate without a legacy list refresh.
8. Add positive completion evidence for every required stage. The Workspaces
   mode must fail closed if the package path is absent/invalid, the package or
   surface was not installed/opened, any lifecycle stage was skipped, either
   input path did not dispatch, reconnect did not establish a fresh baseline,
   or cleanup did not complete.
9. Keep the existing contract-matrix live mode green. Any shared script/test
   harness changes must explicitly preserve its fixture validation, execution,
   and completion oracle.
10. Update the README with the reproducible mode command, its external package
    ownership, its fail-closed semantics, and the downstream handoff.

### Intentional scaffold boundary for this run

Current merged `botster-workspaces` does not yet emit lifecycle-grouped detail;
its README and source explicitly defer that behavior to the downstream ticket.
The human answer forbids consuming that sibling worktree and requires this mode
to merge first. Therefore this ticket's own proof is split deliberately:

- run the current real package through install/enable/navigation/detail/action
  plumbing using public Hub/plugin-worker paths;
- run the canonical Hub-owned session-binding scenario through the exact TUI
  entity/frame/render/identity/input/reconnect path that the mode composes;
- unit-test required-mode validation, stage accounting, and failure behavior;
- preserve the final combined current/ended/missing Workspaces invocation as a
  required, fail-closed downstream command in
  `ticket_1785296184_677408`.

Neither half alone may be reported as final Workspaces lifecycle product proof.
The downstream ticket must run the merged combined mode against its real
package checkout before satisfying its Web/TUI consumer gate.

### Non-scope

- No Workspaces lifecycle projection, grouping, storage, labels, UI tree,
  actions, forms, or product policy in this repository.
- No edits to `botster-workspaces`, `botster-hub`, `botster-hub-client`,
  `botster-ui-contract`, `botster-hub-test-support`, `botster-tui-kit`, Web, or
  Core from this run.
- No sibling-worktree/path override, unmerged branch dependency, copied
  Workspaces package, or TUI-authored fake Workspaces surface.
- No local lifecycle derivation from registry state, process exit, attachment,
  or command results; render only Hub-authored `lifecycle_class` effects.
- No client-local row identity, synthesized payload, plugin-specific production
  dispatch branch, renderer-specific UiNode prop, list-refresh fallback,
  polling, second session store, private protocol, or duplicate contract type.
- No broad `app.rs` split, new async runtime, generalized acceptance framework,
  optional product configuration, or adjacent terminal/package cleanup.
- No claim that code existence or fixture-only rendering is real-package
  lifecycle proof.

## Ownership boundaries and cross-repository dependencies

### `botster-tui` owns in this run

- The acceptance mode and its explicit package-path input.
- Isolated-Hub lifecycle ownership and cleanup for the mode.
- The production client path used by the proof: navigation, active surface
  ownership, subscribed session projection, materialization, frame/hit map,
  input routing, reconnect policy, and public action requests.
- Test-only stage accounting and diagnostics.

### Other repositories retain ownership

- `botster-workspaces` owns workspace records, deliberate session references,
  lifecycle grouping in its owner-authored tree, labels, membership actions,
  and the final real-package lifecycle acceptance invocation. Its downstream
  ticket is `ticket_1785296184_677408`, target
  `tgt_71266a8d976d4535902ffed09c18a7ba`.
- `botster-hub` owns session lifecycle truth, `/session` entity frames,
  subscription ordering, package admission, plugin-worker execution, and the
  Hub UI/action contract.
- `botster-tui-kit` owns reusable Ratatui rendering, hit regions, focus,
  keyboard/mouse routing, and terminal input mechanics.
- `botster-web` owns its sibling real-package acceptance mode under
  `ticket_1785545085_392193`; this TUI run does not substitute for browser
  parity.

There is no new upstream implementation prerequisite at Plan time. All generic
TUI/Hub/TUI-kit capabilities needed by the mode are merged in current pins.
This ticket is itself a registered blocking dependency of the Workspaces
lifecycle ticket. If implementation exposes a missing generic producer or kit
mechanic, stop and register a narrow dependency against that repository's
target instead of patching around it here.

## Assumptions and unknowns

- Assumption: the accepted command shape is an explicit external package path,
  matching the current Workspaces and TUI live-harness documentation. The
  harness must validate the package identity from its manifest rather than
  trusting the directory name.
- Assumption: deterministic setup may call Workspaces MCP tools through
  `DaemonRequest::PluginMcpCallTool`; setup does not count as user-action proof.
  All claimed interaction proof begins from a Hub-delivered rendered control.
- Assumption: ending a controlled Hub session through public lifecycle control
  is sufficient to produce the canonical current-to-ended entity transition;
  the TUI must not derive the resulting class.
- Assumption: reconnect legitimately clears the active plugin surface in the
  current TUI policy. Rehydration therefore means fresh session snapshot plus
  the explicit package navigation/surface pull and owner-authored selection
  action, not retaining a stale surface across transport generations.
- Assumption: package-specific expected UUID visibility and lifecycle grouping
  assertions are acceptable in the acceptance test module, but never in
  production client logic.
- Unknown: the downstream Workspaces surface's final exact node labels/ids are
  not merged. The mode should discover action metadata from rendered nodes and
  keep the semantic oracle limited to UUID visibility and owner-authored
  current/ended/unavailable regions. If no stable cross-client semantic signal
  exists, stop and ask rather than keying the mode to incidental prose.
- Unknown: the cleanest mode switch may be a dedicated argument to
  `script/test-live-hub` or a narrowly named sibling wrapper. Prefer extending
  the existing wrapper without making Workspaces inputs mandatory for the
  contract-matrix mode; choose the smallest shape that prevents skip conflicts
  and duplicated build/Hub setup.
- Convention conflict: none. The plan preserves repository ownership, uses
  public Hub and kit primitives, keeps product semantics in the package, uses
  cold single-path state/action contracts, and treats final product proof as a
  durable downstream gate rather than a waiver.

If the final Workspaces producer gives two plausible meanings for lifecycle
regions or lacks a stable semantic signal needed by both clients, ask through
Project Pipelines. Do not infer from text styling, node-id spelling, or
renderer position.

## Implementation shape

Keep production behavior unchanged unless the real mode exposes a generic bug.

1. Add test-only helpers near the existing isolated-Hub and plugin-contract
   helpers in `crates/botster-tui/src/app.rs` for:
   - validating/using the explicit Workspaces package path;
   - installing, enabling, and reloading it through the public Hub boundary;
   - seeding workspace references through real plugin MCP calls;
   - waiting on authoritative session entity stages;
   - locating and activating delivered regions without constructing requests;
   - rendering lifecycle stages through the production app frame; and
   - recording a named completion ledger.
2. Add one focused live test for the Workspaces mode rather than widening the
   existing contract-matrix oracle. Both tests may reuse existing
   `HubConnection`, `TuiApp`, render, click/key, and request-observation helpers.
3. Extend live-wrapper mode selection and validation so the contract-matrix
   mode keeps its current required fixture and the Workspaces mode requires the
   explicit package path. Every successful exit validates its mode-specific
   completion evidence.
4. If a generic renderer/client defect appears, make the smallest fix at the
   existing owner boundary and add a focused production-path regression plus a
   negative control. Route kit/schema/Hub defects to their owners.

## Affected surfaces and files

Expected changes:

- `crates/botster-tui/src/app.rs`
  - test-only Workspaces live acceptance orchestration;
  - public Hub/package/plugin setup helpers only where existing helpers do not
    already cover the path;
  - stage ledger and real-frame/action/reconnect assertions;
  - a production fix only if the real package reveals a generic TUI defect.
- `script/test-live-hub`
  - explicit mode/path validation and required-mode environment;
  - mode-specific test filter while preserving contract-matrix behavior.
- `README.md`
  - reproducible Workspaces-mode command, external ownership, positive proof,
    and downstream final-consumer handoff.
- `docs/plans/tui-workspaces-lifecycle-acceptance-mode-plan.md`
  - this reviewable plan artifact.

Expected unchanged unless concrete compilation/runtime evidence requires them:

- `crates/botster-tui/src/renderer.rs`
- both Cargo manifests and `Cargo.lock`
- `botster-package.json`
- `crates/botster-tui/tests/package_manifest_test.rs`

Any dependency movement requires an explicit owner/dependency decision and a
single-source contract audit; it is not routine scope for this ticket.

## Risks

- **False product proof:** composing current real-package plumbing with generic
  Hub fixture proof can be mislabeled as final Workspaces lifecycle proof.
  Mitigation: preserve the scaffold boundary in README, artifact, test output,
  and downstream dependency evidence; only the downstream combined invocation
  may close the consumer gate.
- **Required mode silently skips:** ordinary tests intentionally skip live Hub
  work when binaries are absent. Mitigation: required mode disables that skip,
  validates the package path, records every stage, and checks completion before
  success, including conflicting legacy skip settings.
- **Product semantics leak into production:** package names or action IDs could
  enter `TuiApp::handle_action`. Mitigation: confine them to `#[cfg(test)]` and
  dispatch only the request emitted by the rendered hit region.
- **Hand-authored payload false green:** recreating expected action payloads
  would not prove the package/TUI contract. Mitigation: read action metadata
  from the delivered/materialized node and compare the outgoing request.
- **Stale surface mistaken for reconciliation:** rerendering after a fresh
  `PluginSurfaceRender` could hide missing entity-driven updates. Mitigation:
  hold one surface during current-to-ended, assert the render-request count is
  unchanged, and inspect the next production frame after draining entity
  frames.
- **Reconnect false green:** old entity state can satisfy waits immediately.
  Mitigation: require a changed subscription id, an authoritative new snapshot,
  and a post-reconnect surface pull before assertions; do not use a wait helper
  that accepts pre-reconnect state.
- **Historical reference accidentally dropped:** an entity remove can erase the
  bound row while the package must render an unavailable history reference.
  Mitigation: assert the UUID remains visible until a real membership action is
  accepted, then assert only that reference disappears.
- **Identity ambiguity:** selecting row 1 while asserting row 2 payload can
  still produce plausible actions. Mitigation: require exact realized node id,
  payload UUID/workspace id, distinct hit rectangles, and one keyboard plus one
  mouse path.
- **Shared harness regression:** changing fixture inputs or event observations
  can alter the existing contract-matrix reader. Mitigation: keep oracles
  mode-specific and rerun the current required live mode unchanged.
- **Leaked Hub/session processes:** a failed mid-stage assertion can leave
  children or sockets. Mitigation: retain `IsolatedHub` cleanup ownership and
  require clean shutdown in the completion ledger.
- **Colon-bearing worktree path:** macOS Cargo library paths can fail in this
  generated worktree. Mitigation: use explicit colon-free target directories
  for every command.

## Acceptance checks and tests

### Static and unit proof in this ticket

- Source scan finds no `botster-workspaces`, Workspaces lifecycle labels, or
  Workspaces action IDs in non-test production branches.
- Workspaces mode with no path, a missing directory, wrong package manifest,
  or missing `plugin.lua` fails before Hub setup with an actionable diagnostic.
- Required mode plus any legacy skip setting still executes or fails closed.
- The completion ledger requires, at minimum: package validated, installed,
  enabled, navigation opened, owner row selected, current rendered, ended
  rendered, absent/deleted rendered, mouse dispatch, keyboard dispatch, fresh
  reconnect snapshot, surface reopened, historical refs rehydrated, and clean
  shutdown. A unit regression that removes one stage must fail.
- Existing canonical session binding tests continue to prove snapshot,
  current-to-ended patch, remove/unavailable, reconnect, exact bound row ids,
  keyboard/mouse dispatch, and stale-generation rejection through the
  production frame/router path.
- Ablate the new stage check or entity-driven transition barrier narrowly and
  prove the focused regression goes red, then restore it and rerun green.

### Current merged real-package plumbing proof in this ticket

Against a clean checkout of current `botster-workspaces` main, prove through an
isolated current Hub:

- explicit path validation identifies `botster-workspaces` from its manifest;
- install, enable, reload, admitted navigation, and the stable `workspaces`
  surface succeed through public requests;
- the TUI renders the owner-authored index/detail rather than a TUI fixture;
- a workspace row and one owner-authored detail action dispatch through the
  real frame/hit map/router with exact rendered identity; and
- the Hub remains responsive and shuts down cleanly.

Record this as package plumbing and generic action proof, not lifecycle product
proof.

### Required downstream real-package lifecycle proof

After `ticket_1785296184_677408` implements its owner-authored lifecycle tree,
that Workspaces run must invoke the merged TUI mode with its own clean package
checkout. The exact wrapper syntax should be fixed during implementation and
documented in README, with inputs equivalent to:

```sh
BOTSTER_HUB_BIN=/path/to/current/botster-hub \
BOTSTER_SESSION_WORKER_BIN=/path/to/current/botster-session-worker \
BOTSTER_WORKSPACES_PACKAGE_PATH=/path/to/clean/botster-workspaces \
CARGO_TARGET_DIR=/private/tmp/botster-tui-workspaces-live-target \
  script/test-live-hub workspaces
```

That command must positively prove in one isolated run:

1. the real package is installed/enabled and its admitted navigation opens;
2. controlled canonical UUID references render in the owner-authored detail;
3. a referenced live session renders current;
4. its authoritative entity transition moves it to ended without
   `ListSessions`, `PluginSurfaceRender`, package reload, or manual refresh;
5. ended and missing/deleted references stay legible;
6. real mouse and keyboard actions dispatch exact rendered metadata and
   canonical realized row identity;
7. explicit membership removal is the only tested operation that drops a
   deliberate reference;
8. reconnect uses a new session subscription snapshot, then explicit admitted
   navigation/surface pull, and rehydrates the same current/ended/missing view;
9. no stale generation mutates the rehydrated view; and
10. the Hub and every controlled session shut down cleanly.

The downstream Workspaces gate must attach this command's output and merged TUI
commit. No fixture, source inspection, or composition summary may replace it.

### Repository gates

Run from a clean committed TUI worktree with colon-free targets:

```sh
env CARGO_TARGET_DIR=/private/tmp/botster-tui-fmt-target script/fmt
env CARGO_TARGET_DIR=/private/tmp/botster-tui-test-target script/test
env CARGO_TARGET_DIR=/private/tmp/botster-tui-clippy-target script/clippy
cargo run -p botster-tui -- --smoke
git diff --check main...HEAD
```

Also rerun the existing required contract-matrix live command with exact Hub,
session-worker, and public fixture inputs. Pre-existing failures are not a
blanket waiver; attribute any failure to an exact command and base/branch
comparison.

## Pipeline checklist and artifacts

- Run vault checklist: `checklist_1785545337_799698`.
- Run workflow checklist: `checklist_1785545342_874875`.
- Both creation calls timed out after persistence. Listing authoritative run
  checklists found one of each, so they were adopted without retry.
- Attach this file as the Plan artifact, complete repository-routing and vault
  evidence in those checklists, submit `botster_stack_plan_gate`, and advance
  to `botster_stack_plan_review` without overrides.

## Vault gaps worth capturing

No new durable architecture rule is required at Plan time. Existing notes
already cover real-package versus conformance-fixture proof, required-mode
positive execution, entity-frame authority, pull/reconnect, canonical realized
identity, and real input routing.

Capture through the vault inbox only if implementation establishes a reusable
new rule not covered there, such as a general pattern for acceptance modes that
must merge before their real producer can exist, or a stable cross-client
semantic signal required for package-owned lifecycle regions. Repository paths,
environment names, fixture SHAs, and command syntax stay in this plan/README
and pipeline evidence rather than becoming vault conventions.
