# TUI canonical BindList row identity plan

## Outcome

Cold-repin `botster-tui` to the merged Hub row-identity contract and the merged
TUI-kit consumer, then resolve producer-authored `BindList` row ids while the
TUI still has row context. Each expanded `/session` row must reach the existing
kit renderer as a distinct literal `UiNodeId`, so the production Ratatui frame,
`HitMap`, and `InputRouter` dispatch keyboard and mouse activation to the row
that the operator actually selected.

This is a client adoption change, not a new identity scheme. The Hub fixture is
the producer oracle, the TUI performs row-aware materialization, and TUI kit
continues to own literal-id focus and dispatch mechanics.

## Pipeline identity and routing

- Ticket: `ticket_1785438029_926883`
- Run: `run_1785535781_270318`
- Target repository: `trybotster/botster-tui`
- Target id: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Current authoritative main: `c426b8ff9e2dd603c024259897241460992fbb6b`
- Repository charter: `[[botster-tui-playbook]]`
- Role playbooks: `[[planner-playbook]]`, then
  `[[botster-planner-playbook]]`
- Workflow overlay: `[[project-pipelines-playbook]]` for durable checklist,
  artifact, gate, and advancement discipline. No Project Pipelines source is
  in implementation scope.

The target was resolved through the admitted spawn-target registry and the
worktree remote was checked against `trybotster/botster-tui`; it was not
inferred from the ambient directory.

## Context loaded

Repository and architecture guidance:

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
- `[[botster-runtime-reviewer-playbook]]`
- `[[botster-runtime-verifier-playbook]]`

Targeted contract and renderer guidance:

- `[[ui contract row ids can bind before template expansion]]`
- `[[renderer state accepts only realized literal identity]]`
- `[[ui bind list where filters plugin entity rows before template expansion]]`
- `[[ui bind list typed templates are narrower than the runtime wire grammar]]`
- `[[action precedence scopes to one hit region not one node]]`

Workflow guidance loaded by the Botster planner overlay:

- `[[project pipeline orchestration belongs in a device-level botster plugin]]`
- `[[project pipelines needs an operator workbench not more primitives]]`
- `[[project pipelines ui contract belongs in the plugin readme]]`
- `[[botster orchestration should spawn agents with explicit target ids]]`
- `[[botster orchestration prompts must bind agents to explicit worktrees]]`

Repository evidence inspected:

- `README.md`, root and crate `Cargo.toml`, `Cargo.lock`;
- `crates/botster-tui/src/app.rs`, especially `SessionEntityState`,
  `binding_rows`, `materialize_plugin_surface`,
  `materialize_binding_node`, `materialize_binding_children`,
  `plugin_surface_render_root`, and the isolated-Hub contract-matrix proof;
- `script/fmt`, `script/test`, `script/clippy`, and `script/test-live-hub`;
- Hub PR #181 merge `fab44c5de7b28a8756268608662d2b870efb001a`;
- TUI-kit PR #26 merge `3bf8ae81d3e716b196fae8e4a7560dd5fc5c2e69`;
- the merged Hub `UiAuthoredNodeId` contract and revision-25
  `session_plugin_binding_conformance_scenario` / row reference materializer;
- dependency-run artifacts and verification, including the disposable
  downstream proof against this exact TUI base.

The package registry currently publishes `@trybotster/ui-contract@0.2.0` and
`@trybotster/hub-test-support@0.1.18`; the latter is the public fixture required
by the parent campaign.

## Scope

### In scope

1. Cold-repin `botster-hub-client`, `botster-ui-contract`, and
   `botster-hub-test-support` to Hub merge `fab44c5…`, and
   `botster-tui-kit` to kit merge `3bf8ae8…`. Update `Cargo.lock` so one
   `botster-ui-contract` source is shared by the app, client, test support, and
   kit. Do not retain the older Hub or kit paths.
2. Adapt TUI-owned `UiNode.id` construction and reads to
   `UiAuthoredNodeId`. App-authored nodes use literal ids; request/result
   `node_id` remains the realized `UiNodeId` contract.
3. Resolve an item-template root `UiAuthoredNodeId::Bind` from the current
   row before props, children, slots, validation, or rendering cross into TUI
   kit. Require an item-relative path, a string result, and a nonblank literal.
   Do not stringify, prefix, suffix, index, hash, or otherwise rewrite it.
4. Replace the pre-contract `node_tree_has_id` blanket rejection and its
   diagnostic. Materialize the complete surface, then reject realized literal
   ids that can coexist in one render. This permits canonical distinct row ids
   and ID reuse across mutually exclusive responsive alternatives while
   retaining a diagnostic for repeated literal roots, descendants, and a bound
   row id that collides with a static sibling outside its `BindList`.
5. Use the Hub revision-25 `row_expected` stages and
   `materialize_session_plugin_rows` results as exact oracles. Prove the TUI's
   concrete rows carry the same ids and action payloads in producer order.
6. Replace `SESSION_BINDING_FIELDS` and the hand-maintained optional-field
   insertion list with one serialized `DaemonSessionEntity` reference row.
   Construct it as an exhaustive struct literal: no `Default`, constructor, or
   rest initializer, and every current `Option` field set to `Some`. Its keys
   define valid `/session where` fields; absent serialized keys on a real row
   are projected as JSON null. A future DTO field addition must therefore fail
   this repository's compilation until the literal is updated, rather than
   silently becoming unsupported.
7. Raise the production compatibility minimum from 24 to 25 while preserving
   its `>=` semantics, then change only the pinned-fixture tests to assert
   exact revision 25. Update README language to the newly consumed contract.
8. Add app-level production-path tests for distinct per-row hit regions, Tab
   traversal, keyboard activation, mouse press/release activation, and focus
   reconciliation after removal. Preserve all existing single-row and
   negative binding coverage.
9. Extend the isolated live-Hub contract-matrix assertion so the published
   multi-row oracle is checked stage by stage through `TuiApp`'s real session
   reducer, production Ratatui frame, retained hit map, input router, and
   `PluginSurfaceAction` request seam.
10. Run the parent campaign's exact public-fixture workload with
    `@trybotster/hub-test-support@0.1.18`, not a repository-private or stale
    fixture copy.

### Non-scope

- No edits to `botster-hub`, `botster-hub-client`, `botster-ui-contract`,
  `botster-hub-test-support`, or `botster-tui-kit` repositories.
- No client-local identity envelope, synthetic id, local contract struct, DTO
  field mirror, row-key registry, or compatibility fallback.
- No changes to kit focus, hit-map, dispatch, modal, toolbar, table, or list
  mechanics. If distinct realized ids still cannot preserve row-N keyboard
  dispatch, stop and register a new dependency against the TUI-kit target.
- No Project Pipelines or Workspaces plugin policy, browser work, session
  lifecycle policy, terminal transport behavior, broad `app.rs` refactor, or
  adjacent cleanup.
- No widening of the typed `BindList` grammar or support for bound descendant
  ids; contract 0.2.0 deliberately permits a bound id only on the direct item
  template root.

## Ownership boundaries and cross-repository dependencies

`botster-tui` owns the row-aware application adapter, subscribed session
projection, local diagnostics, compatibility requirement, Hub action request,
and app-level production-path proof.

`botster-hub` owns `UiAuthoredNodeId`, valid binding context, sanitized
`DaemonSessionEntity`, conformance revision 25, producer ordering, fixture
surface, expected identity stages, and reference materializers. Closed
dependency `ticket_1785436979_640117` supplies merge `fab44c5…`.

`botster-tui-kit` owns literal-id rendering, hit regions, focus reconciliation,
Tab/key routing, mouse routing, and generic action dispatch. Closed dependency
`ticket_1785443243_233047` supplies merge `3bf8ae8…`. Its verification already
proved this TUI base compiles and 115/116 tests pass with coordinated pins and
authored-id adaptations; the sole expected failure is this ticket's revision
24 assertion.

No additional cross-repository dependency is currently required. A failure of
row-N dispatch after distinct literal ids reach the kit is a blocking new
TUI-kit dependency, not authorization for a workaround here.

## Implementation shape

Keep feature logic beside the existing binding projection in
`crates/botster-tui/src/app.rs`.

1. Add a single reference-row helper that serializes an exhaustive, fully
   populated `DaemonSessionEntity` struct literal to an object. Set
   `lifecycle`, `exit_code`, and `failure_reason` to `Some`; prohibit
   `..Default::default()`, a constructor helper, or a rest initializer. Reuse
   the serialized key set directly for both `where` validation and null
   completion in `binding_rows`; do not introduce a second field-name list as
   code or test oracle.
2. Teach `materialize_binding_node` to materialize `node.id` as well as props
   while row context is available. Literal ids pass through unchanged; bound
   ids resolve to `UiAuthoredNodeId::Literal(UiNodeId(...))`.
3. Preserve producer snapshot/upsert order in the existing session entity
   projection and materialize matching rows in that order. After the complete
   surface is concrete, walk the whole materialized tree once and reject
   duplicate realized literals within each possible render before validation
   or rendering. Keep
   empty-template, unsupported-source, unknown-field, missing-value, and
   absolute-path errors visible.
4. Adapt all compiler-exposed TUI-owned authored-id construction/read sites.
   Keep conversions explicit at the boundary between authored `UiNode.id` and
   realized `UiActionRequest.node_id` so tests cannot compare unlike identity
   phases accidentally.
5. Replace the old unsafe-multi-row regression with collision-negative and
   canonical-distinct-positive tests; extend the published scenario test
   instead of creating a second TUI-authored oracle.
6. Update README contract pins, revision floor, live harness wording, and
   removal of the temporary multi-row limitation in the same cold change.

## Affected surfaces and files

Expected changes:

- `crates/botster-tui/Cargo.toml` — four exact dependency revisions.
- `Cargo.lock` — unified Hub contract/client/test-support and TUI-kit graph.
- `crates/botster-tui/src/app.rs` — compatibility revision, authored-id
  adaptations, reference row, binding materialization, ambiguity diagnostics,
  unit/real-frame/router/live-Hub tests.
- `README.md` — exact pins/revision, shipped multi-row identity behavior, live
  fixture wording, and removal of the temporary rejection limitation.
- `docs/plans/tui-canonical-bind-list-row-identity-plan.md` — this durable plan.

`crates/botster-tui/src/renderer.rs` is expected to remain a thin re-export and
should not change. `script/test-live-hub` should change only if the existing
required fixture input cannot express the public-package provenance; prefer
supplying the extracted public fixture through its existing environment
variable.

## Assumptions and unknowns

Resolved assumptions:

- The durable producer pin is Hub merge `fab44c5…`; the durable renderer pin is
  kit merge `3bf8ae8…`.
- Contract 0.2.0 binds only the direct `BindList.item_template` root id.
- The authoritative revision-25 row order is producer order from the session
  entity projection; the TUI must not sort or renumber it.
- `UiActionRequest.node_id` remains a realized literal `UiNodeId`; only
  `UiNode.id` is authored identity.
- Existing kit identity mechanics are correct once the TUI supplies distinct
  literal ids, as proved by the closed kit dependency.

Implementation must validate early:

- The duplicate pass is deliberately surface-wide, not scoped to one expanded
  list, but render-scoped within that surface: mutually exclusive responsive
  alternatives may reuse an id. Keep concise first-collision wording inside
  the existing binding diagnostic category.
- The public tarball path is resolved:
  `package/fixtures/plugin-contract-matrix`, containing the two files required
  by `script/test-live-hub` plus its README.
- Whether the published multi-row action result mutates presentation. Dispatch
  correctness is mandatory even if the fixture's accepted result is visually
  neutral; assert the outbound typed request at the Hub seam in either case.

These unknowns do not permit weakening the ticket, changing another
repository, or inventing identity. If a different plausible contract meaning
appears, ask the human through Project Pipelines.

## Risks

- **Mixed contract sources:** repinning only the kit or only direct Hub
  dependencies creates incompatible `UiNode` crate instances. Update all four
  pins cold-turkey and assert the dependency graph.
- **Identity phase confusion:** comparing authored `UiNode.id` directly with
  realized request ids can hide unresolved bindings or fail compilation.
  Convert only after materialization and assert literals at the boundary.
- **False success from aggregate proof:** two regions or two requests do not
  prove row-N correctness. Focus row 2 explicitly and compare its exact id and
  payload after both keyboard and mouse gestures.
- **Stale focus after removal:** rebuilding the frame without router
  reconciliation can retain an absent id. Remove the focused row, redraw,
  reconcile, and prove focus moves to a surviving canonical region.
- **Ambiguous identities within one render:** deleting the old blanket guard
  with only per-list detection would miss a bound row id colliding with a
  static sibling. Reject duplicate realized literals that can coexist across
  the complete materialized surface, without rejecting mutually exclusive
  responsive alternatives.
- **Silent DTO drift:** a defaulted or partially populated reference row would
  omit a future optional field under `skip_serializing_if`. Use an exhaustive
  struct literal with every option `Some`, and drive both filtering and null
  projection from that one serialized key set.
- **Fixture drift versus forward compatibility:** the production handshake
  must continue accepting Hub revisions `>= 25`, while pinned fixture tests
  must fail loudly unless the scenario is exactly 25. Do not turn the runtime
  minimum into equality.
- **Test-only materialization:** inspecting a semantic tree is insufficient.
  Require production frame cells, hit regions, real key/mouse events, router
  focus, and outbound Hub action requests.
- **False baseline failure in this worktree:** the generated path contains
  `:`, which makes Cargo's default macOS `DYLD_FALLBACK_LIBRARY_PATH` invalid.
  Use an explicit colon-free `CARGO_TARGET_DIR` for every gate and live run.
- **Public/private fixture mismatch:** the parent blocker used public
  `@trybotster/hub-test-support@0.1.18`. Extract and use that exact tarball;
  do not substitute a sibling Hub checkout fixture.

## Acceptance checks and tests

### Dependency and contract checks

- `cargo tree --locked -i botster-ui-contract` reports exactly one contract
  0.2.0 source at `fab44c5…`, shared by TUI, kit, Hub client, and Hub test
  support.
- `cargo tree --locked -d` is reviewed for accidental old/new Hub contract
  duplication; pre-existing dual `botster-core` sources remain outside this
  ticket unless the repin changes them.
- Raise `MINIMUM_CONFORMANCE_FIXTURE_REVISION` to 25 and keep the production
  compatibility requirement at app.rs:3506 as a minimum. Add a compatibility
  test proving a simulated Hub advertising revision 26 is still accepted.
- Apply exact equality only to the pinned test assertions corresponding to
  current app.rs:5192 and app.rs:5349 and to the published
  `session_plugin_binding_conformance_scenario`; those must equal 25 so a
  dependency-pin bump changes the oracle loudly.
- No old Hub SHA `b403bb72…`, old kit SHA `c66f1ae6…`,
  `SESSION_BINDING_FIELDS`, `node_tree_has_id`, or temporary multi-row
  diagnostic remains in current code or README.

### Materializer and ambiguity proof

- Literal authored ids remain unchanged.
- A direct item-root `{"$bind":"@/session_uuid"}` resolves to the exact row
  string; null, non-string, blank, missing, absolute, and descendant binding
  cases fail visibly.
- Two distinct rows materialize two distinct literal root ids and row-specific
  payloads in producer order.
- A multi-row literal root, repeated descendant, or bound row id equal to a
  static id elsewhere in the same render realizes a duplicate and returns the
  visible ambiguous-tree diagnostic; no colliding hit region is published.
- Mutually exclusive responsive alternatives may reuse the same literal id;
  each rendered width publishes only one corresponding hit region.
- Unknown `where` fields still fail the whole surface visibly rather than
  selecting `empty_template`.
- The exhaustive serialized reference literal exposes every current
  `DaemonSessionEntity` field, and actual absent optional fields project as
  JSON null. Assert that this exact derived key set is consumed by both
  `where` validation and null completion. No separate field-name assertion
  list is allowed; compile failure on a future DTO field is the drift guard.

### Published five-stage oracle

For initial snapshot, ended patch, indeterminate patch, remove, and reconnect:

- compare TUI lifecycle/unavailable projection exactly with
  `scenario.expected`;
- compare realized row ids exactly with `scenario.row_expected` and the Hub
  `materialize_session_plugin_rows` reference output;
- compare each row action payload exactly with its realized id;
- render through the production Ratatui frame and assert expected cells and
  exact hit-region ids;
- assert unresolved-binding fallback text never appears;
- reject stale prior-generation frames before accepting the reconnect
  snapshot.

Keep the existing single-row action, empty-template, absent-reference,
indeterminate, reconnect, optional-null, absolute-path, unknown-field, and
TUI-kit-fallback-negative coverage green.

### Per-row production input proof

With the initial two-row `current` oracle:

1. Render the production app surface and assert one action region for
   `session-transition` and one for `session-stable-current`, with nonoverlapping
   rectangles and row-specific payloads.
2. Start from a known focus, send real Crossterm `Tab` key events through
   `InputRouter::dispatch_event`, and assert traversal reaches both row ids in
   frame order.
3. Focus row 2, send real Enter (and Space if both remain supported production
   activators), and assert the resulting `UiActionRequest.node_id` and
   `payload.session_uuid` both equal `session-stable-current`; then pass it to
   `TuiApp::handle_dispatch` and observe the exact `PluginSurfaceAction`.
4. Send left Down and matching left Up inside row 2's production hit rectangle
   and assert the same row-2 request. Include a press/release mismatch negative
   so redraw/reorder cannot activate the wrong row.
5. Remove the focused row through the authoritative frame, redraw, call
   `InputRouter::reconcile`, and assert the removed id is absent from regions
   and focus while the surviving row remains reachable and dispatches its own
   payload.

The keyboard assertion is the load-bearing regression: it must go red if
bound-id materialization is removed or both rows are forced to one id.

### Live Hub and public-fixture proof

Extend `headless_live_runtime_runs_against_isolated_hub_when_binaries_are_available`
and `assert_plugin_contract_matrix_renders_through_tui` so the real isolated
Hub delivers `contract.sessions`, the app's production session subscription
supplies rows, and the same frame/router assertions run against the published
multi-row oracle stage by stage. No test may inject a second session store or
call the materializer without the production app path as its only live proof.

Extract the exact public package in a temporary directory and use the resolved
`package/fixtures/plugin-contract-matrix` directory. Verify version `0.1.18`,
revision 25, and the ticket-recorded protocol SHA-256
`956b2ce7c07523af848da885006c21f944b580799551b208f59f96450a245c0b` for
campaign traceability. More importantly, verify the exact fixture bytes that
the live test consumes against `metadata.json`:

- `README.md`:
  `ebfd010f4be08c3433335bb380597fcb9e47451a43c4424c4650583f201725f5`
- `botster-package.json`:
  `74b9c4c2cd472eb0b1a678f7179322cf8fc00b9f9623da48a177c73a540c5d83`
- `plugin.lua`:
  `04da20836dbc83da40a3198a0acfd7fb85361f04d75501d26b176ccbd2fdf2ca`

These fixture files are byte-identical to the pinned Hub `fab44c5` crate
assets. The public tarball's TypeScript protocol metadata differs from merged
Hub metadata, but that unrelated artifact is not consumed by this Rust live
path; package version or protocol SHA alone is therefore insufficient fixture
provenance. After the three consumed-file digest checks, run:

```sh
env CARGO_TARGET_DIR=/private/tmp/botster-tui-live-target \
  BOTSTER_HUB_BIN=/path/to/current/botster-hub \
  BOTSTER_SESSION_WORKER_BIN=/path/to/current/botster-session-worker \
  BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE=/path/to/extracted/package/fixtures/plugin-contract-matrix \
  script/test-live-hub
```

The command must reach and pass the parent four-package workload: session
lifecycle, package storage, terminal echo, package install/open, plugin
contract-matrix rendering, multi-row keyboard/mouse routing, and clean Hub
shutdown. The previous `UiChild` decode failure must not be bypassed or
weakened.

### Repository gates

Run from a clean committed worktree with colon-free target directories:

```sh
env CARGO_TARGET_DIR=/private/tmp/botster-tui-fmt-target script/fmt
env CARGO_TARGET_DIR=/private/tmp/botster-tui-test-target script/test
env CARGO_TARGET_DIR=/private/tmp/botster-tui-clippy-target script/clippy
cargo tree --locked -i botster-ui-contract
cargo tree --locked -d
git diff --check main...HEAD
```

Baseline at planning time: `script/fmt` passed, `script/test` passed 116 app
tests plus one package-manifest test, and strict `script/clippy` passed when run
with explicit `/private/tmp` target directories.

For the new row-2 regression, record a negative control by reverting or
disabling bound-id realization, proving the targeted test fails with wrong-row
identity/dispatch, then restore and rerun it green. Pre-existing failures are
not blanket waivers; any unrelated claim needs exact clean-main comparison.

## Vault gaps worth capturing

No planning-stage vault gap is currently open. The durable ownership and
identity rules are already captured by `[[ui contract row ids can bind before
template expansion]]`, `[[renderer state accepts only realized literal
identity]]`, and `[[botster tui consumes tui kit through a thin app policy
adapter]]`.

Capture a new note only if implementation discovers a reusable rule not
covered there, such as a general post-expansion identity-collision invariant
across clients. Repository-specific fixture paths, SHAs, and test commands stay
in this plan/README and pipeline evidence rather than the vault.
