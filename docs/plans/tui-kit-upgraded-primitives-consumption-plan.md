---
ticket: ticket_1783529012_926361
title: Consume upgraded TUI Kit primitives in botster-tui plugin surfaces
step: botster_plan
run: run_1783534317_368569
---

# TUI Kit Upgraded Primitives Consumption Plan

## Context Loaded

- Pipeline context: ticket `ticket_1783529012_926361`, run `run_1783534317_368569`, active step `botster_plan`, gate `botster_plan_gate`; no prior artifacts, reviews, findings, questions, or answers were present.
- Dependency context: dependencies are closed for "Add TUI Kit widgets for UINode application primitives" and "Update hub UINode validation and test support for application primitives".
- Worktree/target context: run target is `tgt_c3d470bab78549df920a41e8fb0e58d8`, base ref is `main`, and this plan is for the assigned checkout only.
- Repo context: compact Rust TUI workspace. Dependency pins live in `crates/botster-tui/Cargo.toml` and `Cargo.lock`; the production user path is `run_loop` -> `DogfoodApp::surface()` -> `renderer::render_node()`; hub/plugin surface state and most tests live in `crates/botster-tui/src/app.rs`; the local adapter in `crates/botster-tui/src/renderer.rs` currently re-exports kit-owned rendering, input routing, hit maps, and capability validation.
- Current dependency observation: `botster-tui-kit` is pinned to `327dc64a540108cba7ce760255aa5759290292b9`; fresh `trybotster/botster-tui-kit` `main` resolves to `8b3ea35fb9742c50919ed4e6435ef21a59019223`. Current `botster-core` is `42538009bc6f6291872c5657bedbe7370f504f8d`; fresh `main` resolves to `978c436865c215828b02a8b0fcca5f8d89413e96`. Current `botster-hub-client` and `botster-hub-test-support` are `3807e1388fa560940c77192f7648bf9638108ab8`; fresh hub `main` resolves to `196d56825a93c9fe8f754e1aa8e8ce18943041b1`.
- Existing implementation context: this repo already consumes `botster-tui-kit` rather than carrying a local renderer, renders delivered plugin surface bodies as `botster_core::ui::UiNode`, and has live plugin contract matrix coverage. This ticket should therefore be a consumption/coverage pass for upgraded kit primitives, not a second renderer implementation.
- Vault context loaded: [[identity]], [[goals]], [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], [[plan steps need reviewable plan artifacts]], [[project pipelines checklist worker timeouts require artifact evidence fallback]], and [[plan agents must author vault context as wikilinks not home paths]].
- Checklist context: `project_pipelines_checklist_instructions` was loaded. Creating the run vault checklist timed out in the plugin worker, so checklist-style evidence is preserved in this plan and gate evidence per [[project pipelines checklist worker timeouts require artifact evidence fallback]].

## Scope

- Upgrade `botster-tui-kit` to the verified merged revision containing the new application primitive widgets. If the kit upgrade requires matching `botster-core`, `botster-hub-client`, or `botster-hub-test-support` revisions for shared `UiNode` types or fixtures, bump those pins narrowly and document why.
- Keep `crates/botster-tui/src/renderer.rs` as a thin adapter/re-export over `botster-tui-kit`. Any local change there should be limited to helper exposure needed by tests; do not copy primitive rendering logic from the kit.
- Route delivered plugin surface `UiNode` rendering through the kit path already used by production: `DaemonResponseKind::PluginSurface` -> `DogfoodApp::plugin_surface` -> `plugin_surface_body_node`/`UiNode` validation -> `renderer::tui_capabilities()` -> `renderer::render_node` or `renderer::render_to_lines`.
- Add focused fixture coverage for a composite application primitive screen that includes the ticketed primitive families: `metric_grid`, `table`, `toolbar` or `action_bar`, `empty_state`, `status_badge`, `section`, enhanced panel/list semantics, and form/action feedback.
- Drive keyboard and mouse dispatch through the real `InputRouter` and `HitMap` path for applicable interactive nodes: list/table selection, toolbar/action buttons, form fields, and action feedback. Tests should assert outgoing semantic action requests or observed hub requests, not only rendered text.
- Preserve existing hub connection, session spawn/attach/drain/send input/resize, terminal rendering, package navigation, package configuration, and live contract matrix behavior.
- Update README manual dogfood instructions if the upgraded fixture adds a new required launch/open step or a new visible capability/diagnostic expectation.

## Non-Scope

- No local implementation of kit-owned primitive widgets in `botster-tui`.
- No new broad TUI navigation system, dashboard, plugin policy, Project Pipelines workflow behavior, browser SPA work, Rails relay work, MCP tool changes, or hub/core runtime policy.
- No terminal data-plane refactor: do not change attach/detach semantics, PTY input forwarding, resize ownership, scrollback/snapshot handling, or session worker behavior except for tests proving no regression.
- No speculative compatibility shims or dual code paths for old primitive names unless the refreshed public contract proves a real transition boundary.
- No private daemon protocol or fixture source parsing as proof. The TUI should consume public hub/client DTOs and shared kit/core types.

## Assumptions And Unknowns

- Assumption: the closed TUI Kit dependency landed on `trybotster/botster-tui-kit` main at or before `8b3ea35fb9742c50919ed4e6435ef21a59019223`.
- Assumption: the upgraded kit owns rendering and input routing for the new primitives; `botster-tui` only needs to bump dependencies, keep production surfaces wired through the kit, and add consumer-side tests.
- Assumption: a composite fixture is available from the upgraded kit, core test support, or hub test support. Prefer importing the producer-owned fixture. If none exists, add the smallest local test fixture built from public `botster_core::ui` types and record the missing shared fixture as a vault/project gap.
- Assumption: the existing dogfood panel is the correct host for plugin-surface render/action evidence; do not add a new screen unless the public DTO requires a different route/open shape.
- Unknown: whether the refreshed `botster-tui-kit` also requires fresh `botster-core` and hub client/test-support revisions. Implementation must let Cargo/API compatibility drive those changes and avoid broad dependency churn.
- Unknown: exact public primitive names for toolbar/action-bar and status-badge in the landed `UiNode` contract. Implementation should follow the contract names in the upgraded dependency and document the mapping in tests.
- Unknown: whether enhanced list/table selection uses current `InputDispatch::Action`, a selection-specific dispatch, or richer payloads from the kit. Tests should assert the actual public dispatch shape.
- Convention conflict status: none found. The plan keeps TUI as a generic hub client, consumes framework/library primitives from the kit, preserves plugin-owned UI state, avoids copied renderer logic, and keeps plan context path-neutral.

## Botster Layers Touched

- TUI/client renderer adapter: dependency consumption, capability validation, production render path verification, and input dispatch tests.
- Hub client/test-support boundary: possible pin updates only if required to consume the closed dependency contracts and composite fixture.
- Docs: README manual dogfood instructions if visible behavior or launch steps change, plus this plan artifact.
- Not touched: Rust hub implementation, core runtime policy, Lua plugin runtime/policy, session/client worker data plane, React SPA, Rails relay, MCP tools.

## Affected Surfaces And Files

- `crates/botster-tui/Cargo.toml`
  - Bump `botster-tui-kit` to the upgraded revision.
  - Bump `botster-core`, `botster-core-test-support`, `botster-hub-client`, and `botster-hub-test-support` only when required by shared public types or fixture availability.
- `Cargo.lock`
  - Update only dependency resolution caused by the required pin changes.
- `crates/botster-tui/src/renderer.rs`
  - Keep as a thin kit adapter. Touch only for helper exports or signature compatibility with the upgraded kit.
- `crates/botster-tui/src/app.rs`
  - Keep production rendering and action paths wired through `DogfoodApp`; add/adjust tests for plugin surface rendering, keyboard/mouse selection, action dispatch, and action feedback.
  - Preserve existing tests for package config, package navigation, contract matrix conformance, terminal attach/input/resize/drain, and compatibility diagnostics.
- `README.md`
  - Update manual dogfood docs if new composite fixture/manual verification is added.
- `docs/plans/tui-kit-upgraded-primitives-consumption-plan.md`
  - Plan-stage review artifact.

## Implementation Outline

1. Verify the upgraded dependency revisions at implementation start. Prefer the minimal pin set: `botster-tui-kit` first, then core/hub pins only if compile errors or shared fixture APIs require them.
2. Compile after the pin change before editing behavior. Let type errors identify any changed kit API names for renderer exports, capabilities, hit maps, or input dispatch.
3. Add a composite primitive rendering test using the best available producer-owned fixture. The assertion should validate the `UiNode`, run TUI capability validation, render through `renderer::render_to_lines`, and assert visible rows for metric grid, table, toolbar/action bar, empty state, status badge, section, panel/list semantics, and form/action feedback.
4. Add input-routing tests that render the composite fixture to a `HitMap`, dispatch keyboard and mouse events through `InputRouter`, and assert semantic actions/selection payloads for actionable nodes.
5. Add or extend a production-path test that applies a `DaemonPluginSurface` response containing the composite surface to `DogfoodApp`, calls `surface()`, and renders with `renderer::render_to_lines`. This proves the actual TUI path consumes the upgraded primitives.
6. Run the existing terminal/session tests to prove no regression in `TerminalForward`, resize, attach, drain, and headless dogfood paths.
7. Update README manual instructions only if the new acceptance path changes what an operator must run to open a plugin app using the new primitives.

## Risks

- Dependency drift risk: `botster-tui-kit` main may require newer core/hub pins. Mitigation: bump only the matching public-contract crates required for compile/test success and explain each pin movement.
- False-green risk: a pure `render_to_lines` fixture test would not prove plugin surfaces use the new behavior. Mitigation: include a `DogfoodApp::apply_response` production-path test with a composite `DaemonPluginSurface`.
- Renderer duplication risk: implementers may fill unsupported primitive gaps inside `botster-tui`. Mitigation: require capability failures or kit changes, not local copies of primitive widgets.
- Contract-name risk: ticket wording may differ from landed primitive names. Mitigation: tests should name both the contract primitive and the ticket family it satisfies.
- Input regression risk: mouse/key dispatch for new tables/forms could disturb terminal input forwarding. Mitigation: preserve and run existing terminal dispatch tests plus add focused composite interaction tests.
- Layout density risk: composite application screens may exceed the existing dogfood panel size. Mitigation: use deterministic `render_to_lines` dimensions in tests and avoid new navigation unless required by public DTO shape.
- Live harness cost risk: `script/test-live-hub` builds external hub binaries and may be slower or environment-sensitive. Mitigation: keep fast unit/integration coverage mandatory and live-hub as acceptance evidence when binaries are available.

## Acceptance Checks And Tests

- `script/fmt`
- `script/test`
- `script/clippy`
- `cargo run -p botster-tui -- --smoke`
- `CARGO_TARGET_DIR=/tmp/botster-tui-kit-primitives-target script/test-live-hub` when the hub/session-worker fixture environment is available.
- Required assertions:
  - `botster-tui-kit` is pinned to the upgraded revision, and any related core/hub pin movement is explained by compile or fixture compatibility.
  - No local copied renderer for `metric_grid`, `table`, `toolbar`/`action_bar`, `empty_state`, `status_badge`, `section`, panel/list semantics, or form/action feedback appears in `botster-tui`.
  - The composite primitive surface validates as `botster_core::ui::UiNode`, passes TUI capability validation, and renders through the kit-backed `renderer::render_to_lines`.
  - A `DogfoodApp` plugin-surface response carrying the composite surface renders through `DogfoodApp::surface()` and the kit-backed renderer.
  - Keyboard and mouse interaction tests drive the real `InputRouter`/`HitMap` path and assert semantic action/selection dispatch for applicable primitives.
  - Existing terminal/session behavior still passes, especially attach, terminal input forwarding, resize, drain/snapshot/scrollback rendering, and headless dogfood.
  - Manual docs explain how to launch the TUI and open a plugin app/surface using the new primitives if the command differs from the existing README dogfood path.

## Pipeline Gates And Artifacts

- Submit `botster_plan_gate` with this plan under the required fields.
- Checklist evidence fallback: `project_pipelines_create_vault_checklist` timed out, so gate evidence and this artifact carry notes read, no convention conflicts, verification plan, and durable capture decision.
- Advancement target: request advance to `botster_plan_review` after gate submission.

## Vault Gaps Worth Capturing

- Capture after implementation if `botster-tui-kit` has a stable downstream consumption pattern for application primitive fixture tests that should be reused by future TUI clients.
- Capture after implementation if the ticketed primitive family names differ from public `UiNode` contract names, especially toolbar/action-bar or status-badge.
- Capture after implementation if no producer-owned composite primitive fixture exists; that would be a shared contract/test-support gap worth documenting.
- No new durable vault note is needed from planning alone.
