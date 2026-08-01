# TUI Workspaces materialized lifecycle keyboard action plan

## Target and context

- Target repository: `trybotster/botster-tui`
- Target id: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Ticket: `ticket_1785558718_179265`
- Run: `run_1785558750_512793`
- Base: clean `main` at merged TUI `c3b4520479d22063b88e585179ed18653a9e8857`
- Repository charter: [[botster-tui-playbook]]

The target was resolved through the admitted spawn-target registry, not from
the ambient directory. Repository evidence inspected includes `README.md`,
the workspace and crate `Cargo.toml` files, `test.sh`, `script/fmt`,
`script/test`, `script/clippy`, `script/test-live-hub`, the existing
Workspaces lifecycle/readiness plans, the merged implementation and current
test harness in `crates/botster-tui/src/app.rs`, the real Workspaces producer
at `fce8aba572e80f07db4041f915f4c2d9860b9e40`, and the current Project
Pipelines ticket, gate, artifact, finding, question, review, and dependency
state.

## Guidance loaded

Role and workflow guidance:

- [[planner-playbook]]
- [[botster-planner-playbook]]
- [[project-pipelines-playbook]]
- [[botster-runtime-reviewer-playbook]]
- [[botster-runtime-verifier-playbook]]

Repository and surface guidance:

- [[botster-tui-playbook]]
- [[botster-tui-kit-playbook]]
- [[botster-hub-client-playbook]]
- [[botster-architecture]]
- [[cli-patterns]]
- [[spa-patterns]]

Targeted atomic guidance:

- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[botster tui uinode event routing captures hit regions during draw]]
- [[plugin authored tui surfaces dispatch via action props not node id literals]]
- [[tui error dedup tests must drive real input handlers]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]
- [[runtime client acceptance must render delivered snapshots through real registry]]
- [[live acceptance tests must not depend on a loop tick window]]
- [[suite wide acceptance criteria make every observed test failure in scope]]
- [[implementation deviations must resync committed plan acceptance checks]]
- [[pipeline vault checklists must cite exact resolvable note titles]]
- [[plan agents must author vault context as wikilinks not home paths]]
- [[vault example paths are not repository placement conventions]]

## Failure and runtime-path trace

The real `workspaces lifecycle` mode now reaches
`HistoricalReferencesRehydrated`. Its final keyboard oracle then searches
`active_surface.body`, the delivered but unmaterialized tree, for the first
`botster_workspaces.remove_session` action whose payload names
`session_ids[2]`. Workspaces authors the same semantic removal action in
mutually exclusive current, ended, indeterminate, and absent row templates.
Tree order therefore makes that search select the current-group template even
though the session is authoritatively absent after reconnect.

The production frame follows the correct path:

1. `TuiApp::surface()` materializes the delivered Workspaces tree from the
   authoritative `/session` entity store.
2. The absent binding realizes the producer-authored historical row and its
   canonical literal action identity.
3. `render_to_lines_with_presentation_state` records that realized row in the
   production hit map.
4. `InputRouter` can focus and dispatch that hit region.

The assertion fails only because its expected identity came from a template
that did not survive materialization. The renderer, hit map, action props,
Workspaces group order, and lifecycle store are not defective.

This ticket is intentionally acceptance-harness-only. The production user
path being proved is still real materialization -> real frame/hit map -> real
Crossterm keyboard dispatch -> exact plugin action request after the complete
real lifecycle and reconnect sequence.

## Scope

Make one surgical oracle correction in `crates/botster-tui/src/app.rs`:

1. Change only the `WorkspacesProfile::Lifecycle` match arm to resolve its
   final removal action from the production hit map rendered from the canonical
   materialized plugin root. Keep the
   already-green `WorkspacesProfile::Plumbing` rename-dialog lookup on its
   existing delivered authored surface; the ticket does not authorize changing
   that profile's oracle, and the common focus/Enter/assertion block remains
   unchanged.
2. Add a small test-only exact-action collector used by the lifecycle arm and
   its focused regression. It must inspect the production hit map after the
   realized tree is rendered, collect every region matching the exact semantic
   action id and payload, and assert exactly one match before returning that
   region's owned literal identity and action. Do not reuse `find_action_node`
   for this lifecycle expectation: its authored-tree first-match behavior
   cannot prove runtime uniqueness. Do not use `active_surface.body` or any
   authored `BindList` template as lifecycle runtime identity.
3. Derive both the expected literal node id and expected action metadata from
   that unique production hit region. This matters because TUI-kit promotes a
   `list_item.actions` action onto the list-item region; the nested materialized
   button id is not itself the dispatch region id. Keep the exact semantic
   action id, exact `session_ids[2]` payload, and literal node identity
   assertions. Explicitly assert that the Lifecycle region identity equals the
   realized absent binding root for `session_ids[2]`; uniqueness alone does not
   prove the action stayed in the historical branch.
4. Render `app.surface()` through the existing production presentation-aware
   frame path, locate that exact action and its owning node in the resulting hit map,
   focus it through the existing mouse-down path, press real Enter through
   `InputRouter`, and assert the dispatched request preserves the exact node,
   action, and payload before `TuiApp::handle_dispatch` accepts it.
5. Add or tighten one deterministic test-only regression that contains the
   same action/payload in mutually exclusive authored templates, materializes
   the absent branch, and proves the oracle selects and dispatches the absent
   realized identity. Reuse the production materializer, real hit map, and
   router; do not create a parallel action or hit-test abstraction.
6. Preserve the existing bounded entity-readiness diagnostics and every
   earlier Workspaces ledger stage. No README contract change is expected
   because the documented contract already says actions come from the
   production frame and hit map using exact delivered/materialized identity.
7. After the accepted removal, rerender the production frame and assert the
   exact action/payload for `session_ids[2]` is absent from its hit map. Prove
   that absence is non-vacuous by resolving another retained historical
   reference action from the same hit map. Do not use whole-screen UUID text:
   the accepted-action feedback intentionally renders a request id derived from
   the removed node id.

The exact-one hit-region collector is the only new helper justified by this ticket. Both
the focused regression and lifecycle live oracle must use it. Keep it test-only
and local to `app.rs`; do not add production API, optional configuration, or a
second materialization implementation.

Every changed line must trace to choosing the realized action identity,
proving the exact dispatch, or documenting this plan.

## Non-scope

- No Workspaces lifecycle grouping, ordering, labels, identities, bindings,
  action ids, payloads, persistence, or membership behavior.
- No changes to `botster-workspaces`, `botster-hub`,
  `botster-hub-client`, `botster-core`, `botster-tui-kit`, `botster-web`, or
  Project Pipelines code.
- No hard-coded Workspaces node-id literals or group names in generic TUI
  materialization, rendering, or input code.
- No pre-materialized template identity as runtime authority.
- No bypass of `app.surface()`, the production frame hit map, or
  `InputRouter`.
- No weakening from exact action/node/payload assertions to action-id-only,
  text, geometry, or “any remove action” assertions.
- No timing retries, new sleeps, wider deadlines, polling/list refresh, or
  surface refresh.
- No broad `app.rs` extraction, generic acceptance framework, dependency
  repin, adjacent cleanup, or documentation rewrite.

## Ownership boundaries and dependencies

`botster-tui` owns this test-only acceptance oracle, the application-level
materialization call, semantic action request context, real key dispatch, and
bounded local diagnostics. `botster-tui-kit` continues to own generic frame
rendering, hit-map capture, focus, and input routing; this plan consumes those
unchanged production mechanics. `botster-hub-client` and Hub remain the
authoritative lifecycle/subscription and plugin-action boundary and are
consumed unchanged.

Workspaces owns the producer-authored lifecycle groups, templates, realized
row identities, and removal actions. The Workspaces producer ticket
`ticket_1785296184_677408`, target
`tgt_71266a8d976d4535902ffed09c18a7ba`, already has this TUI ticket registered
as an open blocking dependency. That is the correct dependency direction: no
new dependency or cross-repository ticket is needed, and this run must not edit
the producer repository.

Adjacent Hub ticket `ticket_1785443253_376782`, target
`tgt_7e208a0c76a44980a83b63af976b1f22`, owns descendant identity grammar for
bound UiNode controls. It does not overlap or block this ticket: this plan
consumes the identity on the realized node and must not re-derive or extend the
contract locally.

After this TUI fix merges, the existing Workspaces run must consume merged TUI
main and attach the real-package lifecycle result before its dependency is
unblocked. A green local fixture cannot replace that downstream proof.

## Assumptions and unknowns

- Verified: the branch is clean and exactly at merged TUI `c3b4520` before the
  plan artifact.
- Verified: `materialized_plugin_root(&app)` already uses the same canonical
  session entity state as the application surface and is already the lifecycle
  harness's materialized-tree oracle.
- Verified: Workspaces `fce8aba` authors removal actions for every lifecycle
  template; its absent row is the `empty_template` of a separate exact-UUID
  binding after current/ended/indeterminate groups.
- Verified: at the failure point `session_ids[2]` is absent, historical
  references have been rehydrated, and the production hit map contains the
  absent row rather than the earlier current template.
- Verified: the producer ticket already depends on this correctly routed TUI
  ticket.
- Required invariant: action id plus exact `session_id` payload has exactly one
  match in the production hit map rendered from the realized Workspaces frame.
  The new collector enforces zero as missing and more than one as ambiguous,
  with bounded matching node ids in its failure diagnostic; no first-match
  assumption remains.
- Assumption: no README text needs updating because it already describes the
  intended materialized production frame/hit-map contract. Implement must
  update the plan if the code diff changes that contract.
- Resolved during implementation: the collector returns the production hit
  region's owned literal identity and action. The real Workspaces producer uses
  a `list_item.actions` child, whose action TUI-kit promotes onto the parent
  list-item hit region, so returning the nested realized button would not match
  the dispatch identity.

No ticket ambiguity, ignored requirement, or convention conflict requires a
human question. The requested exact absent/historical action and forbidden
alternatives determine the implementation boundary.

## Affected surfaces and files

Expected changes:

- `crates/botster-tui/src/app.rs`
  - lifecycle-only final Workspaces keyboard-node selection after historical
    rehydration; shared dispatch assertions and plumbing selection stay intact
  - test-only exact-one production-hit-action collector
  - focused materialized-template regression
- `docs/plans/tui-workspaces-materialized-lifecycle-keyboard-action-plan.md`
  - durable Plan/Review/Verify contract; Implement must commit it with the
    `app.rs` correction

Inspected and expected unchanged:

- `README.md`
- `script/test-live-hub`
- `script/fmt`, `script/test`, and `script/clippy`
- `Cargo.toml`, `Cargo.lock`, and crate dependency pins
- all dependency repositories

## Risks and mitigations

- **The lifecycle oracle still selects an authored template.** Build its
  expected node and action only from the production hit map after materialized
  rendering, and include a focused fixture where authored traversal and the
  production hit map deliberately disagree.
- **A first realized match silently hides ambiguity.** Collect all exact
  action/payload matches and fail unless there is exactly one, reporting only
  bounded matching node ids.
- **The plumbing profile changes without proof.** Leave its match arm and
  authored rename-dialog lookup unchanged; source review must confirm only the
  lifecycle arm switches selection strategy.
- **The assertion becomes weaker while turning green.** Retain equality for
  exact node id, semantic action id, and complete payload, and continue through
  the real hit map and Enter dispatch.
- **A test-only parallel hit map hides production behavior.** Render
  `app.surface()` with the existing presentation state and dispatch through
  `InputRouter`; do not synthesize a region or call `handle_plugin_action`
  directly.
- **Post-removal absence passes because the surface vanished.** Resolve a
  retained historical removal action from the same rerendered hit map before
  asserting the removed action/payload has zero matches.
- **Rendered text falsely reports a removed membership.** The accepted action
  feedback renders `request_id=req-<node_id>-<suffix>`, so the removed UUID
  deliberately remains in shell text. Assert action/payload absence from the
  production hit map instead of scanning all rendered text.
- **Earlier lifecycle regressions are hidden by focusing only on the failure.**
  Require the full lifecycle ledger and its current -> ended -> removal ->
  reconnect -> exact ended rehydration -> historical-reference sequence to
  complete before the keyboard assertion.
- **Timing changes mask identity mismatch.** Change no deadline, retry, yield,
  sleep, spawn order, or readiness predicate.
- **Cross-repository proof is mislabeled complete.** Record exact TUI,
  Workspaces, Hub, and worker/Core provenance for the real run; rerun from
  merged TUI main before the Workspaces blocker is cleared.
- **Plan and implementation drift.** If implementation touches another file
  or changes the selected approach, update this plan's scope and acceptance
  checks before Review.

## Acceptance checks and downstream proof

Baseline at `c3b4520`:

```sh
CARGO_TARGET_DIR=/private/tmp/botster-tui-plan-1785558718 script/test
```

Result: pass, 124 unit tests plus 1 manifest integration test.

Required repository gates, each with a fresh colon-free target directory where
Cargo runs:

1. `script/fmt`
2. `script/test`
3. `script/clippy`
4. Focused deterministic regression proving that an absent materialized branch
   wins over an earlier authored current template carrying the same
   action/payload; the exact-one collector rejects zero and duplicate production
   hit-region matches; and Enter dispatch yields the exact absent realized
   node/action/payload through the real hit map. The live Lifecycle oracle also
   asserts the selected node equals `session_ids[2]`'s realized absent binding
   root, then rerenders after acceptance, resolves a retained historical action
   as a same-frame positive control, and rejects the removed action/payload.
5. `git diff --check` and a focused source scan confirming no new sleeps,
   retries, `ListSessions`, refresh fallback, Workspaces node-id/group literal,
   direct handler bypass, or dependency change; also confirm the plumbing arm
   retains its existing authored rename-dialog lookup.
6. Negative control: revert or ablate only the production-hit-map selection in
   the focused regression and retain its expected nonzero result; restore the
   fix and rerun green.

Required real runtime proof on the implementation branch before Review/Verify:

```sh
BOTSTER_HUB_BIN=/path/to/botster-hub-at-88d343870700994d310f090fd5b2c4dbabb07405 \
BOTSTER_SESSION_WORKER_BIN=/path/to-that-hub-builds-matching-worker \
BOTSTER_WORKSPACES_PACKAGE_PATH=/path/to/clean-botster-workspaces-at-fce8aba572e80f07db4041f915f4c2d9860b9e40 \
CARGO_TARGET_DIR=/private/tmp/botster-tui-workspaces-materialized-keyboard \
  script/test-live-hub workspaces lifecycle
```

Replace placeholders with canonical real paths in the evidence artifact. The
raw output must show all prior lifecycle ledger stages still green through
`HistoricalReferencesRehydrated`, then show keyboard focus/dispatch against
the exact absent/historical realized action and a complete lifecycle ledger.
Retain the existing bounded last-state diagnostics on failure.

The plumbing live command is not newly required because this revision
deliberately leaves the plumbing selection arm unchanged. If implementation
instead changes any shared source used to select the plumbing keyboard node,
the plan must be resynchronized and `script/test-live-hub workspaces plumbing`
becomes a required additional real-runtime gate with its ledger attached.

Required downstream proof before unblocking `ticket_1785296184_677408`:

1. Merge this correction to TUI main.
2. Rerun the same real Workspaces lifecycle command from merged TUI main
   against Workspaces `fce8aba` (or the producer branch commit that contains
   it) and Hub `88d343870700994d310f090fd5b2c4dbabb07405` with its matching
   worker/Core build.
3. Attach TUI merge SHA, Workspaces SHA, Hub SHA, worker/Core provenance,
   complete command, raw ledger/result, and exit status to both this run and
   the blocked Workspaces producer run.
4. Only then resolve the registered TUI dependency and let the Workspaces
   pipeline continue.

Code existence, a synthetic fixture, direct action invocation, a hit-map
membership check without dispatch, or an action-id-only assertion does not
satisfy the runtime proof.

## Vault gaps worth capturing

No new vault gap is established yet. Existing notes already cover realized
identity, render-time hit maps, real input handlers, delivered-snapshot
materialization, exact entity readiness, and timing-safe live acceptance. If
implementation or the focused negative control reveals a reusable distinction
not already captured—specifically that repeated semantic actions in mutually
exclusive authored templates must be resolved only after branch
materialization—capture it through the vault inbox/document/connect/verify
pipeline. Otherwise record `capture_path: nil` with the reason that this ticket
is a direct application of existing guidance.

## Convention check

No loaded convention conflicts with this plan. It reuses the existing
materializer, production renderer, hit map, router, repository scripts, and
public Hub boundary. It adds no dependency, product semantics, shared
primitive, compatibility path, service abstraction, optional configuration,
or timing workaround.
