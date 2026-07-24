---
ticket: ticket_1782257640_505199
title: Render package configuration forms in botster-tui
step: botster_plan
run: run_1782266926_142717
---

# TUI Package Configuration Forms Plan

## Context Loaded

- Pipeline context: `ticket_1782257640_505199`, run `run_1782266926_142717`, active step `botster_plan`, gate `botster_plan_gate`; no prior artifacts, findings, reviews, questions, or answers.
- Dependency context: dependency ticket `ticket_1782257625_477566` ("Persist and expose package configuration values in botster-hub") is closed.
- Repo context: clean worktree before planning. This repo is a compact Rust TUI workspace with one client crate at `crates/botster-tui`; the production live-runtime app, hub requests, package rendering, action handling, and most app tests live in `crates/botster-tui/src/app.rs`; generic ratatui form primitives and keyboard/mouse input routing live in `crates/botster-tui/src/renderer.rs`.
- Current dependency context: `crates/botster-tui/Cargo.toml` pins `botster-hub-client` and `botster-hub-test-support` to `1f4c6e9b8d0deef5ed101a99e644d2bd2e9dd0cf`. Local inspection of that pinned `crates/botster-hub-client/src/lib.rs` shows package listing/process DTOs but no package configuration DTOs or set-configuration request.
- Fresh hub-client context: `trybotster/botster-hub` main at `b1be4f53e2baf17f0c0e645ee0cd4fa139e3c8ea` exposes `DaemonPackage.configuration: DaemonPackageConfiguration` and `DaemonRequest::SetPackageConfiguration { package_name, values: BTreeMap<String, Value> }`. `DaemonPackageConfiguration` carries `schema: Option<Value>`, `effective_values`, `missing_required`, and `diagnostics`; secret values are redacted/write-only marker JSON, not raw secret material.
- Vault context loaded: [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], plus identity/goals context.
- Project Pipelines checklist context loaded via `project_pipelines_checklist_instructions`. Creating the run vault checklist timed out in the plugin worker, matching [[project pipelines checklist worker timeouts require artifact evidence fallback]], so checklist-style evidence is preserved in this plan and gate evidence.

## Scope

- Keep this a `botster-tui` client feature over public `botster-hub-client` DTOs.
- Bump the `botster-hub-client` and `botster-hub-test-support` git revs to a commit that includes the closed dependency's package configuration API, expected to be current main `b1be4f53e2baf17f0c0e645ee0cd4fa139e3c8ea` unless implementation finds a better merged commit.
- Consume `DaemonPackage.configuration` from `ListPackages` / package responses and render package configuration forms inside the existing hub/package live-runtime surface.
- Build form fields from hub-provided schema/value DTOs only. Support the ticketed field set: string, boolean, enum/select, multiline text, and secret placeholders.
- Submit updates through `DaemonRequest::SetPackageConfiguration`, wrapping field drafts into the hub value JSON shape expected by the package configuration API, for example string/select/multiline values with `{ "type": "...", "value": ... }` and secret updates with `{ "type": "secret", "state": "write_only" }`.
- Display hub validation state from `configuration.missing_required` and `configuration.diagnostics` near the relevant package fields, while preserving existing public `DaemonDiagnostic` rendering.
- Preserve keyboard-first flow through the existing `InputRouter`: Tab focus, text editing, checkbox/select changes, and Enter/Space submit. Keep existing mouse hit-map behavior compatible for clickable buttons/fields where it already works.
- Redact secrets in rendered text, stored drafts, submitted fixtures, and tests. Existing redacted secrets should render as placeholders/states, not as editable raw values.
- Add focused tests that drive the production app path: fixture `DaemonPackage` DTOs -> `TuiApp::apply_response` -> `surface()` / renderer output -> `handle_dispatch` submit request observation.
- Update `README.md` Local Hub Production docs if the visible TUI capabilities change.

## Non-Scope

- No changes to `botster-hub`, package validation policy, package persistence, package schema design, plugin runtime, browser SPA, Rails relay, MCP tools, or Project Pipelines plugin policy.
- No TUI-only package configuration schema, private socket protocol, duplicated hub DTO structs, or ad hoc string parsing of daemon frames.
- No full package management UI redesign, package details page, route system, entity-store hydration work, or operator-workbench expansion.
- No support for editing number, integer, path, or URL fields unless it falls out naturally through the same string input wrapper and does not expand the implementation. If unsupported schema fields appear, render them as read-only/unsupported with a clear visible label rather than silently dropping them.
- No final UX polish beyond a usable keyboard-first form. Dense layout refinement and richer package navigation are follow-up work.
- No terminal data-plane, attach/detach, PTY input, resize, scrollback, or session lifecycle refactor.

## Assumptions and Unknowns

- Assumption: the closed hub dependency has landed on `botster-hub` main and the TUI should bump to a merged commit rather than attempting compatibility scaffolding around the old pin.
- Assumption: `DaemonPackage.configuration.schema` remains intentionally `serde_json::Value` in `botster-hub-client`; the TUI may parse the documented schema shape for rendering, but the source of truth is still the hub DTO.
- Assumption: package configuration schema fields use keys such as `key`, `type`, `label`, `description`, `required`, `default`, and select `options`, per the current hub/core schema examples and hub tests.
- Assumption: blank secret drafts mean "leave existing secret state unchanged"; explicit user input for a secret field submits the write-only marker/value shape required by the hub API without retaining the raw value in visible output.
- Unknown: whether `SetPackageConfiguration` responses return `DaemonResponseKind::Packages`, `PackageDecision`, or an operator error in every validation path. Implementation must inspect and test the actual response behavior after bumping the dependency.
- Unknown: whether the existing `InputRouter` textarea behavior is sufficient. Current renderer code treats `Textarea` as read-only, so multiline editing probably requires a small renderer/input-router extension.
- Worktree/target assumption: this run is bound to `target_id=tgt_c3d470bab78549df920a41e8fb0e58d8` and this checkout is the assigned worktree, not an ambient Botster checkout.
- Convention conflicts: none. The plan follows the Botster client boundary, consumes public hub-client DTOs, keeps policy in the hub/plugin layer, and avoids new primitives unless existing form routing must be extended for multiline editing.

## Botster Layers Touched

- TUI: primary layer, including live-runtime app state, package form construction, keyboard form interaction, rendering, and tests.
- Hub client boundary: dependency rev bump and public `botster-hub-client` request/response/DTO consumption.
- Docs: README live-runtime capability text and this plan artifact.
- Not touched: plugin/Lua core, Rust hub implementation, session/client worker data plane, React SPA, Rails relay, MCP tools.

## Affected Surfaces and Files

- `crates/botster-tui/Cargo.toml`
  - Bump `botster-hub-client` and `botster-hub-test-support` revs to a configuration-capable hub commit.
- `Cargo.lock`
  - Update git dependency resolution for the hub-client/test-support bump.
- `crates/botster-tui/src/app.rs`
  - Add package configuration rendering helpers and draft/error state as needed.
  - Build package configuration form UiNodes from `DaemonPackage.configuration`.
  - Handle package configuration submit actions and call `DaemonRequest::SetPackageConfiguration`.
  - Record set-configuration requests in tests, similar to existing observed refresh requests.
  - Add DTO fixtures for schema, effective values, missing required fields, diagnostics, validation errors, and secret redaction.
- `crates/botster-tui/src/renderer.rs`
  - Extend existing form/input routing only as needed for editable multiline text and secret placeholder behavior.
  - Preserve existing generic shared UI primitives and semantic action dispatch.
- `README.md`
  - Update Local Hub Production docs to mention configuration schema/value rendering, update submission, validation display, and secret redaction if implementation changes the visible surface.
- `docs/plans/tui-package-configuration-forms-plan.md`
  - Reviewable Plan-stage artifact.

## Risks

- Public API drift: the current repo pin lacks the required API, and `main` has additional package lifecycle fields. Mitigation: bump both hub dependencies together and compile/test through the real public client types.
- Schema parsing creep: because `schema` is `Value`, the TUI could accidentally invent a parallel schema. Mitigation: parse only the documented hub/core field shape needed for rendering and keep unknown fields untouched/read-only.
- Secret leakage: drafts, rendered frames, debug assertions, or test fixture output could expose raw secret input. Mitigation: render redacted/write-only/unset markers, clear secret drafts after submit, and add negative tests that rendered output/request logs do not contain raw secrets.
- Validation feedback placement: hub validation may arrive as configuration diagnostics, missing-required arrays, operator errors, or public diagnostics. Mitigation: cover each public DTO path with focused tests and render a package-level fallback when field-level mapping is unavailable.
- Input routing regression: touching `renderer.rs` can break existing text input, checkbox/select, action, mouse, or terminal forwarding behavior. Mitigation: keep edits small and run the existing renderer/app tests, especially form and terminal dispatch coverage.
- Layout density: rendering all package fields in the existing hub panel can become noisy. Mitigation: prefer a compact per-package form under the existing package row and avoid a larger navigation redesign in this ticket.
- Live fixture gap: this repo may not have a ready local package fixture with configuration schema. Mitigation: unit tests prove the production app/render/submit path from public DTO fixtures; live hub live-runtime remains a compatibility smoke/runtime proof.

## Acceptance Checks and Tests

- `script/fmt`
- `script/test`
- `script/clippy`
- `cargo run -p botster-tui -- --smoke`
- Existing live-hub runtime proof should still run:
  - `CARGO_TARGET_DIR=/tmp/botster-tui-impl-target script/test-live-hub`
  - If no live configurable-package fixture is available in this repo, document that limitation while still proving the TUI connects to the bumped real hub-client and unit tests cover configuration DTO rendering/submission.
- Focused app tests in `crates/botster-tui/src/app.rs` should prove:
  - package rows render configuration schema/value DTOs from `DaemonPackage.configuration`;
  - string, boolean, select, multiline text, and secret fields render with labels/current values/placeholders;
  - field drafts are visible before submit and submit emits `DaemonRequest::SetPackageConfiguration` with hub-shaped JSON values;
  - hub validation diagnostics and missing-required fields render visibly;
  - secret fields render redacted/unset/write-only markers and never print raw secret values;
  - successful configuration update refreshes or applies package state through the public response path;
  - existing package list/process rendering remains intact.
- Focused renderer tests in `crates/botster-tui/src/renderer.rs` should prove any changed generic form behavior, especially editable textarea behavior if added, while preserving existing text input, checkbox/select, action dispatch, and terminal forwarding tests.

## Pipeline Gates and Artifacts

- Required Plan gate: attach this plan with context loaded, scope/non-scope, assumptions/unknowns, affected files, risks, acceptance checks, and vault gaps.
- Checklist evidence fallback: `project_pipelines_create_vault_checklist` timed out, so gate evidence must include the checklist facts: notes read, no convention conflicts, planned verification commands, and capture decision.
- Advancement target: request advance to `botster_plan_review` after submitting the Plan gate.

## Vault Gaps Worth Capturing

- No new durable vault note is needed from planning alone. Existing notes cover the public hub-client boundary, package ownership in the hub, shared TUI form primitives, secret redaction at the hub/core shape, terminal data-plane boundaries, reviewable plan artifacts, and checklist timeout fallback.
- Capture candidate after implementation: a concrete first-party client convention for rendering `DaemonPackageConfiguration` from `schema: Value`, especially secret draft handling and multiline editing behavior, if the implementation establishes a reusable pattern.
