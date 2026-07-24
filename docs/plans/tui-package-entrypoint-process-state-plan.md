---
ticket: ticket_1781065271_922662
title: Display package entrypoint process state in botster-tui
step: hotwire_plan
run: run_1781111328_232793
---

# TUI Package Entrypoint Process State Plan

## Context Loaded

- Pipeline context: ticket `ticket_1781065271_922662`, run `run_1781111328_232793`, active step `hotwire_plan`, gate `hotwire_plan_gate`; no prior artifacts, reviews, findings, questions, or answers were present.
- Dependency context: dependency ticket `ticket_1781065270_520493` ("Supervise local package entrypoint processes in botster-hub") is closed. Plan Review verified that dependency PR #56 landed on `trybotster/botster-hub` at merge commit `ae73a41` on 2026-06-10, after this repo's current `botster-hub-client` pin. Updating the `botster-hub-client` git revision to a commit including PR #56 is required.
- Repo context: this is a Rust TUI workspace, not a Hotwire Rails app. The governing repo shape is `Cargo.toml`, `crates/botster-tui`, and existing live-runtime docs/scripts. The current package diagnostics surface and package DTO rendering live in `crates/botster-tui/src/app.rs`.
- Existing local surface: `TuiApp` already refreshes status, sessions, and packages; renders package summary and package rows in the hub/status diagnostics panel; and tests package count, package rows, package diagnostics, and refresh request sequencing.
- Current dependency observation: the pinned `botster-hub-client` revision in `crates/botster-tui/Cargo.toml` predates PR #56 and does not expose the required process-state DTOs. Implementation must bump the pin and consume the real public DTOs: `DaemonPackage.runnable_entrypoints: Vec<DaemonPackageRunnableEntrypoint>`, with each entrypoint carrying `process: DaemonPackageProcess`.
- Vault context loaded: [[identity]], [[goals]], [[planner-playbook]], [[hotwire-app-planner-playbook]] for applicability check only, [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], and [[plan agents must author vault context as wikilinks not home paths]].
- Skill context loaded: `botster-customize-tui`, because this ticket changes the first-party Botster TUI client surface.
- Checklist context: `project_pipelines_checklist_instructions` was loaded. Creating the run vault checklist timed out in the plugin worker, matching [[project pipelines checklist worker timeouts require artifact evidence fallback]], so checklist-style evidence is preserved in this plan and gate evidence.

## Scope

- Keep the change inside `botster-tui`.
- Bump `botster-hub-client` to a git revision including PR #56, at or after merge commit `ae73a41`.
- Consume package entrypoint/process state from public `botster-hub-client` DTOs only.
- Extend the existing package/operator diagnostics surface so each package can show entrypoint id, kind, process state, pid/timestamps/exit status when present, and diagnostic/error text when present.
- Preserve existing package registry fields already shown: package name, version, classification, state, requested capabilities, and provider profile admission.
- Add focused tests for rendering running, failed, stopped, and no-URL-public-DTO entrypoint states from hub-client DTO fixtures.
- Optionally add a single README local hub live-runtime sentence mentioning entrypoint process-state visibility through public hub-client DTOs; avoid broader documentation cleanup.
- Keep dependency and lockfile changes limited to the required `botster-hub-client` bump and resulting dependency graph.

## Non-Scope

- No private daemon protocol, private socket parsing, duplicated hub DTOs, locally invented process-state schema, or synthetic URL field.
- No `botster-hub` implementation work, package supervisor policy, lifecycle ownership, process spawning, retry behavior, or diagnostics generation.
- No package install/enable/disable/remove controls in TUI.
- No Project Pipelines plugin, browser SPA, Rails, Lua plugin runtime, or MCP workflow changes.
- No terminal/session behavior changes: avoid terminal attach, input forwarding, resize, scrollback, and session data-plane paths unless the compiler forces a narrow mechanical adjustment.
- No new dashboard or broad renderer redesign.

## Assumptions and Unknowns

- Assumption: the authoritative text values for process states are the hub-client DTO values; the TUI should render them verbatim or through a minimal display helper, not reinterpret lifecycle policy.
- Confirmed DTO path: process state lives under `DaemonPackage.runnable_entrypoints[*].process`, where each `DaemonPackageProcess` carries `state`, `pid: Option<u32>`, `started_at: Option<u64>`, `exited_at: Option<u64>`, `exit_status: Option<String>`, and `diagnostics: Vec<DaemonPackageDiagnostic>`.
- Confirmed entrypoint fields available for display include id, kind, command, args, working_directory, environment, mode, capabilities, may_supervise, and process. The public DTO does not expose a URL field.
- Interpretation of ticket "url or diagnostics": URL is not available from the public DTO, so this TUI slice should not derive or invent one. Render id/kind/state plus process details and diagnostics; the no-URL acceptance case proves the entrypoint remains visible without a URL field.
- Assumption: `runnable_entrypoints` is `#[serde(default)]`, so existing package fixtures and packages with no supervisable entrypoints can keep rendering the current package row without regression.
- Worktree/target assumption: this run is bound to `target_id=tgt_c3d470bab78549df920a41e8fb0e58d8` and this assigned checkout, not an ambient Botster repo.
- Convention conflict status: none. The plan follows Botster's public hub-client boundary and keeps workflow policy out of the TUI.

## Botster Layers Touched

- TUI: primary layer; `TuiApp` package state formatting, hub/status diagnostics surface, and focused tests.
- Hub client boundary: required `botster-hub-client` pin update to a revision including PR #56, then public DTO consumption through `botster-hub-client`.
- Docs: README local hub live-runtime documentation and this plan artifact.

## Affected Surfaces and Files

- `crates/botster-tui/src/app.rs`
  - Extend package formatting helpers to iterate `DaemonPackage.runnable_entrypoints`.
  - Render each entrypoint's id, kind, process state, pid/timestamps/exit status when present, and diagnostics when present.
  - Preserve the existing package row output for packages with zero runnable entrypoints.
  - Keep process-state rendering inside the existing hub/status package diagnostics panel.
  - Add fixture helpers matching the public hub-client DTO shape.
  - Add tests for running, failed, stopped, and no-URL-public-DTO states.
- `crates/botster-tui/Cargo.toml` and `Cargo.lock`
  - Bump `botster-hub-client` to a revision including PR #56 and update the lockfile accordingly.
- `README.md`
  - Optional single-sentence note that local hub live-runtime shows package entrypoint state and diagnostics through public hub-client DTOs.
- `docs/plans/tui-package-entrypoint-process-state-plan.md`
  - Reviewable Plan artifact for this ticket.

## Risks

- Stale dependency pin: the current worktree predates PR #56. Mitigation: update `botster-hub-client` to a revision including PR #56 before implementation work against the DTO.
- DTO regression risk: the implementer must use the real `DaemonPackage.runnable_entrypoints[*].process` structs from `botster-hub-client`, not mirrored local structs.
- Over-interpreting lifecycle state: TUI should not decide whether an entrypoint is healthy or retryable. Mitigation: render hub-provided state, process details, and diagnostics.
- URL mismatch: the public DTO has no URL field despite the ticket wording. Mitigation: do not derive or invent URL; render process details and diagnostics and test that no-URL DTOs still produce visible entrypoint state.
- Diagnostic duplication: package process diagnostics should use the existing diagnostics surface or package row fields, not a parallel error store.
- Terminal regression: nearby app code also owns terminal attach/input behavior. Mitigation: keep edits confined to package state formatting, package fixtures, docs, and dependency pin if needed.
- README scope creep: docs are useful for live-runtime behavior, but this ticket does not require broad documentation. Mitigation: keep docs to at most one local live-runtime sentence, or skip README if tests and UI rendering are self-explanatory.
- Live fixture gap: this repo may not have a local package fixture that launches entrypoints under an isolated hub. Mitigation: unit tests must prove the production TUI rendering path from hub-client DTOs; live-hub verification should still prove no live-runtime regression.

## Acceptance Checks and Tests

- `script/fmt`
- `script/test`
- `script/clippy`
- `cargo run -p botster-tui -- --smoke`
- `CARGO_TARGET_DIR=/tmp/botster-tui-impl-target script/test-live-hub`, unless the environment lacks the required hub/session-worker binaries. If skipped or unavailable, record the exact reason.
- Focused tests in `crates/botster-tui/src/app.rs` must cover:
  - a package entrypoint in a running state with process state visible and no URL field required;
  - a failed state with diagnostic/error text visible;
  - a stopped state visible;
  - an entrypoint DTO with no URL field still renders id/kind/state plus available process details or diagnostics;
  - packages with zero runnable entrypoints preserve the existing package row output;
  - packages with multiple runnable entrypoints render each entrypoint without collapsing state;
  - existing package count/package row diagnostics still render;
  - refresh still pulls the package read model through public `DaemonRequest` paths.
- Runtime-path proof: tests should drive `TuiApp::apply_response` and `surface()`/renderer output, not only call a pure formatting helper.

## Pipeline Gates and Artifacts

- Submit `hotwire_plan_gate` with this plan attached under the required fields.
- Include checklist fallback evidence in the gate: notes read, no convention conflicts, expected verification commands, and no durable vault capture from planning.
- Request advancement to plan review after the gate is submitted.

## Vault Gaps Worth Capturing

- No new durable vault note is needed from planning alone. Existing notes cover public hub-client boundaries, package registry ownership, diagnostics from public DTOs, TUI terminal data-plane constraints, checklist timeout fallback, and path-neutral plan artifacts.
- Capture candidate for implementation: if the closed dependency establishes a recurring first-party client convention for displaying package entrypoint process state, capture that as a Botster client/display note after implementation evidence exists.
