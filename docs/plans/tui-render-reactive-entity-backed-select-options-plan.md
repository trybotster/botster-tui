# TUI: render reactive entity-backed select options

## Plan revision (Plan Review `changes_required`)

Responds to `review_1786481460_700986` / findings:

| Finding | Resolution in this revision |
| --- | --- |
| Kit prerequisite open but not registered | Hard dependency **re-registered** on `ticket_1786480740_176724` (kit target). Plan no longer claims soft-only tracking. Implement is blocked until kit closes and the merged kit commit pins Hub `891cc79…`. |
| Weak live proof residual | Soft residual **removed**. Isolated live Hub production-path proof is **required**. Missing producer/admission becomes an owner-repo dependency, not a waiver. |
| Generation / ordered-gap recovery incomplete | Per-family subscription generation state machine specified (mirror existing `SessionEntityState`). Production-path tests required for stale generation, out-of-order deltas, gap recovery, surface replacement, reconnect. |
| Plan document uncommitted | Plan committed on the run branch (this revision). |

## Outcome

Make `botster-tui` consume the merged Hub `botster-ui-contract` entity-backed
`ui.select` producer (`options_source` / `$kind: entity_options`) as a **generic
client**:

1. collect required entity families from the rendered surface body with the
   **shared** collector;
2. issue explicit `SubscribeEntities` for each family (no list_sessions / list
   refresh fallback);
3. apply authoritative snapshots and ordered entity changes into a multi-family
   store with **per-family subscription generation + sequence discipline** that
   feeds the **shared** projector;
4. materialize `Select` nodes into kit-ready static `options` slots with compact
   labels and metadata;
5. invalidate selection when the bound value disappears or is excluded;
6. submit the **exact** option value through the production hit map and action
   route;
7. re-render on entity frames **without** a surface refresh.

Success is visible on the real production path:

`plugin surface body` → family collect + subscribe → entity frames →
`project_entity_options_from_store` → realized `Select`/`SelectOption` tree →
kit `render_node` / `HitMap` → `InputRouter` keyboard/mouse → draft values →
`handle_plugin_action` / form submit with exact value.

**Live proof is part of done**, not optional residual: an isolated live Hub must
drive that same path end-to-end at least once.

## Pipeline identity and routing

| Field | Value |
| --- | --- |
| Ticket | `ticket_1786474781_871159` — TUI: render reactive entity-backed select options |
| Target repository | `trybotster/botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Run | `run_1786480500_850089` |
| Pipeline | `botster_stack_delivery` |
| Repository charter | [[botster-tui-playbook]] |
| Role overlays | [[planner-playbook]], then [[botster-planner-playbook]] |
| Kit charter (pin seam only) | [[botster-tui-kit-playbook]] |
| Project Pipelines package/plugin code | **Out of scope** — do not load [[project-pipelines-playbook]] for implementation |

Target resolved from admitted spawn targets for
`tgt_c3d470bab78549df920a41e8fb0e58d8` (`botster-tui`), **not** from the ambient
worktree directory name.

## Runtime-teardown class

**Does not apply.** This ticket is ordinary client UI / entity projection /
select materialization. Do not load [[botster runtime teardown lenses]].

## Context loaded

### Ticket intent (authoritative)

Adopt the merged `botster-ui-contract` artifact and render entity-backed
`ui.select` options through a generic entity state path. Implement only
contract-defined projection, ordering, exclusion, metadata display, focus
reconciliation, selection invalidation, and action value submission. Update from
authoritative snapshots and ordered entity changes without a surface refresh.

Show primary label plus available lifecycle / session type / spawn point
metadata in a compact TUI form. Submit the exact bound value. When the selected
value disappears or becomes excluded, move focus safely, prevent stale
submission, and show a clear state change.

Consume the same shared Hub contract fixture as Web. Prove keyboard selection
and submission through the production hit map and action route. Prove source
snapshot, exclusion snapshot, upsert, patch, remove, reappearance, reconnect,
ordered-change gap, duplicate value, Unicode, constrained width, and selection
removal.

**Explicit non-goals in the ticket:** botster-workspaces logic, `/session`
policy ownership, `list_sessions` polling, list refresh fallback, duplicate
contract structs, renderer-specific producer fields.

### Closed parent (Hub contract)

| Field | Value |
| --- | --- |
| Dependency ticket | `ticket_1786474779_865884` — Hub contract: add reactive entity-backed select options |
| Status | **closed** |
| Hub target | `tgt_7e208a0c76a44980a83b63af976b1f22` |
| Merged Hub HEAD (pin target) | `891cc796faeab51ee4bee1a0e8494562b233036e` |
| Contract crate version | `botster-ui-contract` **0.3.2** |
| Shared fixture key | `entity_options_reactive_timeline` via `conformance_fixtures_json()` |

Hub boundary (locked by parent plan/report):

| Layer | Owns |
| --- | --- |
| `botster-ui-contract` | Descriptor validation; `collect_entity_option_families`; pure `project_entity_options` / `project_entity_options_from_store`; `EntityFamilyStore` + frame helpers; shared fixture |
| Hub | Surface admission of source + exclude families; snapshot-then-ordered-changes when subscribed; reconnect baseline |
| Client (this ticket) | Walk body with shared collector; explicit `SubscribeEntities`; run shared projector; render; select; submit exact value |

### Sibling tickets (not owned here)

| Ticket | Target | Relation |
| --- | --- | --- |
| `ticket_1786474780_865627` Web: render reactive entity-backed select options | botster-web | Parallel consumer of the same fixture; do not implement Web here |
| `ticket_1786474780_590414` Workspaces: available sessions picker | botster-workspaces | Product owner of an entity-options surface; not TUI product work |
| `ticket_1786480740_176724` TUI Kit: pin ui-contract to Hub 891cc79 | botster-tui-kit | **Hard blocking pin prerequisite** (registered) |

### Repository facts (current worktree)

- Branch: `project-pipelines/ticket_1786474781_871159` at `d230798` (+ this plan commit).
- Pins today (pre-Implement):
  - Hub client / ui-contract / hub-test-support:
    `0ee42e9b84a0b0e9b0ab89834675535c8b831993` (**pre** entity-options)
  - `botster-tui-kit`: `571523d93a62945208ebb2bb75262dcdd78001a2` (same old Hub pin)
- `third_party/botster-ui-contract` is historical vendored pin copy (`PIN.md`);
  prefer pure dual-git same-rev alignment with kit after kit merge.
- Existing entity stores (`SessionEntityState`, `SessionTypeEntityState`) already
  implement subscription generation via `begin_generation`, `matches`,
  `accepts_delta` (`has_snapshot` + `subscription_id` + `snapshot_seq` strictly
  increasing). Entity-options multi-family store **must reuse that discipline**,
  not only `apply_entity_options_frame` map mutation.
- Plugin path today: `plugin_surface_render_root` →
  `materialize_plugin_surface` (bind_list / `$bind` / bind_if only) → kit.
  `options_source` is **not** a materialization trigger yet.
- Kit already renders realized `Select`/`SelectOption` with `Field` /
  `SelectOption` hit roles. No kit feature work beyond the Hub pin.
- Live harness exists: `script/test-live-hub` +
  `headless_live_runtime_runs_against_isolated_hub_when_binaries_are_available`
  / mode filters. Contract-matrix fixture today has **no** `options_source`
  surface; Hub runtime test
  `entity_options_select_admits_dual_families_and_serves_fresh_snapshots`
  shows the admitted producer shape TUI live proof must exercise (or register
  an owner fixture dependency if that surface cannot be owned in-repo).
- Docs placement prior art: `docs/plans/*.md` (active).

### Playbooks and atomic notes loaded

**Role / charter**

- [[planner-playbook]]
- [[botster-planner-playbook]]
- [[botster-tui-playbook]]
- [[botster-tui-kit-playbook]] (pin / does-not-own boundary only)

**Architecture / surface**

- [[botster-architecture]]
- [[cli-patterns]]
- [[spa-patterns]] (parity expectation with Web; no SPA edits)
- [[tui and browser are equal clients]]
- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[botster tui uinode event routing captures hit regions during draw]]
- [[tui client attach uses hub protocol not session protocol]]
- [[botster tui attach must explicitly pull core entities after subscribing]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]
- [[botster rust consumers that share ui contract must pin one hub revision]]

**Entity / surface binding**

- [[plugin surfaces request model state through ui bindings not hub subscribe]]
- [[plugin surface handlers must validate against hub locked uinode contract]]
- [[plugin dynamic ui lists bind to plugin-owned entities]]
- [[ui bind list empty template renders entity backed empty rows]]
- [[botster entity snapshots are authoritative reconnect baselines]]
- [[session UUID is the sole routing key across all layers]]
- [[cross-client ui should share semantic primitives and actions with renderer-specific adapters]]

**Proof / process**

- [[tui error dedup tests must drive real input handlers]]
- [[renderer acceptance tests must drive real frame backend]]
- [[plan agents must author vault context as wikilinks not home paths]]
- [[pipeline vault checklists must cite exact resolvable note titles]]
- [[vault example paths are not repository placement conventions]]
- [[plan review must verify a plan artifact exists before trusting gate summaries]]
- [[plan steps need reviewable plan artifacts]]

**Not loaded (correctly)**

- [[project-pipelines-playbook]] — package/plugin paths not in implementation scope
- [[botster runtime teardown lenses]] — teardown class does not apply

## Scope

### In scope (`botster-tui`)

1. **Cold pin set** (only after kit prerequisite **closed** and verified):
   - `botster-hub-client`, direct `botster-ui-contract`, `botster-hub-test-support`
     → Hub `891cc796faeab51ee4bee1a0e8494562b233036e`
   - `botster-tui-kit` → **merged** kit revision from
     `ticket_1786480740_176724` whose own `botster-ui-contract` pin is the
     **same** Hub Git source
   - Regenerate `Cargo.lock`; prove a single `botster-ui-contract` source
     (`cargo tree -i botster-ui-contract` unambiguous — no dual Git revs)
   - Refresh `third_party/botster-ui-contract` + `PIN.md` only if patch identity
     is still required after same-rev dual pins
   - Update README pin prose

2. **Multi-family entity-options store with generation discipline** on `TuiApp`:
   - Hold projected field maps compatible with contract `EntityFamilyStore`
   - **Per family** retain: `subscription_id`, `has_snapshot`, `snapshot_seq`
     (same contract as `SessionEntityState`)
   - `begin_generation(family, subscription_id)` on new subscribe, reconnect,
     surface family-set change that re-subscribes that family: clear that
     family's records + seq, set new `subscription_id`, `has_snapshot=false`
   - Apply `DaemonEntityFrame` only when:
     - **Snapshot:** `subscription_id` + `entity_type` match current generation
       → replace family map; set `has_snapshot`; set `snapshot_seq`
     - **Upsert/Patch/Remove delta:** `accepts_delta` = match generation +
       `has_snapshot` + `snapshot_seq` **strictly greater** than current → apply
       then advance seq; else **reject** (return false / no mutate)
     - **Error:** match generation → surface diagnostic; do not apply foreign
       errors
   - **Stale generation:** frames with old `subscription_id` after
     `begin_generation` never mutate the store
   - **Out-of-order / non-advancing seq:** rejected; do not mutate
   - **Gap recovery:** when a delta is rejected because `!has_snapshot`, or after
     subscription Error, or on reconnect: issue a **fresh** `SubscribeEntities`
     (new subscription id / generation) and require an authoritative Snapshot
     before any delta is accepted. Shared fixture step `gap_recovery_snapshot`
     must be exercised through this production reducer path, not only pure
     projector math
   - **Surface replacement:** when active plugin surface body changes, re-run
     `collect_entity_option_families`; `begin_generation` / unsubscribe families
     no longer required; subscribe new ones; never leave cross-surface stale
     families contributing options
   - Reuse existing session / session_type subscriptions when family ids
     overlap; **do not** open a second subscription for the same family.
     Overlap still requires a single generation owner that feeds both navigator
     (if applicable) and options projection without double-apply races
   - Call shared `project_entity_options_from_store` only; no second projector

3. **Materialize entity-backed selects** before kit handoff:
   - For each `UiNodeKind::Select` with `options_source`:
     - deserialize `UiEntityOptionsSource`
     - `project_entity_options_from_store(descriptor, store, selection)`
     - build `options` slot of `SelectOption` children: exact `value`, compact
       `label` (primary display field first), compact secondary metadata from
       remaining present `display_fields`
     - realized tree must satisfy `validate_realized` / kit capabilities (no
       unresolved `options_source` xor-invalid combo for kit)
     - if `selection_valid == false`: clear/refuse draft, set visible field
       `error` / feedback, block silent stale submit
   - Re-materialize whenever the options store changes while a plugin surface
     is active (entity frame path), not only on surface render RPC

4. **Input / action correctness (production routes)**:
   - Keyboard (and existing mouse) through kit hit map + `InputRouter` → drafts
   - Submit carries exact option value string
   - Focus reconciliation when selected option removed/excluded

5. **Tests**
   - Shared `entity_options_reactive_timeline` through TUI store + materializer
     + kit render/hit/action where applicable for every named step:
     `source_snapshot`, `exclude_snapshot`, `source_upsert`, `source_patch`,
     `source_remove`, `exclude_remove`, `duplicate_values`, `unicode_labels`,
     `reconnect_snapshot`, `gap_recovery_snapshot`, `selection_invalid`
   - **Generation / race tests (production reducer path):**
     - old `subscription_id` frames ignored after `begin_generation`
     - out-of-order / non-increasing `snapshot_seq` deltas ignored
     - delta before first snapshot ignored
     - gap recovery via re-subscribe + authoritative snapshot
     - surface replacement drops prior family generation
     - reconnect begins new generation and requires new snapshot
   - Constrained-width render proof
   - Keyboard select + submit through **real** input handlers
   - No `list_sessions` / extra surface-render budget on pure entity updates

6. **Mandatory isolated live Hub proof** (charter / ticket production path):
   - Run against isolated live Hub binaries (existing headless live harness
     pattern under `script/test-live-hub` / `--headless-live-runtime`)
   - Install/enable an **admitted owner-authored** package surface that emits
     `ui.select` with `options_source` (TUI-owned fixture package under
     `crates/botster-tui/fixtures/…` modeled on Hub's admitted dual-family
     producer, **or** a consumable hub-test-support fixture that already
     admits the families at the pinned Hub rev)
   - Prove on that live path, not helpers alone:
     1. surface render → body contains `options_source`
     2. `collect_entity_option_families` → explicit `SubscribeEntities`
     3. authoritative snapshots (source + exclude as applicable) arrive as
        `DaemonEntityFrame`
     4. options materialize into kit-rendered select
     5. keyboard selection through production hit map / `InputRouter`
     6. action submit carries **exact** bound value
     7. at least one post-baseline ordered change updates options **without**
        a second surface render RPC
   - Shared fixture timeline remains required for full matrix coverage in unit
     tests; it does **not** replace live proof
   - **If** no admitted producer can be owned from botster-tui (e.g. exclude
     family admission blocked without a Hub/support fixture change), create and
     **register** an owner-repository dependency ticket instead of waiving live
     proof. Do not mark Implement complete with soft residual

7. **Artifacts**
   - Plan: `docs/plans/tui-render-reactive-entity-backed-select-options-plan.md`
     (committed on the run branch)
   - Implement report later under `docs/reports/`

### Out of scope

- Hub contract schema, projector, or admission changes (except consuming merged pin)
- botster-web implementation
- botster-workspaces product picker / plugin_db membership
- botster-tui-kit renderer feature work beyond the pin prerequisite
- `/session` lifecycle policy ownership
- `list_sessions` polling or list-refresh fallback
- Second / local projector or duplicate DTO structs
- Project Pipelines package or orchestration code
- Runtime teardown / WebRTC / SessionIo peer lifecycle
- Broad refactors of session navigator beyond shared-generation needs

## Repository ownership boundaries and cross-repo dependencies

| Repository | Boundary for this ticket |
| --- | --- |
| **botster-tui** (this run) | App policy: subscribe demand, multi-family generation store, materialization, selection invalidation, action drafts, live fixture ownership for proof, tests |
| **botster-hub** | Closed prerequisite. Consume merged artifact at `891cc79…` only |
| **botster-tui-kit** | Pin prerequisite `ticket_1786480740_176724`. Owns reusable Select render/hit/input. No product options_source logic in kit |
| **botster-web** | Parallel consumer; parity via shared fixture |
| **botster-workspaces** | Future producer surface; not implemented here |
| **botster-core** | Unchanged pin unless hub-test-support forces a reviewed dual-core update |

### Registered dependencies

1. **Closed:** Hub contract `ticket_1786474779_865884`.
2. **Hard open (product gate):** TUI Kit pin `ticket_1786480740_176724` on
   target `tgt_3dfae49c02454037bf13554f552baf7f`.
   - Edge **registered** via `project_pipelines_add_ticket_dependency`.
   - This ticket's pipeline advance / Implement **must not** proceed while the
     kit ticket is open.
   - Before Implement activates: verify kit **merged** commit pins
     `botster-ui-contract` to Hub `891cc796faeab51ee4bee1a0e8494562b233036e`
     (or the exact same Git source the TUI will use). Record that kit SHA in
     the implement report.
   - Do **not** repin Hub on TUI while kit remains on `0ee42e9…` — that splits
     `botster-ui-contract` type identity ([[botster rust consumers that share ui
     contract must pin one hub revision]]).
3. **Conditional (only if live producer cannot be owned in TUI):** register an
   owner-repo dependency for an admitted entity-options surface fixture rather
   than waiving live proof.

## Assumptions and unknowns

### Assumptions

1. Hub main `891cc796faeab51ee4bee1a0e8494562b233036e` is the consumer pin for
   the closed dependency.
2. Kit pin is mechanical (Cargo rev + lock + gates); kit Select already accepts
   realized options.
3. “Existing generic entity state path” means contract `EntityFamilyStore` +
   existing entity-frame / subscribe plumbing extended with generation
   discipline already proven on `SessionEntityState`.
4. TUI can own a minimal admitted live fixture package for dual-family
   `options_source` proof (patterned on Hub runtime test) under
   `crates/botster-tui/fixtures/`. If package admission policy prevents that
   without Hub support changes, that is a registered dependency — not a soft
   residual.
5. Compact metadata formatting is client presentation only.

### Unknowns (resolve in Implement; escalate only if blocking)

1. Exact **merged kit** commit SHA after `ticket_1786480740_176724` closes.
2. Whether overlapping `session` family subscription can be shared with the
   options store without double-apply; if not, document single-owner generation
   choice in implement report.
3. Whether hub-test-support at the pin already ships a consumable
   entity-options surface (current contract-matrix does not).

## Affected surfaces / files (expected)

| Path | Change |
| --- | --- |
| `crates/botster-tui/Cargo.toml` | Hub + kit pins |
| `Cargo.lock` | Single contract source |
| `README.md` | Pin coordinates |
| `third_party/botster-ui-contract/**` + `PIN.md` | Only if patch still required |
| `crates/botster-tui/src/app.rs` | Store + generation, subscribe, materialize, invalidate, frame apply, unit + live tests |
| `crates/botster-tui/fixtures/…` | Live admitted entity-options package (if TUI-owned) |
| `script/test-live-hub` or test filters | Live mode wiring if needed |
| `docs/plans/tui-render-reactive-entity-backed-select-options-plan.md` | This plan (committed) |
| `docs/reports/…` | Implement report (later) |

## Implementation sequence (for Implement)

1. Confirm kit ticket **closed**; record merged kit SHA; confirm its Hub pin is
   `891cc79…`.
2. Repin Hub trio + kit; prove single contract identity; green `./test.sh`
   baseline (document unrelated pre-existing failures with exact filters only).
3. Implement multi-family store + generation/gap discipline; frame apply from
   `DaemonEntityFrame`.
4. On plugin surface accept / reconnect / surface replace:
   `collect_entity_option_families` → ensure subscriptions with
   `begin_generation`.
5. Realize entity-backed selects in materialization / render root; wire store.
6. Selection invalidation + submit guards + compact labels.
7. Unit tests: shared timeline + generation/race matrix + keyboard production
   path + constrained width + no surface refresh on entity updates.
8. **Live Hub proof** via isolated harness + admitted options_source surface;
   record binary provenance and request/frame ledger. If producer cannot be
   owned here, stop and register owner dependency — do not soft-pass.
9. Implement report + gate evidence.

## Risks

| Risk | Mitigation |
| --- | --- |
| Split `botster-ui-contract` types | Hard kit dependency until pin merges; single-source tree check |
| Implement races kit pin | Dependency edge blocks advance while kit open |
| Realizing options only on surface RPC | Re-materialize on every options-store change |
| Reimplementing projection | Shared projector only |
| Stale draft submit | `selection_valid` + clear draft + field error |
| Double-subscribe / double-apply | Dedupe family id; single generation owner |
| Ignoring generation (map-only apply) | Mirror `SessionEntityState` accepts_delta |
| Soft-passing live | Mandatory live checklist; owner dep if producer missing |
| Over-scoping into Workspaces | Generic client only |

## Acceptance checks / tests

### Pin / identity

- [ ] Kit `ticket_1786480740_176724` **closed** before Implement
- [ ] Merged kit SHA pins Hub `891cc796faeab51ee4bee1a0e8494562b233036e`
- [ ] TUI Hub pins = same Hub rev; kit pin = that merged kit SHA
- [ ] Single `botster-ui-contract` source in lock/tree
- [ ] README pin prose updated

### Contract consumption

- [ ] Uses shared collector / projector / `EntityFamilyStore` types — no
      duplicate projector
- [ ] Shared fixture `entity_options_reactive_timeline` drives TUI store
      projections for every named step

### Generation / gap recovery (production path)

- [ ] Per-family `subscription_id` + `has_snapshot` + `snapshot_seq`
- [ ] `begin_generation` on subscribe / reconnect / family re-subscribe
- [ ] Stale subscription frames rejected
- [ ] Out-of-order / non-increasing seq deltas rejected
- [ ] Deltas before first snapshot rejected
- [ ] Gap recovery re-subscribes and applies authoritative snapshot
- [ ] Surface replacement does not leave foreign generation families active
- [ ] Tests cover each bullet above through the real reducer used by the app

### Production path proof (unit + live)

- [ ] Entity-backed select on **plugin surface render root** path
- [ ] Keyboard selection updates drafts through real input handlers / hit map
- [ ] Submit carries exact option value
- [ ] Entity upsert/patch/remove/exclude updates options without additional
      surface render RPC / `list_sessions`

### Behavior matrix (ticket + fixture)

| Case | Proof |
| --- | --- |
| source_snapshot | Options match projector |
| exclude_snapshot | Excluded values absent |
| source_upsert / patch / remove | In-place update |
| reappearance (exclude_remove) | Value returns |
| reconnect_snapshot | New generation + baseline |
| gap_recovery_snapshot | Production recovery path |
| duplicate_values | First-after-sort winner |
| unicode_labels | Labels render; value exact |
| selection_invalid | Clear/block + visible state |
| constrained width | Compact label usable |
| keyboard select + submit | Production hit map + action |

### Live Hub (mandatory)

- [ ] Isolated live Hub binaries invoked through existing TUI live harness
- [ ] Admitted surface with `options_source` rendered
- [ ] Subscribe + snapshot + change + kit render + keyboard + exact submit
- [ ] Ledger proves no second surface render for pure entity update
- [ ] **No soft residual** substitutes for this path

### Repo gates

- [ ] `./test.sh` (or documented `CARGO_TARGET_DIR` + fmt/clippy/test)
- [ ] Targeted unit + live filters green

## Botster layers touched

- **TUI application policy** (primary)
- **Hub client consumption** (subscribe + entity frames + pin)
- **UiNode materialization adapter** (entity-options realization)
- **TUI kit consumption** (realized Select; pin seam)
- Not: Hub runtime, Lua core policy, SPA, Rails, Project Pipelines package

## Worktree / target assumptions

- Pipeline worktree for `ticket_1786474781_871159` on target
  `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Path has no `:`; `test.sh` `CARGO_TARGET_DIR` under `$TMPDIR` remains valid
- `.gitignore` non-empty (73 bytes) at Plan time — re-check before cargo gates

## Pipeline gates and artifacts

| Artifact | Location / id |
| --- | --- |
| Plan document | `docs/plans/tui-render-reactive-entity-backed-select-options-plan.md` (committed) |
| Plan pipeline artifact | `project_pipelines_add_artifact` (kind plan) on each Plan visit |
| Vault checklist | one per Plan visit (skip duplicate ticket checklist with reason) |
| Gate | `botster_stack_plan_gate` with required fields + `plan_uri`, `artifact_id`, `checklist_id`, `target_id`, `target_repository` |

## Required docs updates

- This plan (committed)
- README pin coordinates (Implement)
- Implement report under `docs/reports/` (Implement)
- No plugin README (no package product policy)

## Vault gaps worth capturing

1. **TUI entity-options realization + generation seam** — realize
   `options_source` before kit; re-realize on frames; per-family generation
   mirrors session entity reducers.
2. **Multi-family subscribe dedupe with shared generation** — one subscription
   owner per family when navigator and options overlap.
3. Capture after implement if still novel; else checklist “none”.

## Product decision ledger (defaults)

| Decision | Default |
| --- | --- |
| Projector ownership | Shared contract only |
| Kit feature work | None; pin only |
| Kit dependency | Hard edge until pin closed |
| Compact label format | Primary display field + stable compact metadata tail |
| Invalid selection | Clear draft + visible error; block stale submit |
| Live proof | Mandatory isolated Hub path; owner dep if producer missing — never soft residual |
| Generation model | Mirror `SessionEntityState` per family |
| Workspaces product surface | Follow-up ticket |
| Teardown lenses | N/A |

## Success criteria (verifiable)

Every changed line traces to: (a) Hub/kit pin identity, (b) shared collector /
projector wiring with generation-safe store, (c) select materialization /
invalidation / submit, (d) unit matrix + generation tests, or (e) **live**
production-path proof. Code that exists without the plugin-surface → subscribe
→ frames → kit → input → action route is not done. Unit-only residual is not
done.
