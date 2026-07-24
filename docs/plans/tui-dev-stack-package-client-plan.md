---
ticket: ticket_1782761743_987176
title: Make botster-tui a dev-stack package client over shared hub app surfaces
step: botster_plan
run: run_1782775153_356366
---

# TUI Dev-Stack Package Client Plan

## Context Loaded

- Pipeline context: ticket `ticket_1782761743_987176`, run `run_1782775153_356366`, active step `botster_plan`, gate `botster_plan_gate`; no prior artifacts, reviews, findings, open questions, or answers were present.
- Dependency context: dependency ticket `ticket_1782754706_165704` ("Switch botster-tui to consume botster-tui-kit renderer") is closed.
- Worktree/target context: run target is `tgt_c3d470bab78549df920a41e8fb0e58d8`, base ref is `main`, and this plan is for the assigned checkout only.
- Vault/playbook context: [[identity]], [[goals]], [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], [[plan steps need reviewable plan artifacts]], and [[project pipelines checklist worker timeouts require artifact evidence fallback]].
- Repo context inspected: `botster-package.json`, `README.md`, `Cargo.toml`, `crates/botster-tui/Cargo.toml`, `crates/botster-tui/src/main.rs`, `crates/botster-tui/src/app.rs`, `crates/botster-tui/src/renderer.rs`, `crates/botster-tui/tests/package_manifest_test.rs`, `script/fmt`, `script/test`, `script/clippy`, and `script/test-live-hub`.
- Current implementation context: `botster-tui` already depends on `botster-hub-client` and `botster-tui-kit`, consumes `BOTSTER_HUB_CONNECTION`, lists sessions/packages/apps, renders diagnostics through `TuiApp::surface`, can spawn/attach/drain/send input/resize through `DaemonRequest`, and has an isolated live-Hub headless live-runtime test.
- Current manifest context: `botster-package.json` already declares a `terminal_app` runnable entrypoint `tui` with `foreground_stdio`, `target/debug/botster-tui`, package-root working directory, and required typed Hub connection/data-dir injections.
- Checklist context: `project_pipelines_checklist_instructions` was loaded. `project_pipelines_create_vault_checklist` timed out with `plugin worker invoke timeout`, matching the known fallback note; checklist-style evidence is preserved in this plan and should also be submitted in gate evidence.

## Scope

- Keep the implementation inside `botster-tui` unless acceptance exposes a missing upstream package-app contract that must be registered as a dependency rather than reimplemented here.
- Make `botster-tui` usable as a first-party dev-stack package-launched client, with the package runnable entrypoint and README/script flow reflecting the real `botster-hub apps open botster-tui tui` path instead of a future placeholder.
- Prove the production path consumes the Hub-launched `BOTSTER_HUB_CONNECTION` descriptor plus the required data-dir injection.
- Preserve the public `botster-hub-client` boundary for all hub communication: status, sessions, packages, apps, diagnostics, package entrypoint lifecycle, terminal attach/drain/input/resize, and any app-open/entrypoint actions supported by the pinned DTOs.
- Render installed app/package/surface-adjacent rows only from hub-provided app/package/action/diagnostic DTOs. "Open plugin surfaces where supported" means using hub package app mechanics and hub-provided action descriptors available in the public client protocol; it does not mean inventing a private plugin UI renderer.
- Extend the headless live-runtime harness or add a sibling live-hub acceptance path so it installs/enables the local package in a persistent dev data dir, launches/opens the TUI through hub package app mechanics, and performs at least one real hub action through the launched client.
- Update README commands so reviewers can reproduce the app-open flow against an explicit dev data dir and use the same typed descriptor for direct lower-level runs.

## Non-Scope

- Do not reintroduce `botster-hub`'s embedded TUI or link `botster-tui` against hub internals.
- Do not add private socket frames, copied daemon DTOs, local session-worker protocol constants, or parsed hub stdout contracts.
- Do not implement hub package/app launcher behavior in this repo if the pinned hub does not support it; register a dependency or document the upstream blocker instead.
- Do not add browser SPA behavior, Rails relay behavior, Project Pipelines policy, plugin workflow policy, or generic plugin surface rendering beyond the public app/action descriptors this TUI can consume.
- Do not broaden the package manifest into optional configurability or alternate launch modes unless the hub app-open contract requires it.
- Do not touch unrelated renderer primitives, terminal fidelity, or package lifecycle flows except where needed to prove the package-launched client path.

## Assumptions and Unknowns

- Assumption: the closed `botster-tui-kit` dependency means renderer extraction is ready enough for this ticket; implementation should continue using `botster-tui-kit` rather than moving rendering back into the TUI crate.
- Assumption: `botster-hub apps open botster-tui tui` is the intended dev-stack package app mechanic because existing vault notes identify app CLI selectors and daemon-resolved terminal launch contracts as the compatibility path.
- Assumption: a persistent dev data dir can live under a temporary path for automated acceptance as long as it survives across the hub start/install/open/action sequence and is not the user's durable home Botster state.
- Assumption: the live package-open proof can be headless or PTY-captured. It does not need an interactive human terminal if the launched client performs a real hub action and emits inspectable evidence.
- Unknown: whether the pinned `botster-hub-client`/`botster-hub-test-support` revisions expose a direct test helper for package install plus `apps open`. If not, implementation should either shell through the built `botster-hub` binary in `script/test-live-hub` or register a dependency on the missing helper.
- Unknown: whether the current hub app-open foreground terminal launcher can inject `BOTSTER_PACKAGE_DATA_DIR` and `BOTSTER_HUB_CONNECTION` into a foreground stdio app in the test harness. Acceptance must fail clearly if required manifest injections are declared but not consumed or supplied.
- Unknown: exact support for plugin-owned UI surfaces in the public TUI client protocol. If there is no public `ui_tree_snapshot`/surface-open path in the pinned client DTOs, the implementation should keep this ticket to hub-provided installed app/action descriptors and document the unsupported richer plugin-surface path.
- Convention conflict status: none. The plan follows the public hub-client boundary, keeps product policy in hub/plugin surfaces, uses the existing package manifest, and avoids embedded hub TUI revival.

## Botster Layers Touched

- TUI: primary layer; app launch argument/env handling, hub-client runtime path, diagnostics display, package/app action handling, and tests.
- Hub client boundary: consumed as the public protocol crate only; dependency pins may move only if needed for package app-open DTOs or test support.
- Package manifest/app surface: `botster-package.json` runnable entrypoint and injection contract.
- Scripts/docs: live-hub package-open proof and README instructions.
- Pipeline artifacts: this plan document plus Plan gate evidence with checklist fallback.

## Affected Surfaces and Files

- `botster-package.json`
  - Confirm or adjust the `terminal_app` runnable entrypoint so it exactly matches the hub app-open launch contract.
  - Keep required Hub connection and package data-dir injections aligned with actual runtime consumption.
- `crates/botster-tui/src/app.rs`
  - Validate and display package-launch diagnostics for missing required injected data where the TUI can observe it.
  - Preserve `DaemonRequest` use for session/package/app actions and add only the smallest supported request handling needed for package-open acceptance.
  - Extend headless live-runtime/runtime assertions to prove the launched client performs a real hub action.
- `crates/botster-tui/src/main.rs`
  - Only if argument handling needs a narrow package-launch mode or clearer failure path.
- `crates/botster-tui/tests/package_manifest_test.rs`
  - Strengthen manifest tests so declared required injections are matched by runtime argument/env consumption expectations.
- `crates/botster-tui/Cargo.toml` and `Cargo.lock`
  - Only if the pinned hub client/test-support revision lacks the required app-open package mechanics.
- `script/test-live-hub`
  - Build matching hub/session-worker binaries, start a persistent dev data dir, install/enable the local package, open the app through hub mechanics, and require at least one observed hub action from the launched client.
- `README.md`
  - Replace "future app-open flow" wording with the supported dev-stack package launch flow and keep direct socket launch documented as fallback.
- `docs/plans/tui-dev-stack-package-client-plan.md`
  - Reviewable Plan artifact for this ticket.

## Risks

- Embedded-TUI regression risk: trying to satisfy app-open by calling hub internals could revive the old embedded TUI shape. Mitigation: use `botster-hub-client` and hub package app launcher only.
- Unwired-manifest risk: declared injections may remain documentation if the launched process never reads or validates them. Mitigation: test argument/env consumption and include a negative diagnostic path for missing required launch data.
- False acceptance risk: direct `cargo run -- --headless-live-runtime` proves a client socket path but not package-launched app mechanics. Mitigation: add an acceptance check that goes through package install/open against a persistent data dir.
- Upstream capability risk: package app-open may be absent or incomplete in the pinned hub dependency. Mitigation: verify the public hub-client/test-support surface first, bump pins narrowly when merged support exists, or register a blocking dependency rather than implementing hub policy locally.
- Plugin-surface ambiguity risk: "open plugin surfaces where supported" can mean app rows/action descriptors or full plugin `UiNode` rendering. Mitigation: only implement the public supported path exposed by hub-client DTOs; document unsupported richer surfaces if absent.
- PII/path leakage risk: package-open diagnostics can expose local absolute paths, sockets, usernames, or data dirs. Mitigation: assert rendered diagnostics and README examples stay path-neutral or use placeholders.
- Live-test flake risk: foreground app launch, PTY capture, and hub shutdown can race. Mitigation: use bounded readiness checks, short temp roots, explicit shutdown, and fail with captured diagnostics rather than silent skips when `BOTSTER_TUI_REQUIRE_HUB_TEST=1`.

## Acceptance Checks and Tests

- `script/fmt`
- `script/test`
- `script/clippy`
- `cargo run -p botster-tui -- --smoke`
- `CARGO_TARGET_DIR=/tmp/botster-tui-package-client-target script/test-live-hub`
- Focused tests should prove:
  - `botster-package.json` still validates as a package manifest and exposes exactly one foreground `terminal_app` runnable entrypoint for `botster-tui`.
  - required `hub_connection` and `data_dir` injections are declared and mapped to runtime-observable environment inputs.
  - app startup produces clear diagnostics when the Hub connection descriptor or required package-launch environment is missing.
  - the refresh path issues `Status`, `ListSessions`, `ListPackages`, and `ListApps` through the real `botster-hub-client` request path.
  - installed app/package rows and supported action descriptors come from hub DTOs, not inferred local manifest parsing.
  - package entrypoint start/status/open actions, if wired by the public DTOs, dispatch through typed `DaemonRequest` variants.
  - the live package-open path uses a persistent dev data dir, installs/enables the local checkout as a package, launches/opens `botster-tui` via hub app mechanics, and observes at least one real hub action such as status/list/spawn/attach/drain from the launched client.
  - the live path does not require or mutate the user's real Botster home state and does not render raw local paths, tokens, auth values, or socket paths in user-facing diagnostics.
- Runtime-path proof: acceptance must show the production entrypoint (`main` -> `AppArgs::parse` -> `app::run` or `--headless-live-runtime`) is launched by hub package app mechanics. Source-code existence alone is not sufficient.

## Pipeline Gates and Artifacts

- Submit `botster_plan_gate` with this plan mapped into the required fields.
- Include checklist fallback evidence in the gate:
  - notes read: the vault notes listed in Context Loaded;
  - convention conflicts: none;
  - verification evidence planned: commands in Acceptance Checks;
  - durable capture: none from planning alone;
  - checklist tool status: `project_pipelines_create_vault_checklist` timed out, so evidence is preserved here and in gate evidence per [[project pipelines checklist worker timeouts require artifact evidence fallback]].
- Request advancement to `botster_plan_review` after gate submission.

## Vault Gaps Worth Capturing

- Capture the final package-launched first-party TUI acceptance pattern if implementation establishes a reusable live-hub harness for `packages install` plus `apps open` plus observed client action.
- Capture the exact meaning of "plugin surfaces where supported" for first-party TUI clients once implementation proves whether public hub-client DTOs support only app/action descriptors or richer `UiNode` surface opening.
- Capture any manifest-injection consumption convention if this ticket turns declared `hub_connection`/`data_dir` injections into a repeatable runtime-validation pattern for package clients.
