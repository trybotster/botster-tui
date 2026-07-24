---
ticket: ticket_1781054950_479578
title: Teach botster-tui to display installed package registry state
step: botster_plan
run: run_1781061911_346944
---

# TUI Package Registry Display Plan

## Context Loaded

- Pipeline context: `ticket_1781054950_479578`, run `run_1781061911_346944`, step `botster_plan`, gate `botster_plan_gate`; no prior artifacts, findings, reviews, questions, or answers.
- Dependency context: dependency ticket `ticket_1781054950_975598` ("Implement local path package install enable disable remove flow") is closed, so the TUI can assume the hub package registry client-facing slice exists in the pinned hub-client dependency unless implementation proves the worktree is stale.
- Repo context: clean worktree before planning; current repo has a single `botster-tui` crate plus docs/scripts. The live-runtime app, hub connection, diagnostics rendering, package-aware status fixture fields, and tests are in `crates/botster-tui/src/app.rs`.
- Hub-client context from the pinned `botster-hub-client` revision `24453ef448fb4c89ed63e784ed518de7ca301cd7`: public DTOs include `DaemonRequest::ListPackages`, `DaemonResponse.packages`, `DaemonResponseKind::Packages`, `DaemonPackage { package_name, version, classification, state, requested_capabilities, provider_profile_admitted }`, `DaemonCapability { surface, scope }`, and `DaemonStatus.package_count` / `enabled_package_count`.
- Vault context loaded: [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], plus identity/goals context.
- Checklist context: `project_pipelines_checklist_instructions` was loaded. Creating the run vault checklist timed out in the plugin worker, matching [[project pipelines checklist worker timeouts require artifact evidence fallback]], so checklist-style evidence is preserved in this plan and gate evidence instead of checklist rows.

## Scope

- Keep this a `botster-tui` client slice.
- Store package registry summary state in `TuiApp`: package count, enabled package count, and the current package list from public hub-client responses.
- Refresh package state through public hub-client requests:
  - consume `DaemonStatus.package_count` and `enabled_package_count` whenever status is applied;
  - issue `DaemonRequest::ListPackages` on connect/refresh alongside the existing status/session pulls, if the public request compiles against the pinned dependency;
  - consume `DaemonResponseKind::Packages` / `DaemonResponse.packages` without parsing strings or using hub internals.
- Render package state inside the existing hub/status diagnostics panel so local hub live-runtime shows installed/enabled/disabled states where operators already inspect hub health; package errors and compatibility failures are shown through public diagnostics.
- Render, per package, at least name, version, classification, state, capability summary, and provider admission state. Compatibility/capability diagnostics continue to come from public `DaemonDiagnostic` rows and package capability DTO fields.
- Add focused tests in `crates/botster-tui/src/app.rs` that drive the production `apply_response` / refresh paths and assert visible package count, enabled count, installed/enabled/disabled package rows, verbatim package state display, capability text, and diagnostics.
- Update `README.md` Local Hub Production docs to mention package registry state and package diagnostics in local hub live-runtime mode.

## Non-Scope

- No TUI-owned package install/enable/disable/remove UI.
- No private daemon protocol, frame constants, socket parsing, or duplicated hub DTOs.
- No changes to `botster-hub`, `botster-core`, package admission policy, provider lifecycle, plugin runtime, browser SPA, Rails relay, or Project Pipelines plugin behavior.
- No broad renderer redesign, new package dashboard, entity-store hydration work, or terminal input/data-plane changes.
- No dependency pin changes unless implementation discovers the pinned hub-client revision lacks a required public package DTO or request.

## Assumptions and Unknowns

- Assumption: the pinned hub-client revision is fresh enough because it already exposes package count/status fields and `ListPackages`.
- Assumption: `DaemonPackage.state` is the authoritative package state display value; the current hub protocol emits installed/enabled/disabled-like states, and the TUI should display the value verbatim instead of inventing local state mapping.
- Assumption: package capability diagnostics are read-only display of `requested_capabilities` plus existing `DaemonDiagnostic` rows; no extra compatibility policy belongs in TUI.
- Unknown: whether the live daemon returns `DaemonResponseKind::Packages` after `ListPackages` in all local live-runtime states, including zero packages. Implementation should write tests for both empty and non-empty package responses and keep live-hub verification best-effort if constructing package fixtures through the daemon is not available in this repo.
- Current protocol fact: package errors and compatibility failures are represented through public diagnostics, not a `DaemonPackage.state == "error"` row state. The implementation should render diagnostics through the existing diagnostic path.
- Worktree/target assumption: this run is bound to `target_id=tgt_c3d470bab78549df920a41e8fb0e58d8` and the assigned worktree is this repository checkout, not an ambient Botster checkout.

## Botster Layers Touched

- TUI: primary layer, `TuiApp` state, hub live-runtime status surface, tests.
- Hub client boundary: consumed only through `botster-hub-client` public request/response/DTO types.
- Docs: README local hub live-runtime section and this plan artifact.

## Affected Surfaces and Files

- `crates/botster-tui/src/app.rs`
  - Add package state fields to `TuiApp`.
  - Pull package list on connect/refresh through `DaemonRequest::ListPackages`.
  - Apply package responses and status counts in `apply_response`.
  - Render package summary and rows in `status_panel`.
  - Add package fixture helpers and focused tests.
- `README.md`
  - Update Local Hub Production diagnostics documentation with package state, counts, capabilities, and read-only behavior.
- Possibly `docs/plans/tui-package-registry-display-plan.md`
  - Keep as the reviewable plan artifact.

## Risks

- Public DTO drift: package DTO field names may change if the dependency pin is stale or updated. Mitigation: compile against `botster-hub-client` and avoid mirrored local structs.
- Scope creep into mutations: the hub-client exposes package mutation requests, but the ticket asks for read-oriented display unless stable actions are intentionally in scope. Mitigation: no TUI controls for mutations in this slice.
- Stale package display after status refresh: status counts without a list can diverge from rendered rows if only one request is refreshed. Mitigation: refresh status, sessions, and package list together on connect and manual refresh.
- Diagnostic duplication or stale errors: existing diagnostic retention is keyed by kind/operation/feature. Package-related diagnostics should use the same path and not add parallel local error storage except package rows.
- Terminal fidelity regression: changes near `handle_dispatch`, `TerminalForward`, resize, attach, or terminal output could break PTY input/data ownership. Mitigation: keep package work confined to status/package requests and rendering.
- Live verification gap: this repo may not have package fixture installation helpers. If live hub package setup is unavailable, unit tests should still prove production rendering paths and live-hub live-runtime should verify no regression to session/terminal diagnostics.

## Acceptance Checks and Tests

- `script/fmt`
- `script/test`
- `script/clippy`
- `cargo run -p botster-tui -- --smoke`
- Existing live-hub live-runtime path remains the runtime proof:
  - `CARGO_TARGET_DIR=/tmp/botster-tui-impl-target script/test-live-hub`
  - If live package fixture setup is unavailable in this repo, document that limitation while still proving the TUI connects through the real hub-client and package rendering is covered through `DaemonResponse` fixtures.
- New or updated focused tests in `crates/botster-tui/src/app.rs` should prove:
  - status responses render `package_count` and `enabled_package_count`;
  - package list responses render installed package rows with enabled/disabled states and preserve arbitrary package state text verbatim;
  - requested capabilities render from `DaemonPackage.requested_capabilities`;
  - provider admission state is visible;
  - package diagnostics from public `DaemonDiagnostic` rows remain visible;
  - manual refresh or connect path requests package list through `DaemonRequest::ListPackages`;
  - boundary test still proves `botster-tui` uses `botster_hub_client` and does not add private protocol plumbing.

## Pipeline Gates and Artifacts

- Required Plan gate: attach this plan with context loaded, scope/non-scope, assumptions/unknowns, affected files, risks, acceptance checks, and vault gaps.
- Checklist evidence fallback: checklist creation timed out, so the gate evidence must include notes read, convention conflict status, verification expectations, and durable knowledge capture decision.
- Advancement target: request advance to `botster_plan_review` after submitting the plan gate.

## Vault Gaps Worth Capturing

- No new durable vault note is needed from planning alone. Existing notes already cover the public hub-client boundary, package registry ownership in hub state, diagnostics from public DTOs, TUI terminal data-plane constraints, plan artifacts, and checklist timeout fallback.
- Capture candidate only if implementation discovers a recurring rule not already covered, such as a stable convention for first-party clients refreshing package counts and package lists together after daemon package mutations.
