# TUI Workspaces subscription readiness correction plan

## Delivery identity

- Ticket: `ticket_1785553386_292915` — TUI acceptance: subscribe before
  Workspaces lifecycle spawn and reject empty readiness
- Target repository: `botster-tui` (`trybotster/botster-tui`)
- Target ID: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Base: `main` at `39958202615a16417356175cd3fc574aa7112b1e`
- Pipeline run: `run_1785553416_700864`
- Repository charter: [[botster-tui-playbook]]

The ticket target was resolved through `project_pipelines_current_context` and
the Hub spawn-target registry. The ambient worktree was not used to infer
ownership.

## Context loaded

Role and repository guidance:

- [[planner-playbook]]
- [[botster-planner-playbook]]
- [[botster-tui-playbook]]
- [[botster-tui-kit-playbook]] for the consumed renderer/input boundary; no kit
  change is planned
- [[botster-runtime-reviewer-playbook]]
- [[botster-runtime-verifier-playbook]]
- [[project-pipelines-playbook]] for gate, artifact, checklist, and downstream
  evidence discipline; no Project Pipelines package code is in scope
- [[botster-architecture]], [[cli-patterns]], and [[spa-patterns]]

Targeted notes:

- [[botster client subscriptions should not hydrate global state]]
- [[plugin surfaces request model state through ui bindings not hub subscribe]]
- [[botster hub client state sync is entity frame only]]
- [[botster entity snapshots are authoritative reconnect baselines]]
- [[botster web dogfood session readiness can arrive as entity snapshot]]
- [[fixture driven acceptance smoke tests can prove first party package plumbing]]
- [[conformance harnesses gate on deterministic invariants not timing]]
- [[a regression test must be shown to go red with the fix reverted]]
- [[external client hub tests use subprocess spawned hub test support]]
- [[runtime client acceptance must render delivered snapshots through real registry]]
- [[PTY integration tests poll for readiness not fixed sleeps]]
- [[live acceptance tests must not depend on a loop tick window]]
- [[env blocked cross repo e2e should carry to verify after producer proof]]
- [[colon worktree paths break cargo dyld library paths]]

Repository evidence inspected:

- `README.md`, `Cargo.toml`, `crates/botster-tui/Cargo.toml`, `test.sh`,
  `script/test`, `script/fmt`, `script/clippy`, and `script/test-live-hub`
- `crates/botster-tui/src/app.rs`: `SessionEntityState`, subscription startup,
  `wait_for_authoritative_session`, the generic contract-matrix live fixture,
  and the Workspaces plumbing/lifecycle acceptance profiles
- `docs/plans/tui-workspaces-lifecycle-acceptance-mode-plan.md` and current
  repository plan placement prior art
- Current Project Pipelines context, including gates, dependencies, artifacts,
  findings, questions, reviews, and the linked Workspaces producer run
- Workspaces PR #11 at `fce8aba572e80f07db4041f915f4c2d9860b9e40`
  and its producer implementation evidence

## Failure and production-path trace

The merged lifecycle harness currently performs these steps in the wrong
order:

1. It spawns the two controlled Hub sessions through `DaemonRequest::Spawn`.
2. It then constructs `TuiApp`, whose production `TuiApp::new` path opens the
   app-owned `/session` subscription.
3. It waits only for `SessionEntityState::has_snapshot`.
4. An authoritative but empty snapshot satisfies that wait, so the first
   materialized Current-row assertion runs without either exact spawned UUID.

Workspaces producer proof against TUI `3995820` and Workspaces `fce8aba`
reproduced this at the former `app.rs:10860`. The generic contract-matrix live
path already demonstrates the sanctioned shape: construct `TuiApp`, receive
the subscription baseline, spawn a unique session, and wait for that exact
authoritative row before rendering. The correction will make the exact-row part
of that shape an explicit test-harness invariant and apply the missing
ordering/readiness barrier only to the Workspaces path.

An empty authoritative snapshot remains a valid subscription baseline. It is
not readiness evidence for an expected session row. Conversely, a snapshot is
valid row readiness when it contains the exact expected UUID and lifecycle;
the harness must assert converged entity-store state rather than require a
specific snapshot/upsert/patch frame variant.

This ticket is intentionally harness-only. It proves the production
`TuiApp::new` subscription, `poll_hub`, `SessionEntityState`, UiNode
materialization, real frame/hit-map, and public Hub lifecycle paths; it does not
change ordinary TUI runtime behavior.

## Scope

The smallest correction is confined to `botster-tui` test-only acceptance
orchestration and its documentation:

1. Add a small test-only `SessionEntityState` expectation/wait helper that
   requires an active authoritative subscription baseline plus the exact
   requested session UUID and lifecycle class (or exact absence). On timeout it
   reports bounded last-observed state: subscription id, snapshot
   presence/sequence, expected UUID/lifecycle, and the relevant observed row or
   absence. It must not issue list requests, refresh surfaces, or retain an
   unbounded frame log.

   This helper is deliberately additive to, and does not replace,
   `wait_for_authoritative_session`. That existing function is production code
   used by `run_headless_live_runtime` and several tests; it waits on the
   materialized `app.sessions` attachability contract and returns no lifecycle
   or entity-store observation. The new helper is test-only because this ticket
   must inspect the exact `session_entities` row/lifecycle/absence that UiNode
   bindings consume. Do not migrate unrelated production or test call sites.
2. In the Workspaces lifecycle profile, construct `TuiApp` and wait for its
   authoritative baseline before spawning any controlled lifecycle sessions.
   Preserve the existing fixed deterministic session UUID set and assert its
   first two spawn targets are absent from the fresh isolated-Hub baseline.
   Then spawn and wait until both exact rows are authoritative `current` rows.
   Only then open/materialize the owner-authored Workspaces surface.
3. Preserve the existing Workspaces current rendering, exact ended-row wait,
   exact removal wait, and their current -> ended -> removal program order.
   Route only those bounded entity-state waits through the shared last-state
   diagnostic where necessary. After the fresh reconnect generation and
   authoritative snapshot barrier, require the surviving controlled UUID to be
   present in exact `ended` state before historical rehydration assertions.
   Do not add store-wide sequence comparisons: `snapshot_seq` is diagnostic
   metadata for one active generation, not a per-row lifecycle clock, and the
   existing sequential mutations plus exact row predicates are the
   deterministic ordering barriers.
4. Preserve the existing generic contract-matrix fixture chain: it already
   constructs `TuiApp` before spawning its generated unique UUID, waits for the
   exact authoritative row, then drives exact ended and removal predicates.
   Minimally strengthen it with an initial exact-row absence assertion and the
   same bounded entity-state diagnostic/predicate. Do not rewrite its delivered
   surface, lifecycle, reconnect, or action assertions. This remains the
   deterministic generic package/entity proof without Workspaces semantics.
5. Add a focused deterministic regression assertion that an empty snapshot is
   not ready for an expected UUID while a baseline containing that exact row in
   the requested lifecycle is ready. This supplies the stable negative control
   for the predicate independently of process scheduling.
6. Update the Workspaces live-mode README text to document subscribe-before-
   spawn, exact-row readiness, ordered exact-state proof, and bounded
   diagnostics.

Already correct; do not rewrite:

- the Workspaces ended lifecycle predicate/assertion and exact entity removal
  predicate/assertion;
- the Workspaces current/ended/absent materialization, fixed 16-reference
  seeding, binding ceiling, realized-root ordering, membership removal,
  reconnect generation/stale-frame mechanics, input, and ledger assertions;
- the generic contract-matrix fixture's existing subscribe-before-spawn,
  exact-row, ended, removal, delivered-surface, reconnect, and action chain;
- production `wait_for_authoritative_session` and its unrelated call sites.

Every changed line must trace to subscription ordering, exact readiness,
ordered lifecycle proof, diagnostics required by this ticket, or documentation
of those mechanics.

## Non-scope

- No Workspaces lifecycle classification, labels, surface structure, action
  ids, payloads, persistence, or package-specific production behavior in TUI.
- No changes to `botster-workspaces`, `botster-hub`, `botster-hub-client`,
  `botster-core`, `botster-tui-kit`, `botster-web`, or Project Pipelines.
- No list-session refresh, polling fallback, surface refresh, alternate entity
  store, or direct Core/session-worker access.
- No acceptance of empty hydration as expected-row readiness and no dependence
  on one transport frame variant.
- No longer sleeps, widened deadlines, retry inflation, or incidental loop-tick
  windows.
- No sibling checkout/worktree discovery or overrides.
- No broad `app.rs` extraction, generalized acceptance framework, production
  API, optional configuration, or adjacent cleanup.
- No change to the Workspaces controlled session-id scheme, fixed 16-reference
  seeding, binding scale, or realized-order assertions.
- No weakening or relabeling of the existing plumbing/lifecycle ledgers.

## Repository ownership and cross-repository dependencies

`botster-tui` owns this client-side subscription/readback behavior, test-only
acceptance orchestration, TUI entity-store convergence checks, materialization,
input routing, and local diagnostics. `botster-tui-kit` remains an unchanged
dependency because no generic rendering or routing mechanic is defective.

The Hub owns session lifecycle truth, subscription snapshots/deltas, sequence
ordering, spawning, shutdown, and removal. This plan consumes those public
contracts through the pinned `botster-hub-client` and
`botster-hub-test-support`; it does not broaden this run into Hub work.

Workspaces owns the semantic lifecycle surface. Its producer ticket
`ticket_1785296184_677408`, target
`tgt_71266a8d976d4535902ffed09c18a7ba`, already records this TUI ticket as a
blocking dependency. The correct direction is therefore to merge this generic
consumer correction and rerun the producer's exact package through the merged
mode. No reverse dependency is needed on this TUI ticket, and no Workspaces
edit is authorized here.

The separately routed Web reconnect defect remains owned by
`ticket_1785553389_894623` in `botster-web`; it is not a prerequisite or scope
for this TUI plan.

## Assumptions and unknowns

- Verified: `TuiApp::new(Some(endpoint))` starts the app-owned `/session`
  subscription and `SessionEntityState` accepts deltas only after its active
  generation's snapshot.
- Verified: the existing generic contract-matrix live fixture already creates
  `TuiApp` before spawn and waits for its unique authoritative session row.
- Verified: snapshots are authoritative baselines, but readiness belongs to the
  exact converged row; an empty baseline cannot satisfy an expected-row wait.
- Verified: the Workspaces producer failure is a TUI fixture/oracle defect, not
  a Workspaces lifecycle semantic defect.
- Verified: the Workspaces ended and removal waits already use exact UUID and
  lifecycle/absence predicates, and the generic fixture already has the
  sanctioned subscribe-before-spawn/current/ended/removal chain. Those working
  assertions are preservation constraints, not new implementation scope.
- Verified: `SessionEntityState::snapshot_seq` is a store-wide cursor and
  `begin_generation` resets it. It belongs in bounded diagnostics only; this
  ticket will not present it as a per-row lifecycle ordering proof or compare
  values across generations.
- Assumption: the existing bounded deadline is sufficient once the producer is
  guaranteed an observation opportunity by subscribing before spawn. This
  ticket must not raise it to mask a race.
- Implementation choice: keep the exact lifecycle/absence helper test-only and
  additive. Return only what the assertions need; bounded diagnostic text may
  include active generation and store-wide sequence without exposing a new
  production abstraction.
- No human decision is required: the ticket explicitly chooses subscription
  ordering, exact-row readiness, fixture boundary, forbidden fallbacks, and
  downstream rerun.

## Affected surfaces and files

- `crates/botster-tui/src/app.rs`
  - test-only exact entity-state predicate/wait and bounded diagnostic
  - minimal generic contract-matrix pre-spawn absence/diagnostic strengthening
  - Workspaces lifecycle setup and missing exact-current readiness barrier
  - focused empty-snapshot readiness regression coverage
- `README.md`
  - corrected Workspaces lifecycle acceptance contract and diagnostic behavior
- `docs/plans/tui-workspaces-subscription-readiness-correction-plan.md`
  - durable implementation plan

Inspected and expected unchanged:

- `script/test-live-hub`: its explicit mode/profile/package/binary contract and
  exact execution/completion-ledger checks are already correct.
- `script/test`, `script/fmt`, and `script/clippy`: repository-owned gates.
- `Cargo.toml`, `Cargo.lock`, and crate dependencies: no contract or dependency
  change is needed.
- `botster-tui-kit`: no renderer/input mechanic changes.

## Risks and mitigations

- **Snapshot semantics overcorrected:** rejecting all snapshots would break a
  valid readiness path. Match converged state, not frame type; reject only a
  baseline missing the exact expected row/state.
- **Stale or unrelated row creates a false green:** the generic fixture keeps
  its generated unique UUID. Workspaces keeps its fixed deterministic UUIDs and
  relies on the existing fresh isolated-Hub root; assert the first two are
  absent before spawn, then match UUID and lifecycle exactly.
- **Store-wide sequence is mistaken for a row clock:** include subscription id
  and `snapshot_seq` only as bounded last-state diagnostics. Preserve ordering
  with the existing synchronous mutation sequence and exact row predicates;
  never compare generations.
- **Timeout loses the useful failure:** retain only bounded last state for the
  relevant UUID plus compact subscription metadata; do not dump an unbounded
  entity/frame history.
- **Generic proof accidentally encodes Workspaces:** keep contract-matrix
  assertions in terms of generic `/session` bindings and entity states. All
  Workspaces tree/action assertions remain in the real-package profile.
- **A test passes only because the fix creates its precondition:** add the pure
  empty-versus-exact readiness assertion and prove it goes red under a narrow
  ablation that makes `has_snapshot` alone sufficient. This negative control
  covers the readiness predicate only. Reverting subscribe-before-spawn is
  inherently racy because the later baseline may or may not contain already
  spawned rows, so make no reliable red-on-revert claim for ordering; its
  deterministic local check is pre-spawn absence, and its runtime proof is the
  required post-merge Workspaces rerun.
- **Cross-repository false completion:** repository and generic fixture gates
  are necessary but not final producer proof. Preserve the real Workspaces
  invocation and its exact commit/binary provenance after merge.
- **Colon-bearing worktree path breaks Cargo on macOS:** set a fresh explicit
  colon-free `CARGO_TARGET_DIR` for every repository and live acceptance gate.

## Acceptance checks and downstream proof

Baseline evidence on `3995820`:

- `CARGO_TARGET_DIR=/private/tmp/botster-tui-plan-1785553386 script/test` — pass,
  123 unit tests plus 1 manifest integration test.
- Plain `script/test` in the generated colon-bearing worktree fails before test
  execution because macOS rejects the colon in `DYLD_FALLBACK_LIBRARY_PATH`;
  this is the known worktree-path gotcha, not a test failure.
- `git diff --check` — pass before the plan artifact was added.

Required implementation gates with fresh colon-free target directories:

1. `script/fmt`
2. `script/test`
3. `script/clippy`
4. Focused unit filter for the empty-baseline versus exact-row readiness
   predicate and Workspaces ledger tests.
5. The repository's generic live mode with explicit current Hub and worker
   binaries plus the explicit canonical contract-matrix fixture:

   ```sh
   BOTSTER_HUB_BIN=/path/to/current/botster-hub \
   BOTSTER_SESSION_WORKER_BIN=/path/to/matching/botster-session-worker \
   BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE=/path/to/plugin-contract-matrix \
   CARGO_TARGET_DIR=/private/tmp/botster-tui-generic-readiness \
     script/test-live-hub
   ```

   Require the generated exact row to be absent in the initial baseline, then
   preserve the existing current, ended, and removal chain through the
   production `TuiApp` store and generic delivered surface, now with bounded
   last-state diagnostics.
6. `git diff --check` and a focused source scan confirming the correction adds
   no `ListSessions`, list-refresh fallback, sleeps, Workspaces production
   semantics, or sibling checkout discovery.
7. Regression ablation: narrowly make the readiness predicate accept
   `has_snapshot` alone (or bypass its exact-row check), rerun the focused
   regression, and retain the expected nonzero result. Restore the predicate
   and rerun green.

Required downstream producer proof after this correction is merged:

```sh
BOTSTER_HUB_BIN=/path/to/botster-hub-at-281db04523503c5cf692813ea313344aa6067644 \
BOTSTER_SESSION_WORKER_BIN=/path/to/that-hub-lockfiles-matching-worker \
BOTSTER_WORKSPACES_PACKAGE_PATH=/path/to/clean-botster-workspaces-at-fce8aba572e80f07db4041f915f4c2d9860b9e40 \
CARGO_TARGET_DIR=/private/tmp/botster-tui-workspaces-fce8aba \
  script/test-live-hub workspaces lifecycle
```

The final artifact must replace placeholders with canonical realpaths and
record TUI merge SHA, Workspaces SHA `fce8aba`, Hub SHA `281db045`, the matching
worker/Core provenance, complete command, exit status, and bounded diagnostics
if it fails. Green evidence must prove, in order:

1. subscription and authoritative baseline established before lifecycle spawn;
2. both exact controlled UUID rows observed as current;
3. owner-authored current rows rendered;
4. after the controlled shutdown, one exact row observed and rendered ended;
5. after the controlled removal, the other exact row absent while its
   Workspaces reference remains legible;
6. existing membership-removal, reconnect/new-generation snapshot followed by
   exact surviving-ended-row rehydration, surface reopen, historical
   rehydration, input routing, and completion-ledger checks;
7. no list or surface refresh used to reconcile lifecycle.

This downstream run is the required user/runtime path for this harness ticket;
code existence and generic fixture proof alone are insufficient. Its result
must be attached to `ticket_1785296184_677408` so that producer dependency can
close.

## Vault gaps worth capturing

The existing vault says snapshots can be valid readiness when they contain the
target row, and says waits must use semantic conditions rather than sleeps. It
does not state the complementary acceptance rule exposed here: an authoritative
empty snapshot proves subscription baseline but cannot prove readiness for a
specific expected entity, and bounded failures should retain the last relevant
entity state.

After implementation and ablation evidence confirm the rule, capture one atomic
gotcha through the vault inbox/document/connect/verify pipeline, tentatively
`acceptance readiness requires the exact expected entity not any authoritative
snapshot`. Do not capture it as established knowledge during Plan before the
negative control exists.

## Convention check

No loaded convention conflicts with this plan. The change uses the existing
public Hub client, `TuiApp`, entity reducer, generic fixture, renderer, and
repository scripts. It adds no dependency, abstraction beyond a small repeated
test helper, product policy, polling path, or compatibility branch.
