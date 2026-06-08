# TUI UiNode Renderer Primitives Plan

Ticket: Implement TUI UiNode renderer primitives against core fixtures
Run: run_1780954063_609292
Step: botster_plan

## Context Loaded

- Pipeline context from `project_pipelines_current_context`: ticket, run, active Plan step, gate prompt, closed dependencies, prior Plan artifact/gate, Plan Review verdict `changes_required`, and six open review findings.
- Role playbooks: [[planner-playbook]], [[botster-planner-playbook]].
- Required Botster vault context: [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]].
- Shared UI contract context loaded after Plan Review: [[cross-client ui should share semantic primitives and actions with renderer-specific adapters]], [[tui adapter maps shared primitives onto existing rust render tree without flag day rewrite]], [[botster shared form primitive v1 is intentionally narrow and catalyst first]], [[phase one action ids are semantic botster events not DOM event names]], [[botster wire v2 clients must consume ui tree snapshots and render composites with entity stores]], [[botster-core local process runtime is feature-gated from contract-only embeds]].
- Identity/current goals: [[identity]], [[goals]].
- TUI skill context: `botster-customize-tui` skill, especially shared UI contract forms and renderer-neutral action guidance.
- Repo evidence: `crates/botster-tui` is a scaffold with a placeholder draw path; `crates/botster-tui-spike` contains a prior ratatui/crossterm proof with local UiNode-like fixtures, hit maps, semantic action envelopes, terminal forwarding, and redraw tests.
- Dependency evidence: `gh repo view trybotster/botster-core --json nameWithOwner,defaultBranchRef,url` confirms `trybotster/botster-core` exists with default branch `main`.
- Checklist evidence: run checklist `checklist_1780954148_993742` records vault notes read, convention checks, planned verification commands, and capture decision.

## Scope

Implement the first production `botster-tui` renderer registry for Botster UiNode v1 by consuming the real shared contract from `botster-core`, not by inventing local UiNode types.

Planned work:

- Add a `botster-core` dependency to `crates/botster-tui/Cargo.toml` using the `trybotster/botster-core` git repository with `default-features = false`, so the TUI consumes UI/entity/transport/action contract surfaces and core renderer conformance fixtures without pulling in the local PTY/process runtime.
- Use real `botster-core` UiNode/action/capability/conformance fixture types in renderer code and tests. Local fixture helpers may supplement coverage, but they must not substitute for the core conformance fixture set.
- Add renderer modules under `crates/botster-tui/src/` for primitive registry/dispatch, ratatui rendering, hit-map/action metadata, fixture/conformance test harness, and focused form/fallback handling.
- Implement shared contract primitive names, not local aliases: `stack`, `inline`, `panel`, `scroll_area`, `text`, `badge`, `status_dot`, `empty_state`, `list`, `list_item`, `button`, `dialog`, `text_input`, `textarea`, `checkbox`, `select`, plus unsupported fallback.
- Handle the ticket's `badge/status` wording as the contract's two primitives: `badge` and `status_dot`.
- Treat `textarea` as in scope for form v1 with the documented TUI behavior: display/read-only until multiline editing exists.
- Resolve `table-lite` against the actual core conformance fixtures before implementation. If the core fixture set exposes a table/table-lite primitive, implement that exact shape and name. If it is not in the core contract, ask a human before either dropping it from the ticket scope or adding a non-contract primitive.
- Preserve `ActionBindingV1` semantic action envelope shape and test semantic action ids such as `botster.session.select`, not renderer event names like click/submit/change.
- Port the still-useful spike coverage into the production renderer tests: hit-map lookup, semantic action envelope tests, terminal/input separation if still relevant to shared fixtures, and redraw budgeting if retained by the renderer. Delete `crates/botster-tui-spike` in this ticket after porting that coverage, unless Implement discovers a concrete reason to keep it and records a removal owner.
- Wire the production `crates/botster-tui/src/app.rs` draw path through the new renderer using a core conformance fixture or a documented core-derived sample until hub/core runtime input exists in this repo. This is the runtime/user-path proof for the current scaffold.
- Keep the existing `--smoke` path working.
- Update README and ADR wording where necessary so they no longer describe UiNode primitives as "not included yet" or the renderer as scaffold-only once `app.rs` renders through the registry.

## Non-Scope

- Do not implement hub connection, pairing, auth, Unix socket attach, SessionIo/ClientWorker terminal subscriptions, or entity-store hydration.
- Do not change Lua plugin policy, Project Pipelines workflow policy, browser SPA code, Rails relay code, or MCP tools.
- Do not add product-specific Project Pipelines screens, workflow-specific primitives, or operator-workbench behavior.
- Do not add speculative configurability or a second renderer abstraction. A small registry/dispatch table over the shared contract is enough.
- Do not invent local UiNode/action/capability types as the primary renderer contract.
- Do not add table/grid primitives unless the `botster-core` conformance fixtures prove they are part of this ticket's core contract.

## Assumptions And Unknowns

- Assumption: the current worker is in the assigned worktree for target `tgt_c3d470bab78549df920a41e8fb0e58d8`.
- Assumption: the implementation can reference `botster-core` from `https://github.com/trybotster/botster-core` or equivalent git URL and should set `default-features = false` for contract-only consumption.
- Assumption: `default-features = false` keeps the UI/entity/action/capability/fixture contract available while excluding local PTY/process runtime dependencies, per [[botster-core local process runtime is feature-gated from contract-only embeds]].
- Unknown: exact module paths and public names for UiNode v1, `ActionBindingV1`, `UiCapabilitySetV1`, and renderer conformance fixtures inside `botster-core`. Implement must inspect the dependency and use the public API rather than duplicating it.
- Unknown: whether `table-lite` exists in the landed core conformance fixture set. This must be resolved from `botster-core`; a human question is required if the fixture source contradicts the ticket wording.
- Unknown: whether all core fixtures are static enough to render without entity stores. If a conformance fixture uses bound lists/templates, tests must thread the required entity-store data through the renderer or explicitly document the scaffold limitation and follow-up.

## Affected Surfaces And Files

Expected files/surfaces:

- `crates/botster-tui/Cargo.toml`: add `botster-core` git dependency with `default-features = false`; add only minimal supporting dependencies if the real fixture harness requires them.
- Root `Cargo.toml` and `Cargo.lock`: update workspace membership if deleting the spike crate and lock dependency resolution.
- `crates/botster-tui/src/app.rs`: replace placeholder body with renderer-backed fixture rendering while preserving terminal setup, exit keys, and smoke path.
- `crates/botster-tui/src/main.rs`: likely unchanged except module exposure if needed.
- New `crates/botster-tui/src/renderer.rs` or `crates/botster-tui/src/renderer/mod.rs`: registry and primitive dispatch.
- New focused modules if useful: `hit_map.rs`, `actions.rs`, `fixtures.rs`, `forms.rs`, `fallback.rs`. Keep names small and aligned with code shape.
- `crates/botster-tui-spike/` and its workspace entry: port useful tests into `botster-tui`, then delete the spike crate to avoid dual UiNode renderers.
- `README.md`: update included/not-included scope and commands.
- `docs/adr/0001-ratatui-crossterm-tui-renderer-foundation.md`: update scaffold-only wording if stale after wiring production draw through the renderer.
- `docs/plans/tui-uinode-renderer-primitives-plan.md`: this plan artifact.

Botster layers touched:

- TUI/client renderer and docs only.
- No plugin, Lua core, Rust hub runtime policy, session/client worker, React SPA, Rails relay, MCP, or production transport changes.

Pipeline gates/artifacts:

- Plan gate: this artifact plus structured gate evidence.
- Implement gate should require committed renderer code, `botster-core` contract-only dependency, core conformance fixture test evidence, spike removal or explicit retained-spike justification, README/ADR updates if touched, and command evidence.
- Review/Verify must check the runtime path: `botster-tui` itself must render through the new registry, not only tests or the old spike crate.

## Risks

- Cross-repo dependency risk: `botster-core` may not be referenceable from this standalone `botster-tui` repo due to visibility, feature, or Cargo graph constraints. This is a planning-level blocker if it happens; ask a human before falling back to local types.
- Feature-gating risk: using default features could pull in local runtime or portable-pty surfaces. Mitigate by requiring `default-features = false` and verifying the dependency compiles in the TUI workspace.
- Core fixture shape risk: table-lite and some primitive names may differ from ticket wording. Mitigate by treating the core conformance fixture set as authoritative and asking a human if it conflicts with the ticket.
- Entity-store binding risk: wire v2 separates `ui_tree_snapshot` structure from entity stores; bound list/composite fixtures can render empty if stores are not threaded into the renderer. Mitigate by testing static fixtures directly and documenting the future entity-store injection point, or adding fixture store data if the core conformance set requires it.
- Unwired implementation risk: adding renderer modules without changing `app.rs` would satisfy code-shape evidence but not the user path. Mitigate by rendering a core fixture through the production draw path.
- Dual-renderer drift risk: leaving `botster-tui-spike` with local UiNode definitions beside production code invites vocabulary drift. Mitigate by porting coverage and deleting the spike crate.
- Action contract drift risk: terminal/UI event names can become action ids. Mitigate by asserting `ActionBindingV1` semantic ids and envelope shape.
- Dependency sprawl risk: fixture parsing might tempt extra crates. Use `botster-core`, ratatui, crossterm, and minimal serde-only parsing only if required by the fixture API.

## Acceptance Checks And Tests

Minimum checks for Implement/Verify:

- `script/fmt`
- `script/test`
- `script/clippy`
- `cargo run -p botster-tui -- --smoke`

Required test coverage:

- The actual `botster-core` renderer conformance fixture set renders through `botster-tui` renderer tests. Local fixtures may only supplement this.
- An adapter-mapping test asserts shared primitive names map to the TUI renderer registry and existing ratatui/render-tree shapes: `stack` to horizontal/vertical split behavior, `panel` to block framing, `list/list_item` to list rows, `text` to paragraph content, `text_input` to input rendering, and every implemented primitive name to the shared contract vocabulary.
- Tests cover `badge` and `status_dot` separately.
- Tests cover `text_input`, read-only/display `textarea`, `select`, `checkbox`, field values, and field errors.
- Tests cover `button`/action row behavior and assert `ActionBindingV1` semantic action ids/envelope shape rather than renderer event names.
- Tests cover `dialog`, `empty_state`, `scroll_area` if present in the fixtures, and unsupported primitive fallback.
- If `table-lite` exists in the core conformance fixture set, tests cover its exact core shape. If it does not, the implementation must include a human answer or review-visible decision reconciling ticket wording with the shared v1 contract.
- Useful spike behavior is ported into production tests: hit-map stable node lookup and semantic action envelope coverage at minimum; redraw budgeting only if still part of the production renderer.
- `crates/botster-tui/src/app.rs` draw path uses the renderer registry with a core fixture or core-derived sample, proving the runtime path changed in the scaffold.
- `crates/botster-tui-spike` is removed after coverage is ported, or retained only with explicit review-visible justification and a removal owner.

## Vault Gaps Worth Capturing

- Capture the exact `botster-core` fixture module path and downstream renderer consumption pattern after implementation verifies it.
- Capture any discovered core primitive to TUI registry mapping rule, especially if `table-lite` differs from the shared v1 inventory.
- Capture a follow-up note if `botster-core` contract-only consumption needs additional feature gates or exposes missing fixture APIs.
- Capture durable TUI renderer foundation knowledge under [[cli-patterns]] only after the production scaffold renders through the registry.
