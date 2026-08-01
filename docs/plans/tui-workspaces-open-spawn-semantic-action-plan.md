# TUI Workspaces `open_spawn` semantic action adoption plan

## Outcome and routing

- Ticket: `ticket_1785612604_598776`
- Run: `run_1785615824_755309`
- Target repository: `trybotster/botster-tui`
- Target id: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Repository charter: `[[botster-tui-playbook]]`
- Base inspected: clean ticket branch at `52174be`, identical to `origin/main`

`project_pipelines_current_context` supplied the target id. The admitted spawn-target
registry independently resolved it to `botster-tui`, and this worktree's `origin` is
`git@github.com:trybotster/botster-tui.git`. The repository was resolved before the
ambient worktree was inspected.

## Context loaded

Role, repository, dependency-surface, and workflow guidance:

- `[[planner-playbook]]`
- `[[botster-planner-playbook]]`
- `[[botster-tui-playbook]]`
- `[[botster-tui-kit-playbook]]`
- `[[botster-workspaces-playbook]]`
- `[[botster-runtime-reviewer-playbook]]`
- `[[botster-runtime-verifier-playbook]]`
- `[[project-pipelines-playbook]]` for checklist, artifact, gate, and advancement
  discipline only; Project Pipelines source and policy are not changing
- `[[botster-architecture]]`, `[[cli-patterns]]`, and `[[spa-patterns]]`

Targeted atomic notes:

- `[[tui and browser are equal clients]]`
- `[[botster tui consumes tui kit through a thin app policy adapter]]`
- `[[tui client attach uses hub protocol not session protocol]]`
- `[[tui and socket terminal streams use clientworker transport adapters]]`
- `[[botster tui uinode event routing captures hit regions during draw]]`
- `[[tui error dedup tests must drive real input handlers]]`
- `[[acceptance readiness requires the exact expected entity not any authoritative snapshot]]`
- `[[acceptance harness region oracles must key on node identity not concatenated text]]`
- `[[conformance helpers must dispatch the action id read from the rendered node]]`
- `[[phase one action ids are semantic botster events not DOM event names]]`
- `[[plugin authored tui surfaces dispatch via action props not node id literals]]`
- `[[renderer state accepts only realized literal identity]]`
- `[[renderer acceptance tests must drive real frame backend]]`
- `[[plugin ui action ids are a two site change and hub fails closed on unregistered ids]]`

Repository and dependency context inspected:

- `README.md`, workspace/crate manifests and lockfile, repository gate scripts, the
  installed acceptance schema/examples, and prior TUI Workspaces plans;
- `crates/botster-tui/src/app.rs`, especially `drive_spawn_case`,
  `activate_acceptance_action`, `acceptance_frame`, the request audit, the installed
  `apps open` harness, reconnect, exact-entity readiness, and the older plumbing and
  lifecycle harnesses;
- `crates/botster-tui/src/acceptance.rs` and the v1 scenario/evidence contract;
- producer ticket `ticket_1785611316_167898`, merged Workspaces PR #12 at
  `737ec8133c5f985f4c2bd5a369365049558afa56`, and its clean installed package source;
- predecessor TUI driver ticket `ticket_1785602853_851250`, merged by TUI PR #42;
- current Project Pipelines dependencies, artifacts, gates, findings, questions, and
  same-project tickets.

## Existing production path and defect

The installed TUI already enters acceptance mode through the two v1 file paths,
connects to the caller-owned Hub using the Hub-injected connection and data directory,
renders the presentation-aware production UiNode tree into a real Ratatui frame and
`HitMap`, focuses controls through bounded Tab key events, and activates them with
Enter through `InputRouter`. `activate_acceptance_action` reads a matched hit region's
literal realized node id and authored action, then rejects the dispatch unless node,
action, surface, kind, and payload exactly equal that read-back metadata.

Only the Spawn-opener lookup in `drive_spawn_case` is stale. It currently requests
`botster_workspaces.open` and discriminates it with a reconstructed
`dialog=spawn-target:<workspace_id>` payload value. The merged Workspaces producer now
publishes the one realized detail opener as `botster_workspaces.open_spawn`, preserves
the opaque/dynamic node id and payload, and separately registers the new action with
the same presentation callback.

The other `botster_workspaces.open` sites are not deprecated Spawn selectors: they
select a workspace row, open Rename, or reopen a row after lifecycle reconnect. They
remain in scope only as regression guards and must not be bulk-renamed.

## Scope

1. In the installed acceptance driver's `drive_spawn_case`, locate exactly one
   realized hit region by semantic action id `botster_workspaces.open_spawn` alone.
   Remove the Spawn-specific `botster_workspaces.open` selector and the
   `spawn-target:<workspace_id>` payload discrimination. Do not inspect visible label
   text and do not construct or predict the Workspaces node id. After selection, verify
   the read-back payload still identifies the scenario's workspace; this is an identity
   assertion, never a selection discriminator.
2. Continue using `activate_acceptance_action` for bounded keyboard focus and Enter
   dispatch. Preserve its equality checks so the outgoing `UiActionRequest` carries
   the exact action id, literal node id, surface, kind, and payload read from that
   frame's hit region. No direct payload/action request or mouse event is permitted.
3. Add a focused real-frame regression at the narrow selector/dispatch seam. Its
   authored surface must contain a non-`Spawn` visible label bound to
   `botster_workspaces.open_spawn` and a visible `Spawn` decoy bound to deprecated
   `botster_workspaces.open` with the former dialog payload. Drive the real `HitMap`
   and keyboard router, assert the semantic control's opaque realized node/action and
   read-back payload are dispatched, and assert the copy-matching/generic decoy is not.
   Keep any extracted helper private and production-used; do not create Workspaces
   policy in the renderer or TUI kit.
4. Strengthen the existing installed `apps open` test's evidence assertions. For each
   of all three managed-Git cases, require one case-scoped focused/dispatched
   `botster_workspaces.open_spawn` opener and no case-scoped deprecated generic opener.
   Retain subsequent target selection, branch typing, `botster_workspaces.spawn`,
   correlated accepted results, exact returned Git facts, entity reconciliation,
   reconnect, and request-budget assertions.
5. Update the caller-owned acceptance README wording to name the semantic Spawn action,
   copy independence, and exact read-back dispatch contract. Preserve the v1 scenario
   and JSONL schema/examples byte-for-byte unless implementation exposes a real
   contract defect; this ticket does not version or broaden that file contract.
6. Persist implementation evidence and the negative-control result through Project
   Pipelines. Every changed production/test/doc line must trace to semantic selection,
   its required negative proof, or the existing caller contract.

## Non-scope

- No changes to `botster-workspaces`, `botster-hub`, `botster-hub-client`,
  `botster-tui-kit`, `botster-web`, or Project Pipelines source.
- No compatibility fallback or acceptance of `botster_workspaces.open` for Spawn.
- No changes to shared UiNode/action envelopes, renderer mechanics, HitMap identity,
  InputRouter behavior, package installation, Hub/session lifecycle authority, Git or
  worktree resolution, interactive TUI policy, sibling checkout discovery, or timing
  as a readiness oracle.
- No node-id construction, visible-copy parsing, direct `UiActionRequest`/payload
  dispatch, mouse substitution, list-session polling, list refresh, or synchronization
  surface rerender.
- No broad cleanup of legitimate generic `botster_workspaces.open` callers.

## Ownership boundaries and dependencies

`botster-tui` owns this installed client driver, application-level semantic control
selection, request/evidence assertions, README, and real input-path tests. TUI kit
continues to own generic rendering, hit maps, and keyboard routing unchanged.
Workspaces continues to own the action vocabulary, dynamic node identity, visible
copy, presentation payload, handler registration, and product workflow. Hub continues
to own target admission, Git/worktree resolution, spawning, session identity, and
lifecycle truth.

Registered prerequisites are already correct and closed:

- `ticket_1785611316_167898` (`botster-workspaces`) supplies merged
  `botster_workspaces.open_spawn` producer behavior.
- `ticket_1785602853_851250` (`botster-tui`) supplies the installed caller-owned driver.

The downstream integration ticket `ticket_1785192726_335558` is already registered as
depending on this ticket (`dependency_1785612639_232266`) and remains open. It owns the
final one-long-lived-shared-Hub browser/TUI/Workspaces proof after this and the separate
Web adoption ticket land. No new dependency edge or cross-repository edit is needed.
Open sibling TUI ticket `ticket_1785602865_181673` owns canonical BindList descendant
identity; it may touch `app.rs` but does not own this action selector and is not an
inseparable prerequisite. Whichever sibling run merges second must rebase on the
updated `origin/main` and rerun the full repository gates plus
`script/test-live-hub workspaces installed-driver` after that rebase.

## Assumptions and unknowns

- Assumption: one selected Workspaces detail renders exactly one
  `botster_workspaces.open_spawn` hit region. The driver must fail closed on zero or
  multiple matches; it must not add payload or copy heuristics to choose among them.
- Assumption: the existing generic read-back verification in
  `activate_acceptance_action` remains the dispatch authority and needs no alternate
  request constructor.
- Assumption: Workspaces merge `737ec813...` is the consumed producer baseline. Its
  clean source exposes the semantic action, retains visible `Spawn`, keeps the dynamic
  node id, and registers the descriptor.
- Assumption: no browser parity code belongs here because this run changes only a TUI
  consumer; Web adoption is separately routed and final parity belongs to the
  integration ticket.
- Unknown until implementation: whether the negative fixture can reuse a small existing
  UiNode builder or needs one private selector extraction. Choose the smaller
  production-used seam and do not generalize beyond semantic HitMap selection.
- Baseline caveat: `script/test` from this colon-bearing worktree fails before test
  execution because macOS rejects `:` in Cargo's dylib search path. The same command
  with a fresh colon-free `CARGO_TARGET_DIR` passes all 133 unit tests plus the package
  manifest integration test. This is an environment path issue, not a source failure.

## Expected affected files

- `crates/botster-tui/src/app.rs` — production Spawn-opener selector, focused negative
  regression, and installed-driver evidence assertions.
- `README.md` — precise semantic identity/read-back/copy-independent acceptance contract.
- `docs/plans/tui-workspaces-open-spawn-semantic-action-plan.md` — this durable plan.

Expected unchanged: `acceptance.rs`, v1 scenario/schema/evidence fixtures, manifests,
lockfile, renderer, main entrypoint, TUI kit, Hub client pins, package manifest, and
repository gate scripts.

## Risks

- A broad string replacement would corrupt legitimate row-selection, Rename, or
  lifecycle-reopen actions. Review the exact diff and retain those generic sites.
- Matching `open_spawn` but constructing a node id or payload separately could make the
  test green while production dispatches stale identity. Require equality with the
  rendered hit-region metadata and inspect emitted JSONL.
- A positive test with unchanged `Spawn` copy would not prove copy independence. The
  non-`Spawn` semantic control plus visible generic decoy is mandatory.
- Plain workspace tests skip the live installed-driver body when explicit Hub inputs are
  absent; their green result is not the runtime proof. Run the explicit live profile.
- Live evidence can accidentally use stale sibling packages or binaries. Require clean,
  explicit paths and record exact source SHAs/binary build provenance.
- Acceptance deadlines bound failures but cannot become readiness criteria. Retain exact
  subscription, action-result, entity UUID/lifecycle, membership, and request-count
  predicates.

## Acceptance checks and downstream proof

1. `script/fmt` passes.
2. `CARGO_TARGET_DIR=<fresh-colon-free-dir> script/test` passes all workspace/all-target
   tests, including the focused copy/generic-action negative regression, contract tests,
   and package manifest test. Record the executed test counts; do not treat the
   environment-skipped installed test as live proof.
3. `CARGO_TARGET_DIR=<fresh-colon-free-dir> script/clippy` passes with `-D warnings`.
4. With explicit clean Hub binaries built from the same Hub revision pinned in this
   crate (`fab44c5de7b28a8756268608662d2b870efb001a`) and merged Workspaces paths, run:

   ```sh
   BOTSTER_HUB_BIN=<clean-hub>/target/debug/botster-hub \
   BOTSTER_SESSION_WORKER_BIN=<clean-hub>/target/debug/botster-session-worker \
   BOTSTER_WORKSPACES_PACKAGE_PATH=<clean-workspaces-at-737ec813> \
   CARGO_TARGET_DIR=<fresh-colon-free-dir> \
     script/test-live-hub workspaces installed-driver
   ```

   This must launch the installed TUI via `botster-hub apps open`, complete all three
   `existing_worktree`, `existing_branch`, and `missing_branch` cases, and produce
   schema-valid evidence containing exact `open_spawn` read-back node/action identity,
   no case-scoped deprecated generic Spawn action, two surface renders, zero
   `ListSessions`, a fresh reconnect subscription/snapshot, accepted correlated spawn
   results, exact returned target/branch/worktree facts, and exact current entity plus
   membership reconciliation without a synchronization rerender.
5. Show the new regression goes red when only the production Spawn selector is reverted
   to `botster_workspaces.open` (or when `open_spawn` selection is otherwise ablated),
   then restore the fix and rerun it green. The failure must demonstrate that visible
   `Spawn` copy and the old dialog payload do not rescue the deprecated path.
6. `git diff --check` and a targeted `rg`/diff review confirm no dynamic Workspaces node
   id was added, no compatibility branch/direct payload/mouse path exists, the stale
   Spawn selector is gone, and legitimate generic-open callers remain.
7. After merge, downstream `ticket_1785192726_335558` must import/use the merged TUI
   artifact in its one-long-lived-shared-Hub run and prove browser/TUI parity plus Hub,
   Git/worktree, package, membership, lifecycle, non-destructive collision, history,
   and deletion truth. That ticket must route defects back here rather than patch the
   TUI from the Workspaces repository.

## Pipeline artifacts and gates

- Plan artifact: this file plus the structured `botster_stack_plan_gate` evidence.
- Implementation artifact: commit/PR, exact changed files, explicit Workspaces/Hub source
  provenance, raw repository-gate and live-driver results, red/green ablation evidence,
  runtime entrypoint trace, and residual risks.
- Review/Verify must use the runtime overlays, inspect the real production selector and
  full evidence assertions, rerun repository gates, and independently execute the live
  installed-driver profile rather than carrying forward a skipped unit-test result.

## Convention fit and vault gaps

Convention conflict: none. The plan is a cold switch with no dual path, keeps product
policy in Workspaces, keeps reusable renderer/input mechanics in TUI kit, and makes the
TUI consume realized semantic metadata through its existing thin adapter.

No new durable engineering rule is evident at Plan time: exact semantic read-back,
visible-copy independence, literal realized identity, real-frame input proof, cold
replacement, and cross-repository ownership are already covered by the loaded notes.
Capture only if implementation reveals a repeatable TUI-specific negative-test pattern
not already represented; otherwise record no capture with this rationale.
