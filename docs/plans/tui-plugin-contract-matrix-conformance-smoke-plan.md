---
ticket: ticket_1783280112_685450
title: Add botster-tui plugin contract matrix conformance smoke
step: botster_plan
run: run_1783289813_806661
---

# TUI Plugin Contract Matrix Conformance Smoke Plan

## Context Loaded

- Pipeline context: ticket `ticket_1783280112_685450`, run `run_1783289813_806661`, active step `botster_plan`, gate `botster_plan_gate`; no prior artifacts, reviews, findings, questions, or answers were present.
- Dependency context: upstream dependency `ticket_1783280111_645888` ("Expose reusable plugin UI conformance harness from botster-hub-test-support") is closed.
- Worktree/target context: run target is `tgt_c3d470bab78549df920a41e8fb0e58d8`, base ref is `main`, and this plan is for the assigned checkout only.
- Repo context: compact Rust TUI workspace. The production dogfood app, hub requests, package/app rendering, plugin-adjacent action handling, headless live-hub test, and most assertions live in `crates/botster-tui/src/app.rs`; renderer adapter exports live in `crates/botster-tui/src/renderer.rs`; dependency pins live in `crates/botster-tui/Cargo.toml` and `Cargo.lock`.
- Current dependency observation: this repo pins `botster-hub-client` / `botster-hub-test-support` to `b5f80286605fcb1e432e5b673b506fa124739728`. That pin has public `PluginSurfaceRender` / `PluginSurfaceAction` request types, but does not contain `run_plugin_contract_matrix_conformance` or `PluginContractMatrixConformanceReport`.
- Fresh hub observation: `trybotster/botster-hub` main resolves to `27118ab75f4ff511ccdfcfa754f74b878c0b9b45`. A temporary read-only clone of that revision exposes `botster_hub_test_support::run_plugin_contract_matrix_conformance`, `PluginContractMatrixConformanceReport`, `PluginContractMatrixClientRenderCheck`, and the fixture path `fixtures/plugins/plugin-contract-matrix`.
- Fresh contract-matrix semantics: the hub-owned harness installs/configures/enables `botster.plugin-contract-matrix`, verifies app/settings route descriptors, renders app/empty/settings plugin surfaces, checks a blocked/error surface, checks invalid and valid settings configuration, and exercises plugin action success/error through public `botster-hub-client` requests.
- Vault context loaded: [[identity]], [[goals]], [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], [[plan steps need reviewable plan artifacts]], [[project pipelines checklist worker timeouts require artifact evidence fallback]], [[botster hub client crate is the external client boundary]], [[runtime client acceptance must render delivered snapshots through real registry]], [[plugin surface handlers must validate against hub locked uinode contract]], and [[botster first party client support matrices belong in hub test support]].
- Skill context loaded: `knowledge-vault` and `botster-customize-tui`.
- Checklist context: `project_pipelines_checklist_instructions` was loaded. Creating the run vault checklist timed out in the plugin worker, matching [[project pipelines checklist worker timeouts require artifact evidence fallback]], so checklist-style evidence is preserved in this plan and gate evidence.

## Scope

- Keep this a `botster-tui` client conformance smoke over public `botster-hub-client` and hub-owned `botster-hub-test-support` fixtures.
- Bump `botster-hub-client` and `botster-hub-test-support` to `27118ab75f4ff511ccdfcfa754f74b878c0b9b45` or a later verified main commit that contains the same plugin contract matrix harness. Keep any compile fixes caused by the bump narrow.
- Add TUI handling for `DaemonResponseKind::PluginSurface` and `DaemonResponseKind::PluginActionResult` if the fresh public client types expose those bodies as structured DTOs rather than the current unrendered fields.
- Render delivered plugin surfaces through the production `botster-tui-kit` renderer path, not through a parallel JSON/source assertion. The smoke must parse the hub-delivered `UiNode` body into `botster_core::ui::UiNode`, validate it, run `renderer::tui_capabilities().validate_node`, render it with `renderer::render_to_lines`, and assert visible terminal text/node evidence.
- Add a focused client-render assertion helper around `PluginContractMatrixConformanceReport.client_render_check`. It should prove at least:
  - app surface rendered with `contract-app-panel`;
  - empty/placeholder surface rendered with `contract-empty-message`;
  - settings surface rendered with `contract-settings-panel`;
  - redacted secret state from the settings path is visible and raw secret material is absent;
  - unsupported UiNode primitives fail with a precise unsupported primitive diagnostic from TUI capability validation instead of being treated as a passing render.
- Extend the existing isolated-hub live test path to call `run_plugin_contract_matrix_conformance(&hub, <hub checkout>/fixtures/plugins/plugin-contract-matrix)` after the hub/test-support bump and compare the returned report against the TUI renderer output.
- Prove app/settings surface discovery from hub-owned DTOs: installed app/package route rows must show the contract matrix app route and settings/config path from `ListApps` / `ListPackages`, not only package listing.
- Prove action success and action error are visible in TUI terminal state from `plugin_action_result` and public diagnostics.
- Update README local hub dogfood docs to mention plugin contract matrix conformance coverage and the precise unsupported-primitive behavior if implementation changes visible capability text.

## Non-Scope

- No changes to `botster-hub`, `botster-core`, the contract matrix fixture, package admission, plugin runtime policy, or Lua plugin behavior.
- No private daemon protocol, local mirrored DTOs, or source parsing of the fixture plugin as proof.
- No new broad TUI navigation system, dashboard, route router, entity-store hydration, or operator workbench work.
- No browser SPA, Rails relay, MCP, Project Pipelines plugin policy, terminal data-plane, attach/detach, PTY input, resize, or scrollback refactor.
- No silent green path for unsupported UiNode primitives. Unsupported primitives are explicit findings/diagnostics and must fail the smoke unless a human accepts a waiver.

## Assumptions And Unknowns

- Assumption: the closed dependency is represented on `trybotster/botster-hub` main at `27118ab75f4ff511ccdfcfa754f74b878c0b9b45`, and this TUI branch should consume that merged harness instead of backporting a local fixture.
- Assumption: the existing `script/test-live-hub` shape remains the right runtime proof: build the pinned hub/session-worker binaries, start an isolated hub, run headless dogfood, then run contract-matrix conformance against that same hub.
- Assumption: `botster-tui-kit` owns primitive rendering and unsupported capability validation. `botster-tui` should add client surface plumbing/assertions, not fork renderer behavior.
- Unknown: whether the dependency bump also requires a matching `botster-core` or `botster-tui-kit` pin update. Implementation must compile first and keep any pin movement tied to public type compatibility or capability diagnostics.
- Unknown: whether fresh `PluginSurface` response bodies deserialize directly into `UiNode` or require reading a wrapper DTO's `body` field first. Implementation must use the public `botster-hub-client` structs and avoid ad hoc string manipulation.
- Unknown: whether the current TUI surface has enough visible room for all conformance states in one panel. If not, add compact text rows in the existing hub/status panel rather than a new screen.
- Convention conflict status: none. The plan follows the public hub-client boundary, consumes hub-owned test support, proves the real renderer path, preserves TUI/client ownership, and uses a reviewable plan artifact.

## Botster Layers Touched

- TUI: primary layer; `DogfoodApp` response handling, plugin surface/action rendering, live conformance assertions, and tests.
- Hub client boundary: dependency rev bump and public `botster-hub-client` DTO consumption.
- Hub test support: dev/test dependency only, consuming the reusable plugin contract matrix harness.
- Docs: README local dogfood capability text and this plan artifact.
- Not touched: Rust hub implementation, core package/plugin policy, Lua runtime, session/client worker data plane, React SPA, Rails relay, MCP tools.

## Affected Surfaces And Files

- `crates/botster-tui/Cargo.toml`
  - Bump `botster-hub-client` and `botster-hub-test-support` to a revision containing `run_plugin_contract_matrix_conformance`.
  - Bump `botster-core` / `botster-tui-kit` only if required by the fresh public hub types or TUI capability validation.
- `Cargo.lock`
  - Update only dependency resolution caused by the required pins.
- `crates/botster-tui/src/app.rs`
  - Apply and render plugin surface responses.
  - Apply and render plugin action result success/error state.
  - Render route/settings metadata from `DaemonPackage.routes` and `DaemonApp.route` if the fresh DTO exposes those fields.
  - Add typed fixture/test helpers that drive `DogfoodApp::apply_response`, `surface()`, `renderer::render_to_lines`, `handle_dispatch`, and the existing live-hub harness.
  - Extend `headless_dogfood_runs_against_isolated_hub_when_binaries_are_available` or add a sibling live test for `run_plugin_contract_matrix_conformance`.
- `crates/botster-tui/src/renderer.rs`
  - No planned structural change. Touch only if exposing a small helper for validating/rendering delivered `UiNode` bodies through the existing kit path is cleaner than duplicating test code in `app.rs`.
- `README.md`
  - Update Local Hub Dogfood documentation for plugin surface/settings/action conformance and unsupported primitive diagnostics if visible behavior changes.
- `docs/plans/tui-plugin-contract-matrix-conformance-smoke-plan.md`
  - Reviewable Plan-stage artifact.

## Risks

- Stale dependency risk: current pins cannot satisfy the ticket. Mitigation: bump hub-client/test-support to the verified main revision before implementing the smoke.
- API drift risk: fresh main may move after planning. Mitigation: implementation should verify the target revision at start and compile against actual public structs.
- False-green risk: running `run_plugin_contract_matrix_conformance` alone proves producer/hub behavior, not TUI rendering. Mitigation: assert TUI-rendered output against `client_render_check` fields and delivered surfaces.
- Request-only risk: dispatching `PluginSurfaceRender` / `PluginSurfaceAction` without rendering the returned body would violate acceptance. Mitigation: tests must check terminal-visible lines and action/diagnostic text.
- Unsupported primitive risk: `botster-tui-kit` may not support a primitive present in the fixture. Mitigation: let capability validation fail with the exact primitive/node id and record it as unsupported, not as success.
- Secret leakage risk: settings/config conformance includes a secret state. Mitigation: assert redacted state is visible and raw secret/token values do not appear in rendered lines, diagnostics, or test output.
- Layout density risk: the hub/status panel is already dense. Mitigation: add terse conformance rows only; avoid new navigation.
- Live harness path risk: `script/test-live-hub` builds hub from the pinned dependency checkout, while the fixture path lives under that hub checkout. Mitigation: derive the hub root from cargo metadata like the script already does and pass `fixtures/plugins/plugin-contract-matrix`.

## Acceptance Checks And Tests

- `script/fmt`
- `script/test`
- `script/clippy`
- `cargo run -p botster-tui -- --smoke`
- `CARGO_TARGET_DIR=/tmp/botster-tui-plugin-contract-target script/test-live-hub`
- Focused unit/integration tests should prove:
  - dependency pin exposes `run_plugin_contract_matrix_conformance` and the plugin contract matrix report types;
  - delivered app, empty, and settings plugin surfaces deserialize to `UiNode`, validate against core, pass TUI capability validation, and render through `renderer::render_to_lines`;
  - the app surface visibly includes `contract-app-panel`;
  - the empty/placeholder surface visibly includes `contract-empty-message`;
  - the settings/config surface visibly includes `contract-settings-panel`, configured endpoint/mode state, and redacted secret state;
  - blocked surface render returns visible structured error text with `plugin_invocation_failed` / `plugin_surface_render` or the public error fields from the fresh DTO;
  - invalid configuration diagnostic renders with action-failure/operation evidence and mentions the rejected value without exposing secrets;
  - plugin action success renders accepted state, request id, and normalized message;
  - plugin action error renders error state, request id, public diagnostic kind, and operation;
  - app/settings routes are visible from public app/package DTOs, including the contract app route and settings route support;
  - unsupported UiNode primitives produce a precise unsupported primitive diagnostic/failure rather than a skipped or green assertion;
  - rendered output does not include local checkout paths, socket paths, tokens, auth identities, or raw secret material.
- Runtime-path proof: the live-hub test must install/enable the real fixture plugin through the daemon, then render returned surfaces through the production TUI renderer. Code existence or package listing alone is not acceptance evidence.

## Pipeline Gates And Artifacts

- Submit `botster_plan_gate` with this plan under the required fields.
- Checklist evidence fallback: `project_pipelines_create_vault_checklist` timed out, so gate evidence must include notes read, no convention conflicts, planned verification commands, and durable capture decision.
- Advancement target: request advance to `botster_plan_review` after gate submission.

## Vault Gaps Worth Capturing

- No new durable vault note is needed from planning alone.
- Capture candidate after implementation: a first-party TUI convention for consuming hub-owned plugin contract matrix reports and converting `PluginContractMatrixClientRenderCheck` into renderer assertions.
- Capture candidate after implementation: if unsupported UiNode primitive diagnostics need a stable text format, record that cross-client conformance rule under [[cli-patterns]] / [[botster-architecture]].
