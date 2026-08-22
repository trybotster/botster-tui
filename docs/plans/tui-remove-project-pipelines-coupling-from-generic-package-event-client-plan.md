# TUI remove Project Pipelines coupling from the generic package-event client plan

## Delivery identity

- Ticket: `ticket_1787278327_199618` — TUI: remove Project Pipelines coupling
  from the generic package-event client
- Target repository: `botster-tui` (`trybotster/botster-tui`)
- Target ID: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Base: `origin/main` at `0032fe97c76bcaccb09e540247106a9a998c23c6`
- Pipeline run: `run_1787278336_152073`
- Repository charter: [[botster-tui-playbook]]
- Revision: 3, after Plan Review `review_1787369259_130753`

The ticket target was resolved through `project_pipelines_current_context` and
the Hub spawn-target registry. The ambient worktree was not used to infer
ownership.

## Plan verdict

**Revision 3, after Plan Review `review_1787369259_130753` returned
changes_required.** Revision 2 cleared the pin, charter, artifact, and
subject-oracle findings. Revision 3 answers five new ones, including a blocker
where revision 2's ownership scan would have checked 38 of 30,499 lines of
`app.rs`.

Findings answered in revision 3:

| Finding | Severity | Answered in |
| --- | --- | --- |
| `finding_1787369259_563731` ownership scan checks only the first 38 lines | blocker | Acceptance checks, Ownership scan |
| `finding_1787369259_898279` multi-descriptor runtime not designed or bounded | high | Multi-descriptor subscription runtime, Acceptance checks |
| `finding_1787369259_850438` mandatory live lane reintroduces a product dependency | high | Acceptance checks, Live product proof |
| `finding_1787369259_541565` Hub revision unspecified | medium | Scope, Remaining assumptions |
| `finding_1787369259_336200` stale dependency edge id | info, process | Dependencies |

Revision 2 context, retained: All three registered dependencies are now closed, so the
seam this plan waited for exists. This revision answers every open finding and
replaces the parked verdict.

Revision 1 recorded the original blocker: no Hub or package contract could
supply a generic notice reaction to a client. Human answer
`question_1787278563_302595` selected the strict reading and directed this run
to register shared cross-repository dependencies and park. That park is now
released.

Findings answered in this revision:

| Finding | Severity | Answered in |
| --- | --- | --- |
| `finding_1787349566_641878` UI-contract tag not consumable | blocker | Dependencies, Scope, Affected surfaces, Risks, Acceptance |
| `finding_1787349566_501388` Project Pipelines charter omitted | medium | Context loaded, Ownership boundaries |
| `finding_1787349566_857519` Two acceptance checks lack executable boundaries | medium | Acceptance checks |
| `finding_1787349566_155433` Plan evidence omitted artifact_id | info, process | Gate evidence carries `artifact_id` on both submit_gate and request_step_advance |

No production TUI code changes in this run. This run still produces a plan
only. Implement performs the pin roll and the code change.

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

[[project-pipelines-playbook]] **is loaded** in this revision, per
`finding_1787349566_501388`. Revision 1 excluded it on the reasoning that no
Project Pipelines package path changes. That reasoning was wrong. This plan
removes Project Pipelines durable attention policy from the TUI, changes
`question.opened` targeting, and depends on package-owned emission behavior, so
the charter applies.

Applicable rules from that charter:

- Project Pipelines owns durable workflow policy, package-owned entity frames,
  surfaces, and its emitted event contracts. The TUI owns none of that.
- [[question opened notices target the agent session subject]] is the current
  targeting rule. It supersedes [[question opened clients subscribe with empty subjects]].
- [[TUI transient notices use run only fail closed matching]] is recorded in the
  charter as the superseded product policy that session-subject targeting
  replaces. The human override in `question_1787278563_302595` and
  `question_1787278509_823001` is the decision of record.
- [[each acceptance condition names its authoritative production oracle]]
  governs the acceptance checks below.
- [[cross repo dependency registration must use dependency repo target]]
  governs the dependency edges below.
- [[a transient package event cannot be the sole authority for a durable close]]
  keeps durable question state package-owned, which is why the TUI deletes its
  durable attention logic instead of generalizing it.

Additional notes loaded in this revision:

- [[question opened notices target the agent session subject]]
- [[published package owned notice reaction cutover is ui contract 0 3 3 and hub test support 0 1 41]]
- [[each acceptance condition names its authoritative production oracle]]
- [[Package-event subject filters are exact strings compiled at admission]]

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

## Contract evidence for the original blocker (historical)

This section records why revision 1 parked. The seam it proves absent now
exists. Keep it, because it is the justification for the three dependency edges
and for the cost of this ticket.

Verified against the revisions this repository pinned at revision 1.

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

- Roll all four Git pins together, per `finding_1787349566_641878`:
  - `botster-hub-client` and `botster-hub-test-support` from rev
    `b3b54f1f87e29867da4eb371e9b7f3b18160996a` to the exact reviewed revision
    `baeb04dcb4a11de4c3932d16bf09a8e5ff6ba4b5`. This is a named pin, not "the
    revision that carries the descriptor". A later unrelated Hub roll would make
    the Core-pin, protocol, conformance-floor, README, and live-lane scope
    indeterminate. If Implement finds that revision unusable, it must return the
    plan rather than pick a newer one silently.
  - `botster-ui-contract` from tag `botster-ui-contract-v0.3.2` to tag
    `botster-ui-contract-v0.3.3`. This direct roll is required. Hub merge
    `12e0cc6` moved `botster-hub-client` onto the v0.3.3 tag, so leaving the
    TUI's direct pin at v0.3.2 would put two `botster-ui-contract` sources in
    one graph.
  - Update `Cargo.lock` in the same commit.
- Read descriptors from `DaemonPackage.notice_reactions` and maintain one
  bounded per-descriptor subscription collection. The design is specified in
  "Multi-descriptor subscription runtime" below; Implement does not derive it.
- Resolve notice text through `botster_ui_contract::resolve_notice_text` with the
  descriptor's `text_pointer`. The TUI must not parse the pointer itself and must
  not read a payload field by name.
- Drive subscription, scoping, presentation, TTL, and severity from the
  descriptor. Use the descriptor's `ttl_ms` instead of the local
  `TRANSIENT_NOTICE_TTL` constant.
- Keep the generic mechanisms already present: the
  `Idle`/`Candidate`/`Active` state vocabulary, parked multiplexed frame replay
  after `EventSubscribed`, exact owner-plus-name admission, `EventGap` handling,
  reconnect state clearing, and bounded notice lifetime. The single
  `EventSubscriptionState` field at `app.rs:1474` becomes one state per
  descriptor. The state vocabulary itself does not change.
- Subscribe with the TUI's current session subject instead of `subjects: []`.
  Hub performs the subject match. The TUI must not read `payload.subject`.
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

## Multi-descriptor subscription runtime

Revision 2 said "subscribe once per descriptor" while keeping a singular
`EventSubscriptionState`. That is not a design, and Plan Review was right to
call it underplanned. This section is the design. Implement follows it.

Production today holds one `EventSubscriptionState` field (`app.rs:1474`) and
one subscription id. Descriptors are a set, so the state becomes a keyed
collection.

**Collection key.** `(owner, name)`. This is the only package-event
subscription key, per
[[exact owner plus name is the only package event subscription key]]. Hub
admission rejects duplicate reactions inside one package, and `owner` is the
admitted package name, so the pair is globally unique.

**Per-entry state.** For each key: the descriptor fields (`text_pointer`,
`ttl_ms`, `severity`), the `subject` the entry was minted for, and one
`Idle` / `Candidate(id)` / `Active(id)` state.

**Reverse index.** `subscription_id -> (owner, name)`. Inbound `PackageEvent`
and `EventGap` frames carry a subscription id, so the client must resolve id to
entry without scanning. This replaces the single-id equality checks at
`app.rs:4044` and `app.rs:4076`.

**Desired set.** One deterministic function computes it:

- If no session is focused, the desired set is empty.
- Otherwise, for every package in the `ListPackages` result and every entry in
  its `notice_reactions`, the desired entry is that descriptor with
  `subject` equal to the focused session id.
- Order deterministically by `(owner, name)`.

**No-focus policy, decided here.** With no focused session the TUI holds no
notice subscriptions. `subject_scope` is session-only, so with no session there
is no subject. Subscribing with an empty subject set instead would receive every
package's notices unfiltered, which is the pre-ticket behavior this work removes.
This follows from the merged contract, so it does not need a human question.

**Reconciliation.** One `sync_notice_subscriptions` function owns every
transition. It diffs desired against current:

| Case | Action |
| --- | --- |
| Key in current, absent from desired | Unsubscribe, drop parked frames for that id, remove the entry and its reverse-index row |
| Key in both, `subject` differs | Unsubscribe the old id, then subscribe a fresh id. Never mutate a live entry's subject in place |
| Key in both, same `subject`, descriptor fields differ | Keep the subscription and update `text_pointer`, `ttl_ms`, `severity` locally. Owner, name, and subject are unchanged, so the Hub-side filter is unchanged and a resubscribe would create a needless gap |
| Key in desired, absent from current | Subscribe a fresh id |

**Triggers.** `sync_notice_subscriptions` runs on connect, on package-list
refresh, on focus change including focus to none, and after reconnect. Reconnect
clears the collection and the reverse index first, then syncs, so no identifier
survives a connection.

**Focus A to B.** Handled by the subject-differs row: unsubscribe A's id before
minting B's. The old id leaves the reverse index at unsubscribe time, so an
in-flight A frame arriving afterwards resolves to no entry and drops.

**Late and stale responses.** Per-entry candidate promotion is unchanged. An
entry becomes `Active` only on `EventSubscribed`, then parked frames apply. A
response whose id is not that entry's current candidate is ignored and its
frames dropped through `drop_event_frames_for`. A `PackageEvent` or `EventGap`
whose id does not resolve to an `Active` entry drops silently. This generalizes
the existing single-subscription behavior rather than replacing it.

**Overflow policy.** Hub admits at most 64 package-event subscriptions per
connection (`MAX_SUBSCRIPTIONS_PER_CONNECTION`,
`src/daemon_event_subscriptions.rs:27`). The TUI subscribes in the deterministic
`(owner, name)` order and stops at 64. It must not silently truncate: it records
a bounded diagnostic naming how many descriptors it dropped. A silent cap would
read as full coverage while notices went missing.

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
(`dependency_1787278750_977041` and `dependency_1787349143_516346`):

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

3. `ticket_1787349524_364728` — botster-hub
   (`tgt_7e208a0c76a44980a83b63af976b1f22`): "Hub: publish the
   botster-ui-contract-v0.3.3 Git tag for Rust consumers". Plan Review created
   this ticket and registered `dependency_1787349531_741411` after finding that
   Hub merge `12e0cc6` moved `botster-hub-client` to
   `botster-ui-contract-v0.3.3` while no such remote tag existed.

The first two tickets already existed. The Web run registered them from human
decision `question_1787278509_823001` on `ticket_1787278327_274484`. This run
reuses them and created no duplicate Hub or Project Pipelines ticket, as
directed. Plan Review created and registered the third.

**All three dependencies are closed.** Verified state:

| Dependency | Ticket | Repository | Status |
| --- | --- | --- | --- |
| `dependency_1787278750_977041` | `ticket_1787278643_145174` | botster-hub | closed |
| `dependency_1787349143_516346` | `ticket_1787278658_151737` | botster-project-pipelines | closed |
| `dependency_1787349531_741411` | `ticket_1787349524_364728` | botster-hub | closed |

Ordering: `ticket_1787278643_145174`, then `ticket_1787278658_151737`, then this
TUI ticket. `ticket_1787278658_151737` already declares its own dependency on
the Hub ticket. A subject-scoped subscription receives nothing until the package
emits a subject, so the TUI change cannot land first without dropping live
notice behavior.

`botster-web` `ticket_1787278327_274484` is the sibling consumer of the same two
dependencies. The two client repositories must consume one contract with no
client-specific variant. That is an explicit acceptance line on the Hub ticket.

## Verified merged contract

Revision 1 listed these as assumptions. They are now verified facts, read from
the merged revisions. Implement must still re-verify at its own base.

Hub, `botster-hub` `origin/main` at `baeb04d`:

- `DaemonPackage.notice_reactions: Vec<PackageNoticeReactionDescriptor>`
  (`crates/botster-hub-client/src/lib.rs:1930`). This is the client projection
  that revision 1 proved absent.
- `PackageNoticeReactionDescriptor` (`crates/botster-ui-contract/src/notices.rs:42`)
  carries exactly `owner`, `name`, `subject_scope`, `text_pointer`, `ttl_ms`,
  and `severity`. It carries no entity family, no entity join, no durable-count
  family, and no correlation rule, so it holds the bound the human set.
- `subject_scope` is the single-variant enum `session`. `severity` is `info`,
  `warning`, or `error`.
- `resolve_notice_text` and `decode_notice_text_pointer`
  (`crates/botster-ui-contract/src/notices.rs:288` and `:242`) are the canonical
  text resolvers. `NOTICE_TEXT_MAX_BYTES` is 512. `NOTICE_TTL_MIN_MS` is 1000
  and `NOTICE_TTL_MAX_MS` is 60000.
- `PROTOCOL_VERSION` is 7 and `CONFORMANCE_FIXTURE_REVISION` is 46.
- `botster-hub-test-support` ships a neutral fixture package,
  `fixtures/plugin-contract-matrix`, that declares a session notice for event
  `contract.ready` with `text_pointer: "/notice"`. This is a product-free
  fixture, exactly what the ticket's neutral-fixture requirement needs.

Project Pipelines, `botster-project-pipelines` `origin/main` at `643c4d7`:

- `botster-package.json` declares one `events.notices` entry: name
  `question.opened`, `subject_scope: "session"`, `text_pointer: "/notice"`,
  `ttl_ms: 10000`, `severity: "warning"`. It omits `owner`, so Hub projects the
  admitted package name `project-pipelines`.
- The `question.opened` payload schema now declares optional string `subject`,
  maximum 128 bytes.
- `record_question` sets `payload.subject` to the current run step's
  `agent_session_uuid` after the durable question row commits. It omits the key
  when the question names no run, or when the run step carries no nonempty
  session uuid.

Resolved unknowns from revision 1:

1. Descriptor type name and fields. Resolved above.
2. Subscription subject identity. Resolved. The subject is the run step's
   `agent_session_uuid`, which is the Hub session identity the TUI already holds
   in `selected_session`. Both use the `sess-...` form. Implement must still
   assert this against a live event rather than trusting the shape.
3. Severity vocabulary. Resolved: `info`, `warning`, `error`.
4. Number of reactions per package. `notice_reactions` is a vector, and
   `validate_package_notice_reactions` rejects duplicate reactions. The TUI must
   handle more than one descriptor and must bound the number of active
   subscriptions.

## Remaining assumptions and unknowns

Assumptions:

1. Direct merge into `main`. No pull request. This matches the ticket.
2. The TUI keeps one host-control connection, so all descriptor subscriptions
   share the existing multiplexed path.

Unknowns for Implement to resolve at its own base:

1. Whether `MINIMUM_CONFORMANCE_FIXTURE_REVISION` in `app.rs` must change. The
   TUI floor is 44 and Hub `baeb04dcb4a11de4c3932d16bf09a8e5ff6ba4b5` reports
   46, so the floor still admits. Implement must confirm no fixture the TUI
   relies on moved between 44 and 46, rather than assuming the inequality is
   sufficient.
2. Whether Project Pipelines republishes the durable open-question count through
   a package surface. `ticket_1787278658_151737` keeps durable question state
   package-owned but does not commit to a TUI-visible surface. Until it does,
   removing `question_attention_band` removes that count from the TUI.

Resolved at revision 3, previously unknown:

- **The Core pin does not move.** Hub `baeb04dcb4a11de4c3932d16bf09a8e5ff6ba4b5`
  pins `botster-terminal-protocol` and, in `botster-hub-test-support`,
  `botster-core` and `botster-terminal-ghostty` at
  `7eafa470a18025895995bbedc20d34b58106a03b`. That is the revision the TUI
  already pins. This roll is Hub-only, so no Core-pin cascade applies.
- **The no-focus policy is decided in this plan**, not left to Implement. See
  the subscription design below.

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
- `crates/botster-tui/Cargo.toml` — `botster-hub-client` rev,
  `botster-hub-test-support` rev, and the direct `botster-ui-contract` tag roll
  from `botster-ui-contract-v0.3.2` to `botster-ui-contract-v0.3.3`.
- `Cargo.lock` — pin roll, one `botster-ui-contract` source only.
- `README.md` — pin prose and any documented notice behavior.
- `crates/botster-tui/src/acceptance.rs` — only if a live lane names the
  removed behavior.
- `crates/botster-tui/tests/` — new home for any optional Project Pipelines
  conformance lane moved out of `src`, alongside the existing
  `package_manifest_test.rs`.

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
7. **Split UI-contract graph.** This is the risk `finding_1787349566_641878`
   caught. The TUI pins `botster-ui-contract` directly by tag and also receives
   it transitively through `botster-hub-client`. Rolling only the Hub rev leaves
   the direct pin at v0.3.2 while the transitive path resolves v0.3.3. Cargo
   would then build two `botster-ui-contract` crates, and
   `PackageNoticeReactionDescriptor` from one would not satisfy the other. The
   symptom is a type mismatch on an identically named type, which reads as
   nonsense unless the split source is already suspected. The one-source proof
   below is what catches it.
8. **A scan that reports coverage it does not have.** Revision 2 shipped one.
   A range-limited or attribute-split scan over `app.rs` passes trivially while
   the coupling remains, and it is more dangerous than no scan because it reads
   as proof. The whole-directory rule exists to remove that class.
9. **Unbounded subscription growth.** Descriptors come from installed packages,
   so the desired set grows with package count. Without the 64 ceiling and the
   deterministic order, the TUI could exceed the Hub limit and lose notices in
   an order that varies per run.
10. **Client re-implementing payload policy.** If the TUI parses `text_pointer`
   itself or reads `payload.subject`, it recreates package payload policy inside
   the generic client and re-earns this ticket. The canonical resolvers and Hub
   subject filtering are the boundary.

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

For the implementation, now unblocked:

Each check names its authoritative production oracle, per
[[each acceptance condition names its authoritative production oracle]].

**Ownership scan. Oracle: the TUI repository source tree.**

Revision 2 proposed splitting each file at its first `#[cfg(test)]` attribute
and claimed `app.rs` had one boundary near line 11297. That was wrong, and
`finding_1787369259_563731` is correct. Measured on the current tree:

| File | `#[cfg(test)]` attributes | First at line |
| --- | --- | --- |
| `app.rs` | 70 | 39 |
| `acceptance.rs` | 5 | 37 |
| `renderer.rs` | 4 | 1 |
| `entity_options.rs` | 3 | 274 |

A first-marker split would have scanned 38 of 30,499 lines of `app.rs`. The
check would have passed while the entire coupling remained. It was worse than no
check, because it would have reported coverage it did not have.

Replacement rule, which cannot omit production items:

- Zero occurrences of `project-pipelines`, `project_pipelines`, or
  `question.opened` anywhere under `crates/botster-tui/src`. No line-range
  split, no attribute parsing, no exceptions inside `src`.
- Any optional Project Pipelines conformance test moves out of `src` and into
  `crates/botster-tui/tests/`, which already exists and holds
  `package_manifest_test.rs`. That directory is not production composition, so
  an optional product conformance lane there satisfies the ticket's "only if it
  does not affect production composition" condition.
- Implement adds this as a repository test so a later change cannot reintroduce
  the coupling silently.

This rule is strictly stronger than the ticket's acceptance line and needs no
item-aware scanner, so the plan takes it rather than building one.

**Generic client mechanism. Oracle: the TUI client, driven through the public
protocol decode boundary.**

- A neutral contract fixture proves subscribe, receive, filter, gap, reconnect,
  and notice rendering with a non-product owner and event name. Use the Hub-owned
  `plugin-contract-matrix` fixture identity, event `contract.ready`, which ships
  in `botster-hub-test-support`.
- The fixture enters through the public protocol decode boundary. It must not
  inject a decoded `DaemonEvent`. Current tests call `apply_mux_event` directly,
  so Implement must move the entry point to frame decode and assert it.
- A negative fixture proves a foreign owner, a foreign event name, or a stale
  subscription id changes nothing. These three are client-side gates and the
  client is their oracle.
- Notice text resolution goes through `resolve_notice_text`. A fixture proves the
  TUI surfaces a typed resolver error rather than rendering a notice when the
  pointer is missing, not a string, or oversized.
- A fixture proves the notice expires at the descriptor's `ttl_ms`, not at a
  local constant, and that a newer matching event replaces the current notice.
- A fixture proves `EventGap` clears the visible notice and performs no durable
  write and no request.
- A fixture proves reconnect clears notice and subscription state and does not
  replay past events.

**Subject filtering. Oracle: Hub admission, not the TUI.**

- `finding_1787349566_857519` is right that revision 1 left this ambiguous.
  Foreign-subject rejection is Hub behavior:
  [[Package-event subject filters are exact strings compiled at admission]]
  states Hub compares `payload.subject` against the admitted exact-match set.
- Therefore the foreign-subject proof belongs in the isolated-Hub lane, where a
  producer emits a subject the TUI did not subscribe to and the TUI receives no
  frame. A client-side "foreign subject changes nothing" unit test is rejected,
  because passing it would mean the client parses `payload.subject` and so
  recreates package payload policy.
- The TUI has no canonical client-side subject check in the merged contract. It
  supplies `subjects` at subscribe time and reads no subject field.

**Pin integrity. Oracle: Cargo resolution and the Git remote.**

- `git ls-remote --tags origin` on `botster-hub` lists
  `botster-ui-contract-v0.3.3`. Verified at plan time: the tag exists and
  resolves to `12e0cc6994be18024e4bdfffb22947526a652204`.
- One-source proof: `cargo tree -p botster-tui -i botster-ui-contract` reports
  exactly one `botster-ui-contract` source. More than one source fails the gate.
- `Cargo.lock` contains one `botster-ui-contract` entry.
- `cargo build -p botster-tui --locked` as a separate production build gate.
  `cargo test --no-run` is not production build evidence.
- Search for the old Hub revision and the old UI-contract tag across the
  repository. Update Ghostty live-lane defaults and README pin claims in the
  same commit.

**Live product proof. Oracle: an isolated Hub with the Hub-owned neutral
fixture.**

Revision 2 required the TUI live gate to call `project_pipelines_ask_human`.
`finding_1787369259_850438` is correct that this contradicts
[[event plane client proof uses library contract fixtures]], which assigns
generic Web and TUI conformance to Hub ABI fixtures and assigns `question.opened`
product proof to Project Pipelines. Revision 2 cited that note and then violated
it in the same document.

Corrected lanes:

- **Required generic lane.** An isolated Hub installs the Hub-owned
  `plugin-contract-matrix` fixture package, which declares a session notice for
  event `contract.ready` with `text_pointer: "/notice"`. The fixture emits, and
  the TUI shows one notice targeted by the session subject. This is the live gate.
  It has no product dependency.
- **Subject rejection, same lane.** The fixture emits with a subject the TUI did
  not subscribe to, and the TUI receives no frame. Hub is the oracle, as recorded
  above.
- **Project Pipelines product proof is separate and already closed.** Project
  Pipelines proves its own `question.opened` contract in its own repository at
  `643c4d7`, through `script/test` and `script/test-hub-flow`. This TUI ticket
  does not re-prove it and does not depend on it for conformance.
- **Optional Project Pipelines TUI lane.** May remain, outside generic
  conformance and outside production composition, under
  `crates/botster-tui/tests/`. It is not a gate.
- Shared Ghostty lanes stay terminal-only, per
  [[current shared session client lanes do not prove package events]].

**Subscription runtime proof. Oracle: the TUI client over the decode boundary.**

Covers the design section above, which revision 2 omitted entirely:

- Connect with two descriptors from two fixture packages produces two active
  subscriptions with distinct ids.
- Focus A to B unsubscribes A's id before subscribing B's, and a late A frame
  arriving after the switch changes nothing.
- Focus to none leaves zero notice subscriptions.
- A package refresh that changes only `ttl_ms` keeps the subscription id and
  changes the local lifetime, proving no needless resubscribe.
- A package refresh that removes a descriptor unsubscribes exactly that key.
- Reconnect clears the collection and reverse index, then re-syncs with fresh
  ids.
- A desired set above 64 subscribes exactly 64 in `(owner, name)` order and
  records the dropped count. A silent truncation fails the gate.

**Repository gates.**

- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
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

Revision 1 captured two inbox notes. Both are now published vault notes, along
with a third that revision 1 did not anticipate:

- [[client notice reactions belong to package declarations not client constants]]
- [[question opened notices target the agent session subject]]
- [[published package owned notice reaction cutover is ui contract 0 3 3 and hub test support 0 1 41]]

Remaining gaps:

0. **A production-ownership scan must be whole-directory, not marker-split.**
   Revision 2's first-marker rule covered 38 of 30,499 lines of `app.rs` because
   the file carries 70 `#[cfg(test)]` attributes. The durable rule is that
   test-code exclusion by line range or first-attribute split is not a valid
   ownership boundary in a repository that interleaves `cfg(test)` items through
   a large file. Move optional product code to `tests/` and scan the whole
   source directory instead. Capture this with the measured numbers.
1. **A direct Git tag pin and a transitive pin of the same crate must roll
   together.** `finding_1787349566_641878` found a split
   `botster-ui-contract` graph that no existing note covers. The general rule is
   broader than this ticket: any first-party Rust consumer that pins a crate
   directly and also receives it through a Hub crate must roll both, and must
   prove one source. This is worth a note, because the failure surfaces as a type
   mismatch on an identically named type.
2. **Supersession of the TUI run-filter notes.** When this change merges,
   [[TUI transient notices use run only fail closed matching]] becomes historical
   for the TUI, and the Project Pipelines example inside
   [[client filter tiers require reachable view state]] becomes historical. The
   charter already records the first as superseded product policy. Capture the
   TUI-side supersession at merge, not before.
3. **Hub package event declarations stopped at the Hub boundary before
   `12e0cc6`.** Already captured as an inbox note in revision 1. Confirm it
   survived vault processing, and mark it historical rather than deleting it,
   because it explains why three dependency tickets exist.
