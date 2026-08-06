# TUI authoritative Hub maintenance contract repin plan

## Target and context

- Target repository: `botster-tui` (`git@github.com:trybotster/botster-tui.git`)
- Target id: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Ticket: `ticket_1785976581_841608` — "TUI: remove package compatibility hub_version
  fixture on Hub client bump"
- Project: `project_1785970196_204877` — Botster session types and Hub maintenance
  control plane
- Pipeline run: `run_1786031376_185747` (step `botster_stack_plan`, fifth pass;
  `review_1786032878_586317`, `review_1786033614_966035`,
  `review_1786035659_348398`, and `review_1786036343_780324` each returned
  `changes_required`)
- Base: `main` at `fe03a90` ("Improve TUI application shell"), fetched with
  `git fetch origin --prune`. The first pass planned against `a2ad3ff`; `fe03a90`
  landed afterwards and rewrote `README.md`, `crates/botster-tui/src/app.rs`, and
  `crates/botster-tui/src/renderer.rs` (710 insertions / 340 deletions) across the
  exact surfaces this plan cites. The plan branch was rebased onto `fe03a90`, the
  compile probe was re-run from that base, and every line reference below was
  re-derived from it.
- Baseline at `fe03a90` before any change: `script/test` passes
  (142 unit + 1 integration).
- Repository playbook: [[botster-tui-playbook]]
- Role and surface playbooks loaded: [[planner-playbook]],
  [[botster-planner-playbook]], [[botster-hub-client-playbook]],
  [[botster-tui-kit-playbook]], [[botster-runtime-reviewer-playbook]], and
  [[botster-runtime-verifier-playbook]].
- [[botster-planner-playbook]] Must Load set, loaded in full:
  - [[botster-architecture]] — Botster domain map; confirms the TUI is a client
    over Hub contracts and that protocol authority sits in `botster-hub`.
  - [[cli-patterns]] — Rust CLI/TUI/PTY constraints; governs the entity reducer,
    compatibility handshake, and Cargo gate expectations in this plan.
  - [[spa-patterns]] — React/Catalyst and entity-store frontend constraints.
    **Adds no task-specific constraint here:** this ticket touches no browser
    surface, and the generated-TypeScript consumer of the same Hub revision is
    botster-web `ticket_1785970234_234515`, not this run. Recorded because the
    overlay is required, not because it shaped a decision.
  - [[project pipeline orchestration belongs in a device-level botster plugin]],
    [[project pipelines needs an operator workbench not more primitives]], and
    [[project pipelines ui contract belongs in the plugin readme]] — applied via
    the Project Pipelines charter below.
- [[project-pipelines-playbook]] — **loaded, correcting a pass-4 omission.** The
  role contract requires this charter when Project Pipelines package/plugin paths
  **or workflow policy** are in scope. Passes 1-3 read only the first half and
  recorded "deliberately not loaded — no package or plugin path is in scope."
  That was defensible until pass 4, which added a section defining dependency-edge
  semantics, step-activation behavior, and a manual Implement transition gate —
  workflow policy, squarely. `finding_1786036343_932174` caught it. This is the
  same half-a-clause reading that produced the pass-1 `DaemonStatus.software`
  error, which is why it is recorded as a vault gap rather than a one-off.
  Directly applicable Must Load entries:
  - [[project pipeline step activation gates open ticket dependencies before side effects]]
    — **superseded, and that matters here.** See "How the charter bears on this
    plan" below.
  - [[vault convention notes can document unimplemented behavior as shipped]] —
    the note that superseded it, and the governing discipline for this situation.
  - [[plan review must fetch before trusting remote tracking refs in run worktrees]]
    and [[plan review must reverify the declared base at review time]] — why this
    plan re-fetches and re-declares its base every pass, and why entry-check
    step 4 exists.
  - [[plan review must verify unmerged unregistered ticket dependencies]] — the
    current planning-stage dependency rule, which explicitly does **not** imply
    runtime activation gating exists.
  - [[plan review must check open sibling tickets that own part of the plan scope]]
    — satisfied: `ticket_1785970234_132113` is the same-target sibling owning
    session-type UX, and the data-versus-presentation boundary against it is
    stated in scope and non-scope.
  - [[plan review must verify baseline test execution and register blocking dependencies]]
    — satisfied: the `fe03a90` baseline was executed first-hand (142 unit + 1
    integration) and the kit blocker is a registered edge, not a caveat.
  - Remaining Must Load entries ([[implement gate must verify committed work and pr link before review]],
    [[verify must recheck resolved findings against the live worktree]],
    [[project pipelines sqlite write locks require preserved verdicts and operator restart]],
    [[package owned pipeline reconciliation preserves device local agent selection]],
    [[project pipelines mcp create calls can time out after committing]],
    [[plugin mcp descriptors are the downstream agent contract]]) govern later
    pipeline steps or plugin-internal work and add no constraint to this Plan
    artifact.

### How the charter bears on this plan

The finding asked for a recorded "convention conflict" between the charter's
intended activation gating and the verified runtime. The accurate record is
narrower, and better:

**There is no live convention conflict, because the vault already retracted the
convention.** [[project pipeline step activation gates open ticket dependencies before side effects]]
is `type: drift`, `status: superseded`. Repository-wide verification on
2026-07-16 found no `dependency_ticket_ids` or `allows_open_ticket_dependencies`
implementation and its cited commit did not resolve; it was superseded by
[[vault convention notes can document unimplemented behavior as shipped]]. It must
not be loaded as a current runtime contract — the note says so itself.

So `question_1786034337_423424` is not a surprise contradicting vault guidance. It
is independent runtime confirmation, from a different direction, of what the vault
concluded from repository evidence months earlier. The two agree.

One nuance worth stating precisely: the enforcement mechanism is not merely
unimplemented. `ticket_1785989402_277498` ("Project Pipelines: enforce blocking
dependencies before agent-step activation") is **closed** in this same project, so
the gate exists in `botster-project-pipelines` — but it targets the standalone Hub,
which is not in service here and whose package is not installed by design. The gap
in this environment is therefore **deployment, not implementation**. That
distinction matters for anyone reading this plan later: "the gate does not exist"
would be wrong; "the running legacy plugin does not have it" is right.

What this plan owes the charter, then, is not a conflict waiver but honest
framing, which the fail-closed Implement entry check supplies: treat the edge as
advisory, verify the prerequisite artifact directly, and never state sequencing as
a guarantee. `question_1786034337_423424` authorises that cooperative check for
this run. No implementation scope changes.
  - [[botster orchestration should spawn agents with explicit target ids]] —
    applied directly: the cross-repo prerequisite was routed to the
    `botster-tui-kit` target id rather than this ticket's target.
  - [[botster orchestration prompts must bind agents to explicit worktrees]] —
    applied directly: this plan states its base commit and worktree explicitly,
    and the staleness this review caught is exactly the failure that note guards
    against.
  - [[botster pipeline needs continuous product owner between agent steps]] —
    applied: the product decision ledger for this run is
    `question_1786032001_762094`, `review_1786032878_586317`,
    `review_1786033614_966035`, `question_1786033784_534645`,
    `question_1786034337_423424`, `review_1786035659_348398`, and
    `review_1786036343_780324`.
- Atomic notes loaded: [[botster hub client crate is the external client boundary]],
  [[botster hub client compatibility descriptors belong in client crate]],
  [[daemon event shape changes bump conformance fixture revision not protocol version]],
  [[botster hub client state sync is entity frame only]],
  [[botster entity snapshots are authoritative reconnect baselines]],
  [[acceptance readiness requires the exact expected entity not any authoritative snapshot]],
  [[botster tui consumes tui kit through a thin app policy adapter]],
  [[tui and browser are equal clients]],
  [[cross repo dependency registration must use dependency repo target]],
  [[blocking dependency premises must be revalidated per consuming crate]],
  [[closed dependency tickets signal merged source not a consumable release]],
  [[hub generated protocol changes are a four site release chain]],
  [[stale project pipeline worktrees can miss merged dependency apis]],
  [[adding a hub client feature constant is a three site change]],
  [[external client hub tests use subprocess spawned hub test support]],
  [[live hub target dirs can cache stale same version client schema]],
  [[workspace struct field changes require workspace cargo gates]],
  [[test script required for rust tests not cargo test]],
  [[botster test sh forwards arguments to cargo not custom unit flags]],
  [[colon worktree paths break cargo dyld library paths]],
  [[botster pipeline reviewers must bypass rtk summaries for cargo gate evidence]],
  [[plan steps need reviewable plan artifacts]],
  [[project pipelines checklist worker timeouts require artifact evidence fallback]],
  [[vault example paths are not repository placement conventions]],
  [[plan agents must author vault context as wikilinks not home paths]], and
  [[pipeline vault checklists must cite exact resolvable note titles]].
- Repository context inspected: `README.md`, `Cargo.toml`, `Cargo.lock`,
  `crates/botster-tui/Cargo.toml`, `crates/botster-tui/src/app.rs`,
  `crates/botster-tui/src/renderer.rs`, `crates/botster-tui/tests/`,
  `botster-package.json`, `plugin.lua`, `script/fmt`, `script/clippy`,
  `script/test`, `script/test-live-hub`, `test.sh`, and the existing
  `docs/plans/` repin prior art.
- Dependency source inspected: `botster-hub` at `e8febabf` and `8a60bd58`
  (`crates/botster-hub-client/src/lib.rs`, `crates/botster-ui-contract/`,
  `crates/botster-hub-test-support/src/lib.rs`, `src/session_types.rs`), and
  `botster-tui-kit` at `76e2085` (`Cargo.toml`).
- Pipeline context inspected: ticket, run, gate, dependency edges, artifacts,
  findings, reviews, questions, and question answers through
  `project_pipelines_current_context`; plus the orchestrator's amendment message
  confirming the protocol 6 / conformance 31 target.
- Blocking question `question_1786032001_762094` was asked and answered before
  this plan was finalized. Its three decisions are carried inline below and
  summarized in "Decisions from `question_1786032001_762094`".

## Decisions from `question_1786032001_762094`

1. **`botster-tui-kit` prerequisite — orchestrator handled it.** `botster-tui-kit`
   (`tgt_3dfae49c02454037bf13554f552baf7f`) was added to
   `project_1785970196_204877`; `ticket_1786032168_294170` ("TUI Kit: repin
   botster-ui-contract to Hub 8a60bd58") was created, registered as a blocking
   dependency of `ticket_1785976581_841608`, and `run_1786032180_372466` started
   against it. **This run plans fully, completes Plan Review, then must not edit
   until `ticket_1786032168_294170` closes.** Waiting is correct behavior, not a
   failure. No vendoring, `[patch]`, or path override may be used to get past it.
   The orchestrator independently reproduced the byte-identical
   `botster-ui-contract` finding (0 lines changed for that path against
   61 files / +10685 / -2180 for the full range); if the kit run escalates
   anything non-mechanical, that assumption is wrong and this plan needs revisiting.

   **The hold is NOT engine-enforced** — see "Dependency enforcement is advisory
   in this environment" and the fail-closed Implement entry check that replaces
   the assumption.
2. **Forced adaptation confirmed in scope**, and session-type presentation,
   discovery, and CRUD confirmed out of scope. The six new `DaemonSessionEntity`
   fields are populated as data with zero UX built on them.
3. **Workspaces live lane: option (i).** Rename and rekey the repo fixture now;
   mark the `script/test-live-hub workspaces` lane blocked-pending-`ticket_1785984128_479155`;
   use the contract-matrix live-Hub lane as this run's live evidence. Option (ii)
   was rejected on dependency-chain grounds: `ticket_1785984128_479155` is itself
   blocked on both Web tickets (`ticket_1785970233_750553`,
   `ticket_1785970234_132113`), so blocking on it would serialize this small
   ticket behind the entire Web wave. This decision named the `installed-driver`
   lane specifically; decision 4 below extends it.

## Decision from `question_1786033784_534645`

4. **The Workspaces waiver covers all three profiles, explicitly granted.**
   `question_1786032001_762094` authorized only `installed-driver`. Investigation
   prompted by `finding_1786033614_940964` established that `plumbing` and
   `lifecycle` fail for the identical reason, so the orchestrator explicitly
   granted the wider waiver — installed-driver, plumbing, and lifecycle — under
   four conditions, all carried in scope item 6: attribute all three to
   `ticket_1785984128_479155` by ID at the guard site and state what is
   consequently unproven; state the root cause exactly once as a single problem;
   record it in `README.md` and the implementation report as well as the guard;
   and address `finding_1786033614_940964` explicitly rather than by assertion
   (see "Response to `finding_1786033614_940964`").

## Dependency enforcement is advisory in this environment

**Correction received in `question_1786034337_423424`, after the first three
passes of this plan were written.** This Hub runs the legacy Project Pipelines
plugin. The dependency-gating fix in botster-project-pipelines targets the new
standalone Hub, which is not in service here and whose package is not installed
by design. **Registered blocking dependency edges in this project are advisory
records only. Nothing enforces them.**

Consequences this plan must reflect, and did not before:

- `dependency_1786032176_724975` (this ticket depends on
  `ticket_1786032168_294170`) is a record, not a gate. Plan Review approval can
  activate Implement while the kit ticket is still open. This is not
  hypothetical: `run_1785987292_480836` on `ticket_1785984128_479155` was
  activated into Implement with two blocking dependencies open, and was
  subsequently cancelled.
- Earlier passes of this plan said the run "holds"/"waits" at Implement and cited
  formal registration as the mitigation. That framing presented an ordering
  *claim* as a *guarantee*. It is corrected throughout.
- The ordering requirement itself stands unchanged and is real: the kit revision
  must exist before it can be pinned. Only the enforcement mechanism was
  misdescribed — it is instruction and agent cooperation, backed by the
  fail-closed check below, not the engine.

### Fail-closed Implement entry check — run BEFORE any edit

Implement must perform all of the following before modifying a single file, and
must treat any failure as "stop, change nothing, report still waiting". The
prohibition on `[patch]`, vendoring, and path overrides applies with full force
here: those are exactly the workarounds an implementer reaches for when the
prerequisite is missing.

1. Call `project_pipelines_current_context` and
   `project_pipelines_get_ticket` for `ticket_1786032168_294170`. Require
   `status == "closed"`. Do not accept an in-flight run, an approved plan, or an
   orchestrator assertion as a substitute — per
   [[closed dependency tickets signal merged source not a consumable release]],
   only a merged, pinnable commit satisfies this.
2. Identify the exact `botster-tui-kit` commit that ticket produced, and verify it
   is **merged to the authoritative `trybotster/botster-tui-kit` repository** and
   reachable from its default branch — not merely present on a run worktree or a
   feature branch.
3. Read that commit's `Cargo.toml` and `Cargo.lock` and verify they resolve
   `botster-ui-contract` at Hub `8a60bd58841179f8b1fd4040d9362d18ea244230`.
   Verifying the ticket closed is not the same as verifying the artifact carries
   the pin.
4. `git fetch origin --prune` in this worktree and re-validate the base. If
   `origin/main` has advanced past `fe03a90`, rebase and re-derive every cited
   line reference before editing — see risk 9. This check and the kit checks
   happen at the same transition, because both facts can have moved while the run
   sat idle.

If any of 1–4 fails, Implement makes no edits, submits no gate, and reports the
run still waiting on `ticket_1786032168_294170`, naming which check failed.

Verification discipline generally: an orchestrator statement that a durable record
exists is not evidence that it exists. `question_1786034337_423424` arose exactly
because this plan was told `ticket_1785984128_479155` had been updated when it had
not, and the claim was checked rather than trusted. Every durable condition this
plan cites was verified by direct tool call:
`ticket_1785984128_479155` at `updated_at 1786035488` names all three Workspaces
profiles; `ticket_1785976581_841608` at `updated_at 1786033407` carries the full
forced-adaptation scope.

## Response to `finding_1786033614_940964`

The finding (severity high, from `review_1786033614_966035`) reads:

> **Blocked-lane guard disables two Workspaces profiles outside the authorized
> scope.** The amended ticket says the `script/test-live-hub workspaces
> installed-driver` lane is blocked pending ticket_1785984128_479155, and the
> answered human decision repeatedly refers to that singular lane. The revised
> plan instead inserts an unconditional guard in the top-level `workspaces)` case
> before profile dispatch and proposes the deterministic command
> `script/test-live-hub workspaces lifecycle`. That makes plumbing, lifecycle, and
> installed-driver all exit 1. Disabling two additional live-acceptance lanes is
> broader than the ticket and contradicts the smallest-surgical-change
> requirement; the plan supplies no explicit human waiver or independent evidence
> authorizing those extra gaps.
>
> *Suggested fix:* Move the blocked-pending guard into the `installed-driver`
> profile branch only. […] Preserve plumbing and lifecycle behavior. If those
> profiles also truly cannot run after the repin and must be disabled, ask the
> human explicitly before broadening the coverage waiver.

**Conceded on conduct.** Pass 2 placed the guard in the general `workspaces)`
case, sweeping in `plumbing` and `lifecycle`, when only `installed-driver` had
been authorized. That was done without checking whether the wider scope was even
warranted and without asking. Broadening a coverage waiver is the orchestrator's
decision, and taking it as a side effect of an implementation choice was wrong
independently of whether the wider scope turned out to be justified.

**Disputed on premise, with source evidence.** The suggested fix directs that
narrowing the guard would "preserve plumbing and lifecycle behavior." It would
not. Those lanes cannot run against Hub `8a60bd58` either:

- botster-workspaces `botster-package.json` declares
  `{ "surface": "session_actions", "scope": "session_template_managed_git_spawn" }`.
- botster-hub `src/profile.rs` `default_capability_grants()` grants
  `session_template_spawn` / `session_template_managed_git_spawn` at `e8febabf`
  (lines 192/196) but `session_type_spawn` / `session_type_managed_git_spawn` at
  `8a60bd58`. `grep -c session_template` against that file at `8a60bd58` returns
  `0` — cold cut, no aliases, as the project's migration posture requires.
- botster-hub `src/packages.rs`: an ungranted capability is only a diagnostic at
  install (line 2452, kind `ungranted_capability`), but `PackageRegistry::enable`
  **hard-denies** it, proven by the Hub's own tests
  `enable_succeeds_only_when_requested_capabilities_are_granted` (3661) and
  `enable_denies_ungranted_capability_scope` (3685).
- `workspaces_live_acceptance_runs_against_real_package` — the test backing
  **both** `plumbing` and `lifecycle`, while `installed-driver` runs the separate
  `installed_workspaces_spawn_driver_runs_through_apps_open` — issues
  `DaemonRequest::EnablePackage` and asserts `enabled.error.is_none()` at the
  `PackageEnabledAndReloaded` stage (`app.rs:13378-13392`). `WorkspacesProfile`
  has only `Plumbing` and `Lifecycle`.

Writing "preserves plumbing and lifecycle behavior" into a durable plan artifact
would therefore be recording a false statement. Rather than comply or re-submit
the same guard with better wording, the disproof was escalated in
`question_1786033784_534645`, which is what the finding's own suggested fix
directs for exactly this case.

**Resolution.** The orchestrator independently verified every step of the
disproof and explicitly granted option (a) — the wider waiver covering all three
profiles — under the four conditions carried in decision 4 and scope item 6. The
finding is answered by an explicit grant plus evidence, not by assertion, and
Plan Review should judge the substitution on that evidence.

**Consequence recorded, not softened.** The same evidence establishes something
larger than a test-lane waiver: because `enable` hard-denies rather than warns,
the installed Workspaces package cannot be enabled on a protocol-6 Hub at all —
on any real device, not only in test lanes. `ticket_1785984128_479155` is
therefore what makes Workspaces functional against the shipped Hub contract, not
merely what restores this repository's coverage. The orchestrator is raising that
as a product-priority matter separately. It does not change this run's scope, and
the guard message must not be softened toward "narrow testing artifact" framing.

## Dependency facts

Established by reading source at the exact revisions, plus a compile probe run in
this worktree and then fully reverted (`git status --porcelain` clean).

1. `botster-tui` currently pins `botster-hub-client`, `botster-ui-contract`, and
   `botster-hub-test-support` at botster-hub `e8febabf73259cfd922592346b244ec473c17323`.
   That revision is **protocol 4 / conformance 27**, not protocol 5 / 29. The
   ticket's superseded framing referenced a contract this repository never pinned.
2. The authoritative target `8a60bd58841179f8b1fd4040d9362d18ea244230` is
   botster-hub `main` HEAD and is **protocol 6 / conformance 31**
   (`PROTOCOL_VERSION = 6`, `CONFORMANCE_FIXTURE_REVISION = 31`).
3. `@trybotster/hub-test-support@0.1.24` is the npm release coordinate published
   from `8a60bd58`. `botster-tui` has no `package.json` and consumes the Rust
   `botster-hub-test-support` crate by git revision, so the npm coordinate is
   corroborating provenance for this run, not a consumed artifact. It is the
   botster-web consumption path (`ticket_1785970234_234515`).
4. `git diff e8febabf 8a60bd58 -- crates/botster-ui-contract` is **empty**. The
   UI contract source is byte-identical across the two revisions.
5. `botster-tui-kit` at its pinned revision `76e2085632f2da2f4423100cec85f23527373524`
   (which is kit `main` HEAD) pins `botster-ui-contract` at `e8febabf` in its
   workspace `Cargo.toml`.
6. `botster-hub-client` re-exports `botster-ui-contract` types through its public
   DTOs — `DaemonPluginSurface.body: UiNode`, `DaemonUiTreeSnapshot.body: UiNode`,
   `UiActionRequest`, `UiActionResult`. `botster-tui` passes those values straight
   into kit renderer and input APIs.

Facts 4–6 combine into the blocking constraint recorded below: Cargo keys a git
dependency by source revision, so a `botster-ui-contract` at `8a60bd58` and one
at `e8febabf` are distinct types even though the source is identical.

## Scope

### 1. Prerequisite — consume a repinned `botster-tui-kit`

This repository cannot complete the bump alone. See "Ownership boundaries and
cross-repository dependencies". Implement runs the fail-closed entry check in
"Dependency enforcement is advisory in this environment" first; only once
`ticket_1786032168_294170` is closed **and** its merged commit is verified to
carry the `8a60bd58` `botster-ui-contract` pin does this run pin that
`botster-tui-kit` revision in `crates/botster-tui/Cargo.toml`.

### 2. Advance the pinned Hub contract

In `crates/botster-tui/Cargo.toml`, move `botster-hub-client`,
`botster-ui-contract`, and dev-dependency `botster-hub-test-support` from
`e8febabf73259cfd922592346b244ec473c17323` to
`8a60bd58841179f8b1fd4040d9362d18ea244230`, and refresh `Cargo.lock` so exactly
one `botster-ui-contract` source resolves.

### 3. The ticket's named change

Remove the obsolete `hub_version` field from the `DaemonPackageCompatibility`
fixture in `crates/botster-tui/src/app.rs` (`available_package()`, line 16099).
`DaemonPackageCompatibility` at `8a60bd58` is
`{ botster_requirement, result, diagnostics }`.

### 3b. Consume `DaemonStatus.software` on the production path

The ticket requires that Hub identity come from `DaemonStatus.software` and from
nothing else. The first pass read that as a prohibition only and planned no
consumption; `review_1786032878_586317` correctly rejected that as substituting
non-consumption for a required consumption. This section names the actual
production path.

The repository already has exactly one place where Hub-reported identity facts
are hydrated and displayed, so no new surface is invented:

- **Entry point:** `TuiApp::apply_response_state` (`app.rs:1868-1874`) already
  consumes `DaemonStatus` on every status response, storing `status.schema_version`
  and `status.compatibility`. Add `status.software` to that same block.
- **State:** add one `software: Option<DaemonSoftwareIdentity>` field to `TuiApp`
  (`app.rs:764-790`) with its `None` initializer in
  `new_with_runtime_context` (`app.rs:835-856`), mirroring how `compatibility` is
  held.
- **Render:** surface it in `system_details_panel` (`app.rs:2666`) as a sibling of
  the existing `tui-compatibility` node, sourced from the stored `software` value.
  `DaemonSoftwareIdentity` is `{ product_id, product_name, version, build_revision:
  Option<String> }`; `build_revision` is `skip_serializing_if = "Option::is_none"`
  and must render as absent rather than as a fabricated placeholder. Before
  connect, and on a Hub that sent no status, the identity reads as unknown — the
  same convention `compatibility_text()` already uses for `schema_version`.

This is the smallest change that makes the production render path consume the
authoritative field, and it is deliberately narrow:

- **`installation` is populated in fixtures but not presented.** The ticket names
  `software` as the identity source and says nothing about surfacing installation
  provenance; installation-mode and update-availability presentation belong to
  botster-web `ticket_1785970234_234515`. `installation` is filled where the
  struct requires it and nowhere else.
- **No Hub identity is derived from any package row**, which is the prohibition
  half of the same ticket sentence. `DaemonPackageCompatibility` no longer carries
  `hub_version` at all after scope item 3, so the wrong source is removed in the
  same change that wires the right one.

### 4. Forced compile adaptation

Every item below is a compile error produced by the bump. Each is mechanical
adaptation to the new DTO, not new behavior. Line numbers are pre-change.

All line numbers below were re-derived from base `fe03a90` and each is backed by
an actual compiler error from the re-run probe.

- **Entity frames became generic.** `DaemonEntityFrame::Snapshot.items` is now
  `Vec<serde_json::Value>` and `Upsert.entity` is `Value`, and a new
  `DaemonEntityFrame::Error { subscription_id, entity_type, code, message }`
  variant makes `SessionEntityState::apply`'s match non-exhaustive
  (`app.rs:239-324`; errors at `253`, `257`, `273`, `276`, `282`). The reducer
  decodes each record into `DaemonSessionEntity` with `serde_json::from_value`,
  which is exactly the typed projection the hub-client doc comment prescribes, and
  gains an `Error` arm that reports the subscription error through the existing
  `Result<bool, String>` channel for matching subscription/entity type and ignores
  non-matching frames. This keeps
  [[botster hub client state sync is entity frame only]] and the existing
  generation/snapshot-sequence discipline intact.
- **`DaemonSessionEntity` gained six fields** — `session_type_id`,
  `session_type_source`, `role`, `traits`, `interaction`,
  `session_type_lifecycle`. Every literal construction site must populate them:
  `app.rs:369` (`session_binding_reference_row`), `8115`, `8349`, `8400`, `8911`,
  `15214`, plus the `session_entity` (`15187`) and `snapshot_frame` (`15201`)
  test helpers, and the `Value`-typed frame sites at `9375` and `9449`. Preferred
  shape: keep `session_entity` returning the typed struct and add a thin
  value-producing helper so `snapshot_frame` and `Upsert` sites feed `Value`
  without duplicating literals at every call site.
- **Compatibility requirement renamed and re-semanticized.**
  `DaemonCompatibilityRequirement.minimum_protocol_version` is now
  `protocol_version` (`app.rs:4722`), and `ensure_compatible` changed from
  `>=` to exact equality on protocol version. `MINIMUM_CONFORMANCE_FIXTURE_REVISION`
  moves `27 -> 31` (`app.rs:61`). The conformance floor keeps minimum semantics;
  only protocol version became exact.
- **Compatibility test must be rewritten to the new contract**
  (`app.rs:7478-7530`, `tui_requires_protocol_4_revision_27_and_session_entity_subscriptions`).
  It asserts `PROTOCOL_VERSION == 4`, `MINIMUM_CONFORMANCE_FIXTURE_REVISION == 27`
  (`7484`, `7487`), iterates `for revision in 16..27` and
  `for protocol_version in 2..4`, and matches the old diagnostic text
  `"requires at least 4"`. The new protocol diagnostic is
  `"unsupported protocol version {n}; client requires {m}"`. Rename to the
  protocol-6/revision-31 identity and cover: below-floor conformance revisions
  rejected, non-matching protocol versions rejected in **both** directions
  (exact-match semantics now reject a newer protocol too), current hub accepted,
  and a higher conformance revision still accepted. Two further conformance-floor
  assertions at `7533` and `11560` follow the constant and need no edit beyond it.
- **`DaemonStatus` gained required `software: DaemonSoftwareIdentity` and
  `installation: DaemonInstallationIdentity`** (`app.rs:15252` fixture, in
  `status_response_with_package_counts` at `15219`). Populate both in the fixture.
  Production consumption of `software` is scope item 3b; `installation` is fixture
  data only.
- **`DaemonResponse` field renames** — `session_templates -> session_types`,
  `resolved_session_template -> resolved_session_type` (`app.rs:16291-16292`,
  `base_response`).

### 5. Repository documentation

`README.md` states pinned revisions and the compatibility floor as prose claims
and must stay true. Line references re-derived from `fe03a90`:

- Line 31 — `botster-tui-kit` revision.
- Lines 33-36 — the shared `botster-ui-contract` / botster-hub revision, including
  the sentence asserting that the kit, Hub client, and this crate share one
  revision, plus the dev-only branch-tracked `botster-core` coordinate at line 39
  if `Cargo.lock` resolves a different one.
- Line 129 — the `botster-hub-client` protocol revision under "Live hub verification".
- Lines 297-299 — "The cold compatibility floor is daemon protocol version 4 and
  conformance fixture revision 27. Protocol versions 2–3 and fixture revisions
  16–26 fail through the structured compatibility diagnostic; there is no fallback
  path." This becomes protocol 6 / revision 31 **and** must state the exact-match
  protocol semantics rather than a minimum, because the meaning changed, not just
  the number.
- The System details description (line 76) and the live-Hub section gain,
  respectively, the Hub software identity now rendered per scope item 3b and the
  blocked-lane note from scope item 6.

### 6. Live repo session-source fixture, and a declared coverage gap

`app.rs:13017-13018` writes a repo fixture `.botster/session-templates.json` with
key `{"session_templates": ...}`. At `8a60bd58` the Hub reads
`.botster/session-types.json` with key `session_types`
(botster-hub `src/session_types.rs:205`, `REPO_SESSION_TYPES_FILE`). Under the new
Hub the current fixture is inert, so it is renamed and rekeyed. This is a test
fixture correction forced by the repin, not session-type product work.

Cold-cut required: `.botster/session-templates.json` must not survive alongside
the new file. No dual fixtures.

The fixture fix alone does not make any Workspaces lane pass.

**Root cause — one problem, not three.** `botster-workspaces` declares the legacy
capability scope `session_template_managed_git_spawn`, which Hub `8a60bd58` no
longer grants, so `PackageRegistry::enable` denies it and **every** profile that
enables the package fails at `EnablePackage`. `installed-driver`, `plumbing`, and
`lifecycle` all die at that one call for that one reason. The fix is owned by open
`ticket_1785984128_479155`. Every description of this gap — guard message,
`README.md`, implementation report — states it this way and does not present it as
three separate problems.

**Waiver scope.** All three profiles, explicitly granted in
`question_1786033784_534645` (decision 4). See
"Response to `finding_1786033614_940964`" for the evidence and the conduct
concession behind that grant.

**Skip site — `script/test-live-hub`, `workspaces)` case.** The guard is inserted
immediately after the existing usage validation and **before**
`resolve_workspaces_package`, so it exits without requiring
`BOTSTER_WORKSPACES_PACKAGE_PATH`, `BOTSTER_HUB_BIN`, or any other environment.
`script/test-live-hub` therefore **does change**, and the affected-files list
records it; the pass-1 "every `script/` gate unchanged" claim was wrong and is
corrected.

**Observable contract, so a deliberate gap can never be read as a pass:**

- writes to **stderr** a message that names `ticket_1785984128_479155` **by ID**,
  names the requested profile, states the root cause in the single form above, and
  states what is consequently unproven — that the repo-source session-type path
  and the installed-Workspaces spawn driver are unverified against a protocol-6
  Hub. Naming the ticket by ID is required so a reader six months from now finds
  the owner without archaeology;
- exits **non-zero (`1`)**, so no operator or CI path can interpret it as success;
- emits **no** `test result: ok` line and starts no cargo run;
- `contract-matrix` mode is untouched and must keep working normally — the guard
  is scoped to the `workspaces` case only;
- the message must **not** be softened toward "narrow testing artifact" framing.
  Because `enable` hard-denies rather than warns, the installed Workspaces package
  cannot be enabled on a protocol-6 Hub on any real device, not only in these
  lanes. The wording must not imply otherwise.

**Deterministic check.** Add a test that executes `script/test-live-hub workspaces`
for **each** of `installed-driver`, `plumbing`, and `lifecycle` with no live-Hub
environment set, asserting for every profile that the process exits non-zero, that
stderr contains `ticket_1785984128_479155`, and that stdout contains no
`test result: ok`. Because the guard precedes every `resolve_*` call, this test is
hermetic and needs no Hub binaries. It fails the moment someone deletes the guard
without re-enabling the lanes, which is the regression worth catching. Cover all
three profiles rather than a representative one, since all three are waived.

**Further durable records:** a comment at each affected app.rs test
(`installed_workspaces_spawn_driver_runs_through_apps_open` and
`workspaces_live_acceptance_runs_against_real_package`, the latter backing both
`plumbing` and `lifecycle`) naming the blocking ticket, plus a line in the
`README.md` live-Hub section, so a code reader and a doc reader both see a known
gap rather than inferring a pass from silence. The implementation report carries
the same statement.

`ticket_1785984128_479155` requires that closing it re-enable and prove **all
three** Workspaces lanes green against a protocol-6 Hub, not only
`installed-driver`. Verified directly at `updated_at 1786035488`, not taken on
assertion — an earlier claim that this update had been made was checked and found
untrue at the time, which is what prompted `question_1786034337_423424`.

## Non-scope

- Session-type discovery, management, CRUD, launch UX, and any
  role / interaction / traits / lifecycle presentation. `ticket_1785970234_132113`
  ("TUI: manage and launch authoritative Hub session types") owns that and should
  start from this merged revision.
- Consuming `DaemonSessionType`, `DaemonResolvedSessionType`,
  `DaemonSessionTypeRequest`, `ListSessionTypes`, or the session-type mutation
  requests. This run only satisfies the renamed `DaemonResponse` fields.
- Consuming `CheckHubUpdate` / `DaemonHubUpdate` or rendering Hub update
  availability. That is the Web ticket `ticket_1785970234_234515`; no equivalent
  TUI ticket is open, and inventing one here would be speculative.
- Presenting `DaemonStatus.installation` (installation mode, release channel,
  provenance) or any update-availability state. Scope item 3b consumes `software`
  because the ticket names it as the Hub identity source; `installation` and
  `CheckHubUpdate` presentation belong to botster-web `ticket_1785970234_234515`.
- Any compatibility alias, parallel compatibility field, shim, dual code path, or
  Cargo `[patch]` entry. The probe used a `[patch]` purely to enumerate errors and
  it was reverted; it must not appear in the delivered change.
- Migrating the `botster-tui` package manifest. `botster-package.json` declares
  `"capabilities": []` and no `session_templates` key, and `plugin.lua` has no
  session-template references, so it needs no protocol-6 migration.
- Adding `FEATURE_*` constants or widening the TUI's narrowed feature requirement.

## Ownership boundaries and cross-repository dependencies

### Owned here

Client-side pin selection, the entity-frame reducer's typed projection, the TUI's
compatibility requirement values, TUI fixtures and tests, and README claims. Per
[[botster-tui-playbook]] this repository is a client over Hub contracts and owns
no protocol authority.

### Not owned here

- Protocol shape, DTO fields, conformance revision, and `ensure_compatible`
  semantics — `botster-hub` / [[botster-hub-client-playbook]]. Already merged at
  `8a60bd58` under closed `ticket_1785970233_522967` and `ticket_1785970233_236046`.
- Reusable renderer and input mechanics, and the kit's own `botster-ui-contract`
  pin — `botster-tui-kit` / [[botster-tui-kit-playbook]].
- Workspaces package protocol-6 vocabulary — `botster-workspaces`, open
  `ticket_1785984128_479155`.

### Blocking cross-repository dependency — `botster-tui-kit`

Registered: `ticket_1786032168_294170` against target
`tgt_3dfae49c02454037bf13554f552baf7f`, blocking `ticket_1785976581_841608`;
`run_1786032180_372466` is live. `botster-tui-kit` repins `botster-ui-contract`
from `e8febabf` to `8a60bd58` and merges, so this repository can pin the
resulting kit revision.

Proof this is unavoidable, from the probe: bumping only `botster-tui`'s pins put
two `botster-ui-contract 0.3.1` entries in `Cargo.lock` — one at `8a60bd58` and
one reached through `botster-tui-kit` at `e8febabf` — and produced 19 `E0308`
errors of the form `expected botster_ui_contract::UiNode, found a different
botster_ui_contract::UiNode` across `app.rs` (lines 527, 713, 715, 815, 818, 2044,
2054, 2100, 3870, 3933, 3938, 3951, 3964, 3979, 4018, 4989, 5567) and
`renderer.rs` (lines 17, 18). There is no split-pin escape: `botster-hub-client`
pulls `botster-ui-contract@8a60bd58` transitively regardless of what
`botster-tui` declares directly.

Because the contract source is byte-identical between the two revisions, the kit
change is a mechanical revision bump in `botster-tui-kit/Cargo.toml` plus
`Cargo.lock`, with no kit code impact.

Per [[cross repo dependency registration must use dependency repo target]] the
edge is registered against the `botster-tui-kit` target
`tgt_3dfae49c02454037bf13554f552baf7f`, never against this ticket's target.
`botster-tui-kit` was not a target of `project_1785970196_204877`, so adding it
was an orchestration decision; it was escalated in `question_1786032001_762094`
rather than taken silently, and the orchestrator performed the target admission,
ticket creation, dependency registration, and run start.

Per [[closed dependency tickets signal merged source not a consumable release]],
the kit dependency is satisfied only by a merged kit commit this repository can
pin — not by an approved kit plan and not by the kit run reaching Implement.

### Non-blocking cross-repository seam — `botster-workspaces`

The `script/test-live-hub workspaces <profile>` lanes drive the installed
`botster-workspaces` package, whose protocol-6 migration is open
`ticket_1785984128_479155` (manifest `session_templates -> session_types`,
`session_template_spawn -> session_type_spawn`, Lua
`botster.capabilities.session_templates -> session_types`). Those lanes cannot
pass against a protocol-6 Hub until that lands. They are environment-gated and do
not run under `script/test`, so they do not block this repository's default gates;
the live-Hub evidence for this run comes from the contract-matrix lane.

Per decision 3 this is deliberately **not** registered as a second blocking
dependency: `ticket_1785984128_479155` is itself blocked on
`ticket_1785970233_750553` and `ticket_1785970234_132113`, so the edge would
serialize this ticket behind the whole Web wave for the sake of env-gated lanes.
The cost of that choice is a real coverage gap across all three Workspaces
profiles, which is why scope item 6 requires it to be recorded durably in the
report and in-repo.

**Wider than a coverage gap.** Because `PackageRegistry::enable` hard-denies an
ungranted capability scope rather than warning, the ungranted
`session_template_managed_git_spawn` declaration means the installed Workspaces
package cannot be enabled on a protocol-6 Hub **at all** — on any real device, not
only in these test lanes. `ticket_1785984128_479155` is therefore what makes
Workspaces functional against the shipped Hub contract. That is a product-priority
fact the orchestrator is raising separately; it does not change this run's scope,
and it is recorded here so the gap is not mistaken for a testing artifact.

## Assumptions and unknowns

1. **Assumed:** `8a60bd58841179f8b1fd4040d9362d18ea244230` is the authoritative
   contract. Verified: it is botster-hub `main` HEAD and is reachable from
   `origin/main`, and the orchestrator amendment plus
   `question_1785987597_116465` both name it explicitly.
2. **Confirmed, not assumed:** the forced adaptation in scope item 4 belongs to
   this ticket rather than `ticket_1785970234_132113`. Rationale: each item is a
   compile error, and a ticket that says "prove cargo test compiles against the
   new DTO" necessarily owns whatever the compiler demands. Confirmed in
   `question_1786032001_762094`.
3. **Known production behavior change — must be declared, not assumed away:**
   `session_binding_reference_row()` (`app.rs:361`) builds an intentionally
   exhaustive session-entity row so bind-list templates observe every key.
   Populating the six new fields means production session bind rows gain six new
   keys: `session_type_id`, `session_type_source`, `role`, `traits`,
   `interaction`, `session_type_lifecycle`. This is forced by the struct and
   consistent with the function's documented purpose, but it is the one place in
   this change where production output changes rather than a test fixture. No UI
   consumes the new keys in this run.

   **Required of the implementation report:** declare this under deviations,
   naming all six keys. Do not write `deviations_from_plan: None`. A previous run
   in this project shipped that line while silently narrowing a public descriptor
   and Review caught it; declaring it up front keeps Review arguing about whether
   the call was right rather than why it was hidden.
3b. **Observed contract brittleness — record, do not fix here.**
   `ensure_compatible` moving from `>=` to exact equality means the TUI refuses
   any Hub whose protocol version is not exactly its own, so every future Hub
   protocol bump becomes a hard TUI break instead of a forward-compatible one.
   This is the merged Hub contract's design; conforming to it is correct here and
   changing it would be a `botster-hub` ticket. The orchestrator is recording the
   concern at project level. The implementation report must note it as observed
   behavior so it stays discoverable, and must not soften it locally with a TUI
   workaround.
4. **Assumed:** the TUI's narrowed feature requirement list stays as-is. The eight
   required `FEATURE_*` constants all still exist at `8a60bd58`; per
   [[adding a hub client feature constant is a three site change]], widening the
   list is a separate deliberate decision, not repin fallout.
5. **Unknown until `ticket_1786032168_294170` closes:** the exact
   `botster-tui-kit` revision to pin. The plan carries the constraint, not a
   guessed hash. If that run escalates anything non-mechanical, the
   byte-identical `botster-ui-contract` premise is wrong and this plan must be
   revisited before Implement proceeds.
6. **Unknown:** whether `Cargo.lock`'s dev-only branch-tracked `botster-core`
   (currently `e36435f2cb583c344d6f6ba2d62c39da324c7a64`, reached through
   `botster-hub-test-support`) moves under the new Hub revision. The probe
   resolved a working lock, so this is a "record what the lock actually resolves
   and keep the README truthful" item, not a decision.
7. **Assumed:** no `DaemonRequest` / `DaemonResponseKind` variant the TUI matches
   was removed. The probe's error set contained no non-exhaustive-match error for
   those enums, so the assumption is compiler-verified for every reachable target.

## Affected surfaces and files

All line numbers are against base `fe03a90`.

- `crates/botster-tui/Cargo.toml` — three botster-hub revision pins plus the
  `botster-tui-kit` revision pin.
- `Cargo.lock` — regenerated; must contain exactly one `botster-ui-contract` source.
- `crates/botster-tui/src/app.rs`:
  - forced adaptation — `MINIMUM_CONFORMANCE_FIXTURE_REVISION` (61);
    `SessionEntityState::apply` (239-324); `session_binding_reference_row` (369);
    `tui_compatibility_requirement` (4722); the compatibility test (7478-7530);
    entity-frame test sites (8115, 8349, 8400, 8911, 9375, 9449); test helpers
    `session_entity` (15187) and `snapshot_frame` (15201); the `DaemonStatus`
    fixture (15219-15252); the `DaemonPackageCompatibility` fixture (16097-16101);
    `base_response` (16291-16292);
  - `DaemonStatus.software` consumption (scope 3b) — `TuiApp` struct (764-790),
    `new_with_runtime_context` initializer (835-856), `apply_response_state`
    (1868-1874), `system_details_panel` (2666);
  - blocked-lane record (scope 6) — the live repo session-source fixture
    (13017-13018) and comments at both
    `installed_workspaces_spawn_driver_runs_through_apps_open` and
    `workspaces_live_acceptance_runs_against_real_package` (13333, which backs
    `plumbing` and `lifecycle`).
- `script/test-live-hub` — the `workspaces)` blocked-pending guard. **This
  correction matters:** the first pass claimed every `script/` gate was unchanged
  while also requiring an in-repo skip record, which
  `review_1786032878_586317` flagged as contradictory.
- `README.md` — lines 31, 33-36, 39 (if the resolved dev `botster-core` moves),
  76, 129, 297-299, and the live-Hub section blocked-lane note.
- A new or extended test asserting the blocked-lane guard's exit/output contract.
- `docs/plans/tui-authoritative-hub-maintenance-contract-repin-plan.md` — this plan.
- Not expected to change: `crates/botster-tui/src/renderer.rs`,
  `crates/botster-tui/src/acceptance.rs`, `crates/botster-tui/src/main.rs`,
  `botster-package.json`, `plugin.lua`, `script/fmt`, `script/clippy`,
  `script/test`, and `test.sh`. Any change to these is a signal that scope drifted
  and should be justified explicitly.

## Risks

1. **Scope creep into session-type UX.** The new DTOs make role/interaction/traits
   visible and tempting to render. Mitigation: the non-scope list is explicit, and
   the reviewer should reject any presentation change; `ticket_1785970234_132113`
   owns it.
2. **Compatibility semantics silently weakened.** `ensure_compatible` moved from
   minimum to exact protocol matching. Rewriting the compatibility test carelessly
   — for example deleting the rejection loops instead of re-expressing them —
   would erase real coverage. Mitigation: the rewritten test must reject a
   *newer* protocol as well as an older one, which the old minimum semantics
   allowed, and must keep minimum semantics for the conformance revision.
3. **Entity reducer decode errors becoming silent.** Moving from typed frames to
   `Value` introduces a decode step that could be written to swallow malformed
   records. Mitigation: decode failure returns `Err` through the existing
   `Result<bool, String>` channel and surfaces as a diagnostic, matching how the
   existing id-mismatch and patch-shape errors behave.
4. **Reviewer treating the forced adaptation as unrelated churn.** Mitigation:
   this plan enumerates each site with its compiler error, and the implementer
   should attach the pre-fix `cargo check` error list as evidence so every changed
   line traces to a specific error.
5. **Duplicate `botster-ui-contract` regression.** If the kit pin and Hub pin ever
   drift again the failure is a wall of confusing "a different `UiNode`" errors.
   Mitigation: an explicit `Cargo.lock` assertion in acceptance, plus the README
   sentence that states the shared-revision invariant.
6. **Stale worktree / stale build cache.** Per
   [[stale project pipeline worktrees can miss merged dependency apis]] and
   [[live hub target dirs can cache stale same version client schema]], the
   implementer must confirm the fetched `8a60bd58` and the live-Hub binaries come
   from the same revision, not a cached older build.
7. **Environmental gate failure from the colon worktree path.** This worktree path
   contains `:` (`botster-sessions/git@github.com:trybotster-...`). Per
   [[colon worktree paths break cargo dyld library paths]], Cargo gates must run
   with a colon-free `CARGO_TARGET_DIR`; `test.sh` already defaults it to
   `${TMPDIR}/botster-tui-spike-target`. A dyld path-join failure is an
   environment artifact and must be reported separately from Rust diagnostics.
8. **Kit dependency not honored.** Attempting to unblock with a Cargo `[patch]`,
   a vendored contract copy, or a local path override would be exactly the
   compatibility shim the ticket forbids. Mitigation: the dependency is registered
   formally, and the fail-closed Implement entry check stops the run before any
   edit if the kit artifact is not actually pinnable. Registration alone is not a
   mitigation here — the edge is advisory, so the check is what does the work.
9. **Further main drift while this run waits.** `fe03a90` landed between the first
   and second Plan passes and rewrote every principal surface this plan cites,
   which is how the first plan went stale. Because this run sits idle until
   `ticket_1786032168_294170` closes, the same drift can recur. Mitigation:
   Implement must `git fetch origin --prune`, rebase onto current `origin/main`,
   and re-validate the cited line references before editing, treating any moved
   site as a signal to re-derive rather than to trust this plan's numbers.
10. **The blocked-lane guard becoming permanent.** A guard that exits non-zero is
    honest but easy to leave in place forever. Mitigation: the guard names
    `ticket_1785984128_479155` in its own output, the deterministic test fails if
    the guard is removed without re-enabling the lanes, and the orchestrator has
    recorded on that ticket that closing it must re-enable and prove all three
    Workspaces lanes green against a protocol-6 Hub.
11. **Implement activating while the kit prerequisite is still open.** The
    dependency edge is advisory in this environment, so nothing stops Plan Review
    approval from activating Implement with `ticket_1786032168_294170` open —
    `run_1785987292_480836` shows this already happening on a sibling ticket. The
    danger is not merely a failed build: an implementer facing an absent
    prerequisite is precisely the one tempted into a `[patch]`, a vendored
    contract copy, or a path override, which is the compatibility shim the ticket
    forbids. Mitigation: the fail-closed Implement entry check, which requires the
    ticket closed, the commit merged and pinnable, and the pin actually present in
    that commit's `Cargo.toml`/`Cargo.lock` before any edit.
12. **Generalizing a scoped waiver without checking.** Pass 2 widened an
    installed-driver-only waiver to the whole `workspaces)` case as an
    implementation convenience, without verifying whether the wider scope was
    warranted and without asking. `finding_1786033614_940964` caught it. The wider
    scope turned out to be correct, but that was luck, not method: a coverage
    waiver is the orchestrator's decision and must be requested, not inferred.
    Mitigation for Implement: the waiver is now exactly three named profiles, and
    any further lane found to be blocked requires a new explicit grant rather than
    an extension of this one.

## Acceptance checks and tests

### Static dependency and source proof

- `crates/botster-tui/Cargo.toml` shows `8a60bd58841179f8b1fd4040d9362d18ea244230`
  for `botster-hub-client`, `botster-ui-contract`, and `botster-hub-test-support`,
  and the merged `botster-tui-kit` revision.
- `grep -c 'name = "botster-ui-contract"' Cargo.lock` is `1`, and its `source`
  is the `8a60bd58` git URL. This is the direct regression check for risk 5.
- `grep -rn hub_version crates` returns nothing.
- `grep -rn 'session-templates.json' crates` returns nothing — proves the cold-cut
  fixture rename left no dual file.
- `status.software` appears in `apply_response_state` and the stored value reaches
  `system_details_panel`, so the consumption is wired rather than dead.
- `grep -rn 'session_templates\|resolved_session_template\|minimum_protocol_version' crates`
  returns nothing.
- `grep -rn 'patch\]' Cargo.toml crates/botster-tui/Cargo.toml` returns nothing —
  proves no shim survived from the planning probe.

### Repository gates

Run from a colon-free `CARGO_TARGET_DIR`, capturing raw Cargo output rather than
summarized output per [[botster pipeline reviewers must bypass rtk summaries for cargo gate evidence]]:

- `script/fmt`
- `script/clippy` (`cargo clippy --workspace --all-targets --all-features -- -D warnings`)
- `script/test` (`cargo test --workspace --all-targets`) — the ticket's explicit
  "prove cargo test compiles against the new DTO" requirement. Record the full
  pass count, not just exit status.

Per [[test script required for rust tests not cargo test]] and
[[botster test sh forwards arguments to cargo not custom unit flags]], use the
repository wrappers; per [[workspace struct field changes require workspace cargo gates]],
the gates must be workspace-scoped because DTO field changes ripple.

### Targeted TUI proof

- The rewritten compatibility test asserts `PROTOCOL_VERSION == 6` and
  `MINIMUM_CONFORMANCE_FIXTURE_REVISION == 31`, rejects conformance revisions
  below 31, rejects protocol versions both below and above 6 through the new
  diagnostic text, accepts the current hub, and still accepts a higher conformance
  revision.
- `SessionEntityState` tests cover: a snapshot of `Value` records decoding into
  the typed projection; an upsert whose `Value` lacks `session_uuid` or fails to
  decode surfacing an error rather than being dropped; the existing id-mismatch
  and pre-snapshot-delta cases still holding; and the new
  `DaemonEntityFrame::Error` arm both surfacing a matching-subscription error and
  ignoring a non-matching one.
- Session-entity records carrying the six new fields round-trip through the
  reducer without loss, and `session_binding_reference_row()` emits all six keys —
  the concrete evidence for assumption 3.
- **Authoritative Hub identity consumption (scope 3b).** The `DaemonStatus`
  fixture populates `software` and `installation`; a test drives a status response
  through `apply_response_state` and asserts the rendered System details panel
  shows the Hub product identity carried in `DaemonStatus.software` — proving the
  production render path, not merely that the field is stored. A negative
  assertion proves the identity is not sourced from any package row: feeding a
  status whose `software.version` differs from every installed package version
  must render the `software` value. Absent `build_revision` renders as absent, not
  as a placeholder, and a pre-connect app renders unknown rather than a fabricated
  version.
- A deterministic check that `script/test-live-hub workspaces <profile>` exits
  non-zero with `ticket_1785984128_479155` on stderr and no `test result: ok` on
  stdout, for **each** of `installed-driver`, `plumbing`, and `lifecycle`, and
  that `contract-matrix` mode is unaffected by the guard.
- Existing entity-reducer, rendering, and plugin-surface tests still pass
  unchanged in intent, preserving
  [[acceptance readiness requires the exact expected entity not any authoritative snapshot]].

### Downstream live-Hub proof

Per [[botster-tui-playbook]]'s required gates and
[[external client hub tests use subprocess spawned hub test support]], code
compiling is not proof that the production path changed.

- `script/test-live-hub contract-matrix` against `botster-hub` and
  `botster-session-worker` binaries built from `8a60bd58`, with
  `BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE` pointing at that revision's fixture.
  This exercises the real handshake, so it proves the production compatibility
  requirement — not just a unit fixture — negotiates successfully against a
  protocol 6 / conformance 31 Hub, and that the session entity subscription
  reducer consumes real Hub frames.
- Record the Hub and session-worker binary provenance (revision they were built
  from) alongside the result, per risk 6.
- `script/test-live-hub workspaces` lanes — `installed-driver`, `plumbing`, and
  `lifecycle`: **not run**, blocked pending `ticket_1785984128_479155` per
  decisions 3 and 4. Acceptance is the guard specified in scope item 6 — its
  stderr message naming the ticket by ID, non-zero exit, and the three-profile
  deterministic test above — plus the durable gap record in the implementation
  report, both app.rs test comments, and `README.md`, each naming the blocking
  ticket and stating the single root cause and what is consequently unproven: the
  repo-source session-type path and the installed-Workspaces spawn driver are
  unverified against a protocol-6 Hub. A blocked lane must never be reported as a
  pass or silently omitted.

### Documentation proof

- Every revision hash and the compatibility-floor paragraph in `README.md` match
  the delivered `Cargo.toml` and `Cargo.lock`. The floor paragraph describes exact
  protocol matching, not a minimum.

## Workflow evidence

- This plan is attached as a run artifact and lives at
  `docs/plans/tui-authoritative-hub-maintenance-contract-repin-plan.md`, matching
  this repository's established `docs/plans/` prior art rather than a vault
  example path, per [[vault example paths are not repository placement conventions]]
  and [[plan steps need reviewable plan artifacts]].
- Vault context is written as wiki links, per
  [[plan agents must author vault context as wikilinks not home paths]], and every
  cited note title was validated against an exact vault filename, per
  [[pipeline vault checklists must cite exact resolvable note titles]].
- The blocking cross-repository constraint and the two scope confirmations were
  escalated in `question_1786032001_762094` rather than resolved silently, and
  the answers are carried in "Decisions from `question_1786032001_762094`".
- The Workspaces waiver scope was escalated in `question_1786033784_534645` rather
  than either silently re-submitted or complied with against the evidence; the
  grant and its four conditions are carried in decision 4, scope item 6, and
  "Response to `finding_1786033614_940964`".
- `question_1786034337_423424` was raised because an asserted durable record did
  not exist when checked. It returned two corrections now carried here: the
  three-lane obligation on `ticket_1785984128_479155` is real and verified at
  `updated_at 1786035488`, and dependency edges in this environment are advisory
  records only.
- Every durable condition this plan cites was verified by direct tool call, not
  taken on assertion: `ticket_1785984128_479155` (`updated_at 1786035488`),
  `ticket_1785976581_841608` (`updated_at 1786033407`), and
  `ticket_1786032168_294170` (still `open`, so the Implement precondition is not
  yet met).
- Both planning compile probes were reverted in full. The second pass rebased onto
  `fe03a90`, re-ran the probe from that base, and restored a clean tree; the
  probe-only Cargo `[patch]` must not reappear in delivered work.
- `review_1786036343_780324` returned `changes_required` with one finding,
  `finding_1786036343_932174`, accepted: [[project-pipelines-playbook]] is
  required once workflow policy is in scope, and pass 4 put it in scope while
  still recording the charter as deliberately not loaded. The charter and its
  applicable Must Load entries are now loaded and recorded under "Target and
  context", with "How the charter bears on this plan" documenting the actual
  relationship. One correction to the finding's framing, made deliberately rather
  than silently: the finding asks to record a conflict between the charter's
  intended activation gating and this runtime, but the note it rests on is
  `status: superseded` / `type: drift` and says it must not be treated as a
  current contract. There is no live convention to conflict with — the vault
  retracted it in July on repository evidence, and
  `question_1786034337_423424` corroborates that from the runtime side. Recorded
  that way, plus the sharper fact that `ticket_1785989402_277498` is closed, so
  the gap here is deployment rather than implementation.
- `review_1786035659_348398` returned `changes_required` with one finding,
  `finding_1786035659_833558`, addressed by the new section "Dependency
  enforcement is advisory in this environment": the advisory nature of the edge
  is now stated explicitly, every "holds"/"waits" guarantee is corrected to a
  claim verified at the transition, a fail-closed Implement entry check is
  specified, and the failure mode is carried as risk 11 plus vault gaps 5 and 6.
- `review_1786033614_966035` returned `changes_required` with one finding,
  `finding_1786033614_940964`, answered in full under
  "Response to `finding_1786033614_940964`": its conduct criticism is conceded,
  its factual premise is disproved from source, and the wider waiver it asked me
  to request is now explicitly granted with four conditions.
- `review_1786032878_586317` returned `changes_required` with four findings. All
  four are addressed here: the stale base (rebased onto `fe03a90`, probe re-run,
  every line reference re-derived, baseline `script/test` independently confirmed
  at 142 unit + 1 integration), the missing `DaemonStatus.software` consumption
  (scope item 3b now names the production entry point, state, render site, and
  proof), the unspecified blocked-lane mechanism (scope item 6 now names the exact
  guard site, its exit/output contract, and a deterministic test, and the
  affected-files contradiction is corrected), and the incomplete planner overlay
  attestation (the full [[botster-planner-playbook]] Must Load set is now recorded,
  with explicit no-constraint statements where an overlay does not apply).
- `project_pipelines_create_vault_checklist` returned `plugin worker invoke
  timeout` on two attempts, so vault-discipline evidence is carried on the Plan
  gate and this artifact instead, per
  [[project pipelines checklist worker timeouts require artifact evidence fallback]].
- **Sequencing:** Plan completes, Plan Review completes, then Implement runs the
  fail-closed entry check and proceeds only if every condition holds. The
  dependency edge is advisory in this environment, so sequencing is a claim
  verified at the transition, never a guarantee assumed in advance.

## Vault gaps worth capturing

1. **A shared workspace-external contract crate forces lockstep pins across every
   consumer.** `botster-ui-contract` is reached by `botster-tui` directly, by
   `botster-hub-client` transitively, and by `botster-tui-kit` independently.
   Cargo keys git dependencies by revision, so a byte-identical crate at two
   revisions is two incompatible types, and the symptom — "expected
   `botster_ui_contract::UiNode`, found a different `botster_ui_contract::UiNode`"
   — reads like a code defect rather than a pin-skew defect. Worth a gotcha note
   naming the diagnostic string and the `Cargo.lock` duplicate-source check that
   identifies it in one command.
2. **A Hub repin's blast radius is not knowable from the ticket text.** This
   ticket described a one-field fixture deletion; the actual forced set spans an
   entity-frame representation change, a six-field DTO growth, a renamed and
   re-semanticized compatibility field, and two `DaemonResponse` renames. Worth a
   convention note that Plan should run a throwaway compile probe against the
   target revision before scoping any protocol repin, and revert it.
3. **A plan artifact's base commit is a claim that expires.** This plan was
   authored against `a2ad3ff` and was stale before Plan Review finished, because
   `fe03a90` landed in between and rewrote every principal surface it cited —
   line references, reducer shape, and README claim sites all moved. The plan
   itself still read as authoritative. Worth a convention note that any pipeline
   step consuming a plan must `git fetch origin --prune` and compare the plan's
   declared base against current `origin/main` before trusting a single cited
   line, and that a plan which holds across a blocking dependency must be
   re-validated on resume rather than on authoring. This is the plan-artifact
   analogue of [[stale project pipeline worktrees can miss merged dependency apis]],
   which covers the worktree rather than the plan.
4. **A cold-cut capability rename can break a consumer's *enable* path, not just
   its API surface.** Renaming the granted scope `session_template_managed_git_spawn`
   to `session_type_managed_git_spawn` in the Hub's `default_capability_grants()`
   means any package still declaring the old scope cannot be enabled at all —
   `PackageRegistry::enable` hard-denies, while install only records a diagnostic.
   The consumer needs no code change to break, and the failure surfaces as a
   capability-grant error far from the rename. Worth a gotcha note: when a cold cut
   touches the capability grant list, enumerate every first-party package
   declaring the old scope, because each becomes un-enablable on the new Hub
   rather than merely degraded.
5. **An advisory dependency edge reads exactly like an enforced one, so plans
   silently promote ordering claims into guarantees.** This project's registered
   blocking dependencies are records only in the running environment, yet the edge
   appears in `blocking_dependencies` identically either way, and
   `run_1785987292_480836` was activated into Implement with two open blockers.
   Three passes of this plan wrote "this run holds at Implement" and cited formal
   registration as the mitigation. Worth a convention note: a plan may never state
   sequencing as a guarantee, must name whether an ordering constraint is enforced
   or advisory, and must carry a fail-closed entry check that verifies the
   prerequisite artifact — closed ticket, merged commit, and the expected pin
   actually present — rather than trusting the edge. Pairs with
   [[closed dependency tickets signal merged source not a consumable release]],
   which covers what "closed" is worth; this covers what the edge is worth. It is
   also the *runtime-observable* form of
   [[vault convention notes can document unimplemented behavior as shipped]]:
   that note falsified the gating claim from repository evidence, and this run hit
   the same absence from the operating side without knowing the note existed.
5b. **The Project Pipelines charter's Must Load still points at a superseded
   note, and the gate's status has since changed again.**
   [[project-pipelines-playbook]] lists
   [[project pipeline step activation gates open ticket dependencies before side effects]]
   as a Must Load with the gloss "dependencies gate mutation and spawn", but that
   note is `type: drift`, `status: superseded` and explicitly says it must not be
   loaded as a current runtime contract. An agent following the charter reaches a
   retracted convention. Separately,
   [[vault convention notes can document unimplemented behavior as shipped]]
   records that a replacement note should be derived at merge handoff, and held
   evidence at PR #200 was then still unmerged — but `ticket_1785989402_277498`
   ("Project Pipelines: enforce blocking dependencies before agent-step
   activation") is now **closed**, so the merge handoff that note was waiting for
   may have arrived. Two capture candidates: repoint the charter's Must Load, and
   derive the replacement current note from the merged mechanism — recording that
   the gate exists in `botster-project-pipelines` but is **not deployed** in the
   running legacy plugin, which is a deployment gap rather than an implementation
   gap and is exactly the distinction the original drift note lacked. Follows
   [[superseding a note requires searching derived guidance for its identifiers]].
5c. **A routing rule with an "or" is a two-part condition, and reading one part
   silently skips a required charter.** The role contract loads
   [[project-pipelines-playbook]] when package/plugin paths **or workflow policy**
   are in scope. This plan checked only the path half and recorded "deliberately
   not loaded" — correct for three passes, then wrong the moment pass 4 added
   dependency-edge semantics and a manual activation gate. Structurally identical
   to gap 8 below, where "consume only X" was read as prohibition-without-
   requirement. Worth one note covering both: when a rule or ticket clause has two
   halves, satisfying one and asserting the whole is the failure, and a plan that
   *adds* a new kind of content must re-run its own context-loading decision
   rather than inheriting the earlier pass's answer.
6. **An orchestrator's claim that a durable record exists is not evidence it
   exists.** This run was told `ticket_1785984128_479155` had been updated to
   cover all three Workspaces lanes; a direct `project_pipelines_get_ticket` call
   showed `updated_at` unchanged and the description still naming one lane, and
   the plan refused to cite the condition until it was real. The same posture
   later surfaced the advisory-dependency correction. Worth a convention note that
   any durable condition a plan cites — ticket text, dependency state, a merged
   commit — must be verified by direct tool call and cited with its `updated_at`,
   never carried on assertion.
7. **A coverage waiver is the orchestrator's decision and cannot be widened as an
   implementation convenience.** Pass 2 generalized an installed-driver-only
   waiver to the whole `workspaces)` case without checking whether the wider scope
   was warranted. It happened to be warranted, which is exactly why the habit is
   dangerous — the plan would have read as authorized either way. Worth a
   convention note that a waiver's scope is a granted quantity, and that
   discovering more blocked surface obliges a new request rather than a silent
   extension.
8. **"Consume only X" is a requirement plus a prohibition, and reading only the
   prohibition silently narrows the ticket.** The first pass read "consume only
   `DaemonStatus.software` for authoritative Hub identity" as a ban on package-row
   derivation and concluded that deriving identity from nothing satisfied it.
   Review correctly rejected that. Worth a note that "only" clauses in tickets
   carry both halves, and that a plan satisfying such a clause must name the
   production entry point that performs the consumption.
9. **Protocol compatibility semantics can change without the field name changing
   meaning obviously.** `minimum_protocol_version` becoming `protocol_version` also
   moved `ensure_compatible` from `>=` to `==`. A rename that silently converts a
   floor into an exact match is worth recording next to
   [[daemon event shape changes bump conformance fixture revision not protocol version]],
   because a downstream client can mechanically fix the compile error and lose the
   test coverage that documented the old semantics.
