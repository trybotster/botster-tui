# TUI UiNode Renderer Primitives Plan

Ticket: Implement TUI UiNode renderer primitives against core fixtures
Run: run_1780954063_609292
Step: botster_plan

## Context Loaded

- Pipeline context from `project_pipelines_current_context`: ticket, run, active Plan step, gate prompt, closed dependencies, no prior artifacts, no findings, no open questions, and no prior answers.
- Role playbooks: [[planner-playbook]], [[botster-planner-playbook]].
- Required Botster vault context: [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]].
- Identity/current goals: [[identity]], [[goals]].
- TUI skill context: `botster-customize-tui` skill, especially shared UI contract forms and renderer-neutral action guidance.
- Repo evidence: `crates/botster-tui` is a scaffold with a placeholder draw path; `crates/botster-tui-spike` contains the prior ratatui/crossterm proof with UiNode-like fixtures, hit maps, semantic action envelopes, terminal forwarding, and redraw tests.
- Checklist evidence: run checklist `checklist_1780954148_993742` records vault notes read, no convention conflicts, planned verification commands, and capture decision. Checklist creation initially returned a plugin-worker timeout but persisted successfully on listing.

## Scope

Implement the first production `botster-tui` renderer registry for shared UiNode v1 primitives, using the current ratatui/crossterm foundation and the closed core fixture dependencies.

Planned work:

- Add renderer modules under `crates/botster-tui/src/` for UiNode v1 structures, primitive dispatch, rendering, hit-map/action metadata, fixtures or fixture loading, and tests.
- Render universal primitives required by the ticket: text, badge/status, list/list_item, panel, stack/inline, table-lite, button/action row, form/text_input/select/checkbox, field errors, dialog, empty_state, and unsupported primitive fallback.
- Prove action metadata is preserved as semantic action requests, not renderer-specific event names.
- Prove form validation display for field-level errors and action/validation envelopes.
- Prove unsupported primitive fallback is safe, visible, and non-panicking.
- Wire the production app draw path through the new renderer using a representative fixture tree until hub/core runtime input exists in this repo. This is the runtime/user-path proof for the current scaffold.
- Keep the existing smoke path working.
- Update README or ADR text only where needed to describe the new renderer command/test path and retire stale "not included yet" wording.

## Non-Scope

- Do not implement hub connection, pairing, auth, Unix socket attach, SessionIo/ClientWorker terminal subscriptions, or entity-store hydration.
- Do not change Lua plugin policy, Project Pipelines workflow policy, browser SPA code, Rails relay code, or MCP tools.
- Do not add product-specific Project Pipelines screens, workflow-specific primitives, or operator-workbench behavior.
- Do not add speculative configurability or a second renderer abstraction. A small registry/dispatch table is enough.
- Do not delete the spike crate unless implementation proves it is obsolete and all useful coverage has been ported; cleanup is secondary to the renderer ticket.

## Assumptions And Unknowns

- Assumption: the current worker is in the assigned worktree for target `tgt_c3d470bab78549df920a41e8fb0e58d8`.
- Assumption: because dependencies are closed, implementation should locate and use the actual core UiNode/action/capability/conformance fixture source if it is available to this repo through a crate, copied fixture artifact, or dependency branch.
- Unknown: this repo currently has no `botster-core` dependency, no `botster_core` crate, and no conformance fixture files. `rg "UiNode|ui_tree|ActionBinding|fixture|conformance|botster-core|botster_core|UiCapability|validation" -n .` only finds the README, prior plan/ADR, and `botster-tui-spike` local proof fixtures.
- Blocking rule for Implement: if the actual core fixture source cannot be found, ask a human before substituting invented fixtures as acceptance evidence. Local representative fixtures may supplement tests, but they cannot replace "against core fixtures" without approval.
- Assumption: the renderer can define a narrow local adapter type if the core type is not directly importable, but tests must document the mapping to the core fixture schema.
- Unknown: whether table-lite and status/badge are distinct core primitive variants or encoded as text/list metadata. Implementer must follow core fixture shape over local naming preference.

## Affected Surfaces And Files

Expected files/surfaces:

- `crates/botster-tui/src/app.rs`: replace placeholder body with renderer-backed fixture rendering while preserving terminal setup, exit keys, and smoke path.
- `crates/botster-tui/src/main.rs`: likely unchanged except module exposure if needed.
- New `crates/botster-tui/src/renderer.rs` or `crates/botster-tui/src/renderer/mod.rs`: registry and primitive dispatch.
- New focused modules if useful: `ui_node.rs`, `fixtures.rs`, `hit_map.rs`, `actions.rs`, `forms.rs`. Keep names small and aligned with code shape.
- `crates/botster-tui/Cargo.toml`: add only necessary dependencies if core fixture loading requires `serde`/`serde_json`; do not add broad UI/helper crates.
- `README.md`: update included/not-included scope and commands if the renderer is now part of `botster-tui`.
- `docs/adr/0001-ratatui-crossterm-tui-renderer-foundation.md`: update only if the scaffold-only claim is stale after wiring production draw through the renderer.
- `docs/plans/tui-uinode-renderer-primitives-plan.md`: this plan artifact.

Botster layers touched:

- TUI/client renderer and docs only.
- No plugin, Lua core, Rust hub, session/client worker, React SPA, Rails relay, MCP, or production transport changes.

Pipeline gates/artifacts:

- Plan gate: this artifact plus structured gate evidence.
- Implement gate should require committed renderer code, fixture-backed tests, README/ADR updates if touched, and command evidence.
- Review/Verify must check the runtime path: `botster-tui` itself must render through the new registry, not only tests or the spike crate.

## Risks

- Core fixture availability risk: closed dependency status does not mean the fixture files are present in this TUI repo. Mitigate by locating the actual source first and asking a human if unavailable.
- Unwired implementation risk: adding renderer modules without changing `app.rs` would satisfy code-shape evidence but not the user path. Mitigate by rendering a fixture through the production draw path.
- Overfitting risk: copying the spike too literally could miss required primitives such as badge/status, table-lite, select, checkbox, field errors, empty_state, and unsupported fallback. Mitigate with acceptance tests named after ticket primitives.
- Product-policy drift risk: Project Pipelines examples can leak into renderer primitives. Mitigate with generic fixtures and semantic action ids.
- Action contract drift risk: terminal/UI event names can become action ids. Mitigate by preserving core action metadata and testing semantic envelopes.
- Terminal/input scope risk: terminal_view is not listed in this ticket; do not expand terminal data-plane work while implementing form/action primitives.
- Dependency sprawl risk: fixture parsing might tempt extra crates. Use standard Rust, ratatui, crossterm, and minimal serde-only parsing if needed.

## Acceptance Checks And Tests

Minimum checks for Implement/Verify:

- `script/fmt`
- `script/test`
- `script/clippy`
- `cargo run -p botster-tui -- --smoke`

Required test coverage:

- Representative core fixtures render through `botster-tui` renderer tests.
- Text and badge/status render visible labels/status markers.
- List and list_item render rows and preserve item action metadata.
- Panel, stack, and inline lay out children deterministically.
- Table-lite renders headers and rows without requiring a full table abstraction.
- Button/action row renders actions and emits semantic action metadata.
- Form renders text_input, select, checkbox, field values, and field errors.
- Dialog renders title/body/actions and does not swallow unrelated renderer state.
- Empty_state renders a visible empty placeholder.
- Unsupported primitive fallback renders a safe visible message and does not panic.
- The production `app.rs` draw path uses the renderer registry with a representative fixture tree, proving the runtime path changed in the scaffold.

## Vault Gaps Worth Capturing

- Capture where Botster core UiNode conformance fixtures live and how downstream renderer repos should consume them once implementation resolves that source.
- Capture any discovered mapping rule between core primitive names and TUI registry names, especially if table-lite/status/badge differ from local names.
- Capture a TUI renderer foundation follow-up under [[cli-patterns]] only after implementation proves the production scaffold renders through the registry.
