# TUI: manage and launch authoritative Hub session types

## Plan revision

| Field | Value |
| --- | --- |
| Pass | **3** — restarted Plan after kit merge and cancelled Implement incident |
| Prior run | `run_1786070915_943794` (cancelled after premature Implement activation while kit dependency was open) |
| This run | `run_1786074731_672724` — workspace *"Pipeline - TUI manage Hub session types (post-kit 902650d)"* |
| Product decision | **A** — full management including lossless Update (`question_1786071947_442525`) |
| Approved plan review (prior) | `review_1786072459_764932` (approved with two medium findings carried into this pass) |

### Finding disposition (this pass)

| Finding | Severity | Disposition |
| --- | --- | --- |
| Prior pass-2 blockers (edit seed / kit / artifact / live lane / colon / consumed artifact / freeform product contract) | blocker–low | **Already closed** in pass 2; restated here with consumable pins filled in. |
| `finding_1786072459_713781` — lock invariant omits botster-core (hub-test-support tracks core by branch) | medium | **Adopt.** After repin, assert `Cargo.lock` `botster-core?branch=main` equals hub@`302190e`'s recorded core rev **`33ebcd98d19031d23e91b03d8da0ee3f8d1410d4`**, using `cargo update -p botster-core --precise` if needed. |
| `finding_1786072459_445277` — freeform-Spawn rewrite list includes test-harness Hub seeding | medium | **Adopt.** Cold-cut is **product toolbar / System details only**. Live-lane harness may keep `DaemonRequest::Spawn` for controlled Hub seeding (Workspaces lifecycle restored by `ticket_1786036326_597046`). Mandatory rewrites limited to product-path tests. |
| `finding_1786072459_895473` — Assumption 8 / engine dependency enforcement | low (waived) | **Keep operational hold language historically correct** (Implement activated while kit open on prior run). On **this** run both dependencies are **closed**; no Implement hold remains for kit. |

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`git@github.com:trybotster/botster-tui.git`) |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Ticket | `ticket_1785970234_132113` — "TUI: manage and launch authoritative Hub session types" |
| Project | `project_1785970196_204877` — Botster session types and Hub maintenance control plane |
| Pipeline / run | `botster_stack_delivery` / `run_1786074731_672724` |
| Current step | `botster_stack_plan` |
| Base | `main` at `16d10b4` (PR #46 Workspaces acceptance restore) |
| Worktree | Pipeline worktree for this ticket; authoritative base_target_path is the botster-tui checkout for `tgt_c3d470bab78549df920a41e8fb0e58d8` |

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
- [[botster-tui-kit-playbook]] — renderer/input mechanics consumed; kit pin now consumable
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

### Deliberately not loaded as implementation scope

- [[project-pipelines-playbook]] — no Project Pipelines package/plugin code is edited in this ticket. Infra owner tickets for worktree `:` and step-activation dependency enforcement are registered separately against the project-pipelines target.

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

Exact pin from the answer: Hub **`302190e`** (`302190ec2acc5ecee744432a6c9ffd1f040ebe01`), origin/main tip after PR #196 at decision time. `c57d388` would also be acceptable; do not pin earlier than `2b8361b` / `6ad6dfa`. This plan standardizes on **`302190e`**.

Note: hub `origin/main` tip has advanced past `302190e` (e.g. Ghostty/Zig 0.16 work). **Do not chase hub tip** for this ticket; pin the authoring-view / conformance-32 coordinate the kit already resolved against.

### Registered dependencies

| Depends on | Title | Target | Status |
| --- | --- | --- | --- |
| `ticket_1785970233_236046` | Hub: make session types flexible, editable, and authoritative | hub | **closed** (merged source; not the consumable pin) |
| `ticket_1786071998_949850` | TUI kit: repin botster-ui-contract to Hub 302190e | `tgt_3dfae49c02454037bf13554f552baf7f` | **closed** — kit main merge **`902650d`** (`902650dfbd56a5bdc99c1e88c04ba2e62442f703`, PR #30) |

### Same-target sibling (non-blocking, non-owned)

| Ticket | Title | Relation |
| --- | --- | --- |
| `ticket_1786038825_352271` | contract-matrix live failure / System-details visibility | Owns red `script/test-live-hub contract-matrix` (`assert!(rendered.contains("connection:"))` near `app.rs:13258`) and `legacy_test_needs_system_details()` policy. FILE CONTENTION section names this ticket. **Do not run concurrent ownership fights on that helper.** This run does **not** change `legacy_test_needs_system_details()`. |

### Non-blocking owner tickets (pipeline infra)

| Ticket | Title |
| --- | --- |
| `ticket_1786071999_889350` | Project Pipelines: worktree directory names must not contain `:` |
| (filed by orchestrator) | Project Pipelines: enforce open blocking dependencies at step activation (incident on prior run) |
| (filed by orchestrator) | Project Pipelines: `resolve_finding` must not INSERT-crash on unknown `finding_id` |

## Consumed artifact (required)

Per [[closed dependency tickets signal merged source not a consumable release]]:

| Question | Answer |
| --- | --- |
| How does botster-tui consume Hub? | **Git rev** pins in `crates/botster-tui/Cargo.toml` for `botster-hub-client`, `botster-ui-contract`, and `botster-hub-test-support` from `https://github.com/trybotster/botster-hub.git`. Not npm. |
| How does botster-tui consume kit? | **Git rev** pin for `botster-tui-kit` from `https://github.com/trybotster/botster-tui-kit.git`. Prior art pins **merge commits** on kit main (e.g. `551feb1` for PR #29). |
| Base pin today (TUI main / this worktree base) | Hub crates at `8a60bd58841179f8b1fd4040d9362d18ea244230` — protocol **6**, conformance **31**. Kit at `551feb151f531d59d362efdae0cc7d3a34d8e311`. Has session-type CRUD shapes + entity subscriptions. **Lacks** `ShowSessionTypeDefinition` / `session_type_authoring` in the pin. |
| Required Hub pin | **`302190e`** (`302190ec2acc5ecee744432a6c9ffd1f040ebe01`) — protocol **6**, conformance **32**. Contains `DaemonRequest::ShowSessionTypeDefinition`, `DaemonSessionTypeEditableDefinition`, support-matrix `session_type_authoring`, published `hub-test-support` **0.1.25** package tree. |
| Required kit pin | **`902650d`** (`902650dfbd56a5bdc99c1e88c04ba2e62442f703`) — "Merge pull request #30" on kit main; kit `Cargo.toml` already pins `botster-ui-contract` to Hub `302190e`. |
| Authoring source commit | `2b8361b` — lossless session-type authoring view; conformance 31 → 32. |
| Proof tokens at Hub `302190e` | `PROTOCOL_VERSION == 6`, `CONFORMANCE_FIXTURE_REVISION == 32`; matrix section `session_type_authoring.request_type == "show_session_type_definition"`; authored fields absent from published row: full `environment`, relative working-directory **path** (row only has `working_directory_policy`). |
| Core lock side-effect | `botster-hub-test-support` depends on `botster-core` by **branch=main**. Hub@`302190e` `Cargo.lock` records **`33ebcd98d19031d23e91b03d8da0ee3f8d1410d4`**. TUI base today has `e36435f2…` for that source. Implement must re-lock to **`33ebcd98…`**, not tip of core main (post-Zig-0.16 tip is not what hub@`302190e` was tested against). |

**Implement first actions (no longer blocked on kit):**

1. Pin `botster-tui-kit` → `902650d`
2. Pin `botster-hub-client` / `botster-ui-contract` / `botster-hub-test-support` → Hub `302190e`
3. Ensure `Cargo.lock` has **exactly one** `botster-ui-contract` source at Hub `302190e`
4. Ensure `Cargo.lock` `botster-core?branch=main` is **`33ebcd98d19031d23e91b03d8da0ee3f8d1410d4`** (`cargo update -p botster-core --precise 33ebcd98d19031d23e91b03d8da0ee3f8d1410d4` if needed)
5. Preflight: client constants protocol 6 / conformance 32; matrix includes `session_type_authoring`

Live binaries for acceptance must be built from Hub ≥ `302190e` with protocol 6 / conformance ≥ 32 (prefer building the exact pin).

## Code reality on base (`16d10b4`)

| Surface | Current state |
| --- | --- |
| Entity subscriptions | Only `subscribe_session_entities` (`session`). No `session_type` store/pump |
| Feature flag | Hub exports `FEATURE_SESSION_TYPE_ENTITY_SUBSCRIPTIONS` (`session_type_entity_subscriptions`) at `302190e` |
| Product Spawn | `botster.tui.spawn` → `spawn_session()` → `DaemonRequest::Spawn { command }` with `DEFAULT_COMMAND` (`app.rs:53`, `:1116`, `:1521`) |
| Command form | System details `command_form()` still offers freeform Spawn |
| Session types UI | None |
| Spawn targets | No production `ListSpawnTargets` consumption |
| README IA | Aspirational Hub settings → Session types; code has workspace + System details |
| Live harness seeding | Workspaces / attach / contract-matrix helpers use raw `DaemonRequest::Spawn` as **Hub-client test scaffolding** (~13793, ~14387, ~14664, ~14753) — **allowed to remain** |

## Scope

### In scope

0. **Prerequisite consumption (first Implement actions)**
   - Pin kit `902650d` + Hub crates `302190e`.
   - Confirm single `botster-ui-contract` in `Cargo.lock`.
   - Confirm `botster-core?branch=main` == `33ebcd98…`.
   - Preflight: protocol 6 / conformance 32; matrix includes `session_type_authoring`.

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
   - Operator toolbar `botster.tui.spawn` opens target-first flow → `DaemonRequest::SpawnSessionType { session_type_id, session_id, request }`.
   - Ordering: admitted target first → session types for that `target_id` (show `available == false` disabled with diagnostics, never silently drop) → optional context (e.g. prompt for interactive) → spawn.
   - Use Hub effective `session_type_id` values exactly.

4. **Freeform Spawn product removal (decided contract — product path only)**
   - Remove System details command form as a product spawn affordance.
   - Stop routing product `botster.tui.spawn` to freeform `DaemonRequest::Spawn { command }`.
   - Delete product fields `self.command` / `DEFAULT_COMMAND` **or** confine them to `#[cfg(test)]` helpers only.
   - **Mandatory rewrites (product path):**
     - Hermetic tests using `app.command = DEFAULT_COMMAND` + toolbar spawn (`run_headless_live_runtime` ~4574, spawn toolbar ~7317, pending spawn ~9584, and any other product-toolbar Spawn tests).
   - **Explicitly allowed to keep raw `DaemonRequest::Spawn`:**
     - Live-lane **test harness** Hub seeding inside Workspaces lifecycle / attach history / contract-matrix helpers (`~13793`, `~14387`, `~14664`, `~14753`). These are not operator product affordances; rewriting them would force session-type fixtures into Workspaces lifecycle seeding and risk regressing `ticket_1786036326_597046`.
   - Product hermetic invariant: activating **toolbar** Spawn never emits freeform `DaemonRequest::Spawn { command }`.

5. **Session classification presentation**
   - When present on session entities, show Hub `session_type_id` / `session_type_source` / `role` / `traits` / `interaction` / `session_type_lifecycle` without reclassifying.

6. **Tests and docs**
   - Hermetic + real-input + dedicated live `session-types` profile (below).
   - README: Session types surface, lossless edit contract, target-first launch, pin/feature expectations, live profile, CARGO_TARGET_DIR workaround.

### Non-scope

- Hub protocol implementation (consume only).
- `botster-tui-kit` product/renderer work (kit already merged).
- Full multi-page Hub settings IA.
- `botster-web` / Workspaces package code.
- `ticket_1786038825_352271` contract-matrix fix and `legacy_test_needs_system_details()` redesign.
- Row-seeded editors.
- Client-side reimplementation of Hub validation.
- `session_template*` aliases.
- Local filesystem writes of `.botster/session-types.json`.
- Speculative `ResolveSessionType` preflight as a product gate.
- Rewriting live test-harness `DaemonRequest::Spawn` seeding into `SpawnSessionType`.
- Chasing hub tip past `302190e` / core tip past `33ebcd98`.

## Repository ownership boundaries and cross-repo dependencies

### Owned here

App policy, Session types UI, entity projection, Hub request dispatch, target-first launch, freeform product-spawn removal, acceptance harnesses, README, Cargo pins to already-merged kit + Hub coordinates.

### Not owned here

| Concern | Owner |
| --- | --- |
| Session-type policy, authoring view, entity frames | `botster-hub` (consume `302190e`) |
| `botster-ui-contract` pin inside the kit | `botster-tui-kit` (done: PR #30 / `902650d`) |
| Browser Session types UX | `botster-web` |
| Workspaces package spawn form | `botster-workspaces` |
| contract-matrix never-connected assertion | sibling `ticket_1786038825_352271` |
| Pipeline worktree path characters | `ticket_1786071999_889350` |
| Step-activation dependency enforcement | Project Pipelines engine tickets (orchestrator-filed) |

### Cross-repo actions

| Action | Status |
| --- | --- |
| Kit repin ticket | **closed** `ticket_1786071998_949850` |
| Dependency registration | `dependency_1786072005_676257` **closed** |
| Consumable kit merge commit | **`902650d`** (record in Implement evidence) |
| Project-pipelines colon ticket | `ticket_1786071999_889350` (non-blocking) |
| Implement hold for kit | **lifted** on this run |

## Assumptions and unknowns

### Assumptions (explicit)

1. Acceptance requires Hub **`302190e`** (conformance 32 + authoring view), verified against hub source and support matrix at that rev — not hub tip.
2. Canonical entity type string is `session_type`; frames are snapshot/upsert/remove/error (no patch).
3. Effective `session_type_id` values are Hub-authoritative (qualified `source/id` form).
4. `ListSpawnTargets` is the public control-plane enumeration for target pickers (not a parallel owner of session-type state).
5. System details is the surgical Session types entry point; full Hub settings shell remains out of scope.
6. Operator Spawn is exclusively target-first `SpawnSessionType`.
7. This run does not modify `legacy_test_needs_system_details()` visibility policy.
8. Prior-run incident: engine activated Implement while kit dependency was open. **On this run both dependencies are closed**; Implement still records kit + hub + core lock SHAs in evidence as belt-and-braces.
9. Live harness may continue to seed sessions via public Hub client `DaemonRequest::Spawn` without reintroducing product freeform spawn UI.
10. Branch-tracked `botster-core` through hub-test-support must be **precisely** `33ebcd98…` after the repin.

### Unknowns (non-blocking for Plan)

1. Whether default empty hub fixtures need create-first for live accessory cases (likely create-then-assert).
2. Dense form focus order under System details scroll — prove via InputRouter.
3. Exact line numbers in `app.rs` will shift after large edits; tests should be identified by name, not frozen line numbers.

## Affected surfaces / files

| Path | Change |
| --- | --- |
| `crates/botster-tui/Cargo.toml` + `Cargo.lock` | Pin kit `902650d`; hub crates `302190e`; lock core branch source to `33ebcd98…` |
| `crates/botster-tui/src/app.rs` | session_type store/pump; System details Session types; ShowSessionTypeDefinition edit; CRUD; target-first SpawnSessionType; remove product freeform spawn; rewrite **product-path** tests |
| `script/test-live-hub` | Add `session-types` profile **independent** of contract-matrix |
| `README.md` | Surface, edit contract, launch, pins, live profile, CARGO_TARGET_DIR |
| `docs/plans/tui-manage-and-launch-authoritative-hub-session-types-plan.md` | This plan (committed on the run branch) |

## Risks

| Risk | Mitigation |
| --- | --- |
| Dual `botster-ui-contract` sources | Pin kit first then hub crates; lockfile single-source check |
| Core branch drift / Zig tip | Precise pin `33ebcd98…`; assert in Implement evidence |
| Row-seeded edit data loss | Hard rule: edit only after authoring read; hermetic negative test |
| Parallel list-refresh ownership | Entity subscription is store of truth |
| Freeform Spawn residual in product UI | Product removal + hermetic invariant on **toolbar** path only |
| Accidental rewrite of Workspaces live seeding | Explicit allow-list for harness `DaemonRequest::Spawn` |
| contract-matrix red misread as this ticket | Separate live profile; sibling ownership explicit |
| Colon worktree aborts tests | Documented colon-free `CARGO_TARGET_DIR` |
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

Plan Review (prior run) verified base is green under a colon-free `CARGO_TARGET_DIR` (153 + 1 passed). `script/test-live-hub` already uses mktemp target dirs. Owner fix: `ticket_1786071999_889350`.

### Default gates (required)

- `script/fmt`
- `script/test` (with colon-free `CARGO_TARGET_DIR` as above)
- `script/clippy` (same)
- `git diff --check`
- `Cargo.lock` contains a **single** `botster-ui-contract` source at Hub `302190e`
- `Cargo.lock` `botster-core?branch=main` source equals **`33ebcd98d19031d23e91b03d8da0ee3f8d1410d4`**
- Kit pin in `Cargo.toml` / lock equals **`902650d`**

### Hermetic proof (under `script/test`)

1. Session-type entity reducer: snapshot, upsert, remove, generation ignore, error frame.
2. Rendering: editable vs package read-only; overrides/diagnostics; unknown role/trait literals.
3. Create/delete dispatch payloads match Hub sources/definitions.
4. **Lossless edit:** open edit path issues `ShowSessionTypeDefinition`; Update body preserves authored relative working-directory path and environment keys that were not edited; negative test: production path does not build Update solely from `DaemonSessionType` fields.
5. Target-first launch: no type control before target; toolbar Spawn → `SpawnSessionType`.
6. Product freeform spawn absent: **toolbar / System details product path** does not emit `DaemonRequest::Spawn { command }`.
7. Real keyboard path via InputRouter + HitMap for list/detail/create/edit/delete/launch.

### Live Hub proof — **`session-types` profile only**

Add `script/test-live-hub session-types` (or equivalent name) that:

- Builds/uses Hub + session-worker binaries from **≥ `302190e`** (prefer exact pin).
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

- Kit: already merged; only consume pin.
- Web parity: edit via authoring read (behavioral parity), no shared code.
- Workspaces: no package change; live harness Spawn seeding preserved.

## Implementation sequence

1. Pin kit `902650d` + Hub `302190e` crates; force core branch source to `33ebcd98…`; verify single ui-contract; run default gates.
2. Add `SessionTypeEntityState` + pump + reconnect (`FEATURE_SESSION_TYPE_ENTITY_SUBSCRIPTIONS`).
3. Session types System details section (list/detail).
4. Create/delete + **ShowSessionTypeDefinition → Update** path.
5. Target-first SpawnSessionType; remove product freeform spawn; rewrite **product-path** tests only.
6. Hermetic + real-input tests.
7. Live `session-types` profile + README.
8. Default gates + live evidence in Implement report (record kit/hub/core SHAs + single-source proofs).

## Product decision ledger

- **Default:** Entity subscription owns session-type state.
- **Default:** Product launch is target-first `SpawnSessionType` only.
- **Default (Review fix):** Launch step-one is the union of `ListSpawnTargets` and distinct `session_type.target_id` values from the entity store (synthetic labels for `device:local` / `package:*`); spawn flow is a workspace dialog reachable from the toolbar without System details.
- **Default:** Edit seed = `ShowSessionTypeDefinition` only; Update is wholesale definition replacement.
- **Default:** Hub pin `302190e`; kit pin `902650d`; core branch source `33ebcd98…`.
- **Default:** Freeform command spawn is not a product affordance; harness may still seed via raw Spawn.
- **Default (answered `question_1786075802_958194`):** `MINIMUM_CONFORMANCE_FIXTURE_REVISION` stays **31**. Session types degrade surface-locally when authoring / `session_type_entity_subscriptions` is missing. Do not hard-refuse conformance-31 Hubs.
- **Default:** `FEATURE_SESSION_TYPE_ENTITY_SUBSCRIPTIONS` deliberately stays **out** of `required_features`.
- **Default:** Live `session-types` profile fail-closes on observed handshake conformance `>= 32` and feature presence (evidence provenance, not the global client minimum).
- **Default:** Lock invariants record **one** `botster-ui-contract` at Hub `302190e` and **two** `botster-core` sources: direct rev `16bf08f2…` (unchanged) and `branch=main` at `33ebcd98…` via hub-test-support.
- **Non-goal:** Full Hub settings shell; kit renderer features; contract-matrix fix; row-seeded edit; rewriting Workspaces live seeding.
- **Ask-human threshold:** only if `902650d` or `302190e` become unresolvable or dual ui-contract sources cannot be eliminated without further kit work.

## Vault gaps worth capturing

1. TUI Session types entry under System details vs future Hub settings shell.
2. Client cold-cut: freeform product `Spawn` → `SpawnSessionType` as product launch (harness Spawn remains valid).
3. Lossless edit requires authoring view (entity row is not an edit seed) — cross-client gotcha.
4. `botster-hub-test-support` branch-tracked core must be re-locked to the hub pin's recorded core rev on every consumer repin.
5. Pipeline worktree `:` vs Cargo DYLD — after owner ticket lands.
6. Capture after Implement with exact pins and proof commands — not at Plan time as decisions.

## Botster layers touched

TUI application policy + hub client consumption + coordinated pin. Not: Hub runtime, kit mechanics (beyond pin), web, Workspaces, Project Pipelines package code.

## Worktree / target assumptions

- Implement only in the pipeline worktree for `tgt_c3d470bab78549df920a41e8fb0e58d8`.
- Always use colon-free `CARGO_TARGET_DIR` for `script/test` / `script/clippy` in this worktree.
- Do not treat ambient `Projects/` checkouts as edit authority.
- Do not pin early dual-source states; kit is already merged so pin kit + hub in one coherent step.

## Pipeline gates and artifacts

| Gate / artifact | Requirement |
| --- | --- |
| Plan file | `docs/plans/tui-manage-and-launch-authoritative-hub-session-types-plan.md` committed on the run branch |
| `project_pipelines_add_artifact` | `kind=plan`, uri to that path; artifact id in gate evidence |
| `botster_stack_plan_gate` | All required fields; reference decision A, kit pin `902650d`, Hub pin `302190e`, core lock `33ebcd98…` |

## Convention conflicts

**None.** Decision A aligns with browser/TUI parity, entity-frame-only state, Hub authority, kit/app ownership split, and closed-dependency-as-merged-source-not-pin.
