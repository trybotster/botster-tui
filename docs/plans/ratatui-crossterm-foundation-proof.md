# Ratatui + Crossterm Foundation Proof Plan

Ticket: Prove ratatui plus crossterm as botster-tui foundation
Run: run_1780941265_930078
Step: botster_plan

## Context Loaded

- Pipeline context from `project_pipelines_current_context`: ticket, run, active Plan step, gate prompt, no prior artifacts, no open findings, no questions or answers.
- Role playbooks: [[planner-playbook]], [[botster-planner-playbook]].
- Required Botster vault context: [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]].
- Identity/current goals: [[identity]], [[goals]].
- Repo evidence: this worktree is currently an initial commit with no `botster-tui` crate, no Rust source tree, and no docs tree before this plan artifact. `rg --files` only reported `mise.local.toml`; `git log` showed `Initial commit`.
- Checklist evidence: run checklist `checklist_1780942074_677112` records loaded context, convention check, expected verification, and vault capture decision.

## Scope

Implement an implementation-oriented architecture spike that proves whether ratatui plus crossterm is a suitable foundation for Botster's shared `UiNode` TUI renderer.

Planned deliverables:

- Add an ADR under `docs/adr/` that chooses the foundation or records a blocker. The ADR must include a brief comparison against OpenTUI and Bubble Tea, biased toward Rust-native ratatui/crossterm unless the proof reveals a concrete blocker.
- Add a minimal Rust prototype or testable harness in this repo. Because the current worktree has no existing crate, the smallest acceptable shape is a new minimal Cargo workspace or crate dedicated to the spike, not a broad Botster runtime import.
- Exercise split panes, list, form, and dialog rendering through semantic `UiNode`-like fixtures.
- Capture crossterm mouse hover, click, and scroll events and translate them into renderer-local hit tests.
- Record a stable-node hit map keyed by semantic node ids rather than screen-only coordinates.
- Emit semantic action requests such as selection, button activation, field focus, field edit, dialog dismiss, and terminal focus/input forwarding.
- Model `terminal_view` focus/input forwarding separately from ordinary widget actions so nested terminal/TUI passthrough remains explicit.
- Prove frequent-output redraw behavior with a deterministic harness, not only a static screenshot or code assertion.
- Document implementation shape for renderer-local hover/click/focus state versus cross-process semantic `UiAction` requests.

## Non-Scope

- Do not implement the full production Botster shared renderer.
- Do not change hub, SessionIo, ClientWorker, Rails relay, browser SPA, plugin runtime, or Project Pipelines product policy.
- Do not add plugin workflow primitives or pipeline UI workbench changes.
- Do not introduce a long-lived abstraction layer beyond what the spike needs to express the renderer boundary.
- Do not vendor OpenTUI or Bubble Tea prototypes unless ratatui/crossterm evidence exposes a real blocker that needs direct comparison.
- Do not make terminal output authoritative in the TUI renderer; Botster terminal truth remains in the session/backend data plane.

## Assumptions And Unknowns

- Assumption: the empty repo is intentional for this ticket's proof work, so the implementer may scaffold a minimal spike crate and ADR here.
- Assumption: no production entry point can be wired in this worktree until a real `botster-tui` codebase exists. The implementation must therefore state whether the proof is intentionally scaffold-only.
- Assumption: dependency versions must be looked up at implementation time before adding them; do not rely on remembered ratatui/crossterm versions.
- Assumption: semantic action names can be local spike vocabulary if the repo has no existing `UiAction` contract, but the ADR must map them back to Botster's cross-client semantic-action direction.
- Unknown: whether the eventual production renderer will consume an existing `UiNodeV1` Rust type from another Botster repo or a future extracted contract crate.
- Unknown: whether crossterm mouse reporting is sufficient for all nested terminal modes Botster cares about, especially scroll/focus passthrough with rich nested TUIs.
- Unknown: whether the test harness should be a binary demo, integration test, snapshot test, or a combination. The plan accepts any shape that is runnable or testable and proves the ticket's runtime paths.

## Affected Surfaces And Files

Expected files/surfaces:

- `docs/plans/ratatui-crossterm-foundation-proof.md`: this Plan artifact.
- `docs/adr/NNNN-ratatui-crossterm-tui-renderer-foundation.md`: ADR recording choice, alternatives, risks, and implementation shape.
- `Cargo.toml` and, if needed, a minimal crate such as `crates/botster-tui-spike/`.
- Rust prototype/test files under that crate, likely covering:
  - semantic node fixtures,
  - ratatui layout/render adapter,
  - crossterm event translation,
  - stable hit-map recording,
  - semantic action emission,
  - terminal-view focus/input forwarding model,
  - frequent-output redraw harness.

Botster layers touched:

- TUI/client renderer spike only.
- Docs/ADR.
- No plugin, Lua core, Rust hub, session/client worker, React SPA, Rails relay, MCP, or production transport changes expected.

Worktree/target assumptions:

- Current run target id: `tgt_c3d470bab78549df920a41e8fb0e58d8`.
- Current worker is already in its assigned worktree.
- No ambient base checkout path should be treated as authoritative.

Pipeline gates/artifacts:

- Plan gate: this artifact plus submitted structured gate evidence.
- Implement gate should require committed spike work, ADR, and runnable/testable command evidence.
- Review/Verify should reject an unwired claim unless the implementation explicitly documents scaffold-only proof due to the empty initial repo.

Required docs:

- ADR is required by ticket acceptance.
- A short README or module doc for running the harness is acceptable if the command is not obvious from `cargo test` or `cargo run`.

## Risks

- Empty repo risk: an implementer may accidentally produce a toy unrelated to Botster. Mitigation: ADR and test fixtures must use Botster-shaped concepts: `UiNode`, stable node ids, semantic `UiAction`, `terminal_view`, and renderer-local presentation state.
- Dependency risk: ratatui/crossterm APIs may have changed. Mitigation: look up current crate versions before adding dependencies and record versions in the ADR or lockfile.
- Over-scope risk: proving the foundation can drift into building the renderer. Mitigation: keep production integration out of scope unless actual production files appear.
- False-positive proof risk: static rendering proves little about Botster's runtime needs. Mitigation: tests/harness must exercise input event translation, hit maps, semantic actions, focus forwarding, and redraw under frequent output.
- Terminal passthrough risk: Botster has known nested TUI input pitfalls. Mitigation: model `terminal_view` focus and input forwarding as a first-class part of the spike instead of treating it as ordinary widget clicks.
- Cross-process boundary risk: hover/focus state can leak into durable semantic state. Mitigation: ADR must separate renderer-local hover/click/focus state from cross-process semantic action requests.

## Acceptance Checks And Tests

Minimum acceptance checks for the Implement step:

- `cargo test` for the spike crate or workspace passes.
- A runnable demo/harness command exists and is documented if test output alone does not prove the interactive model.
- Tests or harness output prove split panes plus list/form/dialog rendering.
- Tests or harness output prove mouse hover/click/scroll capture through crossterm-style events.
- Tests prove hit-map entries are stable-node-id based and map screen coordinates back to semantic nodes.
- Tests prove semantic action emission for at least list selection, button/dialog action, form focus/edit, and terminal focus/input forwarding.
- A redraw test or harness proves frequent output can trigger bounded redraw behavior without relying on a constant tick loop.
- ADR includes ratatui/crossterm decision, OpenTUI comparison, Bubble Tea comparison, risks, unresolved questions, and implementation shape for local renderer state versus cross-process semantic `UiAction`.
- Verification evidence must state whether the runtime path is scaffold-only because this initial repo has no production `botster-tui` entry point.

## Vault Gaps Worth Capturing

- If the empty worktree is intentional for architecture spikes, capture a Project Pipelines note that Plan agents should call out scaffold-only proof explicitly rather than treating missing production code as implicit acceptance.
- If the spike supports ratatui/crossterm, capture a durable note after implementation, likely under `cli-patterns`, for the Botster TUI renderer foundation decision.
- If crossterm mouse/focus handling exposes a blocker for nested terminal passthrough, capture it alongside [[nested rich tuis lose scrolling when botster consumes mouse reports or control keys]] and [[terminal accessory reattach must restore nested tui input passthrough]].

