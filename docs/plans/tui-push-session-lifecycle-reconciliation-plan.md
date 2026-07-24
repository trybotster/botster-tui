# Push Session Lifecycle Reconciliation Plan

Ticket: TUI: reconcile sessions from push lifecycle entity frames

## Target and context loaded

- Target repository: `botster-tui`.
- Target id: `tgt_c3d470bab78549df920a41e8fb0e58d8`.
- Pipeline run: `run_1784767761_688148`; Plan step:
  `botster_stack_plan`; run step: `run_step_1784767761_754534`.
- The authoritative target was resolved from `project_pipelines_current_context`
  and the admitted spawn-target registry, not inferred from the ambient path.
  The assigned worktree's `origin` is `trybotster/botster-tui`.
- Repository playbook: [[botster-tui-playbook]].
- Role and architecture playbooks/maps loaded in required order:
  [[planner-playbook]], [[botster-planner-playbook]],
  [[botster-architecture]], [[cli-patterns]], and [[spa-patterns]].
- Repository-boundary overlay loaded: [[botster-tui-kit-playbook]].
- Ticket-specific notes loaded:
  [[botster hub client state sync is entity frame only]],
  [[botster entity snapshots are authoritative reconnect baselines]],
  [[clients subscribe to entities not ptys]],
  [[botster tui attach must explicitly pull core entities after subscribing]],
  [[attach reconnect must drop stale outbound requests before resubscribe]],
  [[tui v2 entity frames must keep legacy lua state populated until layout migration finishes]],
  [[botster client subscriptions should not hydrate global state]],
  [[botster local client api lives over hubruntime not raw core routers]],
  [[tui and browser are equal clients]],
  [[botster tui consumes tui kit through a thin app policy adapter]],
  [[tui client attach uses hub protocol not session protocol]],
  [[tui and socket terminal streams use clientworker transport adapters]],
  [[external client hub tests use subprocess spawned hub test support]],
  [[shared conformance fixtures that contradict the core contract teach clients the wrong state machine]],
  [[botster web live-runtime session readiness can arrive as entity snapshot]], and
  [[runtime client acceptance must render delivered snapshots through real registry]].
- The Botster planner's Project Pipelines policy notes were loaded for workflow
  discipline. [[project-pipelines-playbook]] was not loaded because neither the
  Project Pipelines package/plugin nor its workflow policy is an implementation
  surface for this ticket.
- Repository guidance and prior art loaded from `README.md`, workspace/crate
  manifests, `test.sh`, `script/{fmt,test,clippy,test-live-hub}`, the current
  `app.rs` production path, and prior committed plans under `docs/plans/`.
- Planning baseline is clean at `09fca91`, equal to the worktree's
  `origin/main` at inspection time.

## Dependency and production-path evidence

- Registered dependency `ticket_1784752212_173295`, "Hub test support: publish
  session lifecycle subscription conformance", targets `botster-hub`
  (`tgt_7e208a0c76a44980a83b63af976b1f22`) and is closed.
- The dependency merged as botster-hub commit
  `02bffebd0e29cb69a8e1e639e01f704f6dfffe48` (PR #159). It
  publishes conformance fixture revision 16 and
  `@trybotster/hub-test-support@0.1.9`, and exposes the Rust helpers
  `session_lifecycle_subscription_conformance_scenario` and
  `run_session_lifecycle_subscription_conformance`.
- That merged hub-client revision exposes `DaemonEntitySubscription`,
  `subscribe_session_entities`, `DaemonEntityFrame`, and
  `FEATURE_SESSION_ENTITY_SUBSCRIPTIONS`. The frame vocabulary is authoritative
  `Snapshot`, `Upsert`, `Patch`, and `Remove`, scoped by `subscription_id` and
  ordered by `snapshot_seq`.
- Current TUI production flow is `run_loop -> TuiApp::poll_hub ->
  HubConnection::request`. `try_connect` calls `refresh_read_models`, which
  issues `ListSessions`; `spawn_session` inserts a running row locally and
  immediately attaches; `apply_response_state` replaces the session list from
  `Sessions` or `Spawned` responses. These are the exact legacy paths this
  ticket replaces.
- Terminal selection, attach hydration, input, resize, readback, and drain are
  already separate state in `TuiApp`. They remain on the public
  `botster-hub-client` request/event boundary.

## Scope

Botster layer touched: the first-party Rust TUI client and its downstream live
hub conformance proof.

1. Pin `botster-hub-client` and `botster-hub-test-support` to merged hub commit
   `02bffebd0e29cb69a8e1e639e01f704f6dfffe48`, update `Cargo.lock`, require
   `FEATURE_SESSION_ENTITY_SUBSCRIPTIONS`, and raise the minimum conformance
   fixture revision from 14 to 16.
2. Open one held-open `session` entity subscription after each successful hub
   connection. Feed its blocking reader into the app loop without blocking
   rendering or input, using the standard-library thread/channel primitives
   already available rather than a new runtime or dependency.
3. Add a small TUI-owned session reconciliation model in `app.rs` that:
   - accepts no delta until the active generation receives its authoritative
     snapshot;
   - replaces authoritative rows on every matching snapshot, including an
     overflow/resync snapshot and an empty snapshot;
   - applies matching-generation upsert, sparse patch, and remove frames only
     when `snapshot_seq` advances;
   - drops late frames from prior subscription ids/generations;
   - projects `DaemonSessionEntity` into the existing session row UI while
     preserving hub lifecycle and failure fields needed for visible state and
     attachability.
4. Keep immediate spawn feedback as a separate client-local pending overlay
   keyed by the requested session UUID. Add it before the spawn request. Remove
   it on a failed spawn response; replace it only when the authoritative
   snapshot/upsert contains that UUID. Do not synthesize a running authoritative
   row from the request response.
5. Make spawn, selection, and attachment distinct transitions. The spawn action
   no longer auto-attaches. Pending and authoritative rows can be selected, but
   only an authoritative attachable lifecycle row can initiate terminal attach.
6. Remove `ListSessions` from initial connection, manual refresh, reconnect,
   spawn, and normal synchronization. Remove response-driven session-list
   replacement. There is no polling or legacy fallback path.
7. On entity-subscription disconnect, invalidate that generation and reconnect
   through a fresh subscription id. Preserve no old generation's authority;
   reconcile from the new snapshot before applying its deltas or restoring a
   selected session's attach eligibility.
8. Extend the live headless path and isolated-hub test to exercise the app's
   actual subscription/reducer/render path, then consume the hub-owned runtime
   conformance runner as an independent shared-contract check.
9. Update `README.md` to describe push session synchronization, pending spawn
   feedback, distinct selection/attachment, reconnect generation rules, and the
   revision-16 live-hub proof.

## Non-scope

- No hub, hub-client, core, session-worker, or TUI-kit implementation changes;
  the required hub contract is already merged under its own target.
- No private socket framing, daemon-to-session-worker protocol reuse, or local
  duplicate of `DaemonEntityFrame`.
- No `ListSessions` compatibility fallback, periodic session refresh, dual
  session source, feature flag, or optional synchronization mode.
- No automatic terminal attachment after spawn, selection, reconnect, or
  authoritative appearance.
- No change to terminal attach/input/resize/drain/readback semantics, terminal
  renderer behavior, UiNode schema, mouse routing, packages, apps, plugin
  surfaces, or Project Pipelines policy.
- No speculative generic entity-store crate or reusable subscription framework.
  The smallest app-owned session reducer is sufficient for the one entity
  family this ticket asks the TUI to display.

## Ownership boundaries and cross-repository dependencies

- `botster-tui` owns connection policy, local pending feedback, session list
  reconciliation, selection, attachment policy, rendering, and the background
  adapter that delivers public hub-client frames to its app loop.
- `botster-hub-client` owns subscription helpers, DTOs, feature negotiation,
  wire framing, and subscription cleanup. The TUI consumes those APIs without
  copying them.
- `botster-hub` / CoreDaemon / session worker own authoritative lifecycle
  state, ordering, overflow/resync behavior, and disconnect cleanup. The TUI
  does not infer or repair server lifecycle transitions.
- `botster-tui-kit` continues to own reusable UiNode rendering and input
  mechanics. No kit change is required because reconciliation is TUI app
  policy, not a reusable renderer mechanism.
- The only cross-repository prerequisite is the already registered and closed
  botster-hub dependency above. Implementation must update immutable pins to
  its merge commit; it must not use an ambient sibling checkout, path override,
  or unmerged branch. No additional cross-repository dependency is planned.

## Assumptions and unknowns

- Assumption: the merged `02bffebd0e29cb69a8e1e639e01f704f6dfffe48` APIs and
  revision-16 fixture remain the
  dependency authority. If Cargo resolution shows different public signatures,
  Implementation must stop and reconcile against that exact commit rather than
  inventing a local adapter protocol.
- Assumption: `DaemonEntitySubscription` and its socket reader can run on a
  standard thread and deliver owned `DaemonEntityFrame` values through
  `std::sync::mpsc`; the implementation should confirm `Send` at compile time.
- Assumption: a `Spawned` response acknowledges the command but the entity
  snapshot/upsert is the only success transition that clears pending state.
  A response carrying `error` is an authoritative command failure and clears
  pending immediately.
- Assumption: hub entity `lifecycle: None` is non-attachable and should render
  from `registry_state`; only explicit `lifecycle == "running"` is attachable.
- The exact private shape of the reducer and subscription pump is left to the
  implementer. It must stay in `app.rs` unless size or ownership evidence
  demands a narrow sibling module; no service/framework abstraction is planned.
- Unknown to resolve in implementation tests: whether dropping a reader thread
  always closes promptly on reconnect. If not, use a bounded read timeout and
  generation cancellation; never block the UI loop waiting to join a stale
  reader.
- No part of the ticket is ignored or waived, and there are no multiple
  plausible product meanings requiring a human question.

## Affected surfaces and files

- `crates/botster-tui/Cargo.toml`
  - bump both hub-client and hub-test-support git revisions to
    `02bffebd0e29cb69a8e1e639e01f704f6dfffe48`.
- `Cargo.lock`
  - record only the expected hub dependency graph movement and audit unrelated
    changes.
- `crates/botster-tui/src/app.rs`
  - import the public entity subscription DTO/helper/feature;
  - own active subscription generation, frame receiver, sequence/baseline
    state, authoritative session rows, and pending spawn overlay;
  - reconcile frames and selection without coupling selection to attach;
  - remove `ListSessions` synchronization and spawned-response list mutation;
  - reconnect the subscription and ignore stale-generation frames;
  - update headless and unit/live-hub coverage.
- `README.md`
  - replace list-oriented session synchronization prose with the subscription,
    pending, selection/attach, and reconnect contract.
- `docs/plans/tui-push-session-lifecycle-reconciliation-plan.md`
  - this reviewable Plan-stage artifact.
- `script/test-live-hub`
  - expected unchanged: it already resolves/builds the pinned hub and session
    worker and runs the production headless test. Touch only if the published
    runner requires an additional explicit input that cannot be supplied by the
    existing harness.

## Implementation sequence

1. Update the hub git pins and lockfile, import the merged DTOs/helpers, require
   the subscription feature and fixture revision 16, and compile before
   changing behavior.
2. Introduce the app-local authoritative session reducer plus pending overlay.
   Unit-test it directly with the hub-owned normalized conformance scenario,
   including empty and overflow snapshots, ordered deltas, sparse patches,
   removal, and stale generation/sequence rejection.
3. Add the non-blocking subscription pump and lifecycle ownership to
   `TuiApp`. Start it after control connection readiness; drain frames in
   the production poll loop; make disconnect create a new generation and wait
   for its snapshot.
4. Remove `refresh_sessions`, `ListSessions` observations/tests, and
   `Sessions`/`Spawned` response reconciliation. Keep non-session status,
   package, app, navigation, terminal, and diagnostic requests unchanged.
5. Convert spawn to pending-first and manual attach. Update the production
   button label and headless flow so it waits for authoritative appearance,
   selects explicitly, then attaches explicitly.
6. Add production-path unit tests for empty baseline rendering, pending
   feedback, authoritative replacement/failure removal, external upsert,
   lifecycle patch, remove, selection stability, attach gating, disconnect,
   fresh snapshot, and stale-frame rejection.
7. Extend the isolated live-hub test to prove those same user-visible states
   through `TuiApp::poll_hub` and `TuiApp::surface`, and invoke
   `run_session_lifecycle_subscription_conformance` against the real isolated
   Hub/Core/session-worker topology.
8. Update README ownership/runtime documentation, run all repository gates, and
   audit the final diff so every changed line traces to session reconciliation,
   required dependency consumption, or necessary test/documentation updates.

## Risks and mitigations

- Blocking subscription reads could freeze the UI. Isolate them on a bounded,
  cancellable background reader and drain only ready frames in the 100 ms app
  loop.
- An old reader can outlive reconnect and deliver plausible late frames. Treat
  the generated subscription id as a generation token at the reducer boundary;
  no frame may mutate state unless it matches the active generation.
- Deltas before a baseline or out-of-order/duplicate sequences can corrupt the
  list. Gate deltas on a received snapshot and strictly advancing sequence;
  allow authoritative snapshots, including overflow snapshots, to replace the
  family state.
- Mixing pending and authoritative rows can create duplicates or false running
  state. Keep pending state separate and let only matching authoritative entity
  frames replace it.
- Reconnect can accidentally auto-attach a stale selection. Restore only the
  session read model; attachment remains a separate explicit user action.
- A narrow unit reducer can pass while the production loop remains on
  `ListSessions`. Add a source/request negative assertion plus live headless
  proof through the actual subscriber, reducer, surface, and manual attach
  path.
- The dependency bump may move transitive crates. Audit `Cargo.lock`, and reject
  unrelated upgrades rather than accepting lockfile churn wholesale.
- Live tests can pass against stale same-version hub artifacts. Keep
  `script/test-live-hub` on its explicit fresh hub target directory and pinned
  manifest resolution.

## Acceptance checks and downstream proof

Unit and production-path acceptance in `app.rs` must prove:

- an empty authoritative baseline renders no authoritative sessions;
- spawn renders a pending row before the request completes and does not attach;
- spawn error removes pending feedback, while snapshot/upsert replaces matching
  pending state with the authoritative row;
- a session created outside the TUI appears without Refresh or `ListSessions`;
- ordered resize/lifecycle patches update the visible row, an exited row is not
  attachable, and remove deletes it;
- duplicate/out-of-order sequences and prior-generation frames do not mutate
  the store;
- subscription disconnect creates a fresh id, waits for a fresh authoritative
  snapshot, and accepts its lower or restarted sequence as the new baseline;
- selection remains distinct from attachment and all existing terminal
  attach/input/resize/readback assertions continue to pass;
- neither production code nor test request logs retain `ListSessions` as a
  session synchronization fallback.

Live downstream proof must use the existing isolated hub builder and real
`botster-hub` plus `botster-session-worker` binaries. It must:

- invoke
  `botster_hub_test_support::run_session_lifecycle_subscription_conformance`
  and assert its revision-16 runtime report;
- drive the TUI app through empty snapshot, pending spawn, authoritative
  appearance, externally spawned visibility, lifecycle update, natural
  exit/removal, subscriber disconnect, and fresh reconnect snapshot;
- render the converged states through `TuiApp::surface()` and the production
  TUI renderer, not only inspect a standalone reducer;
- explicitly attach only after authoritative appearance, then preserve the
  existing terminal input/readback/reconnect evidence.

Repository commands:

```sh
script/fmt
script/test
script/clippy
CARGO_TARGET_DIR=/tmp/botster-tui-impl-target script/test-live-hub
cargo run -p botster-tui -- --smoke
```

The live command is required, not optional skip evidence. Any pre-existing
failure must be identified by exact command/output and shown unrelated; it is
not a blanket waiver.

## Pipeline gates and artifacts

- Plan artifact: this file under the repository's established `docs/plans/`
  hierarchy.
- Plan gate evidence must name the target/repository charter, loaded notes,
  scope/non-scope, ownership/dependency boundary, assumptions, files, risks,
  checks, and vault disposition.
- Implement evidence must include the hub pin/lockfile audit, unit and source
  negative assertions, live-hub command output, and runtime conformance report.
- Review/Verify must use the Botster runtime overlays because this changes
  client lifecycle and transport consumption. They must reject any hidden
  `ListSessions` fallback, unwired reducer, auto-attach regression, stale-frame
  acceptance, or fixture-only proof.

## Vault gaps worth capturing

- Existing notes cover entity-frame authority and reconnect snapshots, but not
  the precise client reducer rule "fresh subscription generation must receive a
  snapshot before deltas, while pending commands remain a separate overlay."
  If implementation confirms that pattern and exposes a reusable failure mode,
  capture it through the vault inbox pipeline after delivery.
- Existing test-support notes describe downstream subprocess proof but do not
  yet name the published revision-16 session lifecycle runner. Capture that
  durable helper/version relationship only if it remains useful beyond this
  ticket; do not write a direct vault note during Plan.
- No convention conflict was found. The plan follows cold-turkey migration,
  framework/library reuse, repository ownership, and real-runtime downstream
  proof.
