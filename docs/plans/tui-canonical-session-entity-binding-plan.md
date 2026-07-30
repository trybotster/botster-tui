# TUI canonical session entity binding plan

## Outcome

Make `botster-tui` consume the Hub-owned `/session` entity family as the
binding context for plugin-authored `UiNode` surfaces. The application will
materialize canonical `UiChild::BindList`, `$bind`, and `bind_if` values from
its existing authoritative session subscription before handing a concrete
tree to `botster-tui-kit`. The kit remains the owner of reusable Ratatui
rendering, hit maps, and input routing.

Success is visible in the real application path:

1. a Hub/plugin-worker surface supplies the structural binding tree;
2. the TUI's existing session entity subscription supplies snapshot and delta
   rows;
3. the TUI application adapter resolves the tree against that store;
4. the production kit renderer paints the resolved tree and captures its hit
   regions;
5. subsequent upsert, patch, remove, and reconnect-snapshot frames change the
   next rendered frame without polling or a surface/list refresh.

## Pipeline identity and routing

- Ticket: `ticket_1785298229_854008`
- Target repository: `trybotster/botster-tui`
- Target id: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Run: `run_1785433256_101696`
- Repository charter: `[[botster-tui-playbook]]`
- Role overlays: `[[planner-playbook]]`, then
  `[[botster-planner-playbook]]`
- Workflow overlay: `[[project-pipelines-playbook]]` because this plan is
  gated and advanced through Project Pipelines. No Project Pipelines package
  or engine code is in implementation scope.

The target was resolved with the admitted spawn-target registry, not inferred
from the pipeline worktree path.

## Context loaded

Repository and surface guidance:

- `[[botster-architecture]]`
- `[[cli-patterns]]`
- `[[spa-patterns]]`
- `[[botster-tui-playbook]]`
- `[[botster-tui-kit-playbook]]`
- `[[tui and browser are equal clients]]`
- `[[botster tui consumes tui kit through a thin app policy adapter]]`
- `[[tui client attach uses hub protocol not session protocol]]`
- `[[tui and socket terminal streams use clientworker transport adapters]]`
- `[[botster tui uinode event routing captures hit regions during draw]]`
- `[[tui error dedup tests must drive real input handlers]]`
- `[[renderer acceptance tests must drive real frame backend]]`
- `[[session UUID is the sole routing key across all layers]]`
- `[[tui v2 entity frames must keep legacy lua state populated until layout
  migration finishes]]`
- `[[botster entity snapshots are authoritative reconnect baselines]]`
- `[[botster tui attach must explicitly pull core entities after subscribing]]`

Binding-specific atomic guidance:

- `[[plugin dynamic ui lists bind to plugin-owned entities]]`
- `[[ui bind list where filters plugin entity rows before template expansion]]`
- `[[ui bind list empty template renders entity backed empty rows]]`
- `[[ui contract row ids can bind before template expansion]]`
- `[[ui bind list typed templates are narrower than the runtime wire grammar]]`
- `[[plugin surfaces request model state through ui bindings not hub
  subscribe]]`

Repository/code evidence:

- `origin/main` is `b289eabd01b52e2ee3f67708d5a98a5cd7ddf1c7`
  and contains the merged prerequisite ticket
  `ticket_1785295085_796645`.
- The pipeline branch began at
  `0d26ce0b7ff71acaa5594a680d88436658426828`, so implementation must
  incorporate current `origin/main` before feature edits.
- Current main pins `botster-hub-client`, `botster-ui-contract`, and
  `botster-hub-test-support` to Hub commit
  `b403bb72c1065f633ae59fd876b13024e2ab54a7`, and
  `botster-tui-kit` to
  `c66f1ae60235d7d0ce0993f4e9ed89068a12b7d2`.
- That Hub revision exposes required
  `DaemonSessionEntity.lifecycle_class`, canonical `/session` binding grammar,
  conformance revision 24, the published
  `session_plugin_binding_conformance_scenario`, its reference materializer,
  and the real plugin-worker `contract.sessions` surface.
- `TuiApp` already owns the subscribed `SessionEntityState`, subscription
  generation, snapshot/delta reducer, reconnect invalidation, and fresh
  subscription baseline.
- `plugin_surface_render_root` currently sends unresolved `BindList` children
  to the kit. The kit intentionally paints only `empty_template` for an
  unresolved bound list, which is not binding proof.
- Hub documents `plugin_surface.ui_tree_snapshot.body` as the renderer
  entrypoint while preserving the same validated tree in
  `plugin_surface.body`. Hub has not deprecated either field. At the pinned
  revision, `daemon_transport.rs` has one `DaemonPluginSurface` construction
  site and always emits an identity-matched snapshot whose
  package/surface/body equal the outer surface. Reading only the documented
  snapshot entrypoint is a deliberate strict client choice; the missing or
  mismatched branch is a contract guard, not a migration away from a
  Hub-deprecated field.

## Human decision and explicit assumption

Question `question_1785433514_880579` resolved an apparent dependency cycle.
This run must implement and prove the complete reusable TUI binding seam with
Hub's real published `contract.sessions` fixture. The owner-authored
Workspaces detail surface is downstream proof owned by
`ticket_1785296184_677408` and final integration ticket
`ticket_1785192726_335558`, after this client capability merges.

Therefore:

- this run does not reverse dependencies or wait on Workspaces;
- it does not add Workspaces-specific policy or a local Workspaces fixture;
- its acceptance evidence must be sufficient for the downstream Workspaces
  ticket to consume the revision-24 exact-UUID producer shape without another
  TUI implementation change.

Review later proved that the pinned contract cannot express distinct
row-relative `UiNode.id` values for a `bind_list` matching multiple rows.
Question `question_1785437646_635577` approved a non-blocking follow-on chain
for that newly discovered contract gap: this run may complete against
revision 24 with unsafe ID-bearing multi-row expansion rejected visibly; Hub
ticket `ticket_1785436979_640117` publishes the canonical row-identity
contract; TUI follow-up `ticket_1785438029_926883` then owns the repin,
removing the rejection, and real per-row keyboard/hit-region proof before
Workspaces consumes it.

## Scope

### In scope

1. Incorporate current `origin/main` so the implementation uses the already
   merged Hub/UI-contract/TUI-kit package surface pins and live-harness work.
2. Treat the identity-matched `ui_tree_snapshot.body` as the only incoming
   structural plugin tree because it is the documented renderer entrypoint.
   Verify the pinned producer invariant that snapshot package, surface, and
   body equal the outer surface; normalize the snapshot into the app-owned
   active tree used for accepted replacement behavior. Reject a missing or
   mismatched snapshot visibly instead of adding a second read path through
   the outer `body`. Update every existing local surface constructor that sets
   `ui_tree_snapshot: None` to build the canonical identity-matched field.
3. Resolve the `/session` binding family from the existing
   `SessionEntityState.entities` map. Serialize/read that authoritative map at
   materialization time; do not create a second session entity cache.
4. Materialize binding constructs before the tree crosses the
   `botster-tui-kit` renderer boundary:
   - filter `BindList` rows by exact top-level `where` equality;
   - expand `item_template` once per matching row;
   - use `empty_template` only when the filtered result is empty;
   - resolve the item-relative `@/...` values emitted by the first-party
     `contract.sessions` producer from the current row;
   - replace exact `{ "$bind": path }` sentinels recursively inside node
     property values;
   - evaluate entity `bind_if` truthiness while preserving the existing
     presentation-state conditional path;
   - recurse through both positional children and named slots.
   These mechanics stay bounded to the pinned typed `UiBindList`, `UiBind`,
   and `UiBindIf` contract. Absolute `/session/...` value-path semantics have
   no first-party producer or worked semantics at this revision and are not
   defined locally in this ticket. If implementation encounters such a path,
   fail visibly and stop to route the missing canonical semantics rather than
   inventing client-local grammar or another hydration mechanism.
5. Keep `lifecycle_class` authoritative. The TUI must render Hub values
   `current`, `ended`, and `indeterminate`; it must not derive a class from
   `lifecycle`, `registry_state`, or local attachment state.
6. Re-materialize from current state for each frame/draw so snapshot, upsert,
   patch, remove, and fresh-generation reconnect snapshots change visible
   output and hit regions without an imperative refresh.
7. Produce a visible materialization diagnostic for malformed sources,
   unsupported families, missing required bound fields, or invalid resolved
   trees. A present entity with a bad/missing field must not be mislabeled as
   an unavailable reference.
8. Extend unit and isolated-Hub tests through the production Ratatui frame,
   `HitMap`, `InputRouter`, session reducer, Hub client subscription, and real
   plugin-worker surface route.
9. Update the repository README to document shipped entity-backed plugin
   binding support and remove the now-obsolete “not included yet” statement.

### Non-scope

- Changes to `botster-hub`, `botster-ui-contract`,
  `botster-hub-client`, `botster-hub-test-support`, or
  `botster-tui-kit`.
- A reusable generic materializer added to TUI kit. If implementation proves
  that typed, renderer-generic mechanics must change there, stop and route a
  separate `botster-tui-kit` child ticket.
- Workspaces-specific grouping, labels, actions, selection state, or lifecycle
  policy.
- Authoring or changing the Workspaces detail surface in this repository.
- Plugin-owned entity-family hydration beyond the already sanctioned and
  subscribed built-in `/session` family.
- Polling, list refreshes, surface rerender requests after entity changes, a
  second session truth store, local protocol/contract structs, compatibility
  binding grammar, or fallback from `ui_tree_snapshot` to
  `plugin_surface.body`.
- Broad `app.rs` cleanup, renderer refactoring, cache abstractions, optional
  binding configuration, or adjacent terminal/session behavior changes.

## Ownership boundaries and dependencies

### `botster-tui` owns in this run

- The live session subscription/store and its generation handling.
- The binding context that exposes those sanitized rows to the active plugin
  surface.
- The thin application adapter that turns a structural bound tree into a
  concrete `UiNode` tree for the kit.
- App-level error presentation and production-path conformance tests.

### Other repositories retain ownership

- `botster-hub`: `/session` identity/lifecycle truth,
  `DaemonSessionEntity.lifecycle_class`, subscription frames, binding grammar,
  snapshot identity, plugin-worker `contract.sessions`, and published
  conformance fixtures.
- `botster-tui-kit`: reusable Ratatui widgets, viewport/presentation handling,
  frame layout, hit-region capture, and keyboard/mouse input routing.
- `botster-workspaces`: semantic workspace membership and the eventual
  owner-authored detail tree.

### Dependency ledger

- Closed prerequisite `ticket_1785295607_887142` (`botster-hub`) supplies the
  canonical producer contract and fixture.
- Closed prerequisite `ticket_1785295085_796645` (`botster-tui`) supplies the
  merged dependency pins and current package/plugin production proof.
- Downstream `ticket_1785296184_677408` (`botster-workspaces`) will author and
  prove the real workspace detail surface after this generic TUI capability.
- Downstream `ticket_1785192726_335558` owns the final clean-stack
  Workspaces/Web/TUI/Hub click-through.

No new reverse dependency is added: Project Pipelines correctly rejected it
as a cycle, and the human answer preserved the current sequencing.

## Implementation shape

Keep the change local to the existing app adapter in
`crates/botster-tui/src/app.rs`.

1. Add small pure helpers near the existing plugin-surface helpers for:
   - selecting/identity-checking the canonical snapshot;
   - exposing the current session map as JSON values;
   - resolving producer-backed item-relative paths;
   - resolving recursive value sentinels;
   - expanding one child and recursively materializing a node.
2. On `DaemonResponseKind::PluginSurface`, validate and normalize the canonical
   snapshot before installing the active owner. Preserve the current
   presentation reset behavior when package/surface ownership changes.
3. Continue using the app-owned active body for accepted action replacement,
   but seed it from the documented snapshot entrypoint rather than the outer
   body field.
4. Change `plugin_surface_render_root` to accept the session binding context,
   materialize the active tree, then apply action-result field errors and
   perform final contract/capability validation before returning it to
   `renderer::render_node_with_presentation_state`.
5. Do not mutate the structural source tree when entity frames arrive.
   Materialization remains a projection, so focus/hit reconciliation observes
   each new concrete frame while the session store remains the only data
   truth.

This is intentionally not a new renderer framework or cache. It is the
smallest app-owned adapter needed to connect the already-owned store to the
already-owned kit renderer.

## Affected surfaces and files

Expected feature edits:

- `crates/botster-tui/src/app.rs`
  - `SessionEntityState` binding view and reducer-adjacent tests;
  - `TuiApp::apply_response` canonical snapshot installation;
  - `TuiApp::plugin_shell_surface` binding-context handoff;
  - `plugin_surface_body_node` / `plugin_surface_render_root` validation and
    materialization;
  - unit, frame/backend, hit-map/input, and isolated-Hub tests.
- `README.md`
  - foundation/live-Hub contract wording;
  - shipped plugin binding behavior;
  - removal of “Entity-store hydration for bound plugin lists” from non-scope.
- `docs/plans/tui-canonical-session-entity-binding-plan.md`
  - this repository-routed plan and its durable assumptions.

Current-main integration will also bring in the already merged prerequisite's
`Cargo.toml`, `Cargo.lock`, README, app, and prior plan changes. No additional
dependency revision or manifest edit is expected for this ticket. If
implementation discovers a newer merged producer is required, stop and route
that as an explicit dependency decision rather than silently changing pins.

`crates/botster-tui/src/renderer.rs`, `script/test-live-hub`, and other files
should change only if the existing public test helpers cannot exercise the
production path; any such edit must trace directly to acceptance evidence.

## Acceptance checks

### Canonical contract and source checks

- Confirm the implementation worktree contains current `origin/main` before
  feature edits.
- Confirm `botster-hub-client`, `botster-ui-contract`, and
  `botster-hub-test-support` resolve to one Hub revision and
  `botster-tui-kit` resolves to its merged prerequisite revision.
- Source-scan the final diff for:
  - no local session lifecycle class derivation;
  - no local contract structs or compatibility grammar;
  - no polling/list/surface refresh added for entity changes;
  - no Workspaces package/id/action branches;
  - no unresolved `BindList` reaching the production kit path for `/session`.
- Scope this convergence claim to one Hub revision for
  `botster-hub-client`, `botster-ui-contract`, and
  `botster-hub-test-support`, plus the pinned TUI-kit revision. `Cargo.lock`
  already contains two `botster-core` sources: the TUI's exact Core pin and a
  locked branch-sourced Core pulled transitively by Hub test support. That
  deterministic pre-existing Hub-owned dev-dependency is out of scope and is
  not a single-Core-source acceptance claim for this ticket.

### Reducer and materializer tests

Use Hub's published
`session_plugin_binding_conformance_scenario()` as the primary oracle, not a
TUI-authored copy.

- Assert the scenario's exact `conformance_fixture_revision == 24` so a future
  producer pin change cannot silently alter the oracle.
- At every stage, extract the TUI materializer's resolved
  `session_uuid -> rendered lifecycle/unavailable` map and require exact
  equality with the published
  `scenario.expected.{initial, after_ended_patch,
  after_indeterminate_patch, after_remove, after_reconnect}` map. The Hub
  `materialize_session_plugin_bindings` reference result may be asserted as a
  second equivalent oracle, but it does not replace real frame/hit-map proof.
- Treat those published expected maps as the cross-client parity oracle shared
  with sibling Web ticket `ticket_1785298229_125024`; do not hand-author a
  separate TUI expectation table.
- Initial snapshot:
  - matching references render `current`, `ended`, and `indeterminate`;
  - the deliberately missing UUID renders the fixture's unavailable template;
  - matching rows do not render the empty template.
- Upsert:
  - an initially absent referenced UUID becomes a concrete bound row from an
    authoritative public `DaemonEntityFrame::Upsert`;
  - no list/surface refresh request is involved.
- Patch:
  - the transition row changes `current -> ended -> indeterminate` from the
    authoritative `lifecycle_class` values;
  - a present row with a missing/malformed required bound value produces a
    materialization diagnostic, not unavailable.
- Remove:
  - removing the referenced UUID selects the unavailable template;
  - unrelated current/ended/indeterminate rows remain unchanged.
- Reconnect:
  - a fresh subscription generation clears the prior baseline, rejects stale
    prior-generation deltas, and converges on the fixture reconnect snapshot;
  - no automatic terminal attach or plugin surface refresh occurs.
- Binding grammar:
  - exact top-level `where` equality runs before template expansion;
  - item-relative paths resolve from the same canonical row store;
  - recursive `$bind` values and `bind_if` truthiness work in children and
    slots;
  - presentation conditionals retain their existing scoped behavior;
  - an absolute value path with no producer-defined semantics fails visibly
    and triggers stop-and-route rather than a client-local interpretation.

### Real renderer and input proof

- Render every lifecycle stage through the production Ratatui
  `Frame<TestBackend>` path, not only `render_to_lines` or a semantic tree
  walker.
- Assert backend cells contain the expected current/ended/indeterminate or
  unavailable output for each stage.
- At every stage, assert no backend cell contains the kit's unresolved
  fallbacks: `bind /`, `bind @/`, or
  `bound list: waiting for entities`. These negative controls directly prove
  the concrete tree crossed the kit boundary rather than relying on its
  plausible-looking fallback output.
- Assert the materialized node ids appear in the production `HitMap` and the
  unmatched item-template ids do not.
- Include a typed single-match bound actionable-row/control test that resolves
  its visible value and action payload, proves canonical `UiActionRequest`
  dispatch from the materialized hit region, exercises real mouse focus, and
  proves entity removal clears focus and the actionable region. Real
  multi-row keyboard activation and per-row hit-region proof are owned by
  follow-up `ticket_1785438029_926883` after the canonical row-identity repin.
  This is complementary to, not a replacement for, the published Hub fixture.
- Reconcile focus after entity-driven region changes and prove a removed row
  cannot retain a stale clickable region.

### Isolated live-Hub/plugin-worker proof

Extend
`headless_live_runtime_runs_against_isolated_hub_when_binaries_are_available`
and `assert_plugin_contract_matrix_renders_through_tui` on top of current main:

- install/run the real Hub-owned plugin contract matrix;
- construct the live `TuiApp` with the isolated Hub endpoint so
  `TuiApp::try_connect -> start_session_subscription ->
  subscribe_session_entities` creates the app's own production subscription;
  poll the app until its `SessionEntityState` has accepted the authoritative
  snapshot and contains the live spawned UUID before rendering any matching
  assertion;
- issue the real `contract.sessions` render through that app's real
  `HubConnection` as
  `DaemonRequest::PluginSurfaceRender { package_name, surface_id:
  "contract.sessions", payload: { "session_uuids": [live_uuid,
  missing_uuid] } }`, then feed the returned response through
  `TuiApp::apply_response`. This exercises the production request/response,
  canonical-snapshot installation, binding materialization, renderer, and
  input path while only the reference set is test-authored;
- leave `TuiApp::open_package_navigation` unchanged with `payload: json!({})`.
  Do not add a session-reference argument channel, plugin-specific navigation
  branch, configurable payload, or Workspaces policy to production code;
- assert the matching rendered `lifecycle_class` equals the entity held in the
  live app's non-empty `SessionEntityState`, proving the test did not inject a
  parallel binding store;
- prove a matching live row and a missing-reference negative control;
- prove live spawn/upsert, natural shutdown/ended patch, remove/unavailable,
  and fresh-generation reconnect rendering. For reconnect, drive
  `force_reconnect`, assert a new app-owned session subscription generation,
  wait for its fresh authoritative snapshot, and only then assert the rebound
  surface;
- prove the resulting concrete frame/hit map comes from the same application
  surface path used by the interactive TUI;
- retain the published scenario's deterministic indeterminate-class proof;
- assert observed requests contain no polling/list refresh introduced by
  binding reconciliation.

The existing explicit binary and fixture provenance remains mandatory:

```sh
BOTSTER_HUB_BIN=/path/to/botster-hub \
BOTSTER_SESSION_WORKER_BIN=/path/to/botster-session-worker \
BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE=/path/to/hub/packages/hub-test-support/fixtures/plugin-contract-matrix \
  script/test-live-hub
```

### Repository gates

Run from a clean final worktree:

```sh
script/fmt
script/test
script/clippy
script/test-live-hub
```

Also inspect the final diff and dependency/source scan. Pre-existing failures
are not blanket waivers; any failure claimed unrelated needs the exact command,
failure text, and a clean-main comparison.

### Downstream proof

This ticket's revision-24 exact-UUID producer proof must leave no additional
TUI implementation gap for that shipped shape. Generic ID-bearing multi-row
support follows the explicit chain approved in
`question_1785437646_635577`. The actual owner-authored Workspaces detail
surface is an explicit deferred acceptance check:

- `ticket_1785438029_926883` must consume the Hub row-identity contract, repin
  the TUI, lift the fail-safe rejection, and prove real per-row keyboard and
  hit-region dispatch.
- `ticket_1785296184_677408` depends on that TUI follow-up and must render its
  owner-authored workspace detail through the merged generic capability with
  no Workspaces-specific client code.
- `ticket_1785192726_335558` must repeat the clean-stack keyboard click-through
  using merged artifacts and the production package runtime.

Those downstream checks are not replaced by this ticket's contract fixture,
and they are not pulled into this repository's implementation scope.

## Risks and mitigations

- **Stale pipeline base:** the branch is behind the merged prerequisite.
  Incorporate `origin/main` first and review the merge/rebase diff separately
  from feature behavior.
- **False unavailable state:** the kit's unresolved `empty_template` fallback
  currently makes every bound list look absent. Resolve and filter in the app
  before rendering; test matching and empty controls together.
- **Duplicate session truth:** a second generic entity cache could drift from
  `SessionEntityState`. Project directly from the existing typed map.
- **Lifecycle derivation drift:** local inference from `lifecycle` or
  `registry_state` could disagree with Hub policy. Bind only the delivered
  `lifecycle_class`.
- **Generation races:** old deltas can arrive after reconnect. Preserve the
  existing generation id, required-baseline, and strictly advancing sequence
  gates and test them with visible output.
- **Silent binding failure:** replacing a missing field with empty text can
  masquerade as success. Return a visible diagnostic and keep unavailable
  reserved for the empty exact-UUID result.
- **Stale hit regions/focus:** entity removal changes the concrete tree.
  Re-render, rebuild the hit map in the same draw, and run the existing router
  reconciliation before accepting input.
- **Repeated node identity in multi-row expansion:** the pinned contract cannot
  bind `UiNode.id`, so expanding an ID-bearing template more than once would
  make keyboard focus and action dispatch ambiguous. Reject that expansion
  visibly and route canonical row-bound identity to Hub/UI-contract ticket
  `ticket_1785436979_640117`; do not invent client-local ID rewriting.
- **Dual structural read paths:** reading the outer
  `plugin_surface.body` when the documented snapshot entrypoint is absent
  would let the client accept two structural sources. Verify the pinned
  producer equality invariant, normalize only the identity-matched snapshot,
  and fail visibly otherwise without claiming Hub deprecated the outer field.
- **Accidental kit ownership expansion:** moving the entity store or product
  subscription into the kit violates both charters. Route any truly generic
  missing kit mechanism as a separate child ticket.
- **Test-only resolver:** a helper proven only with hand-authored values can
  diverge from the producer. Anchor grammar and lifecycle expectations in the
  published Hub fixture and real plugin-worker route.

## Assumptions and unknowns

Resolved assumptions:

- Hub revision `b403bb7` is the merged producer required by this ticket.
- The TUI's existing built-in session subscription is the sanctioned
  `/session` hydration path; no additional pull API is required.
- Materialization belongs in the TUI app adapter, not the kit, unless
  implementation produces contrary concrete evidence.
- Owner-authored Workspaces proof is downstream by explicit human decision.

Unknowns for implementation to validate early:

- Production Hub construction is verified to carry an identity-matched
  `ui_tree_snapshot`. Existing local test constructors that currently use
  `ui_tree_snapshot: None` must be updated to exercise that invariant.
- Whether current kit text/content regions are sufficient for the published
  `contract.sessions` hit-map assertion; actionable bound-region input proof
  may require a complementary typed contract tree in the TUI test module.
- Absolute `/session/...` value paths are not emitted by a first-party fixture
  and have no worked canonical lookup semantics at the pinned revision. They
  are excluded rather than guessed; encountering one is a stop-and-route
  event.
- Review proved that the pinned `UiNode.id` cannot express the row-relative
  identity needed by a `bind_list` matching multiple rows. Per the
  human-approved non-blocking chain, Hub run `run_1785436979_236604` feeds TUI
  follow-up `ticket_1785438029_926883`, which owns the repin and real per-row
  keyboard/hit-region proof. This run fails ID-bearing multi-row expansion
  visibly but does not wait on that chain.

None of these unknowns authorize a compatibility fallback, a local contract
copy, or a cross-repository edit.

## Vault gaps worth capturing

- If implementation confirms this ownership seam, capture the durable rule
  that `botster-tui` materializes canonical entity bindings from its subscribed
  app store before handing a concrete tree to TUI kit; the kit does not own
  entity hydration.
- If canonical snapshot normalization and accepted client-local replacement
  reveal a reusable rule, capture the distinction between immutable delivered
  structural snapshots and the app-owned active replacement tree.
- The typed-template versus wider historical wire-grammar mismatch is already
  captured by `[[ui bind list typed templates are narrower than the runtime
  wire grammar]]`; do not create a duplicate note unless this implementation
  changes that conclusion.
- The stale-branch observation is already covered operationally by Project
  Pipelines plan-review fetch guidance. No duplicate vault capture is needed
  unless Plan-stage target worktrees repeatedly start behind their declared
  base after this run.
