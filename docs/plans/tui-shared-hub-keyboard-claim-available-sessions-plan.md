# TUI: shared-Hub keyboard claim via Available sessions entity_options

## Pipeline identity and routing

| Field | Value |
| --- | --- |
| Ticket | `ticket_1786529885_807584` — TUI: shared-Hub keyboard claim via Available sessions entity_options |
| Target repository | `trybotster/botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Run | `run_1786546300_948152` |
| Pipeline | `botster_stack_delivery` |
| Step | `botster_stack_plan` |
| Repository charter | [[botster-tui-playbook]] |
| Role overlays | [[planner-playbook]], then [[botster-planner-playbook]] |

Target resolved from admitted spawn targets for `tgt_c3d470bab78549df920a41e8fb0e58d8` (`botster-tui` at `/Users/jasonconigliari/Projects/botster-tui`), **not** from the ambient worktree directory name.

## Why this ticket exists

Parent integration `ticket_1786474783_285888` (claim-stack) Review finding
`finding_1786529668_405039` states:

- The parent campaign omits the required **TUI keyboard claim** on the **same clean Hub** as Web.
- Supporting `script/test-live-hub workspaces lifecycle` owns another Hub and uses plugin MCP to seed membership; its keyboard path **removes** membership and does **not** claim an external session through realized `botster_workspaces.add_session`.
- Supporting consumer proofs are necessary but insufficient.

This ticket is the routed TUI owner dependency (`dependency_1786529892_674521`) that must expose a **public production keyboard seam** the parent can drive.

## Runtime-teardown class

**Does not apply.** Ordinary client UI / acceptance-driver work over existing entity-options materialization and Workspaces keyboard helpers. Do not load [[botster runtime teardown lenses]].

## Consumer of Hub session-type eligibility parent

**Does not apply.** This ticket consumes merged entity-options + Workspaces Available sessions product pins, not Hub session-type eligibility work. Do not inject list_session_types_for_target parent-pin ritual.

## Context loaded

### Ticket intent (authoritative)

Provide a production keyboard path (or public acceptance seam) so a parent-owned clean Hub can claim an unclaimed running session through the shared owner-authored Workspaces Available sessions `entity_options` form without package MCP `add_session`, `list_sessions` polling, or surface refresh.

Acceptance from the ticket:

1. Caller-owned shared Hub attach (`BOTSTER_LIVE_DATA_DIR` / Hub connection injection, or documented `apps open` injection) without reinstalling packages.
2. Open Workspaces surface → open Add existing session → keyboard-select Available sessions option for a seeded unclaimed UUID → dispatch realized `botster_workspaces.add_session`.
3. Assert exact `session_uuid` submission, membership join, and entity-frame exclusion.
4. No force interaction; no direct MCP claim as the UI proof path.
5. Document the exact entrypoint for parent claim-stack consumption.

### Parent and product lineage

| Ticket | Role | Status |
| --- | --- | --- |
| `ticket_1786474783_285888` | Parent claim-stack integration (Workspaces target) | open; blocked on this ticket |
| `finding_1786529668_405039` | Parent Review finding that routed this ticket | product/high |
| `ticket_1786474781_871159` | TUI reactive entity-backed select options | closed; merged PR #50 at `abc804e1…` |
| `ticket_1786474780_590414` | Workspaces Available sessions picker | closed |
| `ticket_1786507221_760227` / membership publish stack | Membership entity fanout after claim/remove | closed |
| `ticket_1786494180_266672` | Hub package entity fanout + empty snapshot arrays | closed |

### Repository facts (authoritative product tip)

- **Authoritative base for Implement:** `origin/main` ≥ `abc804e19bc3e01465cd308c11de5f4292331c3d` (entity-options + implement-report SHA). Prefer current `origin/main` tip when this plan is executed (includes later Ghostty terminal client work); do not implement from the stale pipeline spawn tip.
- **Pipeline worktree at Plan time** was branched at `d230798…` and **lacks** `crates/botster-tui/src/entity_options.rs`. First Implement step must hard-sync/rebase onto `origin/main` ≥ entity-options before product edits.
- Already on main (≥ `abc804e1`):
  - Multi-family `EntityOptionsStore` with generation discipline
  - Materialization of `options_source` into kit-ready `Select`/`SelectOption` before kit handoff
  - Process-wide reuse of `/session` (and `session_type`) subscriptions for options demand
  - Isolated live entity-options fixture proof
  - Workspaces keyboard helpers: `select_acceptance_value`, `select_only_acceptance_value`, `type_acceptance_text`, `activate_acceptance_action`, Tab focus over hit map
  - Caller-owned **spawn** acceptance mode via `BOTSTER_TUI_ACCEPTANCE_SCENARIO` + `BOTSTER_TUI_ACCEPTANCE_EVIDENCE` and `BOTSTER_HUB_CONNECTION` / `BOTSTER_HUB_DATA_DIR`
  - Isolated `script/test-live-hub workspaces {plumbing,lifecycle,installed-driver}` lanes
- **Missing on main:** any public shared-Hub path that opens the owner-authored Available sessions form and keyboard-claims through realized `botster_workspaces.add_session` with exact UUID oracles (the gap named by the parent finding).

### Workspaces producer surface (consume, do not edit)

On Workspaces Available sessions tip (parent claim-stack package):

| Control | Identity |
| --- | --- |
| Open Add dialog | Button action `botster_workspaces.open` with payload `{ selected_workspace, dialog = "add:" .. workspace.id }` |
| Available sessions select | Node id `botster-workspaces-add-session-id`, field name `session_id`, label product “Available sessions”, `options_source` entity_options |
| Value field | Hub `/session` `session_uuid` |
| Exclude family | `/botster-workspaces.membership` by `session_uuid` |
| Advanced historical | `botster-workspaces-add-session-id-advanced` / `session_id_advanced` — **not** the normal claim path |
| Submit | Form action `botster_workspaces.add_session` |

Display fields when present: label, session_uuid, lifecycle, lifecycle_class, session_type_id, spawn_point.

### Env naming note

Parent ticket prose says `BOTSTER_LIVE_DATA_DIR`. This repository’s established caller-owned injectors are:

- `BOTSTER_HUB_CONNECTION` — validated hub connection JSON
- `BOTSTER_HUB_DATA_DIR` — hub data directory context

Implement must document the exact env names the parent should export. Prefer reusing existing injectors rather than inventing a parallel `BOTSTER_LIVE_DATA_DIR` alias unless parent integration already hard-codes that name; if an alias is required for parent ergonomics, map it to the existing decode path without dual sources of truth.

## Playbooks and atomic notes loaded

### Role / charter

- [[planner-playbook]]
- [[botster-planner-playbook]]
- [[botster-tui-playbook]]
- [[botster-tui-kit-playbook]] (does-not-own boundary only; no kit feature work expected)

### Architecture / client equality

- [[botster-architecture]]
- [[cli-patterns]]
- [[tui and browser are equal clients]]
- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[botster tui uinode event routing captures hit regions during draw]]
- [[plugin authored tui surfaces dispatch via action props not node id literals]]
- [[conformance helpers must dispatch the action id read from the rendered node]]
- [[session UUID is the sole routing key across all layers]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]
- [[botster entity snapshots are authoritative reconnect baselines]]
- [[acceptance harness region oracles must key on node identity not concatenated text]]
- [[tui error dedup tests must drive real input handlers]]
- [[shared hub workspaces acceptance omits package path without skipping its lane]]

### Process / plan hygiene

- [[plan agents must author vault context as wikilinks not home paths]]
- [[pipeline vault checklists must cite exact resolvable note titles]]
- [[vault example paths are not repository placement conventions]]
- [[plan review must verify a plan artifact exists before trusting gate summaries]]
- [[plan steps need reviewable plan artifacts]]

### Not loaded (correctly)

- [[project-pipelines-playbook]] — Project Pipelines package/plugin paths not in implementation scope
- [[botster runtime teardown lenses]] — teardown class does not apply
- [[botster-web-playbook]] / [[botster-workspaces-playbook]] as ownership charters for edits — consumers only; product edits stay in `botster-tui`

## Scope

### In scope (`botster-tui` only)

1. **Hard-sync Implement worktree** to `origin/main` ≥ entity-options pin before coding.
2. **Caller-owned shared-Hub keyboard claim acceptance seam** (primary deliverable), modeled on the existing spawn-driver acceptance mode:
   - Activate via explicit env scenario/evidence (or a dedicated schema sibling such as `botster.tui.workspaces-claim-driver/v1`) **or** a clearly named profile of the acceptance entry that cannot silently collide with spawn scenario validation.
   - Require `BOTSTER_HUB_CONNECTION` + `BOTSTER_HUB_DATA_DIR` (and document any parent alias).
   - **Never** start/stop Hub, install/enable packages, reinstall packages, or call package MCP `add_session` / `list_sessions` as the claim proof path.
   - Assume parent already: cleaned Hub data dir, installed/enabled Workspaces package, created workspace `W`, spawned/seeded unclaimed running session `S` outside membership.
3. **Production path sequence** (all through realized hit map + `InputRouter` key events):
   1. Connect; wait for authoritative `/session` baseline that includes exact `S` ([[acceptance readiness requires the exact expected entity not any authoritative snapshot]]).
   2. Open admitted Workspaces surface (existing navigation/`botster.tui.navigation.open` path).
   3. Select/open workspace detail for `W` if required by presentation state.
   4. Keyboard-activate realized **Add existing session** control (`botster_workspaces.open` + dialog payload) — action id and node id from rendered hit map, not hardcoded force dispatch.
   5. Wait until field `session_id` / node `botster-workspaces-add-session-id` materializes entity-backed options including exact `S` (options come from entity-options projection of `/session` with membership exclude).
   6. Keyboard-select option value `S` via production select open/Down/Enter path (`select_acceptance_value` or equivalent).
   7. Submit the form through production keyboard/submit path so realized action id is `botster_workspaces.add_session`.
   8. Assert request audit / action payload carries exact `session_uuid`/`session_id` value equal to `S` ([[session UUID is the sole routing key across all layers]]).
   9. Assert membership join observable through package action result and/or membership entity frames.
   10. Assert Available sessions option for `S` **excludes** after claim via entity-frame reconciliation **without** surface refresh used as synchronization and without reopening the form as the only recovery path for exclusion proof (dialog close from owner replacement is allowed after successful claim if that is producer behavior; exclusion must still be proven on a subsequent held open of Add or via store/options observation that does not call `list_sessions`).
4. **Structured evidence** JSONL (or sibling schema) with stages the parent can correlate: connect, surface open, add dialog open, option present, keyboard select, submit, exact uuid, membership join, option excluded.
5. **Hermetic unit/integration coverage** that drives real key handlers for the claim select+submit path (fixture frames / local entity-options pumps acceptable for offline unit proof; not a substitute for shared-Hub parent seam).
6. **README documentation** of the exact parent entrypoint: env vars, scenario schema, command shape, minimum Workspaces/Hub pins, and explicit non-goals (no MCP claim, no force interaction).
7. **Optional supporting live isolated smoke** only if it reuses the same production driver without becoming the parent’s shared-Hub substitute. Prefer not inventing a fourth Workspaces isolated profile unless needed to keep `./test.sh` green when binaries are present.

### Non-scope

- Workspaces package product changes (Available sessions authoring, membership DB, publish).
- Hub fanout / ui-contract / entity_options contract changes.
- Web harness or dual-browser race (parent owns campaign orchestration).
- Using lifecycle MCP seed/`botster_workspaces.add_session` tool as the UI claim proof.
- Advanced historical UUID field as the primary claim path (may be documented out-of-scope residual for parent C5).
- Full reconnect/sequence-gap matrix for claim pickers (parent Web owns named drop control; TUI gap already covered by entity-options unit/live residual unless a defect appears).
- Kit feature work, Ghostty terminal-client redesign, session-type eligibility synthesis.
- Project Pipelines package/plugin code.
- Speculative configurability, broad refactors, or cleanup unrelated to the claim seam.

## Repository ownership boundaries and cross-repo dependencies

| Layer | Owns | This ticket |
| --- | --- | --- |
| `botster-tui` | Client keyboard path, entity-options materialization consumption, acceptance seam, evidence, docs | **Implement here** |
| `botster-tui-kit` | Generic Select hit map / InputRouter | Consume only |
| `botster-hub` / ui-contract | Entity frames, options_source contract | Consume pins ≥ entity_options + membership fanout |
| `botster-workspaces` | Owner-authored Available sessions form, `add_session`, membership publish | Consume installed package on shared Hub; **no package edits** |
| Parent Workspaces claim-stack | Clean Hub lifecycle, Web dual-browser, race, historical fallback, pin ledger | Downstream consumer of this seam |

### Cross-repository prerequisites

| Dependency | Target | Registration |
| --- | --- | --- |
| Entity-options TUI product | botster-tui main ≥ `abc804e1…` | Already closed/merged — Implement bases on it |
| Available sessions + membership publish | Workspaces main ≥ Available sessions tip used by parent | Already closed — parent supplies package pin |
| Hub fanout + empty arrays | Hub main ≥ parent pin | Already closed |
| Parent claim-stack | `ticket_1786474783_285888` | Already depends on this ticket (`dependency_1786529892_674521`); do not reverse-register |

If Implement discovers a product defect in Workspaces/Hub/Web, stop and register a dependency on that repository’s target rather than patching foreign product code in this run.

## Assumptions and unknowns

### Assumptions

1. Parent seeds `W` and unclaimed running `S` on a clean shared Hub before invoking the TUI claim seam.
2. Workspaces package on the shared Hub is already installed/enabled at a pin that authors Available sessions `entity_options` (not the pre-picker Session ID text field).
3. Entity-options materialization on TUI main correctly projects `/session` + membership exclude into field `session_id` options without surface refresh.
4. Existing spawn acceptance env pattern is the right model; claim needs a **non-colliding** schema/activation so spawn scenario validation does not reject claim scenarios.
5. Process-wide `/session` subscription is sufficient for Available sessions options; membership exclude family requires entity-options subscription (not process-wide today) and must be demanded by the rendered surface.

### Unknowns to resolve during Implement (not Plan blockers)

1. Exact preferred activation shape: sibling env schema (`workspaces-claim-driver/v1`) vs profile flag vs dual-mode scenario file. Choose the smallest non-colliding design.
2. Whether successful claim’s owner `replacement` tree closes the dialog before exclusion can be observed on the open form; if so, prove exclusion by reopening Add once **after** claim **without** surface-refresh-as-sync and without MCP, or by observing options store/membership frames while documenting the producer close behavior.
3. Whether parent literally requires env name `BOTSTER_LIVE_DATA_DIR` vs documented mapping to `BOTSTER_HUB_DATA_DIR`.
4. Whether post-entity-options Ghostty main tip requires acceptance-driver adjustments beyond claim work (fix only if claim path breaks).

## Affected surfaces / files (expected)

| Path | Change |
| --- | --- |
| `crates/botster-tui/src/acceptance.rs` | Claim scenario/evidence schema + validation (or sibling module) |
| `crates/botster-tui/src/app.rs` | Claim acceptance driver sequence; evidence stages; unit tests for keyboard claim path |
| `crates/botster-tui/src/entity_options.rs` | Only if claim proof exposes a real gap (prefer no change) |
| `crates/botster-tui/fixtures/*claim*` | Scenario/evidence fixtures if file-based |
| `crates/botster-tui/src/main.rs` | Module wiring only if new module |
| `README.md` | Document parent entrypoint, env vars, pins, non-goals |
| `docs/plans/tui-shared-hub-keyboard-claim-available-sessions-plan.md` | This plan (tracked) |
| `docs/reports/*claim*implement-report.md` | Implement report (Implement stage) |
| `script/test-live-hub` | Only if a supporting isolated profile is strictly necessary |

## Implementation sequence

1. Sync worktree to `origin/main` ≥ `abc804e1…` (prefer current main tip); restore `.gitignore` if wiped; set colon-free `CARGO_TARGET_DIR` if path requires it.
2. Re-read Workspaces Available sessions form ids on the parent-supplied package pin (do not invent field names).
3. Design non-colliding claim acceptance activation + evidence schema.
4. Implement production keyboard driver reusing `InputRouter` helpers; forbid MCP/`list_sessions`/force paths in the proof.
5. Hermetic tests for keyboard select+submit exact uuid.
6. Live shared-Hub manual/scripted proof with parent-shaped injectors (or documented `apps open` injection).
7. README entrypoint for parent claim-stack.
8. `cargo fmt`, clippy `-D warnings`, `./test.sh`, targeted live proof when binaries present.
9. Implement report with exact commands, pins, and residual risks.

## Risks

| Risk | Mitigation |
| --- | --- |
| Stale pipeline worktree missing entity-options | Hard-sync first; fail closed if `entity_options` module absent |
| Spawn scenario schema collision | Separate schema/profile; deny_unknown_fields validation |
| MCP lifecycle path mistaken for claim proof | Explicit assertions forbidding tool claim; evidence stages name keyboard path |
| Option exclusion races / dialog close | Wait oracles on exact option presence/absence; document producer close; no surface-refresh sync |
| Advanced field steals precedence | Leave advanced empty; assert primary field supplies uuid |
| Parent env name mismatch | Document exact env contract in README + evidence |
| Over-broad product edits | Surgical acceptance + docs; no Workspaces/Hub patches |

## Acceptance checks / tests

### Product path (required)

Production entry must be:

`caller Hub inject` → `TuiApp` connect → Workspaces surface → realized Add open → entity-options materialize Available sessions → keyboard select exact `S` → form submit → realized `botster_workspaces.add_session` with exact uuid → membership join + option exclusion without `list_sessions` / MCP claim / force interaction.

Code existence of `entity_options` alone is **not** enough; the claim seam must exercise that path.

### Repository gates

| Check | Requirement |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | pass |
| `git diff --check` | pass |
| `./test.sh` | pass (unit + package) |
| Hermetic claim keyboard test(s) | pass; drive real InputRouter keys |
| Shared-Hub claim seam | Documented command + evidence stages green against clean Hub with seeded `W`/`S` |
| README entrypoint | Names exact env vars, schema, action id, field id, non-goals |

### Downstream proof for parent

Parent `ticket_1786474783_285888` C2 must be able to:

1. Export connection + data-dir injectors for its clean Hub.
2. Invoke the documented TUI claim entrypoint with workspace id + session uuid (scenario or args).
3. Read evidence asserting exact uuid on `botster_workspaces.add_session` and exclusion/join facts.

This ticket does **not** run the full Web dual-browser campaign.

### Explicit fail conditions

- Using package MCP `add_session` as the UI proof.
- Typing advanced historical field for the normal claim.
- Force-dispatching action payloads without hit-map focus.
- Polling `list_sessions` or refreshing surface to “make” the option appear/disappear.
- Waiting only for “any snapshot” without exact `S`.

## Vault gaps worth capturing

1. **Caller-owned shared-Hub claim seam pattern** — sibling of spawn-driver acceptance; parent multi-client campaigns need documented keyboard claim entrypoints, not only isolated lifecycle lanes.
2. **Lifecycle MCP seed ≠ claim UI proof** — isolated Workspaces lifecycle that MCP-seeds membership does not satisfy shared-Hub claim keyboard acceptance.
3. Possibly: **owner replacement after claim closes dialogs** — exclusion observation strategy for clients after successful `add_session` replacement trees.

Capture only after Implement proves the live path; author as wiki-linkable note titles via inbox/pipeline.

## Product decision ledger

| Decision | Choice |
| --- | --- |
| Primary deliverable | Caller-owned shared-Hub keyboard claim acceptance seam |
| Interaction | Production InputRouter keyboard only |
| Value identity | Exact Hub `session_uuid` |
| Action identity | Realized `botster_workspaces.add_session` from hit map |
| Package MCP claim | Forbidden as UI proof |
| Advanced historical field | Out of primary path |
| Teardown lenses | N/A |
| Base tip | `origin/main` ≥ `abc804e1…` |

## Completion evidence for this Plan step

Gate evidence must include: `plan_uri`, `artifact_id`, `checklist_id`, `target_id`, `target_repository`.
