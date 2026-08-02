# TUI canonical BindList descendant identity plan

## Outcome

Cold-repin `botster-tui` to the exact merged Hub and TUI-kit revisions, then
extend the existing row-aware surface materializer to consume the Hub-owned
`UiAuthoredNodeId::BindListDescendant` form. Resolve the direct
`BindList.item_template` root `$bind` first; while that row identity remains in
scope, realize each keyed descendant with the canonical
`botster_ui_contract::realize_bind_list_descendant_id` Rust helper. Only the
resulting literal `UiNodeId`s may reach validation, the production Ratatui
frame, focus, `HitMap`, `InputRouter`, or `UiActionRequest.node_id`.

This is a cold consumer adoption, not a new identity scheme. The Hub owns the
grammar, helper, fixture, and expected row/control oracle. TUI-kit owns
literal-id renderer/input mechanics. This repository owns row-context
materialization, app dispatch, diagnostics, reconnect behavior, and proof that
the real user path targets the intended row and control.

## Pipeline identity and routing

- Ticket: `ticket_1785602865_181673`
- Run: `run_1785614950_505375`
- Target repository: `trybotster/botster-tui`
- Target id: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Planning base: `52174be7e131983c655a506e7d8d57f67e5b049d`
- Integrated implementation base: `4faa221da665e001a8802c4ecad50ea1f1077812`
- Repository charter: `[[botster-tui-playbook]]`
- Role playbooks: `[[planner-playbook]]`, then `[[botster-planner-playbook]]`
- Workflow overlay: `[[project-pipelines-playbook]]` for durable checklists,
  artifact, gate, and advancement evidence; Project Pipelines source is not in
  implementation scope.

The target was resolved through the admitted spawn-target registry and checked
against the worktree's `trybotster/botster-tui` remote. It was not inferred from
the ambient process directory.

## Context loaded

Repository and runtime guidance:

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
- `[[acceptance readiness requires the exact expected entity not any authoritative snapshot]]`
- `[[botster-runtime-reviewer-playbook]]`
- `[[botster-runtime-verifier-playbook]]`

Targeted identity, fixture, and test guidance:

- `[[ui contract row ids can bind before template expansion]]`
- `[[renderer state accepts only realized literal identity]]`
- `[[post expansion identity uniqueness is scoped to one render not one tree]]`
- `[[ui bind list typed templates are narrower than the runtime wire grammar]]`
- `[[acceptance harness region oracles must key on node identity not concatenated text]]`
- `[[action precedence scopes to one hit region not one node]]`
- `[[a regression test must be shown to go red with the fix reverted]]`

Workflow guidance:

- `[[project pipeline orchestration belongs in a device-level botster plugin]]`
- `[[project pipelines needs an operator workbench not more primitives]]`
- `[[project pipelines ui contract belongs in the plugin readme]]`
- `[[botster orchestration should spawn agents with explicit target ids]]`
- `[[botster orchestration prompts must bind agents to explicit worktrees]]`
- `[[plan agents must author vault context as wikilinks not home paths]]`
- `[[pipeline vault checklists must cite exact resolvable note titles]]`
- `[[vault example paths are not repository placement conventions]]`
- `[[project pipelines mcp create calls can time out after committing]]`

Repository evidence inspected:

- `README.md`, root/crate `Cargo.toml`, `Cargo.lock`, and repository-owned
  `script/fmt`, `script/test`, `script/clippy`, and `script/test-live-hub`;
- `crates/botster-tui/src/app.rs`, especially
  `plugin_surface_render_root`, `materialize_plugin_surface`,
  `node_requires_binding_materialization`, `materialize_binding_node`,
  `materialize_binding_children`, the existing full-render duplicate-id
  collector, app dispatch tests, and isolated-Hub contract-matrix proof;
- prior mainline plan
  `docs/plans/tui-canonical-bind-list-row-identity-plan.md` as repository-owned
  placement and seam history;
- Hub PR #188 merge `1955c9e0713281093f609d09f6597a1dcfaf07d3`
  introduced contract `0.3.0`, conformance revision 26, the canonical helper,
  and the multi-row × three-control fixture/oracle. The separately routed Hub
  follow-up `ticket_1785617154_342333` superseded that consumer pin with merge
  `e8febabf73259cfd922592346b244ec473c17323`, contract `0.3.1`, and
  conformance revision 27 so required bindable props validate before
  materialization.
- TUI-kit PR #27 merge `c2123d95a9c8d13bd43b90d8ef6c8fde824b139c`
  established literal-only renderer behavior. The separately routed TUI-kit
  follow-up `ticket_1785625932_361733` superseded that consumer pin with merge
  `76e2085632f2da2f4423100cec85f23527373524`, aligning the kit to Hub
  `e8febab...` and one compatible contract crate.

## Scope

### In scope

1. Cold-repin `botster-hub-client`, `botster-ui-contract`, and
   `botster-hub-test-support` to Hub merge `e8febab...`, and
   `botster-tui-kit` to kit merge `76e2085...`. Update only those four packages
   in `Cargo.lock` so the app, Hub client/test support, and kit resolve one
   `botster-ui-contract` 0.3.1 source. Do not blanket-refresh the lockfile or
   silently advance the test-support-only `botster-core?branch=main` source;
   remove the old Hub/kit revisions in the same change.
2. Extend TUI-owned binding detection and the exhaustive
   `UiAuthoredNodeId` match for `BindListDescendant`. Unresolved keyed identity
   must force materialization and must never be treated as a literal or absent
   id.
3. Thread a private row-identity context through the existing materializer.
   Resolve the direct item-template root `Bind` to its exact row literal first;
   resolve descendant keys only below that bound root; reset the context at
   each nested `BindList`; and use only
   `realize_bind_list_descendant_id(row_id, key)` for final identity.
4. Validate the authored tree at `plugin_surface_body_node` before any identity
   materialization so contract 0.3.1 owns blank, misplaced, and template-global
   duplicate descendant-key diagnostics. Then preserve the current
   props/children/slots traversal, producer row order, post-materialization
   `UiNode::validate`, capability validation, and full render-scoped duplicate-id
   collector. The collector remains the final realized-collision gate before
   renderer state; authored-key validation is contract infrastructure, not a
   second TUI collision subsystem.
5. Raise the production compatibility minimum and pinned fixture assertions
   from conformance revision 25 to 27 while retaining runtime `>=` semantics.
6. Adapt the canonical fixture test from one row action to multiple rows ×
   three controls. Compare exact row ids, descendant ids, keys, action payloads,
   and producer order against Hub test support rather than reimplementing the
   encoder or hard-coding the prefix as a production oracle.
7. Prove real Tab traversal plus exact Enter/Space and left mouse Down/Up
   dispatch through the production frame, `HitMap`, and `InputRouter` to the
   intended row/control. Pass the dispatch through `TuiApp::handle_dispatch`
   and assert the emitted `PluginSurfaceAction` and `UiActionRequest.node_id`.
8. Retain and extend collision diagnostics for duplicate authored descendant
   keys rejected by contract validation and duplicate final realized ids across
   rows, descendants, and static siblings. Include delimiter-like and Unicode
   row/key values to prove the canonical UTF-8 byte-length encoding is
   injective without local parsing or synthesis.
9. Reconcile focus after authoritative row removal, then prove the surviving
   row's controls remain reachable and dispatch their own identity/payload.
   Repeat the exact identity assertions after a fresh reconnect snapshot while
   rejecting stale-generation frames.
10. Extend the real isolated-Hub contract-matrix path so the Hub-delivered
    canonical producer fixture, production session entity store, renderer,
    hit map, router, outbound action request, and returned result node ids are
    all exercised together.
11. Update the README's exact pins, conformance floor, and shipped BindList
    behavior. State that row/control identity is producer-authored and
    canonically materialized before TUI-kit.
12. Record the concurrent same-target `ticket_1785612604_598776` sequencing
    hazard in the implementation report. Its `open_spawn` action work is not
    scope overlap, but both branches edit `app.rs`; whichever lands second must
    rebase across the contract 0.3.x exhaustive-match change explicitly. The
    sibling landed first as `4faa221...`; this run integrated it without
    changing the producer-authored `botster_workspaces.open_spawn` behavior.

### Non-scope

- No edits to Hub, Hub client, UI contract, Hub test support, TUI-kit,
  botster-core, Web, Workspaces, or Project Pipelines repositories.
- No local encoder, prefix builder/parser, delimiter scheme, hash, index,
  synthesized fallback id, descendant full-ID `$bind`, compatibility grammar,
  alias, or dual old/new materialization path.
- No widening or reinterpretation of TUI-kit product policy and no changes to
  generic kit focus, hit maps, rendering, routing, or terminal forwarding. A
  newly discovered kit mechanic defect becomes a separately targeted TUI-kit
  dependency instead of a local workaround.
- No bypass of the production frame/hit map/router for acceptance proof, no
  second entity store, no fixture-only semantic tree assertion standing in for
  runtime proof, and no text-derived region oracle.
- No session lifecycle, terminal transport, package policy, browser parity
  implementation, broad `app.rs` refactor, optional configuration, speculative
  abstraction, or adjacent cleanup.

## Ownership boundaries and cross-repository dependencies

`botster-tui` owns the entity-aware row context, materialization timing,
client-side diagnostics, compatibility floor, focus reconciliation, semantic
Hub requests, reconnect behavior, and app-level keyboard/mouse/live-Hub proof.

`botster-hub` and its sibling `botster-ui-contract` package own the authored
`BindListDescendant` grammar, validation context, key uniqueness, canonical
UTF-8 byte-length helper, sanitized session DTOs, fixture revision 27,
producer fixture, expected row/control identities, and action result oracle.
Closed dependency `ticket_1785443253_376782`, correctly targeted to
`tgt_7e208a0c76a44980a83b63af976b1f22`, introduced merge `1955c9e...`;
closed follow-up dependency `ticket_1785617154_342333` supplies the consumed
Hub merge `e8febab...` and contract `0.3.1`.

`botster-tui-kit` owns literal-only renderer state, Ratatui layout, focus,
`HitMap`, `InputRouter`, Tab/key/mouse routing, and generic action dispatch.
Closed dependency `ticket_1785602855_922302`, correctly targeted to
`tgt_3dfae49c02454037bf13554f552baf7f`, introduced merge `c2123d9...`; closed
follow-up dependency `ticket_1785625932_361733` supplies the consumed kit merge
`76e2085...` and its cold current-TUI single-contract compile proof.

No cross-repository prerequisite remains open. Browser parity is owned by
its separately routed consumer ticket and is not silently folded into this run.
If the exact realized literals reach the kit but generic routing still targets
the wrong region, stop and register a new dependency against the TUI-kit target.

## Implementation shape

Keep the change beside the existing binding adapter in
`crates/botster-tui/src/app.rs`:

1. Update `node_requires_binding_materialization` to recognize both unresolved
   authored identity variants.
2. Remove the binding-tree validation skip in `plugin_surface_body_node` and
   require `surface.body.validate()` before materialization. Add focused visible
   diagnostics for blank descendant keys, keyed identity outside a bound item
   root, duplicate keys in ordinary siblings, and duplicate keys across
   mutually exclusive authored branches. If the contract unexpectedly rejects
   an otherwise valid bound tree for unrelated prop-schema reasons, stop and ask
   a human; do not add local key validation.
3. Replace the boolean `bound_id_allowed` handoff with the narrow context needed
   to distinguish an item-template root from descendants and carry the resolved
   root `UiNodeId`. Root `Bind` resolution establishes the context; child and
   slot recursion inherits it; nested BindList expansion starts a new row
   context rather than inheriting the outer row.
4. Match `UiAuthoredNodeId::BindListDescendant` only in a valid descendant
   context and call the imported canonical helper. Store its returned
   `UiNodeId` as `UiAuthoredNodeId::Literal`. Treat missing row context or helper
   errors as visible binding diagnostics before validation/rendering.
5. Leave literal and absent identity behavior unchanged. After the complete
   tree is concrete, run the existing render-scoped duplicate collector, then
   existing contract/capability validation, then the unchanged kit renderer.
6. Upgrade existing published-oracle and live-Hub tests instead of creating a
   TUI-owned duplicate fixture. Use helper-produced expected ids from Hub test
   support structures as equality oracles, with independent Unicode and
   delimiter-like test inputs calling the canonical helper—not local spelling.

## Affected surfaces and files

Expected changes:

- `crates/botster-tui/Cargo.toml` — exact Hub and TUI-kit revisions.
- `Cargo.lock` — unified 0.3.1 contract/client/test-support/kit graph.
- `crates/botster-tui/src/app.rs` — conformance floor, exhaustive enum
  adaptation, row-context descendant materialization, retained duplicate
  diagnostics, fixture/runtime/input/reconnect tests.
- `README.md` — exact pins, revision 27, canonical descendant behavior, and
  live-fixture proof language.
- `docs/plans/tui-canonical-bind-list-descendant-identity-plan.md` — this
  durable plan artifact.

`crates/botster-tui/src/renderer.rs`, `script/test-live-hub`, and acceptance
scenario/schema files are regression surfaces but are not expected edit targets.
Change one only if the existing public production path cannot express a ticket
requirement, and record the necessity as a plan deviation.

## Assumptions and unknowns

Resolved assumptions:

- The final exact cold pins are Hub merge `e8febab...` and TUI-kit merge
  `76e2085...`; the contract version is 0.3.1 and conformance revision is 27.
- `UiActionRequest.node_id` remains a realized `UiNodeId`; only authored
  `UiNode.id` carries `Bind` or `BindListDescendant` before materialization.
- The canonical helper is the only identity spelling authority. Tests may
  compare its outputs with fixture literals but production code may not parse
  or construct the `botster-ui-descendant-v1` spelling itself.
- “Published real producer fixture” means the Hub-owned contract-matrix fixture
  and scenario exposed by the exact git-pinned `botster-hub-test-support` and
  delivered through a real isolated Hub. This Rust consumer does not require an
  npm publication path.
- The existing render-scoped duplicate collector is retained infrastructure;
  collision detection is not a new subsystem.
- The current test-support-only floating `botster-core?branch=main` lock entry is
  `e36435f2cb583c344d6f6ba2d62c39da324c7a64`; a surgical repin must preserve
  that SHA unless the new Hub test-support dependency forces a move that the
  implementation report names and justifies.

Implementation must validate early:

- The exact authored-context shape needed to reset identity under nested
  BindLists. Do not allow an outer row id to realize a nested-list descendant.
- Contract 0.3.1 authored-tree validation is the required mechanism for blank,
  misplaced, and template-global duplicate descendant-key diagnostics. It must
  run before conversion; post-materialization validation cannot recover those
  authored semantics. If it rejects valid bound props for an unrelated reason,
  ask a human instead of reimplementing contract validation locally.
- Which two canonical controls best prove returned Hub results. Hub test support
  already uses same-row `rename` and `remove`; retain those exact independent
  result assertions unless the merged fixture API requires all three.

These do not authorize a weaker test, local grammar, or cross-repository edit.
If the merged APIs reveal another plausible contract meaning, ask the human
through Project Pipelines rather than choosing silently.

## Risks

- **Mixed dependency identities:** a partial repin yields distinct contract
  crate instances and incompatible `UiNode` types. Repin all four consumers
  cold-turkey and inspect both inverse and duplicate dependency trees.
- **Floating dev dependency drift:** refreshing `Cargo.lock` can silently advance
  `botster-core?branch=main` through `botster-hub-test-support`. Use targeted
  Cargo updates and compare its resolved SHA before and after.
- **Wrong expansion order:** realizing a descendant before the item root can
  use absent, authored, or outer-row identity. Establish the exact current row
  literal first and pass it only down that realized item subtree.
- **Local encoder drift:** manually formatting byte lengths can pass ASCII and
  fail Unicode or delimiter-like data. Import the canonical helper and prove
  Unicode through it.
- **Unresolved identity leakage:** treating the new variant as absent or a
  string fallback can publish shared focus/hit/action state. Require no keyed
  variant remains before the tree reaches the kit.
- **Aggregate dispatch false positives:** six regions or successful actions do
  not prove row/control targeting. Focus and click exact controls, then compare
  both request `node_id` and row-specific payload at the Hub seam.
- **Mouse bypass:** calling an inner activation helper skips production capture
  semantics. Use real Down/Up coordinates from the rendered `HitMap`, including
  mismatched release and non-target negatives.
- **Collision diagnostic regression:** converting keys directly to literals can
  obscure authored duplicate-key errors; final collision checks can also miss
  static siblings if narrowed per row. Preserve both producer validation and
  the full-render collector.
- **Stale focus/reconnect state:** removed ids or prior-generation rows may
  survive in router state. Redraw, reconcile, reject stale frames, and prove the
  fresh snapshot reproduces exact canonical ids without auto-dispatch.
- **Fixture-only confidence:** source objects and helper unit tests are not user
  path proof. Require real Hub delivery, production entity convergence, frame,
  hit map, router, outbound request, and result echo.
- **Baseline path issue:** the worktree path contains `:`, so use explicit
  colon-free `CARGO_TARGET_DIR` locations for Cargo gates and live proof.

## Acceptance checks and tests

### Dependency and compatibility proof

- `cargo tree --locked -i botster-ui-contract` reports one 0.3.1 source at
  `e8febab...`, shared by the app, kit, Hub client, and Hub test support.
- `cargo tree --locked -d` contains no old/new contract or TUI-kit split caused
  by this change. Review pre-existing unrelated duplicates separately.
- Compare the `botster-core?branch=main` lock source before and after. It remains
  at `e36435f2...`; if the repinned Hub test support forces a different commit,
  the implementation report must list and justify that extra dependency move.
- No old Hub SHA `fab44c5...`, old kit SHA `3bf8ae8...`, local descendant
  encoder/prefix builder, compatibility alias, or fallback grammar remains.
- Raise `MINIMUM_CONFORMANCE_FIXTURE_REVISION` to 27 and keep the production
  requirement as `>= 27`; pinned scenario/report assertions use exact equality
  where fixture drift must be loud. Add/retain a forward-compatibility test that
  revision 28 is accepted.

### Materialization and collision proof

- Literal and absent ids retain existing behavior; root `Bind` resolves exactly
  as before; `BindListDescendant` without a valid bound root fails visibly.
- For two rows and `spawn`/`rename`/`remove`, the materialized tree contains the
  exact helper-produced row/control ids and payloads in producer order, and no
  unresolved authored identity reaches the kit.
- Delimiter-like and multibyte Unicode row/key pairs produce distinct exact
  helper outputs and dispatch intact; no client parser or synthesized index is
  present.
- Contract-invalid blank, misplaced, or duplicate descendant keys retain clear
  diagnostics from pre-materialization contract validation. Include duplicate
  descendant-key reuse across mutually exclusive authored branches, which the
  contract intentionally rejects template-wide. Repeated final literals within
  one row, across rows, or against a static sibling fail before hit regions
  publish. Existing render-scoped reuse remains accepted only for literal/root
  identities in mutually exclusive alternatives, not descendant keys.
- Add a real-collision test whose canonical descendant output equals a static
  sibling literal; this proves the existing collector checks final realized
  identity rather than only authored keys.

### Production keyboard and mouse proof

Using the canonical initial two-row × three-control fixture:

1. Render through `TuiApp::surface` and the production Ratatui backend; assert
   exact, nonoverlapping hit regions for all six helper-produced control ids.
2. From a known focus, send real Crossterm Tab events through
   `InputRouter::dispatch_event` and assert traversal reaches controls across
   both rows in rendered order.
3. Focus row 2 `rename`; send Enter (and Space if production supports it); assert
   `UiActionRequest.node_id`, action id, operation, and session UUID exactly;
   pass the dispatch to `TuiApp::handle_dispatch` and observe the exact
   `PluginSurfaceAction` request.
4. Send left Down and matching left Up within row 1 `remove`'s captured region;
   assert its exact distinct request. A mismatched release must not activate a
   neighboring row/control.
5. Apply the authoritative remove frame for a focused row, redraw and reconcile;
   prove every removed control id leaves the hit map/focus, then Tab and dispatch
   a surviving control with its own identity and payload.
6. Begin a fresh generation, reject a stale old-generation delta, apply the
   reconnect snapshot, and repeat exact control identity and dispatch checks.

The load-bearing row/control regression must use an identity-preserving wrong-row
mutation: realize one row's descendants against the neighboring row id, or swap
two rows' realized control ids. All six ids must remain unique, materialization
and duplicate collection must pass, and the production frame/HitMap must still
publish six regions; the test must fail only because keyboard or mouse dispatch
returns a `node_id`/payload owned by the wrong row. Restore and rerun green.
Separate unresolved-identity and shared-id mutations may remain as secondary
coverage for the pre-render rejection and collision gates.

### Canonical fixture and live-Hub proof

- Update `canonical_session_bindings_follow_published_oracle_through_frames_and_reconnect`
  to compare each stage with revision-27 `SessionPluginMaterializedRow.controls`
  and `materialize_session_plugin_rows`, including lifecycle transitions,
  removal, reconnect, payloads, and exact helper-derived identities.
- Extend
  `headless_live_runtime_runs_against_isolated_hub_when_binaries_are_available`
  and its contract-matrix assertion so the real Hub-delivered `contract.sessions`
  surface drives TUI entity state, production render/hit/input dispatch, same-row
  rename/remove requests, and returned `UiActionResult.node_id`/payload equality.
- Run `script/test-live-hub` with binaries built from Hub merge `e8febab...`, its
  Cargo.lock-pinned session worker, and the exact fixture directory at that Hub
  revision. Do not substitute a private TUI fixture or bypass the hit map.

### Repository gates

Run from a clean committed worktree with isolated colon-free targets:

```sh
env CARGO_TARGET_DIR=/private/tmp/botster-tui-descendant-fmt script/fmt
env CARGO_TARGET_DIR=/private/tmp/botster-tui-descendant-test script/test
env CARGO_TARGET_DIR=/private/tmp/botster-tui-descendant-clippy script/clippy
cargo tree --locked -i botster-ui-contract
cargo tree --locked -d
git diff --check origin/main...HEAD
```

Run the live gate with explicit Hub/session-worker binaries and the exact merged
fixture path:

```sh
env CARGO_TARGET_DIR=/private/tmp/botster-tui-descendant-live \
  BOTSTER_HUB_BIN=/path/to/hub-e8febab/botster-hub \
  BOTSTER_SESSION_WORKER_BIN=/path/to/hub-e8febab/botster-session-worker \
  BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE=/path/to/hub-e8febab/plugin-contract-matrix \
  script/test-live-hub
```

Planning baseline on `52174be...`: `script/fmt` exit 0; `script/test` exit 0
with 133 app/acceptance tests plus one package-manifest test; strict all-feature
`script/clippy` exit 0. The closed TUI-kit dependency independently cold-repinned
this exact TUI base and recorded the expected pre-fix exhaustive-match failure
for `BindListDescendant`; that failure is the implementation starting point,
not a waiver.

## Vault gaps worth capturing

No new planning-stage vault gap is open. Existing notes already cover the
entity-aware consumer boundary, literal-only renderer state, render-scoped
collision domain, real-frame hit routing, exact entity readiness, and structured
identity test oracles. Capture durable knowledge only if implementation reveals
a reusable rule not covered there; repository-specific SHAs, fixture fields,
and commands remain in this plan and pipeline evidence.
