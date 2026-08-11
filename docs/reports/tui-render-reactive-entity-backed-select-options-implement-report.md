# Implement report: TUI reactive entity-backed select options

## Target

| Field | Value |
| --- | --- |
| Ticket | `ticket_1786474781_871159` |
| Target repository | `botster-tui` / `trybotster/botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Run | `run_1786480500_850089` |
| Step | `botster_stack_implement` |
| Plan | `docs/plans/tui-render-reactive-entity-backed-select-options-plan.md` (approved `review_1786487889_465243`) |
| Runtime-teardown class | Does not apply |

## Repository playbook and other guidance applied

**Role / charter**

- [[implementer-playbook]]
- [[botster-implementer-playbook]]
- [[botster-tui-playbook]]

**Targeted atomic notes**

- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[botster tui uinode event routing captures hit regions during draw]]
- [[botster tui attach must explicitly pull core entities after subscribing]]
- [[botster entity snapshots are authoritative reconnect baselines]]
- [[botster rust consumers that share ui contract must pin one hub revision]]
- [[plugin dynamic ui lists bind to plugin-owned entities]]
- [[plugin surfaces request model state through ui bindings not hub subscribe]]
- [[plugin surface handlers must validate against hub locked uinode contract]]
- [[tui error dedup tests must drive real input handlers]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]
- [[cross-client ui should share semantic primitives and actions with renderer-specific adapters]]
- [[test script required for rust tests not cargo test]]
- [[implementation artifacts must match actual git state]]
- [[implement gate must verify committed work and pr link before review]]
- [[implementation steps must persist report artifacts for review]]
- [[pipeline vault checklists must cite exact resolvable note titles]]

**Not loaded (correctly)**

- [[project-pipelines-playbook]] — package/plugin orchestration not in implementation scope
- [[botster runtime teardown lenses]] — teardown class does not apply

**Convention conflicts:** none.

## Ownership boundaries preserved

| Boundary | Status |
| --- | --- |
| botster-tui owns app policy, multi-family generation store, materialization, selection invalidation, live fixture, tests | Preserved |
| botster-ui-contract owns projector / collector / EntityFamilyStore pure helpers | Consumed only (shared projector; no local reimplementation) |
| botster-tui-kit owns Select render/hit/input | Pin only; no kit feature work |
| botster-hub owns admission + entity provider frames | Consumed at pin; no Hub edits |
| botster-web / botster-workspaces | Not edited |

## Cross-repo dependencies

| Dependency | Status |
| --- | --- |
| Hub contract `ticket_1786474779_865884` | Closed; pin `891cc796faeab51ee4bee1a0e8494562b233036e` |
| TUI Kit pin `ticket_1786480740_176724` / `dependency_1786481683_121026` | Closed; kit `9d4a566f309e9d848771b5448764a87f4721468e` |
| Conditionally registered owner fixture | Not needed; TUI-owned live package admitted |

## Files changed

| Path | Change |
| --- | --- |
| `crates/botster-tui/Cargo.toml` | Hub trio + kit pins to entity-options revision |
| `Cargo.lock` | Single `botster-ui-contract` 0.3.2 source |
| `README.md` | Pin coordinate prose |
| `crates/botster-tui/src/entity_options.rs` | Multi-family generation store + materialize + unit tests |
| `crates/botster-tui/src/app.rs` | Wire store, SubscribeEntities demand, surface materialize, reconcile, unit + live tests |
| `crates/botster-tui/src/main.rs` | Register `entity_options` module |
| `crates/botster-tui/fixtures/entity-options-reactive/*` | Live admitted options_source package |
| `docs/reports/tui-render-reactive-entity-backed-select-options-implement-report.md` | This report |

## Implementation summary

1. **Pins:** Hub client / ui-contract / hub-test-support → `891cc796…`; kit → `9d4a566…`. `cargo tree -i botster-ui-contract` resolves one source.
2. **Store:** `EntityOptionsStore` with per-family `subscription_id` / `has_snapshot` / `snapshot_seq` mirroring `SessionEntityState`. Applies `DaemonEntityFrame` through shared `apply_entity_options_frame` after generation gates.
3. **Demand:** On plugin surface accept, `collect_entity_option_families` → SubscribeEntities for non-process-wide families. `session` / `session_type` reuse navigator subscriptions via projection merge.
4. **Materialize:** Before kit handoff, realize `options_source` into static `SelectOption` slots with compact labels (`display_fields` joined by ` · `). Invalid selection clears draft, stamps field error, blocks silent stale submit.
5. **Re-materialize on frames:** Entity option drains re-reconcile drafts; `surface()` re-projects without PluginSurfaceRender.

## Deviations from plan

1. **Live fixture exclude family:** Live package uses a single plugin source family (`entity-options-reactive.item`) rather than dual plugin item+exclude. Dual-family exclude matrix remains fully covered by shared `entity_options_reactive_timeline` unit path. Hub dual-family admission pattern uses core `/session` + one plugin exclude family; dual plugin families failed admission for `.exclude` in this environment. This is a scoped live-fixture simplification, not a projector/store waiver.
2. **`third_party/botster-ui-contract` patch:** Not refreshed; same-rev dual git pins give single type identity without patch.
3. **Session/session_type overlap:** Single owner is the typed navigator store; options projection injects those maps at materialize time (no second SubscribeEntities).

No plan acceptance-check rewrite required beyond noting the live-fixture source-only shape; shared fixture + generation tests still cover exclude, gap, and reconnect steps.

## Tests and downstream proof

### Unit / fixture

- `./test.sh` (no live hub env): **186** binary unit tests + **1** package manifest test — all green.
- New filters:
  - `entity_options::tests::shared_fixture_timeline_matches_contract_projector` (full shared timeline)
  - `entity_options::tests::generation_rejects_stale_subscription_and_out_of_order_seq`
  - `entity_options::tests::materialize_builds_options_and_invalidates_selection`
  - `app::tests::entity_options_select_materializes_keyboard_submit_and_invalidates_without_surface_refresh` (production hit map + InputRouter keyboard + PluginSurfaceAction exact value + no surface re-render on entity remove)

### Live Hub (mandatory)

- `app::tests::entity_options_live_hub_proof_when_binaries_are_available`
- Binaries: local `botster-hub` / `botster-session-worker` from Projects hub tree (includes entity-options contract surface; pin consumer is TUI `891cc79`)
- Proven path: package install/enable → PluginSurfaceRender → SubscribeEntities → authoritative snapshot → kit hit-map options → keyboard selection → exact PluginSurfaceAction value → remove + resubscribe snapshot without second PluginSurfaceRender
- Ledger line: `entity-options-live-proof: selected=opt-alpha surface_renders=1`

### Pin identity

- Single `botster-ui-contract v0.3.2` at Hub `891cc796…` via hub-client, kit, direct, and hub-test-support.

## Unverified behavior / residual risk

1. Dual **plugin** exclude family admission on live Hub was not exercised (unit fixture covers exclude projection). If product surfaces need dual plugin families, re-verify Hub admission under the exact pin binary.
2. Live binary used for proof was Projects hub `debug` build (HEAD after pin); contract pin identity is still enforced by Cargo. Ideal re-proof against hub binary built exactly at `891cc79` if CI requires byte-identical hub.
3. Offline unit path: `request_and_apply` without a client records PluginSurfaceAction then transport-errors and resets surface (existing app behavior); invalidation path re-seeds surface after that check.

## Missing vault guidance discovered

1. TUI entity-options realization + generation seam (realize `options_source` before kit; re-realize on frames; per-family generation) — candidate capture after Review.
2. Multi-family subscribe dedupe: process-wide session/session_type feed options projection without a second subscription — candidate capture.
3. Dual plugin entity-provider admission edge cases for live fixtures — optional follow-up note if still novel after Review.

## Success criteria check

| Criterion | Evidence |
| --- | --- |
| Pin identity | Cargo.toml + lock + tree |
| Shared collector/projector only | `entity_options.rs` imports |
| Generation gates | unit tests |
| Materialize + invalidation | unit + app production path test |
| Keyboard + exact submit | app unit + live |
| Live path without surface re-render on entity update | live test `surface_renders=1` after remove+resubscribe |
| No list_sessions / workspaces product logic | not introduced |
