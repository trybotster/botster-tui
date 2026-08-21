# TUI remove Project Pipelines coupling from the generic package-event client plan

## Delivery identity

- Ticket: `ticket_1787278327_199618` — TUI: remove Project Pipelines coupling
  from the generic package-event client
- Target repository: `botster-tui` (`trybotster/botster-tui`)
- Target ID: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Base: `origin/main` at `0032fe97c76bcaccb09e540247106a9a998c23c6`
- Pipeline run: `run_1787278336_152073`
- Repository charter: [[botster-tui-playbook]]

The ticket target was resolved through `project_pipelines_current_context` and
the Hub spawn-target registry. The ambient worktree was not used to infer
ownership.

## Plan verdict

**This ticket is blocked before Implement.** No current Hub or package contract
can supply a generic notice reaction to a client. Human answer
`question_1787278563_302595` selected the strict reading and directed this run
to register shared cross-repository dependencies and park.

No production TUI code changes in this run.

## Context loaded

Role and repository guidance:

- [[planner-playbook]] — generic Plan role contract.
- [[botster-planner-playbook]] — Botster Plan overlay.
- [[botster-tui-playbook]] — repository ownership charter for `botster-tui`.

Targeted atomic notes:

- [[event plane client proof uses library contract fixtures]]
- [[current shared session client lanes do not prove package events]]
- [[TUI transient notices use run only fail closed matching]]
- [[question opened clients subscribe with empty subjects]]
- [[client filter tiers require reachable view state]]
- [[package event contracts live on HubPackageManifest not Core PackageManifest]]
- [[botster packages should enforce core hub cli plugin provider boundaries]]
- [[botster hub is a first party host profile over core]]
- [[botster workspace records are plugin owned references not hub authority]]
- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[optional always on entity families back off admission retries independently]]
- [[live context fields must belong to live published entity families]]

[[project-pipelines-playbook]] was not loaded. No Project Pipelines package or
plugin path is in scope for this repository-owned change.

[[botster runtime teardown lenses]] was not loaded. See the runtime-teardown
class section below.

Repository context read:

- `crates/botster-tui/src/app.rs` lines 99-104, 157-162, 2270-2292, 3937-4175
- `crates/botster-tui/src/entity_options.rs`
- `crates/botster-tui/Cargo.toml`, `crates/botster-tui/src/main.rs`, `test.sh`
- `docs/plans` and `docs/reports` placement conventions

Pinned dependency contracts read:

- `botster-hub` rev `b3b54f1f87e29867da4eb371e9b7f3b18160996a`:
  `src/packages.rs`, `src/client_api.rs`,
  `crates/botster-hub-client/src/lib.rs`
- `botster-project-pipelines` at `cd7c2f9`: `botster-package.json`

## The coupling that exists today

`crates/botster-tui/src/app.rs` carries Project Pipelines policy in generic
production code:

| Location | Coupling |
| --- | --- |
| `app.rs:99` | `QUESTION_OPENED_OWNER = "project-pipelines"` |
| `app.rs:100` | `QUESTION_OPENED_NAME = "question.opened"` |
| `app.rs:101-104` | `WORKFLOW_CONTEXT_ENTITY_FAMILIES` names two Project Pipelines entity families |
| `app.rs:3967-4009` | `subscribe_question_opened_events` hardcodes owner, name, and `subjects: []` |
| `app.rs:4037-4073` | `handle_package_event` reads `notice`, `question_id`, `kind`, `run_id` payload fields |
| `app.rs:4075-4086` | `handle_event_gap` writes a `question.opened` error string |
| `app.rs:4087-4132` | `package_event_matches_active_run` and `active_workflow_run` join `project-pipelines.session_request` rows |
| `app.rs:4134-4161` | `open_question_count_for_active_run` counts `project-pipelines.question` rows |
| `app.rs:4163-4173` | `question_attention_band` renders that durable count |

## Contract evidence for the blocker

Verified against the exact revisions this repository pins.

1. Hub declares package event contracts on `HubPackageManifest.events.emitted`
   (`botster-hub` `src/packages.rs:80-112`). `HubEmittedEvent` carries only
   `name`, `payload_schema`, `audience`, and optional `owner`.
2. That declaration never reaches a client. `HubClientPackage`
   (`src/client_api.rs:1381-1396`) has no `events` field, and `DaemonPackage`
   (`crates/botster-hub-client/src/lib.rs:1916-1941`) has no `events` field.
   No `DaemonRequest` variant returns event declarations.
3. `botster-hub` `main` is at the same commit `b3b54f1` that this repository
   pins. The gap is not a stale-pin artifact.
4. Even if `events.emitted` were projected to clients, it carries no client
   reaction semantics: no validated text pointer, no scope rule, no TTL, and no
   severity.
5. The Project Pipelines `question.opened` payload schema declares
   `additionalProperties: false` and no `subject` property
   (`botster-package.json`). Hub subject filters compare only
   `payload.subject`, so a subject-scoped subscription receives nothing until
   the package emits a subject.

A generic client therefore cannot learn what to subscribe to, how to scope it,
or how to present it. The ticket instructs this run to stop and register the
missing public seam rather than implement around it.

## Human decision

`question_1787278563_302595` asked whether a thin binary composition root in
`crates/botster-tui/src/main.rs` could supply the Project Pipelines reaction.
The answer selected the strict reading and added scope constraints:

- Production TUI source, including `main.rs`, must not name Project Pipelines.
  A thin binary composition root would only relocate the ownership violation.
- Use the same shared dependencies that the Web run is registering through
  `question_1787278509_823001`. Do not create duplicate Hub or Project
  Pipelines tickets.
- The Hub dependency owns a package-declared, client-visible, generic notice
  reaction in the canonical UI/client contract and the `DaemonPackage`
  projection.
- The Project Pipelines dependency declares that reaction and emits
  `payload.subject` as the active agent session uuid when available.
- The TUI subscribes with its current session subject.
- The generic reaction descriptor must not carry Project Pipelines entity
  families, entity joins, durable-count families, or arbitrary correlation
  rules. It covers only bounded transient presentation: exact package event
  name, session subject scope, validated text pointer, TTL, and severity.
- Durable question and attention UI stays package-owned through package
  surfaces or entity-backed package UI. The TUI removes the Project Pipelines
  durable attention logic rather than generalizing it.
- Register the shared dependencies and park before Implement.

## Scope

In scope for this Plan step:

- Record the contract blocker with exact revisions and line references.
- Register the shared Hub and Project Pipelines dependencies against this
  ticket, reusing the tickets the Web run registers.
- Specify the deferred TUI implementation precisely enough that Implement can
  start without re-deriving the design.

In scope for the deferred TUI implementation, after both dependencies merge:

- Roll the `botster-hub` pin to the revision that ships the client-visible
  notice reaction descriptor.
- Drive subscription, scoping, presentation, TTL, and severity from the
  descriptor read through the public client contract.
- Keep the generic mechanisms already present: the
  `Idle`/`Candidate`/`Active` subscription state machine, parked multiplexed
  frame replay after `EventSubscribed`, exact owner-plus-name admission,
  `EventGap` handling, reconnect state clearing, and bounded notice lifetime.
- Subscribe with the TUI's current session subject instead of `subjects: []`.
- Delete the Project Pipelines owner, event name, payload field names, and
  entity-family constants from production source.
- Delete `WORKFLOW_CONTEXT_ENTITY_FAMILIES`, `workflow_context_entity_families`,
  `ActiveWorkflowRun`, `active_workflow_run`, `package_event_matches_active_run`,
  `ambiguous_workflow_context`, `open_question_count_for_active_run`, and
  `question_attention_band`, together with the always-on subscription and
  backoff plumbing that exists only to serve them.
- Replace the Project Pipelines client unit tests with neutral contract
  fixtures that enter through the public protocol decode boundary.

Explicitly out of scope:

- Any production TUI code change in this run.
- A TUI-local package registry or any second event protocol.
- Generalizing the durable question count or attention band into the generic
  descriptor.
- Policy in `botster-tui-kit`. The kit stays policy-free.
- Changing terminal, attach, session, or Workspaces behavior.
- Implementing the Hub descriptor or the Project Pipelines declaration. Those
  belong to their own repositories.

## Repository ownership boundaries and cross-repository dependencies

Ownership:

- `botster-tui` owns client subscription behavior, filtering, gap reaction,
  reconnect behavior, and notice rendering. It does not own event contracts.
- `botster-hub` owns package event admission policy and the client-visible
  contract projection. The descriptor is a Hub-owned public seam.
- `botster-project-pipelines` owns its emitted event contract, its declared
  reaction, its subject value, and its durable question and attention UI.
- `botster-tui-kit` stays policy-free and gains nothing from this work.

Dependencies registered against this ticket
(`dependency_1787278750_977041` and `dependency_1787278755_379534`):

1. `ticket_1787278643_145174` — botster-hub
   (`tgt_7e208a0c76a44980a83b63af976b1f22`): "Hub: publish a package-owned
   client notice reaction descriptor in the canonical UI/client contract".
   It defines the descriptor in `@trybotster/ui-contract`, admits it on
   `HubPackageManifest` beside `events.emitted`, projects it onto
   `DaemonPackage`, publishes the DTO through the generated daemon protocol and
   `@trybotster/hub-test-support` metadata, and supplies a Hub-owned fixture
   package that exercises the public ABI with no product plugin.
2. `ticket_1787278658_151737` — botster-project-pipelines
   (`tgt_a72ca1a83d504385b8648f71409119ab`): "Project Pipelines: declare the
   question.opened notice reaction and emit the session subject". It declares
   one reaction for `question.opened` and emits `payload.subject` as the active
   agent session uuid when that context exists, and omits the subject
   otherwise.

Both tickets already existed. The Web run registered them from human decision
`question_1787278509_823001` on `ticket_1787278327_274484`. This run reuses them
and created no duplicate Hub or Project Pipelines ticket, as directed.

Ordering: `ticket_1787278643_145174`, then `ticket_1787278658_151737`, then this
TUI ticket. `ticket_1787278658_151737` already declares its own dependency on
the Hub ticket. A subject-scoped subscription receives nothing until the package
emits a subject, so the TUI change cannot land first without dropping live
notice behavior.

`botster-web` `ticket_1787278327_274484` is the sibling consumer of the same two
dependencies. The two client repositories must consume one contract with no
client-specific variant. That is an explicit acceptance line on the Hub ticket.

## Assumptions and unknowns

Assumptions:

1. The Hub descriptor exposes exact `owner` and `name` derived from the
   admitted package identity, a standard session subject scope, a validated
   text pointer such as `/notice`, and bounded presentation fields such as TTL
   and severity. `ticket_1787278643_145174` states these fields directly.
2. The descriptor reaches the TUI as a field on `DaemonPackage`, through the
   existing `ListPackages` or `ShowPackage` response path. The Hub ticket
   states the `DaemonPackage` projection, so this repository needs no new
   transport and no new request vocabulary.
3. Project Pipelines keeps `question.opened` as the emitted event and adds
   `subject` to its payload schema. Its current schema sets
   `additionalProperties: false`, so the schema change is required, not
   optional.
4. Direct merge into `main`. No pull request. This matches the ticket.

Unknowns for the Implement step to resolve against the merged Hub contract:

1. The exact descriptor type name, field names, and whether severity is an
   enumeration or a string.
2. Which TUI session identity is the subscription subject. Project Pipelines
   emits the active agent session uuid. The TUI holds `selected_session`, and
   the Hub session identity for a spawned agent session is the same uuid, so
   `selected_session` is the likely subject. Implement must confirm this
   against the merged contract and must not guess. If it is wrong, the
   subscription silently receives nothing.
   Related: the TUI must also decide what to do when no session is focused.
   Project Pipelines omits the subject when no agent session context exists, so
   a subject-less subscriber receives no session-scoped notice by design.
3. Whether the descriptor permits more than one reaction for one package, and
   how the TUI bounds the number of active subscriptions if it does.
4. Whether the Hub roll also moves the Core pin. If it does, the charter
   requires a separate production build gate and README pin prose updates.
5. Whether Project Pipelines republishes the durable open-question count
   through a package surface. `ticket_1787278658_151737` keeps durable question
   state as package-owned entity or surface state, but does not commit to a
   TUI-visible surface. Until it does, removing `question_attention_band`
   removes that count from the TUI.

## Affected surfaces and files

This run:

- `docs/plans/tui-remove-project-pipelines-coupling-from-generic-package-event-client-plan.md`
  (new, this plan).

Deferred implementation:

- `crates/botster-tui/src/app.rs` — constants at lines 99-104, the helper at
  157-162, the connect path at 2289, the event client at 3937-4173, and the
  Project Pipelines unit tests near lines 28750-29500.
- `crates/botster-tui/src/entity_options.rs` — only if the always-on family
  plumbing becomes unreachable after the durable attention logic is removed.
- `crates/botster-tui/Cargo.toml` — Hub pin roll.
- `Cargo.lock` — pin roll.
- `README.md` — pin prose and any documented notice behavior.
- `crates/botster-tui/src/acceptance.rs` — only if a live lane names the
  removed behavior.

## Risks

1. **Live behavior regression.** Parking this ticket keeps the current coupled
   notice working, so this run carries no regression. The deferred change does:
   if the Project Pipelines declaration or subject emission is wrong, the TUI
   shows no notice at all. Acceptance must prove a live notice, not only that
   the subscription was accepted.
2. **Durable attention loss.** Removing `question_attention_band` removes the
   open-question count from the TUI until Project Pipelines republishes it
   through a package surface. This is the directed outcome, not an accident,
   and Implement must state it plainly in the report.
3. **Scope creep into a generic correlation engine.** The descriptor is bounded
   transient presentation only. Any entity join, durable count, or correlation
   rule added to it re-creates the coupling in generic clothing. Plan Review
   should reject such a descriptor.
4. **Subject filter silently empty.** A non-empty `subjects` list rejects
   events whose payload omits `subject`. Ordering the merges Hub, then Project
   Pipelines, then TUI is what prevents a silent dead subscription.
5. **Divergent client contracts.** Two client repositories consume the same
   seam. If `botster-web` and `botster-tui` land different readings of the
   descriptor, the Hub ticket's "one contract with no client-specific variant"
   acceptance line fails after both merge. Implement should re-read the merged
   Hub contract and the Web consumer rather than the plan text.
6. **Pin roll side effects.** Rolling the Hub pin can move the conformance
   floor or the Core pin. The charter requires searching for old revisions,
   updating Ghostty live-lane defaults and README pin claims in the same
   commit, and running `cargo build -p botster-tui --locked` as a separate
   production gate.

## Acceptance checks and tests

For this Plan step:

- The plan artifact records the target repository, target id, charter,
  contract evidence with exact revisions and line references, scope, ownership
  boundaries, dependencies, assumptions, risks, and acceptance checks.
- Both shared dependencies are registered against `ticket_1787278327_199618`
  and are visible through `project_pipelines_list_ticket_dependencies`.
- No production TUI file changed. Proved by `git status` showing only the new
  plan document.
- Worktree hygiene: tracked `.gitignore` is intact at 73 bytes, and the
  worktree path contains no `:`, so no `CARGO_TARGET_DIR` override is needed.
  `test.sh` already sets a colon-free `CARGO_TARGET_DIR`.

For the deferred implementation, after both dependencies merge:

- `grep -rn "project-pipelines\|project_pipelines\|question.opened"` over
  production TUI source returns no hit. Test-only optional conformance code may
  remain.
- A neutral contract fixture proves subscribe, receive, filter, gap, reconnect,
  and notice rendering using a non-product owner and event name.
- The fixture enters through the public protocol decode boundary. It must not
  inject a decoded `DaemonEvent` after decoding. Assert the decode entry point
  explicitly, because the current tests call `apply_mux_event` directly.
- A negative fixture proves an event for a foreign owner, a foreign event name,
  a stale subscription id, or a foreign subject changes nothing.
- A fixture proves the notice expires at the declared TTL and that a newer
  matching event replaces the current notice.
- A fixture proves `EventGap` clears the visible notice and performs no
  durable write and no request.
- A fixture proves reconnect clears notice and subscription state and does not
  replay past events.
- The optional Project Pipelines conformance lane
  (`package_events_live_runtime_runs_against_isolated_hub_when_binaries_are_available`)
  may stay, but only if it does not enter production composition.
- Downstream proof required by the charter: an isolated-Hub live lane shows one
  real notice from a real producer. Soft residual evidence is not accepted. The
  shared Ghostty lanes stay terminal-only, per
  [[current shared session client lanes do not prove package events]].
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- `cargo build -p botster-tui --locked` as a separate production build gate
  after the pin roll. `cargo test --no-run` is not production build evidence.
- `./test.sh` passes with one result tally and zero failures.

## Runtime-teardown class

`teardown_class_applies`: **false**.

This ticket changes where a client event reaction is declared and how a
transient notice is scoped. It does not touch WebRTC or peer lifecycle,
`SessionIo` or `ClientWorker` teardown, multi-peer ownership, CPU, battery, or
file-descriptor spin, or terminal-state versus live-runtime divergence. The
subscription lifecycle it does touch is a bounded control-plane subscription on
the existing multiplexed host-control connection, and this run changes none of
it. [[botster runtime teardown lenses]] was therefore not loaded, per the
instruction not to apply it to ordinary client tickets.

## Vault gaps worth capturing

1. **Client notice reactions belong to a package declaration, not to client
   constants.** Two independent human decisions now agree:
   `question_1787278509_823001` on the Web ticket and
   `question_1787278563_302595` on this ticket. A thin binary composition root
   was rejected explicitly, because it relocates the ownership violation rather
   than removing it. No current note states this.
2. **A generic client reaction descriptor is bounded transient presentation
   only.** The explicit exclusion of entity families, entity joins, durable
   counts, and correlation rules is the load-bearing constraint, and it is the
   most likely thing a later planner re-adds.
3. **Superseded TUI notes.** Once the deferred change lands, three current
   notes become historical for the TUI:
   [[TUI transient notices use run only fail closed matching]] (run-only
   fail-closed matching is replaced by session subject scope),
   [[question opened clients subscribe with empty subjects]] (the TUI will
   subscribe with a non-empty subject), and
   [[client filter tiers require reachable view state]] (the reachability
   analysis assumed a client-side entity join). Capture the supersession when
   the change merges, not before.
4. **Hub package event declarations stop at the Hub boundary.** That
   `HubPackageManifest.events.emitted` has no client projection at `b3b54f1` is
   a non-obvious gotcha that cost this Plan step a full contract trace.
