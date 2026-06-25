---
ticket: ticket_1782361546_916491
title: Render hub-provided apps and launch diagnostics in botster-tui
step: botster_plan
run: run_1782404671_259486
---

# TUI Installed App Registry Plan

## Context Loaded

- Pipeline context: ticket `ticket_1782361546_916491`, run `run_1782404671_259486`, active step `botster_plan`, gate `botster_plan_gate`; no prior artifacts, reviews, findings, open questions, or answers were present.
- Dependency context: dependency ticket `ticket_1782361545_680661` ("Expose installed app registry and structured app launch DTOs in botster-hub") is closed.
- Worktree/target context: run target is `tgt_c3d470bab78549df920a41e8fb0e58d8`, base ref is `main`, and this plan is for the assigned checkout only.
- Repo context: this is a Rust TUI workspace. The main production surface is `crates/botster-tui/src/app.rs`; docs and command expectations live in `README.md`; dependency pins live in `crates/botster-tui/Cargo.toml` and `Cargo.lock`.
- Current TUI context: `DogfoodApp` already connects through `botster-hub-client`, refreshes status/sessions/packages, renders package rows and diagnostics in the existing hub/status panel, and has focused tests in `crates/botster-tui/src/app.rs`.
- Current dependency observation: this repo pins `botster-hub-client` to `3c7a44890cb26793d97cf87a1c7f866add2b15d9`, which does not expose `DaemonRequest::ListApps`, `DaemonResponseKind::Apps`, `DaemonResponse.apps`, or `DaemonApp`.
- Fresh hub protocol observation: `trybotster/botster-hub` main resolves to `b91d774f31fabe1d8f0d28d538dca8e372988298`. A read-only shallow checkout of that revision exposes `DaemonRequest::ListApps`, `DaemonResponseKind::Apps`, `DaemonResponse.apps: Vec<DaemonApp>`, and `DaemonApp { package_name, app_id, entrypoint_id, kind, launch_mode, lifecycle_state, diagnostics, actions, blocked_reasons, launch_target }`.
- Fresh DTO semantics: `DaemonApp.launch_target.kind` mirrors `web_app` or `terminal_app`; `launch_target.local_url` is optional and hub-provided only for eligible web apps. Clients must not derive it from stdout, stderr, command args, package names, known ports, diagnostics, environment, or paths.
- Vault context loaded: [[identity]], [[goals]], [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], and notes surfaced through the playbooks including [[botster hub client crate is the external client boundary]], [[installed apps are daemon app rows projected from package runnable entrypoints]], [[structured output fields need producer paths or explicit scaffold disposition]], [[botster hub diagnostics use daemon diagnostic rows in client dtos]], [[tui client attach uses hub protocol not session protocol]], and [[plan steps need reviewable plan artifacts]].
- Checklist context: `project_pipelines_checklist_instructions` was loaded. `project_pipelines_create_vault_checklist` timed out in the plugin worker, matching the known checklist fallback path; checklist-style evidence is preserved in this plan and should also be submitted in gate evidence.

## Scope

- Keep the implementation inside `botster-tui`.
- Bump `botster-hub-client` and `botster-hub-test-support` to a hub revision that includes the closed installed-app registry dependency, expected at or after `b91d774f31fabe1d8f0d28d538dca8e372988298` unless implementation finds a newer compatible main revision at start.
- Add app registry state to `DogfoodApp`, populated only from `DaemonResponseKind::Apps` / `DaemonResponse.apps`.
- Issue `DaemonRequest::ListApps` from the existing hub refresh path so the production TUI user path receives app rows from the running hub.
- Render installed apps in the existing hub/status diagnostics panel, near package/runtime diagnostics rather than as a new dashboard.
- For each app row, display hub-provided package/app/entrypoint ids, `kind`, `launch_mode`, `lifecycle_state`, blocked reasons, diagnostics, and action descriptors.
- For `web_app` launch targets, show the hub-provided `launch_target.local_url` when present, plus terse copy/open instructions if the TUI cannot launch a browser itself.
- For `terminal_app` launch targets, show launchability through the hub-provided action descriptors, blocked reasons, lifecycle state, and diagnostics. Do not invent a background URL.
- Preserve existing session, terminal, package registry, configuration, lifecycle/dependency/update, and diagnostics rendering.
- Update `README.md` only as needed to document local dogfood app registry display and the current direct/open flow.
- Add focused tests proving production response handling and rendering use authoritative hub-client DTO data.

## Non-Scope

- No broad TUI redesign, navigation overhaul, dashboard rebuild, or new renderer primitive.
- No hub implementation work, app supervisor policy, launch-result generation, app-open implementation, browser-opening integration, clipboard integration, or package lifecycle policy.
- No inference of lifecycle, update, dependency, compatibility, blocked, or launch state from package rows, process rows, diagnostics text, command strings, package names, local paths, ports, or environment values.
- No private daemon protocol, locally mirrored DTOs, or parsing generated TypeScript fixtures as the TUI source of truth.
- No Project Pipelines plugin, browser SPA, Rails, Lua plugin runtime, or MCP workflow changes.
- No terminal/session attach, input forwarding, resize, scrollback, or data-plane changes except for unavoidable mechanical compile updates from the hub-client bump.

## Assumptions and Unknowns

- Assumption: the app registry should be displayed in the existing dogfood hub/status panel because this is the smallest surface already used for package and diagnostic state.
- Assumption: `DaemonApp.actions` are read-only descriptors for this ticket. The TUI should render action availability and mapped request metadata, but should not add new command buttons unless the existing TUI action plumbing already supports the request safely and narrowly.
- Assumption: copy/open instructions can be text instructions. Native clipboard or browser launch is out of scope because the ticket asks for copyable/open instructions, not OS integration.
- Unknown: the exact newest compatible hub main revision at implementation time. The implementer must verify `git ls-remote https://github.com/trybotster/botster-hub.git refs/heads/main` before bumping pins.
- Unknown: whether `botster-hub-test-support` at the fresh revision introduces compile changes outside `app.rs`. Keep any mechanical fixes narrow and caused by the pin update.
- Unknown: whether a live isolated-hub fixture can produce real app rows in this repo. If not, unit tests must still prove the production `apply_response` and rendered surface path, and live smoke should prove no regression to existing hub/session/package behavior.
- Convention conflict status: none. The plan follows the public hub-client boundary, keeps product policy in the hub/plugin side, and avoids TUI-side inference.

## Botster Layers Touched

- TUI: primary layer; app state storage, refresh sequencing, rendering, and tests in `DogfoodApp`.
- Hub client boundary: dependency pins updated to consume public app DTOs from `botster-hub-client`.
- Docs: local dogfood README wording and this plan artifact.
- Pipeline artifacts: Plan gate evidence plus checklist fallback evidence.

## Affected Surfaces and Files

- `crates/botster-tui/src/app.rs`
  - Import `DaemonApp` and any needed app/action DTOs from `botster-hub-client`.
  - Add `apps: Vec<DaemonApp>` to `DogfoodApp`.
  - Send `DaemonRequest::ListApps` in the existing refresh path, alongside status/sessions/packages.
  - Apply `DaemonResponseKind::Apps` by replacing `self.apps` from `response.apps`.
  - Render an app summary and rows from `self.apps`.
  - Add helpers such as `app_text`, `app_launch_target_text`, and reuse/extend existing package diagnostic/action text helpers.
  - Add tests and fixture helpers for web app URL display, terminal app launchability, blocked reasons, diagnostics, actions, and existing flow preservation.
- `crates/botster-tui/Cargo.toml`
  - Bump `botster-hub-client` and `botster-hub-test-support` git revisions to a compatible hub commit containing app DTOs.
- `Cargo.lock`
  - Update only the lockfile changes caused by the required hub dependency bump.
- `README.md`
  - Update the Local Package / local hub dogfood section to describe TUI app registry display, web app URL instructions, and terminal app launchability from hub DTOs.
- `docs/plans/tui-installed-app-registry-plan.md`
  - Reviewable plan artifact for this ticket.

## Risks

- Stale dependency risk: implementing against the current pin cannot satisfy the ticket. Mitigation: verify and bump to a hub revision with `ListApps` before app-code changes.
- DTO drift risk: hub main may have moved after this plan. Mitigation: inspect the fresh `botster-hub-client` structs after the bump and update tests to the actual public Rust DTOs.
- Policy inference risk: app state is tempting to derive from existing package runnable entrypoints. Mitigation: store and render only `DaemonResponse.apps` for installed app rows.
- PII/path leakage risk: app rows, actions, diagnostics, and request mappings must stay sanitized. Mitigation: tests should include fixture values that would fail if local paths, socket paths, tokens, or host environment values are rendered.
- UI overcrowding risk: the existing panel already shows package details. Mitigation: use compact one-line app rows plus subordinate diagnostics/action rows only when present.
- Action execution scope creep: adding clickable app actions would expand behavior beyond display. Mitigation: render descriptors first; only wire buttons if the existing dispatch/request plumbing makes it a small direct mapping and tests prove it.
- Terminal regression risk: `app.rs` owns terminal attach and input behavior near the same state machine. Mitigation: keep edits to app registry refresh/rendering and avoid terminal data-plane paths.
- Live verification gap: live app rows may be hard to produce in the isolated test harness. Mitigation: use unit tests for DTO rendering and run live hub smoke for regression.

## Acceptance Checks and Tests

- `script/fmt`
- `script/test`
- `script/clippy`
- `cargo run -p botster-tui -- --smoke`
- `CARGO_TARGET_DIR=/tmp/botster-tui-apps-target script/test-live-hub`, unless required hub/session-worker binaries or fixture support are unavailable; record the exact reason if skipped.
- Focused tests in `crates/botster-tui/src/app.rs` must prove:
  - `DaemonResponseKind::Apps` updates `DogfoodApp.apps` from `DaemonResponse.apps`.
  - the refresh path issues `DaemonRequest::ListApps` through the same production request path as other hub reads.
  - a `web_app` row renders package/app/entrypoint ids, `launch_mode`, lifecycle, and the hub-provided `launch_target.local_url`.
  - a `web_app` row without `local_url` remains visible and shows diagnostics or blocked reasons instead of deriving a URL.
  - a `terminal_app` row renders launchability from lifecycle/action descriptors and never renders a fake URL.
  - blocked launch reasons and `DaemonPackageDiagnostic` rows are visible.
  - action descriptors render `action_id`, status, reason/diagnostics, required references, and request mapping fields that are safe to show.
  - package, session, terminal, compatibility, and existing package diagnostics tests continue to pass.
  - no fixture local paths, socket paths, raw env values, auth values, or tokens appear in rendered app output.
- Runtime-path proof: tests should drive `DogfoodApp::apply_response`, the real refresh request method, and `surface()`/rendered text extraction, not only pure formatting helpers.

## Pipeline Gates and Artifacts

- Submit `botster_plan_gate` with this plan attached under the required fields.
- Include checklist fallback evidence in the gate:
  - notes read: the vault notes listed in Context Loaded;
  - convention conflicts: none;
  - verification evidence planned: commands in Acceptance Checks;
  - durable capture: none from planning alone.
- Request advancement to `botster_plan_review` after gate submission.

## Vault Gaps Worth Capturing

- No new durable vault note is needed from planning alone. Existing notes already cover the relevant boundaries: public hub-client DTO ownership, installed apps as daemon app rows, structured launch outputs requiring producer paths, diagnostics from public DTOs, TUI hub protocol usage, path-neutral plan artifacts, and checklist timeout fallback.
- Capture candidate for implementation: if app action descriptor rendering becomes a reusable first-party client convention, capture a Botster note after implementation and verification evidence exists.
