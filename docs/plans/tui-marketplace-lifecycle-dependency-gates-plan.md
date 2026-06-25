---
ticket: ticket_1782338823_663615
title: Render marketplace package lifecycle and dependency gates in botster-tui
step: botster_plan
run: run_1782348887_199357
---

# TUI Marketplace Lifecycle And Dependency Gates Plan

## Context Loaded

- Pipeline context: `ticket_1782338823_663615`, run `run_1782348887_199357`, active step `botster_plan`, gate `botster_plan_gate`; no prior artifacts, findings, reviews, questions, or answers were present.
- Dependency context: both upstream dependencies are closed. PR #72 "Resolve package dependency and feature availability matrices" merged at `74fb6dd72b559ae9eef0c830cc1dd558102a36f9` on 2026-06-25, and PR #73 "Complete hub package lifecycle actions for marketplace v1" merged at `3c7a44890cb26793d97cf87a1c7f866add2b15d9` on 2026-06-25. `trybotster/botster-hub` main currently resolves to `3c7a44890cb26793d97cf87a1c7f866add2b15d9`.
- Repo context: clean worktree before planning. This repo is a compact Rust TUI workspace. The production dogfood app, hub requests, package rows, package configuration, package entrypoint process display, action handling, and most tests live in `crates/botster-tui/src/app.rs`; generic ratatui form/action/input routing lives in `crates/botster-tui/src/renderer.rs`.
- Current dependency context: `crates/botster-tui/Cargo.toml` pins `botster-hub-client` and `botster-hub-test-support` to `b1be4f53e2baf17f0c0e645ee0cd4fa139e3c8ea`, which predates PR #72/#73. The current pin has package listing/configuration/entrypoint state, but not marketplace available package DTOs, install/update plan/status DTOs, or package/dependency/feature availability DTOs.
- Fresh hub-client context from `3c7a44890cb26793d97cf87a1c7f866add2b15d9`: public DTOs add `DaemonRequest::ListAvailablePackages`, `InspectAvailablePackage`, `PreviewPackageInstall`, `InstallPackageRegistryEntry`, `CheckPackageUpdate`, `PreviewPackageUpdate`, `ApplyPackageUpdate`; `DaemonResponse.available_packages`, `install_plan`, and `update_status`; `DaemonPackage.availability`, `dependency_availability`, `feature_availability`, `surfaces`; and related `DaemonAvailablePackage`, `DaemonPackageInstallPlan`, `DaemonPackageUpdateStatus`, `DaemonPackageAvailabilityReason`, and `DaemonPackagePin` types.
- Vault context loaded: [[identity]], [[goals]], [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], and the `botster-customize-tui` skill.
- Checklist context loaded: `project_pipelines_checklist_instructions` was loaded. Creating the run vault checklist timed out in the plugin worker, matching [[project pipelines checklist worker timeouts require artifact evidence fallback]], so checklist-style evidence is preserved in this plan and gate evidence.

## Scope

- Keep this a `botster-tui` client feature over public `botster-hub-client` DTOs.
- Bump `botster-hub-client` and `botster-hub-test-support` to `3c7a44890cb26793d97cf87a1c7f866add2b15d9` or a later main commit only if implementation verifies it is required by the same merged dependency surface.
- Render installed package availability from `DaemonPackage.availability`, `dependency_availability`, and `feature_availability` inside the existing hub/package diagnostics panel.
- Render package-level blocked reasons, dependency rows, feature rows, reason/action vocabulary, package names, capability requirements, and config/auth requirement labels exactly from hub DTO fields. The TUI must not infer blocked/auth/update/dependency state from package state, config schema, or capability lists.
- Render marketplace/available package rows from `DaemonResponse.available_packages` when list/inspect/preview/install requests are applied, including entry id, package name, version, classification, source kind/label, first-party flag, installed-vs-available state, compatibility, requested capabilities, and pin/update metadata when present.
- Render lifecycle action results from `DaemonPackageDecision`, `DaemonPackageInstallPlan`, and `DaemonPackageUpdateStatus`, including install/update effects, diagnostics, mutates-registry, starts-entrypoints, update availability, reload-required, and restart-required.
- Add keyboard/mouse-accessible actions where the current dogfood surface can support them without a larger navigation redesign: installed package enable/disable/remove, entrypoint start/stop/restart/status, and refresh/update status actions should use existing shared `ui.button` action dispatch. Registry install/update preview actions may require a test-only or configurable registry path; if no operator-safe path source exists, render preview/install results from DTO fixtures and document the runtime limitation instead of inventing a hidden path convention.
- Preserve the existing package registry, configuration, entrypoint process, terminal, compatibility, and diagnostics behavior.
- Update README local hub dogfood docs to mention marketplace available rows, availability gates, blocked reasons, lifecycle actions, and that the TUI consumes hub-resolved state.

## Non-Scope

- No `botster-hub`, `botster-core`, package resolver, registry fetcher, lifecycle policy, update policy, auth provider, or capability admission changes.
- No private daemon protocol, private socket parsing, duplicated DTO structs, handwritten generated protocol mirrors, or TUI-side dependency/auth/update inference.
- No hosted marketplace, remote fetch/clone UI, package source authoring, registry editor, or package manager policy.
- No broad TUI redesign, separate marketplace screen, entity-store hydration, route system, plugin surface renderer, or Project Pipelines operator workbench expansion.
- No terminal data-plane, attach/detach, PTY input, resize, scrollback, or session lifecycle refactor.
- No PII, local path, token, auth identity, raw secret, socket path, or worktree path display in rendered output or tests.

## Assumptions And Unknowns

- Assumption: the authoritative source for dependency gates and lifecycle action availability is the merged `botster-hub-client` DTO surface at `3c7a44890cb26793d97cf87a1c7f866add2b15d9`, not local TUI policy.
- Assumption: reason/action strings such as `missing_package/install_package`, `disabled_package/enable_package`, `missing_provider/install_provider`, `missing_capability/grant_capability`, `missing_config/configure_package`, `missing_auth/authenticate`, `package_disabled/enable_package`, and `fix_configuration` are stable display vocabulary supplied by the hub.
- Assumption: the existing dogfood hub panel is the smallest viable place to render lifecycle and gate state because it already renders package list/config/process state and public diagnostics.
- Unknown: how the TUI should discover an operator-selected marketplace registry path. Implementation should avoid adding speculative config. If no existing CLI arg/env/test-support path exists, keep registry path actions out of runtime UI and prove available package rendering from public DTO fixtures.
- Unknown: whether all lifecycle action requests return refreshed `packages`, only `package_decision`, or operator errors in every branch. Implementation must inspect compile errors and add tests around the observed public response shapes after the dependency bump.
- Unknown: whether live hub test support includes marketplace registry fixtures for list/preview/install/update. If not, unit tests should prove the production TUI rendering/action path from public DTO fixtures, while live-hub dogfood remains the runtime compatibility smoke.
- Worktree/target assumption: this run is bound to `target_id=tgt_c3d470bab78549df920a41e8fb0e58d8` and this checkout is the assigned worktree.
- Convention conflict status: none. The plan follows Botster's public hub-client boundary, keeps package policy in the hub, uses existing shared UI primitives, and limits scope to the TUI client surface.

## Botster Layers Touched

- TUI: primary layer; `DogfoodApp` state, package formatting, package action handling, hub/status package panel, and focused tests.
- Hub client boundary: dependency rev bump and consumption of public `botster-hub-client` request/response/DTO types only.
- Docs: README local dogfood capability text and this plan artifact.
- Not touched: Rust hub implementation, core package resolution, Lua plugin runtime, session/client worker data plane, React SPA, Rails relay, MCP tools, Project Pipelines plugin policy.

## Affected Surfaces And Files

- `crates/botster-tui/Cargo.toml`
  - Bump `botster-hub-client` and `botster-hub-test-support` to the merged marketplace lifecycle/availability commit.
- `Cargo.lock`
  - Update git dependency resolution from the hub-client/test-support bump.
- `crates/botster-tui/src/app.rs`
  - Extend imports and test fixtures to the new public hub-client DTOs.
  - Add optional app state for `available_packages`, `install_plan`, and `update_status` if response rendering needs state beyond installed rows.
  - Apply `DaemonResponseKind::AvailablePackages`, `PackageInstallPlan`, `PackageUpdateStatus`, `Packages`, and `PackageDecision` without losing existing package rows or diagnostics.
  - Extend package row rendering with availability, dependency availability, feature availability, and reason/action text.
  - Render marketplace rows and lifecycle/update/install plan summaries.
  - Add action handling for supported lifecycle requests through existing semantic action buttons.
  - Add focused tests that drive `DogfoodApp::apply_response`, `surface()`, renderer output, and `handle_dispatch` request observation.
- `crates/botster-tui/src/renderer.rs`
  - No planned changes. Touch only if existing button/action routing cannot express required keyboard/mouse-accessible lifecycle actions.
- `README.md`
  - Update Local Hub Dogfood docs to mention hub-resolved marketplace rows, package lifecycle action results, dependency/feature gates, blocked reasons, and no TUI-side inference.
- `docs/plans/tui-marketplace-lifecycle-dependency-gates-plan.md`
  - Reviewable Plan-stage artifact.

## Risks

- Stale dependency risk: current pin predates the closed dependency APIs. Mitigation: bump hub-client/test-support first and compile against public types.
- API drift risk: available package/install/update DTOs may change after PR #73. Mitigation: use the latest verified main commit and avoid local mirrors.
- Policy creep risk: the TUI could infer state from package state, config, auth, or capability fields. Mitigation: render only hub-provided availability, reason/action, plan, decision, and diagnostic rows.
- Registry path UX risk: marketplace list/install requests require `registry_path`, but this repo may not yet have an operator-facing path selection convention. Mitigation: do not invent a broad registry source UI in this ticket; render DTOs and add actions only where an existing path source is available.
- PII/path leakage risk: registry/source/update DTOs and diagnostics must stay path-neutral. Mitigation: assert rendered output does not include fixture local paths, tokens, raw secrets, auth identities, or socket/worktree paths.
- Layout density risk: package rows already include config and entrypoint data. Mitigation: keep formatting compact and textual in the existing panel; avoid new screens unless the current panel cannot hold the required acceptance evidence.
- Input regression risk: lifecycle buttons are near generic action routing and terminal focus. Mitigation: prefer existing `ui.button` dispatch and run existing renderer/app tests.
- Live fixture gap: full marketplace install/update may not be reproducible in this repo's live-hub harness. Mitigation: unit tests prove production rendering/action paths from public DTO fixtures; live-hub test proves bumped client runtime compatibility.

## Acceptance Checks And Tests

- `script/fmt`
- `script/test`
- `script/clippy`
- `cargo run -p botster-tui -- --smoke`
- Existing live-hub runtime proof should still run:
  - `CARGO_TARGET_DIR=/tmp/botster-tui-impl-target script/test-live-hub`
  - If marketplace registry fixtures are unavailable, record the exact limitation while still proving live hub compatibility and package DTO rendering through unit tests.
- Focused app tests in `crates/botster-tui/src/app.rs` should prove:
  - installed packages render `availability.state` and package-level reason/action rows;
  - dependency availability rows render id, package name, state, and each blocked reason/action;
  - feature availability rows render id, state, and each blocked reason/action;
  - missing package, disabled package, missing provider, missing capability, missing config, missing auth, package disabled, and invalid configuration reasons render from DTOs without local inference;
  - available package responses render entry id, source labels, first-party flag, compatibility result/diagnostics, pin metadata, and requested capabilities;
  - install preview/plan responses render effects, diagnostics, `mutates_registry`, and `starts_entrypoints`;
  - update status responses render `update_available`, `reload_required`, `restart_required`, pin metadata, and diagnostics;
  - lifecycle action buttons emit the correct public `DaemonRequest` variants for enable/disable/remove and entrypoint start/stop/restart/status where implemented;
  - package decisions remain visible after action responses and refreshed package rows are applied;
  - existing package registry, package configuration, entrypoint process, diagnostics, terminal attach, and compatibility tests still pass;
  - rendered output and test fixtures do not expose local paths, tokens, raw secrets, socket paths, worktree paths, or auth identities.
- If `botster-hub-test-support` exposes first-party marketplace/availability fixtures at the bumped commit, consume those typed fixtures instead of hand-building every DTO. If not, keep local fixtures typed against `botster_hub_client` structs and do not duplicate protocol JSON.

## Pipeline Gates And Artifacts

- Submit `botster_plan_gate` with this plan under the required fields.
- Checklist evidence fallback: `project_pipelines_create_vault_checklist` timed out, so gate evidence must include notes read, no convention conflicts, expected verification commands, and capture decision.
- Advancement target: request advancement to `botster_plan_review` after gate submission.

## Vault Gaps Worth Capturing

- No new durable vault note is needed from planning alone. Existing notes cover public hub-client boundaries, hub-owned package policy, generated/shared protocol drift, package resolution matrix ownership, package mutation ownership, TUI terminal data-plane constraints, reviewable plan artifacts, and checklist timeout fallback.
- Capture candidate after implementation: a first-party client convention for rendering package availability/reason/action matrices and marketplace lifecycle action state in compact terminal clients, if the implementation establishes a reusable display pattern.
