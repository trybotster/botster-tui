# TUI: drop client session-type eligibility synthesis; use Hub spawn-point list

## Plan metadata

| Field | Value |
| --- | --- |
| Ticket | `ticket_1786387865_677482` — TUI: drop client session-type eligibility synthesis; use Hub spawn-point list |
| Run | `run_1786394077_353311` |
| Step visit | **Pass 2** — Plan after Plan Review `changes_required` (`review_1786395012_823938`) |
| Prior Plan visit | sequence 1 (`run_step_1786394078_443203`); prior artifact `artifact_1786394337_692666` |
| Pipeline | `botster_stack_delivery` |
| Project | `project_1785970196_204877` |
| Base | ticket `main` worktree for target below |
| Parent dependency | `ticket_1786387816_590636` (Hub eligibility) — **closed**; Hub main includes PR #202 |
| Runtime-teardown class | **Does not apply** (client eligibility/presentation; no WebRTC/SessionIo/ClientWorker teardown) |

### Finding disposition (Plan Review pass 1 → this plan)

| Finding | Severity | Disposition |
| --- | --- | --- |
| `finding_1786395012_510678` — Hermetic gate commands fail in this worktree as written | high | **Adopt.** One colon-free `CARGO_TARGET_DIR` for **all** of `script/fmt`, `script/test`, `script/clippy`, and `script/test-live-hub`. Cite `ticket_1786071999_889350` as non-blocking workflow owner of colon paths. Live gate records exact Hub/session-worker binary provenance. |
| `finding_1786395012_365187` — Plan ignores unrelated worktree damage | high | **Adopt.** Restore tracked `.gitignore` before Implement (already restored on this Plan visit). Require clean tracked state except the plan/code for this ticket. Implement report + commit gate must prove `.env` / `mise.local.toml` are gitignored and uncommitted. |
| `finding_1786395012_724262` — Loading state requires unplanned async design | medium | **Adopt.** Drop loading-state requirement. Use existing **synchronous** `request` / `request_and_apply` on pick-target. Store successful list rows on spawn-flow state; surface transport/operator errors immediately. |
| `finding_1786395012_584413` — Project Pipelines workflow context and sibling risk missing | medium | **Adopt.** Load [[project-pipelines-playbook]]. Record open same-target sibling `ticket_1786038825_352271` (app.rs contending). Require fresh rebase + one-writer check before Implement mutates. |
| `finding_1786395012_785952` — Plan completion evidence omits durable artifact; duplicate checklist | medium | **Adopt.** This visit includes new plan `artifact_id` in gate + advance evidence. Duplicate Plan vault checklist `checklist_1786394332_347576` items marked **skipped** with supersession evidence. |
| `finding_1786395012_114899` — Error-state acceptance proof underspecified | medium | **Adopt.** Mandatory hermetic production-handler test: real input → pick target → list fails (operator or transport) → no stale selectable rows, no successful advance to spawn pick, recoverable. |

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`git@github.com:trybotster/botster-tui.git`) |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Resolved path (spawn target) | admitted Hub target `botster-tui` → path from `list_spawn_targets` (not ambient cwd name) |
| Run worktree | pipeline worktree for this ticket (path may contain `:` → always set colon-free `CARGO_TARGET_DIR`) |

Authoritative routing comes from the ticket/run `target_id`. Do **not** infer repository identity from the process working directory string.

## Repository playbook loaded

- [[botster-tui-playbook]] — ownership charter for this target

## Other role / surface playbooks and atomic notes loaded

### Role overlays (required order)

1. [[planner-playbook]]
2. [[botster-planner-playbook]]
3. [[botster-tui-playbook]]
4. [[project-pipelines-playbook]] — workflow policy: worktrees, artifacts, gates, findings, one-writer, vault checklists (in scope for pipeline-run discipline even though package/plugin code is not edited)

### [[botster-planner-playbook]] Must Load set (consulted)

- [[botster-architecture]]
- [[cli-patterns]]
- [[spa-patterns]] — peer product parity (Web is sibling consumer, not an edit target)
- [[project pipeline orchestration belongs in a device-level botster plugin]]
- [[project pipelines needs an operator workbench not more primitives]]
- [[project pipelines ui contract belongs in the plugin readme]]
- [[botster orchestration should spawn agents with explicit target ids]]
- [[botster orchestration prompts must bind agents to explicit worktrees]]
- [[plan agents must author vault context as wikilinks not home paths]]
- [[pipeline vault checklists must cite exact resolvable note titles]]
- [[vault example paths are not repository placement conventions]]

### [[project-pipelines-playbook]] notes applied this visit

- [[pipeline run worktrees allow only one active writer]]
- [[verification evidence is scoped to a stable commit and clean tree]]
- [[plan review must check open sibling tickets that own part of the plan scope]]
- [[plan review must verify a plan artifact exists before trusting gate summaries]]
- [[implement gate must verify committed work and pr link before review]]

### Targeted atomic notes (ticket surface)

- [[tui and browser are equal clients]] — same Hub list contract Web will use
- [[tui client attach uses hub protocol not session protocol]] — public hub-client boundary only
- [[device hub owns admitted spawn targets not ambient repo cwd]] — pick real admitted `T`, never invent `device:local`
- [[hub qualifies effective session type ids as source name slash id]] — render/spawn Hub effective ids (`device/…`, `repo/…`)
- [[incomplete repo local session types drop the hub client connection]] — pin/fixture migration when hub pin advances
- [[adding a hub client feature constant is a three site change]] — expect **no** new feature constant; list-for-target is additive on conformance 33
- [[web-session-creation-must-be-target-first]] — target-first product flow stays; only eligibility source changes
- [[botster-hub-client-playbook]] — consume published DTOs/request from pin; do not invent protocol
- [[external client hub tests use subprocess spawned hub test support]] — live proof shape
- [[tui error dedup tests must drive real input handlers]] — error and keyboard proofs use production handlers

### Explicitly not loaded

- [[botster runtime teardown lenses]] — ticket is not runtime-teardown class
- [[botster-hub-playbook]] as edit charter — Hub work is closed parent; this run only **consumes** the published seam
- [[botster-web-playbook]] as edit charter — Web is sibling ticket, out of scope

## Context loaded

### Ticket intent (condensed)

Align first-party TUI with **fat Hub, skinny client** session-type spawn:

1. For admitted spawn point `T`, request Hub’s target-scoped list (same contract Web will use).
2. Remove client synthesis of synthetic launch targets (`device:local`, `package:<name>`) that paper over Hub list holes.
3. Spawn with Hub-returned `session_type_id` + real `target_id = T`.
4. Keep management/catalog UX on Hub entity/actions; do not re-filter Global types out of spawn for `T`.
5. Prove keyboard-accessible launch against pinned Hub: Global device type visible for real spawn point `T` and launches successfully.

### Parent Hub contract (closed dependency — consumed artifact)

Hub PR #202 / main at plan time: `cb93df53d66fead323973b5233d4589562cf57b1` (merge of eligibility work; feature commit `9d5dab7`, follow-up `8cf9636`).

| Seam | Behavior |
| --- | --- |
| `DaemonRequest::ListSessionTypesForTarget { target_id }` | Additive request; response kind `SessionTypes` with eligible **available** winners for enabled admitted `T` |
| Eligibility Option A | Device Global types (no exclusive pin) eligible at **every** enabled admitted spawn point; list projects `target_id = T` (list context) |
| Precedence | Filter eligible sources **before** package < device < repo precedence |
| Spawn acceptance | Same set as list-for-target for `target_id = T` |
| Conformance | Hub client `CONFORMANCE_FIXTURE_REVISION = 33` |
| Management catalog | Unchanged: entity subscription / `ListSessionTypes` |

### Current TUI production behavior (problem evidence)

In `crates/botster-tui/src/app.rs`:

| Site | Current behavior | Ticket problem |
| --- | --- | --- |
| `launch_target_options` | Admitted targets ∪ synthetic entity `target_id`s (`device:local`, `package:…`) | Client-owned launch-target invention |
| `target_first_spawn_nodes` PickSessionType | Filters entities with `entity.target_id == selected` | Fat-client re-derivation; Globals never match real `T` |
| Product/live spawn tests | Drive `device:local` | Encodes rejected Option B |
| `request` / `request_and_apply` | **Synchronous** from input handlers | Plan must not invent async loading UX |

### Pin state at plan time

| Dep | Current TUI pin | Required |
| --- | --- | --- |
| hub-client / ui-contract / hub-test-support | `302190ec2acc5ecee744432a6c9ffd1f040ebe01` | ≥ `cb93df53d66fead323973b5233d4589562cf57b1` |
| `ListSessionTypesForTarget` on current pin | **Absent** | Present after pin |
| `MINIMUM_CONFORMANCE_FIXTURE_REVISION` | `31` | **33** |
| Live session-types floor | ≥ 32 | ≥ **33** + pin-matched binaries |

### Worktree hygiene (this visit)

| Item | Status |
| --- | --- |
| Tracked `.gitignore` emptied by bootstrap | **Restored from HEAD** on this Plan visit |
| `.env` / `mise.local.toml` | Covered by restored ignore rules; must never be committed |
| Allowed untracked before Implement commit | Plan file (and later implement code only) |
| Colon in worktree path | Present; all cargo gates use colon-free `CARGO_TARGET_DIR` |

### Sibling / contending tickets (same target)

| Ticket | Status | Relation |
| --- | --- | --- |
| `ticket_1786387816_590636` Hub eligibility | closed | Registered dependency; consumed |
| `ticket_1786038825_352271` never-connected connection-error / shell surface (also referenced from README for contract-matrix live failure history) | **open** | Same target; touches `app.rs`; **separable** product scope but concurrent-writer / rebase risk |
| `ticket_1786071999_889350` colon path / cargo target dir workflow | open (non-blocking) | Owns durable fix for colon worktree paths; this plan **works around** with explicit `CARGO_TARGET_DIR` |

**Implement preflight (required):** one-writer check; `git fetch` + rebase onto current `origin/main` if needed; do not start concurrent mutation with another active app.rs writer on this worktree ([[pipeline run worktrees allow only one active writer]]).

## Scope

### In scope (this run / this repo only)

1. **Repin** Hub crates to parent-capable revision ≥ `cb93df5`; refresh `Cargo.lock`; preserve dual `botster-core` discipline (direct rev + hub-test-support branch pin). Record locked core branch rev in implement report.
2. **Product launch target step:** only **enabled admitted** `DaemonSpawnTarget` rows. Delete entity-based synthesis of `device:local` / `package:…`.
3. **Product session-type step:** on pick of real `T`, **synchronously** call `DaemonRequest::ListSessionTypesForTarget { target_id: T }` via existing `request` / `request_and_apply`. Store returned rows on **spawn-flow state only** (do not overwrite session-type entity subscription store). Render picker from those rows.
4. **Spawn:** `SpawnSessionType` with Hub effective `session_type_id` + `DaemonSessionTypeRequest { target_id: Some(T), … }` (`T` admitted; never invent `device:local` for product launch).
5. **Error handling (sync, no loading UX):** on transport or operator error for list-for-target, set `error` / feedback, keep flow recoverable (stay on target pick or empty picker without selectable stale rows from a prior target). On success with empty list, honest empty state. **No** new asynchronous request subsystem.
6. **Tests (hermetic)** — rewrite synthesis-positive tests; add proofs for list request observation, Global via list not entity equality, spawn `target_id = T`, source-scan against production entity-equality filter, InputRouter keyboard/mouse path, **and list-failure error path** (below).
7. **Live proof:** pin-matched Hub; real admitted `T`; Global via list-for-target through product path; launch with `target_id = T`; record binary provenance.
8. **Docs:** README Session types cold-cut; implement report under `docs/reports/`.
9. **Conformance:** `MINIMUM_CONFORMANCE_FIXTURE_REVISION = 33` + unit updates.
10. **Worktree/commit hygiene:** keep restored `.gitignore`; never stage `.env` / `mise.local.toml`; commit gate proves ignore coverage.

### Non-scope

- Hub list/materialize/eligibility implementation (parent closed).
- Web client implementation (sibling).
- Core taxonomy / kit-only styling.
- Async request redesign or spinner/loading UX for list-for-target.
- Management catalog entity subscription redesign (beyond pin-forced compile fixes).
- Freeform product `DaemonRequest::Spawn`.
- Option B `device:local`-only product launches without human waiver.
- Durable fix for colon-in-path cargo (owned by `ticket_1786071999_889350`; workaround only).
- Fixing `ticket_1786038825_352271` never-connected shell (separable; rebase only).

## Repository ownership boundaries and cross-repo dependencies

| Layer | Owns in this change | Does not own |
| --- | --- | --- |
| **botster-tui** | Product presentation, target-first dialog, list-for-target call, skinny spawn payload, tests/docs, pin consume | Eligibility policy |
| **botster-hub** (parent closed) | Eligibility, list, spawn acceptance | TUI UI |
| **botster-hub-client** (pin) | Request/response DTOs | App policy |
| **botster-web** | Browser parity (sibling ticket) | This PR |
| **botster-core** / **botster-tui-kit** | — | This change |

### Cross-repo / pipeline dependencies

| Dependency | Status | Action |
| --- | --- | --- |
| Hub eligibility `ticket_1786387816_590636` | closed | Consume pin; already registered |
| Hub pin ≥ `cb93df5` | required consume | In-repo pin only |
| Web sibling | out of scope | Do not broaden run |
| `ticket_1786038825_352271` | open same-target | Separable; one-writer + rebase; **not** a blocking dependency edge |
| `ticket_1786071999_889350` colon paths | open | Non-blocking; workaround with `CARGO_TARGET_DIR` |

## Assumptions and unknowns

1. Parent Option A remains product law.
2. Hub list-for-target returns **available winners only**; spawn picker does not re-show unavailable catalog rows.
3. Zero enabled admitted targets → honest empty launch (no synthetic device target).
4. Effective ids stay source-qualified; pass through without inventing bare ids.
5. List response must not clobber entity subscription catalog — flow-local storage.
6. `request` remains synchronous; no mid-request render of loading state is required or planned.
7. Raising conformance floor to 33 is intentional cold-cut for this client.
8. Workspaces live lanes may need pin-matched binaries/fixtures; fix only within TUI ownership unless incomplete repo-local types force fixture migration ([[incomplete repo local session types drop the hub client connection]]).

### Ask-human thresholds

- Pin missing `ListSessionTypesForTarget` or binaries unavailable at pin SHA.
- Product reverses Option A toward synthetic `device:local` only.
- Operator requires concurrent Implement while another agent owns this worktree writer slot.

## Affected surfaces / files (expected)

| Path | Change |
| --- | --- |
| `crates/botster-tui/Cargo.toml` | Hub pin bump |
| `Cargo.lock` | Pin + dual-core consistency |
| `crates/botster-tui/src/app.rs` | launch options, spawn flow, sync list-for-target, picker, spawn target_id, tests, live profile, conformance |
| `README.md` | Session types product launch + pin SHAs + gate env note |
| `.gitignore` | **No product change** — restore/maintain HEAD content only if bootstrap empties it again |
| `docs/plans/tui-drop-client-session-type-eligibility-synthesis-plan.md` | This plan |
| `docs/reports/…-implement-report.md` | Implement later |

## Implementation sequence

1. **Preflight:** confirm one-writer; restore `.gitignore` if emptied again; `git status` clean except this ticket; set `export CARGO_TARGET_DIR=/tmp/botster-tui-cargo-tgt-session-types` (or any colon-free absolute path).
2. Repin Hub crates ≥ `cb93df5`; verify `ListSessionTypesForTarget` compiles; lock dual core.
3. Raise `MINIMUM_CONFORMANCE_FIXTURE_REVISION` to 33.
4. Spawn-flow state for Hub list rows (and clear on cancel / new target pick). No loading flag.
5. `launch_target_options` → admitted-only.
6. `spawn_pick_target(T)` → sync `ListSessionTypesForTarget`; on Ok store rows; on Err surface error and do not present selectable stale rows.
7. Render/pick from flow rows → `SpawnSessionType` with real `T`.
8. Hermetic tests + error-path + InputRouter path + source-scan.
9. Live session-types profile: real `T`, Global, launch with `T`; record binary paths + SHAs.
10. README cold-cut; gates; commit without secrets; PR; implement report with artifact/path evidence.

## Risks

| Risk | Mitigation |
| --- | --- |
| Colon worktree breaks cargo | Single colon-free `CARGO_TARGET_DIR` for all script gates; cite `ticket_1786071999_889350` |
| Bootstrap empties `.gitignore` again | Preflight restore; commit gate proves ignore of `.env` / `mise.local.toml` |
| List response clobbers entity catalog | Flow-local rows only |
| Silent entity `target_id` filter returns | Source-scan / negative test |
| Concurrent app.rs writers (`ticket_1786038825_352271`) | One-writer + rebase preflight |
| Live binaries older than pin | Fail closed; record provenance |
| Failed list leaves stale picker | Explicit error-path test |

## Acceptance checks / tests

### Environment for **all** repo gates (required)

Pipeline worktree paths often contain `:`. Cargo then fails on `DYLD_FALLBACK_LIBRARY_PATH` joins unless `CARGO_TARGET_DIR` is colon-free.

```sh
export CARGO_TARGET_DIR="/tmp/botster-tui-cargo-tgt-session-types"
# Non-blocking workflow owner of durable colon-path fix: ticket_1786071999_889350
script/fmt
script/test
script/clippy
```

Do **not** run bare `script/test` without that export in this class of worktree.

### Hermetic proofs (required)

1. No production launch-target synthesis of `device:local` / `package:` from entities.
2. Product pick of admitted `T` observes `ListSessionTypesForTarget { target_id: T }`.
3. Picker membership comes from Hub list rows for `T`, including a device Global presented for real `T` (not via entity equality filter).
4. `SpawnSessionType` carries Hub `session_type_id` + `request.target_id = T`.
5. Keyboard/hit-map path through production InputRouter handlers for the launch dialog.
6. Conformance floor 33 unit assertions green.
7. **List failure path:** production handler (real input or production pick_target entry) against operator error and/or transport error for list-for-target → no selectable stale session-type rows for a failed load, does not complete a successful spawn pick from empty/stale data, error surfaced, flow remains cancellable/recoverable (e.g. cancel or re-pick target).
8. Management/catalog entity paths remain covered (no accidental CRUD coverage loss).

### Live / downstream proof (required)

```sh
export CARGO_TARGET_DIR="/tmp/botster-tui-cargo-tgt-session-types"
# BOTSTER_HUB_BIN and BOTSTER_SESSION_WORKER_BIN = binaries built from pin ≥ cb93df5
# Record in implement report: absolute paths, `botster-hub --version` / identity, and git SHA used to build
script/test-live-hub session-types
```

Must prove production path:

1. Handshake conformance ≥ 33; session type management feature present as today.
2. Real admitted spawn point `T` in the test hub.
3. Device Global appears via **list-for-target through TUI product spawn flow** for `T`.
4. Launch succeeds with `target_id = T` (not `device:local`); authoritative session carries expected `session_type_id`.
5. Report records Hub + session-worker binary provenance (paths + pin SHA).

### Production path statement

Toolbar `botster.tui.spawn` → `begin_target_first_spawn` → pick admitted `T` → sync `ListSessionTypesForTarget` → pick listed type → `execute_spawn_session_type` / `SpawnSessionType` with `target_id = T`. Evidence must show that chain.

### Worktree / commit acceptance (required by review)

- Tracked tree clean aside from this ticket’s intentional files.
- `.gitignore` matches mainline ignore of `/target/`, `/.env`, `/.env.*`, `/mise.local.toml`.
- Commit and implement report assert `.env` and `mise.local.toml` are not in the commit.

## Vault gaps worth capturing

1. First-party clients must not invent launch targets from session-type entity `target_id`; spawn pickers call Hub `ListSessionTypesForTarget` for admitted `T`.
2. Spawn-point list returns available winners only — unavailable diagnostics stay on management catalog.
3. Pipeline worktrees with `:` in the path require colon-free `CARGO_TARGET_DIR` for **all** cargo-backed script gates (until `ticket_1786071999_889350` lands).

If already captured from Hub parent, Implement records “no new capture” with note titles.

## Product decision ledger

| Item | Decision |
| --- | --- |
| Eligibility owner | Hub (parent Option A) |
| Client synthesis of `device:local` | **Remove** |
| Picker data source | `ListSessionTypesForTarget` only |
| Request model | Existing **sync** client request |
| Loading UX | **Out of scope** |
| Management catalog | Entity/authoring unchanged |
| Minimum conformance | **33** with pin |
| Web | Sibling non-goal |
| Runtime-teardown | N/A |

## Gate evidence map

| Required field | Location |
| --- | --- |
| target_repository | `botster-tui` |
| target_id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| repository_playbook | [[botster-tui-playbook]] |
| playbooks_notes_loaded | sections above (includes [[project-pipelines-playbook]]) |
| context_loaded | ticket + parent Hub + code + worktree hygiene + sibling tickets |
| scope | Scope / Non-scope |
| ownership_boundaries_dependencies | tables |
| assumptions_unknowns | section |
| affected_surfaces_files | table |
| risks | Risks |
| acceptance_checks_tests | Acceptance (incl. error path + CARGO_TARGET_DIR + provenance) |
| vault_gaps | section |
| teardown fields | N/A |
| durable plan artifact | re-attached this visit with `artifact_id` in gate/advance evidence |
