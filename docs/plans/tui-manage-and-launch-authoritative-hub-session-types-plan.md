# TUI: manage and launch authoritative Hub session types

## Plan revision

| Field | Value |
| --- | --- |
| Pass | 2 — Plan Review `changes_required` (`review_1786071830_585081`) |
| Product decision | **A** — full management including lossless Update (`question_1786071947_442525`) |
| Open findings closed by this revision | all seven from `review_1786071830_585081` |

### Finding disposition

| Finding | Severity | Disposition |
| --- | --- | --- |
| `finding_1786071830_619143` — edit path at pin 8a60bd58 drops authored cwd path + environment | blocker | **Adopt A.** Repin to Hub `302190e` (conformance 32). Edit seeds **only** from `ShowSessionTypeDefinition`. Never row-seed from `DaemonSessionType`. |
| `finding_1786071830_659401` — repin drags unregistered kit dependency | blocker | **Adopt.** Registered blocking kit ticket `ticket_1786071998_949850` on `tgt_3dfae49c02454037bf13554f552baf7f` (`dependency_1786072005_676257`). Implement must not start until that ticket is closed and a consumable kit commit exists. |
| `finding_1786071830_862984` — no plan artifact | blocker | **Adopt.** Commit this plan on the run branch; attach via `project_pipelines_add_artifact` (`kind=plan`); cite returned artifact id in gate evidence. |
| `finding_1786071830_538769` — live contract-matrix lane red / sibling owns it | high | **Adopt.** Session-types live profile is independent of contract-matrix and of the never-connected `connection:` assertion. Sibling `ticket_1786038825_352271` remains the owner of contract-matrix + `legacy_test_needs_system_details()` policy; this run does not change that helper. |
| `finding_1786071830_111578` — script/test fails in colon-bearing worktree | high | **Adopt.** Document colon-free `CARGO_TARGET_DIR`. Raised non-blocking owner ticket `ticket_1786071999_889350` on project-pipelines target. |
| `finding_1786071831_219670` — closed Hub dep without consumed-artifact check | medium | **Adopt.** Explicit consumed-artifact section: git-rev consumption, required coordinate Hub `302190e` / conformance 32 / `session_type_authoring`. |
| `finding_1786071831_832844` — product Spawn preference not contract | low | **Adopt.** Decision ledger pins toolbar → `SpawnSessionType` only; freeform command spawn removed from product UI; tests named for rewrite. |

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`git@github.com:trybotster/botster-tui.git`) |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Ticket | `ticket_1785970234_132113` — "TUI: manage and launch authoritative Hub session types" |
| Project | `project_1785970196_204877` |
| Pipeline / run | `botster_stack_delivery` / `run_1786070915_943794` |
| Current step | `botster_stack_plan` (return visit after Plan Review) |
| Base | `main` at `16d10b4` (PR #46 Workspaces acceptance restore) |
| Worktree | Pipeline worktree for this ticket; authoritative path from run `base_target_path` `/Users/jasonconigliari/Projects/botster-tui` |

Authoritative target comes from the ticket/run `target_id`, **not** from the process working directory name.

## Repository playbook loaded

- [[botster-tui-playbook]] — ownership charter for this target

## Other role / surface playbooks and atomic notes loaded

### Role overlays (required order)

1. [[planner-playbook]]
2. [[botster-planner-playbook]]
3. [[botster-tui-playbook]]

### [[botster-planner-playbook]] Must Load set

- [[botster-architecture]]
- [[cli-patterns]]
- [[spa-patterns]] — peer product parity (web consumer), not an edit target
- [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]]
- [[botster orchestration should spawn agents with explicit target ids]]
- [[botster orchestration prompts must bind agents to explicit worktrees]]
- [[plan agents must author vault context as wikilinks not home paths]]
- [[pipeline vault checklists must cite exact resolvable note titles]]
- [[vault example paths are not repository placement conventions]]

### [[botster-tui-playbook]] Must Load set (task-relevant)

- [[tui and browser are equal clients]]
- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[botster-tui-kit-playbook]] — renderer/input mechanics consumed; **kit pin is a blocking cross-repo dependency**
- [[tui client attach uses hub protocol not session protocol]]
- [[tui and socket terminal streams use clientworker transport adapters]]
- [[botster tui uinode event routing captures hit regions during draw]]
- [[tui error dedup tests must drive real input handlers]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]

### Targeted atomic notes

- [[botster hub client state sync is entity frame only]]
- [[botster client subscriptions should not hydrate global state]]
- [[botster entity snapshots are authoritative reconnect baselines]]
- [[plugin-owned dynamic state uses plugin-namespaced entity frames]] — contrast: canonical family is bare `session_type`
- [[hub qualifies effective session type ids as source name slash id]]
- [[session template override sources use package device repo explicit precedence]]
- [[botster tui attach must explicitly pull core entities after subscribing]]
- [[live acceptance tests must not depend on a loop tick window]]
- [[external client hub tests use subprocess spawned hub test support]]
- [[botster hub client crate is the external client boundary]]
- [[closed dependency tickets signal merged source not a consumable release]] — closed Hub ticket ≠ consumable pin
- [[cross repo dependency registration must use dependency repo target]]
- [[plan review must verify a plan artifact exists before trusting gate summaries]]

### Peer consumer reference (not an edit target)

- `botster-web` Session types management + lossless `show_session_type_definition` edit path on `@trybotster/hub-test-support@0.1.25` (tickets `ticket_1785970233_750553`, `ticket_1786039279_917823`).

### Deliberately not loaded

- [[project-pipelines-playbook]] as implementation scope — no Project Pipelines package/plugin code is edited. Owner ticket for worktree naming is registered separately against the project-pipelines target.

## Context loaded

### Ticket intent

Consume the authoritative Hub session-type contract in `botster-tui`:

1. Keyboard-accessible discovery of effective session types with Hub provenance and descriptors.
2. Source-aware create / **lossless update** / delete for editable device and repo sources; package provenance read-only.
3. Verbatim presentation of `role`, `interaction`, `traits`, `lifecycle`, and related Hub fields — never infer type from command/name.
4. Target-first launch through `SpawnSessionType`.
5. State channel: canonical `session_type` entity subscription; no TUI-owned session policy.
6. Proof matrix: agent, interactive accessory, service accessory, unknown namespaced descriptors, CRUD errors, reconnect/restart projection, launch metadata, **and authoring-view round-trip of path + environment**.

### Product decision (recorded)

**A** from `question_1786071947_442525` (answered): full management including lossless Update. Do **not** ship a row-seeded editor. Do **not** choose B.

Exact pin from the answer: Hub **`302190e`** (`302190ec2acc5ecee744432a6c9ffd1f040ebe01`), origin/main tip after PR #196. `c57d388` would also be acceptable; do not pin earlier than `2b8361b` / `6ad6dfa`. This plan standardizes on **`302190e`**.

### Registered dependencies

| Depends on | Title | Target | Status |
| --- | --- | --- | --- |
| `ticket_1785970233_236046` | Hub: make session types flexible, editable, and authoritative | hub | **closed** (merged source; not the consumable pin) |
| `ticket_1786071998_949850` | TUI kit: repin botster-ui-contract to Hub 302190e | `tgt_3dfae49c02454037bf13554f552baf7f` | **open — blocking** (`dependency_1786072005_676257`) |

### Same-target sibling (non-blocking, non-owned)

| Ticket | Title | Relation |
| --- | --- | --- |
| `ticket_1786038825_352271` | contract-matrix live failure / System-details visibility | Owns red `script/test-live-hub contract-matrix` (`assert!(rendered.contains("connection:"))` at `app.rs:13258`) and `legacy_test_needs_system_details()` policy. FILE CONTENTION section names this ticket. **Do not run concurrent `app.rs` ownership fights.** This run does **not** change `legacy_test_needs_system_details()`. |

### Non-blocking owner ticket (pipeline infra)

| Ticket | Title |
| --- | --- |
| `ticket_1786071999_889350` | Project Pipelines: worktree directory names must not contain `:` |

## Consumed artifact (required)

Per [[closed dependency tickets signal merged source not a consumable release]]:

| Question | Answer |
| --- | --- |
| How does botster-tui consume Hub? | **Git rev** pins in `crates/botster-tui/Cargo.toml` for `botster-hub-client`, `botster-ui-contract`, and `botster-hub-test-support` from `https://github.com/trybotster/botster-hub.git`. Not npm. |
| Base pin today | `8a60bd58841179f8b1fd4040d9362d18ea244230` — protocol **6**, conformance **31**. Has session-type CRUD + entity subscriptions. **Lacks** `ShowSessionTypeDefinition` / `session_type_authoring`. |
| Required pin for this ticket | **`302190e`** (`302190ec2acc5ecee744432a6c9ffd1f040ebe01`) — protocol **6**, conformance **32**. Contains `DaemonRequest::ShowSessionTypeDefinition`, `DaemonSessionTypeEditableDefinition`, support-matrix `session_type_authoring`, and the published `hub-test-support` **0.1.25** package tree. |
| Authoring source commit | `2b8361b` — lossless session-type authoring view; conformance 31 → 32. |
| Proof tokens at `302190e` | `PROTOCOL_VERSION == 6`, `CONFORMANCE_FIXTURE_REVISION == 32`; matrix section `session_type_authoring.request_type == "show_session_type_definition"`; authored fields absent from published row: `context`, `environment`, `working_directory`. |
| Kit coupling | Kit `origin/main` still pins `botster-ui-contract` to `8a60bd58`. TUI cannot bump alone without dual `botster-ui-contract` sources (documented by prior bump `466adb3` + kit `551feb1`). |

**Implement must not begin** until `ticket_1786071998_949850` is closed with a **merged** kit commit this repository can pin, then this repo pins:

1. kit → that merged commit  
2. `botster-hub-client` / `botster-ui-contract` / `botster-hub-test-support` → Hub `302190e`  
3. `Cargo.lock` resolves **exactly one** `botster-ui-contract` source  

Live binaries for acceptance must be built from Hub ≥ `302190e` (same protocol 6 / conformance ≥ 32).

## Code reality on base (`16d10b4`)

| Surface | Current state |
| --- | --- |
| Entity subscriptions | Only `subscribe_session_entities` (`session`). No `session_type` store/pump |
| Product Spawn | `botster.tui.spawn` → `spawn_session()` → `DaemonRequest::Spawn { command }` with `DEFAULT_COMMAND` (`app.rs:53`, `:1116`, `:1521`) |
| Command form | System details `command_form()` still offers freeform Spawn |
| Session types UI | None |
| Spawn targets | No production `ListSpawnTargets` consumption |
| README IA | Aspirational Hub settings → Session types; code has workspace + System details |

## Scope

### In scope

0. **Prerequisite consumption (first Implement actions, after kit closed)**  
   - Pin kit + Hub crates to the coordinated revs above.  
   - Confirm single `botster-ui-contract` in `Cargo.lock`.  
   - Preflight: client constants protocol 6 / conformance 32; matrix includes `session_type_authoring`.

1. **`session_type` entity subscription and store**  
   - Held-open `subscribe_entities(..., "session_type", ...)`.  
   - Store parallel to `SessionEntityState`, keyed by Hub `session_type_id`, decode → `DaemonSessionType`.  
   - snapshot / upsert / remove; unexpected patch → diagnostic failure.  
   - `entity_error` surfaces Hub `code` + `message`; no resubscribe loop; no list-refresh fallback as truth.  
   - Connect/reconnect lifecycle mirrors sessions.  
   - Missing `session_type_entity_subscriptions` → surface-local unsupported state from Hub feature list.

2. **Session types management UI (keyboard-accessible)**  
   - First-class **Session types** section inside System details (surgical entry; not a full multi-page Hub settings shell).  
   - List grouped by Hub `source`; rows render Hub descriptors only (`label` never humanized from `id`; unknown roles/traits as literals).  
   - Detail shows command/args/policy/overrides/context keys/override chain.  
   - **Edit/Delete** gated solely on Hub `editable`.  
   - **Create** gated on writable sources: device; plus enabled admitted repo targets from `ListSpawnTargets`.  
   - **Update (lossless):**  
     - Open edit **only** after `ShowSessionTypeDefinition { session_type_id }` succeeds.  
     - Seed form from `DaemonSessionTypeEditableDefinition.definition` + retain `source` for the mutation.  
     - Submit `UpdateSessionType { source, definition }` as **wholesale** replacement of that definition (user-edited fields overlaid on the authoring read).  
     - **Forbidden:** seeding edit UI from entity row / `DaemonSessionType` (drops `environment` and relative working-directory path).  
   - Package rows: no edit/delete; static read-only from `editable == false`; Hub package mutation errors still render if forced.  
   - Form semantic rejections: Hub `kind` + `message` only.

3. **Target-first product launch**  
   - Operator toolbar `botster.tui.spawn` opens target-first flow → `DaemonRequest::SpawnSessionType`.  
   - Ordering: admitted target first → session types for that `target_id` (show `available == false` disabled with diagnostics, never silently drop) → optional context (e.g. prompt for interactive) → spawn.  
   - Use Hub effective `session_type_id` values exactly.

4. **Freeform Spawn product removal (decided contract)**  
   - Remove System details command form as a product spawn affordance.  
   - Remove or stop routing product `botster.tui.spawn` to freeform `DaemonRequest::Spawn { command }`.  
   - Delete product fields `self.command` / `DEFAULT_COMMAND` **or** confine them to `#[cfg(test)]` helpers only.  
   - **Tests that must be rewritten** (currently call raw `DaemonRequest::Spawn` or depend on command spawn):  
     - `workspaces_live_acceptance_runs_against_real_package` (`app.rs` ~13793) — keep Workspaces package spawn path (plugin), not freeform command; do not reintroduce product freeform spawn.  
     - `assert_live_attach_history_readback` (~14387, ~14664) — reseed sessions via `SpawnSessionType` or a `#[cfg(test)]` helper that is not product UI.  
     - `assert_plugin_contract_matrix_renders_through_tui` (~14753) — same.  
     - Any hermetic tests using `app.command = DEFAULT_COMMAND` + toolbar spawn (~4574 `run_headless_live_runtime`, ~9584 pending spawn, ~7317 spawn toolbar).  
   - Product hermetic invariant: activating toolbar Spawn never emits freeform `DaemonRequest::Spawn { command }`.

5. **Session classification presentation**  
   - When present on session entities, show Hub `session_type_id` / `session_type_source` / `role` / `traits` / `interaction` / `session_type_lifecycle` without reclassifying.

6. **Tests and docs**  
   - Hermetic + real-input + dedicated live `session-types` profile (below).  
   - README: Session types surface, lossless edit contract, target-first launch, pin/feature expectations, live profile, CARGO_TARGET_DIR workaround.

### Non-scope

- Hub protocol implementation (consume only).  
- `botster-tui-kit` product/renderer work beyond the registered pin ticket.  
- Full multi-page Hub settings IA.  
- `botster-web` / Workspaces package code.  
- `ticket_1786038825_352271` contract-matrix fix and `legacy_test_needs_system_details()` redesign.  
- Row-seeded editors.  
- Client-side reimplementation of Hub validation.  
- `session_template*` aliases.  
- Local filesystem writes of `.botster/session-types.json`.  
- Speculative `resolve_session_type` preflight.

## Repository ownership boundaries and cross-repo dependencies

### Owned here

App policy, Session types UI, entity projection, Hub request dispatch, target-first launch, freeform spawn removal, acceptance harnesses, README, Cargo pins **after** kit merge.

### Not owned here

| Concern | Owner |
| --- | --- |
| Session-type policy, authoring view, entity frames | `botster-hub` (consume `302190e`) |
| `botster-ui-contract` pin inside the kit | `botster-tui-kit` / `ticket_1786071998_949850` |
| Browser Session types UX | `botster-web` |
| Workspaces package spawn form | `botster-workspaces` |
| contract-matrix never-connected assertion | sibling `ticket_1786038825_352271` |
| Pipeline worktree path characters | `ticket_1786071999_889350` |

### Cross-repo actions

| Action | Status |
| --- | --- |
| Kit repin ticket created | `ticket_1786071998_949850` |
| Dependency registered | `dependency_1786072005_676257` on this ticket |
| Implement hold | Wait for closed kit ticket + merged kit commit before TUI pin edits |
| Project-pipelines colon ticket | `ticket_1786071999_889350` (non-blocking) |

## Assumptions and unknowns

### Assumptions (explicit)

1. **Invalidated:** “pin 8a60bd58 is sufficient.” Replaced by: acceptance requires Hub **`302190e`** (conformance 32 + authoring view), verified against hub source and support matrix at that rev.  
2. Canonical entity type string is `session_type`; frames are snapshot/upsert/remove/error (no patch).  
3. Effective `session_type_id` values are Hub-authoritative.  
4. `ListSpawnTargets` is the public control-plane enumeration for target pickers (not a parallel owner of session-type state).  
5. System details is the surgical Session types entry point; full Hub settings shell remains out of scope.  
6. Operator Spawn is exclusively target-first `SpawnSessionType`.  
7. This run does not modify `legacy_test_needs_system_details()` visibility policy.  
8. Engine dependency gates on **run start**, not every step activation — Implement must still **manually** verify kit dependency closed before editing pins (record kit commit in Implement evidence).

### Unknowns (non-blocking for Plan)

1. Exact kit merged commit hash (unknown until `ticket_1786071998_949850` lands).  
2. Whether default empty hub fixtures need create-first for live accessory cases (likely create-then-assert).  
3. Dense form focus order under System details scroll — prove via InputRouter.

## Affected surfaces / files

| Path | Change |
| --- | --- |
| `crates/botster-tui/Cargo.toml` + `Cargo.lock` | Pin hub crates to `302190e`; pin kit to closed kit commit |
| `crates/botster-tui/src/app.rs` | session_type store/pump; System details Session types; ShowSessionTypeDefinition edit; CRUD; target-first SpawnSessionType; remove product freeform spawn; rewrite named tests |
| `script/test-live-hub` | Add `session-types` profile **independent** of contract-matrix |
| `README.md` | Surface, edit contract, launch, pins, live profile, CARGO_TARGET_DIR |
| `docs/plans/tui-manage-and-launch-authoritative-hub-session-types-plan.md` | This plan (committed on the run branch) |

## Risks

| Risk | Mitigation |
| --- | --- |
| Dual `botster-ui-contract` sources | Kit dependency first; lockfile single-source check |
| Row-seeded edit data loss | Hard rule: edit only after authoring read; hermetic test fails if Update is built from entity row fields |
| Parallel list-refresh ownership | Entity subscription is store of truth |
| Freeform Spawn residual | Product removal + hermetic invariant on toolbar path |
| contract-matrix red misread as this ticket | Separate live profile; sibling ownership explicit |
| Colon worktree aborts tests | Documented `CARGO_TARGET_DIR` |
| Concurrent `app.rs` edits with sibling | Coordinate; do not take ownership of sibling helpers |
| Stale live binaries | Preflight protocol/conformance/features against ≥ `302190e` |

## Acceptance checks / tests

### Environment for default gates in **this** worktree

The assigned pipeline worktree path contains `:` (from `git@github.com:…`), which breaks Cargo on macOS when the target dir lives under the worktree:

```sh
# Required in this run's worktree for script/test and script/clippy:
export CARGO_TARGET_DIR="/tmp/botster-tui-cargo-tgt-ticket_1785970234_132113"
script/fmt
script/test
script/clippy
```

Plan Review verified base is green under a colon-free `CARGO_TARGET_DIR` (153 + 1 passed). `script/test-live-hub` already uses mktemp target dirs. Owner fix: `ticket_1786071999_889350`.

### Default gates (required)

- `script/fmt`
- `script/test` (with colon-free `CARGO_TARGET_DIR` as above)
- `script/clippy` (same)
- `git diff --check`
- `Cargo.lock` contains a single `botster-ui-contract` source at Hub `302190e`

### Hermetic proof (under `script/test`)

1. Session-type entity reducer: snapshot, upsert, remove, generation ignore, error frame.  
2. Rendering: editable vs package read-only; overrides/diagnostics; unknown role/trait literals.  
3. Create/delete dispatch payloads match Hub sources/definitions.  
4. **Lossless edit:** open edit path issues `ShowSessionTypeDefinition`; Update body preserves authored relative working-directory path and environment keys that were not edited; negative test: building Update solely from `DaemonSessionType` fields is not used by production path (source-scan or unit assertion).  
5. Target-first launch: no type control before target; toolbar Spawn → `SpawnSessionType`.  
6. Product freeform spawn absent: toolbar / System details product path does not emit `DaemonRequest::Spawn { command }`.  
7. Real keyboard path via InputRouter + HitMap for list/detail/create/edit/delete/launch.

### Live Hub proof — **`session-types` profile only**

Add `script/test-live-hub session-types` (or equivalent name) that:

- Builds/uses Hub + session-worker binaries from **≥ `302190e`**.  
- Does **not** invoke the red contract-matrix headless path that asserts never-connected `connection:`.  
- Does **not** depend on `legacy_test_needs_system_details()` sibling ownership.  
- Proves:

| Case | Observation |
| --- | --- |
| Interactive agent | Create/list/edit (authoring round-trip)/launch; session entity classification |
| Interactive accessory | Orthogonal descriptors |
| Service accessory | Service interaction does not force agent classification |
| Unknown namespaced tokens | Literal render |
| Package read-only | `editable: false`; no edit/delete; Hub error if forced |
| Device + repo CRUD | create/update/delete; entity upsert/remove without list refresh |
| Authoring path/env | create with relative WD path + env; edit unrelated field; path/env preserved |
| CRUD errors | Hub kind+message |
| Reconnect | exact expected `session_type_id` after snapshot |
| Launch metadata | spawned session entity carries Hub session-type fields |

**Explicit non-claim:** this ticket does **not** claim green `script/test-live-hub contract-matrix`. That remains `ticket_1786038825_352271`.

### Downstream / peer

- Kit: only the registered pin ticket.  
- Web parity: edit via authoring read (behavioral parity), no shared code.  
- Workspaces: no package change.

## Implementation sequence

1. **Hold** until kit `ticket_1786071998_949850` closed; record merged kit commit.  
2. Pin kit + Hub `302190e` crates; verify single ui-contract; run default gates.  
3. Add `SessionTypeEntityState` + pump + reconnect.  
4. Session types System details section (list/detail).  
5. Create/delete + **ShowSessionTypeDefinition → Update** path.  
6. Target-first SpawnSessionType; remove product freeform spawn; rewrite named tests.  
7. Hermetic + real-input tests.  
8. Live `session-types` profile + README.  
9. Default gates + live evidence in Implement report.

## Product decision ledger

- **Default:** Entity subscription owns session-type state.  
- **Default:** Product launch is target-first `SpawnSessionType` only.  
- **Default:** Edit seed = `ShowSessionTypeDefinition` only; Update is wholesale definition replacement.  
- **Default:** Hub pin `302190e`; kit pin from closed kit ticket.  
- **Default:** Freeform command spawn is not a product affordance.  
- **Non-goal:** Full Hub settings shell; kit renderer features; contract-matrix fix; row-seeded edit.  
- **Ask-human threshold:** only if Hub `302190e` is not consumable after kit merge (unexpected).

## Vault gaps worth capturing

1. TUI Session types entry under System details vs future Hub settings shell.  
2. Client cold-cut: freeform `Spawn` → `SpawnSessionType` as product launch.  
3. Lossless edit requires authoring view (entity row is not an edit seed) — cross-client gotcha.  
4. Pipeline worktree `:` vs Cargo DYLD — after owner ticket lands.  
5. Capture after Implement with exact pins and proof commands — not at Plan time as decisions.

## Botster layers touched

TUI application policy + hub client consumption + coordinated pin. Not: Hub runtime, kit mechanics (beyond pin), web, Workspaces, Project Pipelines package code.

## Worktree / target assumptions

- Implement only in the pipeline worktree for `tgt_c3d470bab78549df920a41e8fb0e58d8`.  
- Always use colon-free `CARGO_TARGET_DIR` for `script/test` / `script/clippy` in this worktree.  
- Do not treat ambient `Projects/` checkouts as edit authority.

## Pipeline gates and artifacts

| Gate / artifact | Requirement |
| --- | --- |
| Plan file | `docs/plans/tui-manage-and-launch-authoritative-hub-session-types-plan.md` committed on the run branch |
| `project_pipelines_add_artifact` | `kind=plan`, uri to that path; artifact id in gate evidence |
| `botster_stack_plan_gate` | All required fields; reference decision A, kit dependency, consumed pin `302190e` |

## Convention conflicts

**None.** Decision A aligns with browser/TUI parity, entity-frame-only state, Hub authority, kit/app ownership split, and closed-dependency-as-merged-source-not-pin.
