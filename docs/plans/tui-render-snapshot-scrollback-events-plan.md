# TUI Snapshot And Scrollback Event Rendering Plan

## Context Loaded

- Pipeline context: ticket `ticket_1782241198_649017`, run `run_1782241216_614144`, step `botster_plan`, gate `botster_plan_gate`, target `tgt_c3d470bab78549df920a41e8fb0e58d8`.
- Repo context: branch `project-pipelines/ticket_1782241198_649017` at `50bb817`; worktree was clean before this plan artifact was added.
- Ticket context: botster-tui must render `DaemonEvent::Snapshot` and `DaemonEvent::Scrollback` from the public `botster-hub-client` event path, append renderable `data` in attach/drain order before later `TerminalOutput`, and avoid hub/core changes or private protocol plumbing.
- Current code context: `DogfoodApp::apply_response` in `crates/botster-tui/src/app.rs` appends `DaemonEvent::TerminalOutput { data, .. }`, handles `ProcessExit` and `AttachState`, and ignores all other daemon events. The terminal renderer already consumes `DogfoodApp.terminal_output` through the `TerminalView` primitive.
- Dependency context: current `crates/botster-tui/Cargo.toml` pins `botster-hub-client` and `botster-hub-test-support` to `ae73a41b36c7ea9ba4d5bbb5bacf0d0ebcb452a5`. That cached revision exposes `Snapshot` and `Scrollback` with byte counts only. `trybotster/botster-hub` main was checked at `1f4c6e9b8d0deef5ed101a99e644d2bd2e9dd0cf`, where daemon projection includes renderable `data` for both history event variants.
- Required vault/playbook context: [[identity]], [[goals]], [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], [[plan agents must author vault context as wikilinks not home paths]].

## Scope

- Work only in `botster-tui`.
- Update `botster-hub-client` and `botster-hub-test-support` pins only if needed to compile against the public `Snapshot`/`Scrollback { data, bytes, .. }` DTOs. The expected target is at or after `1f4c6e9b8d0deef5ed101a99e644d2bd2e9dd0cf`.
- In `DogfoodApp::apply_response`, handle `DaemonEvent::Snapshot` and `DaemonEvent::Scrollback` beside `TerminalOutput`.
- Append non-empty renderable `data` from history events to the same `terminal_output` buffer used by live output, preserving the order events arrive in each response.
- Extract the existing terminal-output append and trim behavior into a small private helper if needed so `TerminalOutput`, `Snapshot`, and `Scrollback` share the same 8,000-character cap.
- For empty history `data`, preserve current terminal output and optionally set a non-fatal `action_feedback` message using the existing status pattern, including byte metadata only as diagnostics text. Do not treat empty history as an error.
- Add focused TUI tests proving history events render through the same terminal primitive and appear before later live output.

## Non-Scope

- No edits to `botster-hub`, `botster-core`, web, Rails, Project Pipelines plugin, or private daemon/session-worker protocol code.
- No local history cache, scrollback cache, terminal parser, or alternate data-plane path in the TUI.
- No hand-rolled socket frames, private protocol constants, or mirrored DTOs.
- No renderer redesign; the current `TerminalView` surface and `terminal_output` string remain the rendering path.
- No broad dependency updates beyond the hub-client/test-support pin and lockfile changes required by this ticket.
- No PII or local absolute path references in committed artifacts.

## Assumptions And Unknowns

- Assumption: the authoritative event shape is the public `botster-hub-client` `DaemonEvent` enum, not the older cached daemon transport shape.
- Assumption: append order is the order of `response.events`; no extra sorting or cursor reconciliation belongs in the TUI.
- Assumption: `Snapshot`/`Scrollback` data is already renderable UTF-8 text from hub-client projection, so TUI should append `data` directly and not parse `bytes`.
- Unknown: the exact dependency bump may bring unrelated public DTO additions already present on hub main. Implementation should keep compile fixes limited to TUI fixture construction and the public client boundary.
- Unknown: live attach/drain may not deterministically emit `Snapshot`/`Scrollback` variants because [[daemon attach drain cannot force snapshot or scrollback variants]] says public attach/drain may replay history as `TerminalOutput`. Therefore fixture coverage of the DTO projection is required, while live dogfood remains a boundary/regression check.
- Decision: no human question is needed. The ticket explicitly says renderable `data` is required; the old byte-count-only pin is stale rather than a competing interpretation.

## Botster Layers Touched

- TUI client only.
- Public dependency boundary: `botster-hub-client` consumed as the external client protocol crate.
- Test harness: Rust unit tests and existing live isolated hub dogfood script.

## Worktree And Target Assumptions

- The current pipeline run is already bound to target `tgt_c3d470bab78549df920a41e8fb0e58d8` and the ticket branch worktree.
- Implementer and reviewer should continue using this assigned worktree/branch, not an ambient checkout.

## Affected Surfaces/Files

- `crates/botster-tui/src/app.rs`
  - Add a small append helper or inline history handling in `apply_response`.
  - Add/adjust fixture helpers for `Snapshot` and `Scrollback` event responses.
  - Add tests for history ordering, empty-data fallback, trim preservation, and terminal primitive rendering.
- `crates/botster-tui/Cargo.toml`
  - Likely bump `botster-hub-client` and `botster-hub-test-support` revisions to a public-client revision exposing history `data`.
- `Cargo.lock`
  - Update only as required by the dependency pin change.
- No README/docs update is required unless implementation finds user-facing dogfood behavior changed enough to document.

## Risks

- Stale dependency risk: implementing against the current pin cannot satisfy the ticket because `Snapshot`/`Scrollback` lack `data`. Mitigation: bump to a hub-client revision that exposes renderable data before coding.
- Boundary regression risk: a tempting workaround would be to inspect private frames or byte metadata. Mitigation: keep the existing boundary test and add no local protocol structs/constants.
- Ordering regression risk: handling history in a separate branch or buffer could reorder late-subscriber history after live output. Mitigation: process variants in the existing `for event in response.events` loop and append immediately.
- Buffer trim risk: duplicating the existing trim logic could diverge by variant. Mitigation: share the append/trim helper.
- Empty-history noise risk: byte-only or empty-data events should not become fatal errors. Mitigation: use existing `action_feedback` style only when a clear non-fatal status is useful.
- Live-test trigger risk: isolated hub dogfood may not force Snapshot/Scrollback variants. Mitigation: require direct event-fixture tests for DTO handling and keep live dogfood for public-client runtime path health.

## Acceptance Checks/Tests

- `script/fmt`
- `script/test`
- `script/clippy`
- `script/test-live-hub`
- Focused tests in `crates/botster-tui/src/app.rs`:
  - `snapshot_and_scrollback_events_append_before_later_terminal_output`: build a `DaemonResponse` with `Snapshot`, `Scrollback`, and `TerminalOutput` in that order; assert `terminal_output` and rendered terminal content preserve that order.
  - `empty_history_event_data_is_non_fatal`: send a `Snapshot` or `Scrollback` with empty `data` and non-zero `bytes`; assert no error is set and existing terminal output is preserved, with any status feedback matching the existing action feedback pattern.
  - `history_events_use_same_terminal_output_cap`: seed near the 8,000-character cap, append history data, and assert trimming matches live `TerminalOutput` behavior.
  - Existing `terminal_output_renders_as_terminal_primitive_content` or a nearby assertion proves history-appended output still renders through `dogfood-terminal`, not the old text child hit target.
  - Existing `tui_hub_boundary_uses_public_client_without_private_protocol_plumbing` continues to pass and should be extended only if new forbidden private-protocol patterns appear.

## Runtime Path Proof

The production path to prove is:

`run_loop` -> `DogfoodApp::poll_hub` / `request_and_apply` -> public `HubConnection::request` using `botster_hub_client::{write_frame, read_frame_from_reader}` -> `DogfoodApp::apply_response` -> `terminal_output` -> `DogfoodApp::surface` -> `renderer::render_node` for `UiNodeKind::TerminalView`.

The implementation must show `Snapshot` and `Scrollback` enter at the same `apply_response` event loop as `TerminalOutput`; tests that mutate `terminal_output` directly are not sufficient for the ticket.

## Pipeline Gates And Artifacts

- Plan artifact: `docs/plans/tui-render-snapshot-scrollback-events-plan.md`.
- Plan gate evidence should attach this plan and the context/scope/assumption/risk/test details above.
- Project Pipelines checklist evidence:
  - Context notes loaded: listed in "Context Loaded".
  - Convention conflicts: none. The plan follows TUI-only scope, public hub-client boundary, path-neutral vault references, explicit target/worktree assumptions, and existing Rust test scripts.
  - Verification evidence: planning commands inspected the pipeline context, required vault notes, `crates/botster-tui/src/app.rs`, dependency pins, script test harnesses, current branch, and upstream hub main.
  - Durable capture: no new vault note is needed from planning alone; the stale-pin observation is already covered by existing public-client-boundary/history-data notes unless implementation finds a repeatable new gotcha.

## Vault Gaps Worth Capturing

- No durable vault gap is mandatory from the planning pass.
- Capture candidate only if implementation confirms recurrence beyond this ticket: "botster-tui history-event work must check hub-client pin freshness before judging missing DTO fields."
