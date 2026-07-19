# TUI Kit Activate-On-Release Mouse Consumption Plan

Ticket: Adapt botster-tui to kit activate-on-release mouse

## Context Loaded

- Pipeline context: run `run_1784390013_966244`, Plan step `botster_plan`, and gate
  `botster_plan_gate`. The registered kit dependency ticket is closed. There are
  no questions or question answers. Plan Review
  `review_1784390555_210819` returned changes required on verification strength:
  exercise a fresh hit map between Down and Up, exercise TerminalView's complete
  Down+Up stream, and stop treating the static `--smoke` output as mouse runtime
  evidence.
- Planning authority: [[planner-playbook]], [[botster-planner-playbook]],
  [[botster-architecture]], [[cli-patterns]], and [[spa-patterns]], plus the
  Botster planning notes required by the overlay. The directly constraining TUI
  notes are [[cross-client ui should share semantic primitives and actions with renderer-specific adapters]],
  [[botster tui uinode event routing captures hit regions during draw]],
  [[focused mouse mode terminal passthrough needs complete sgr reports]], and
  [[TUI text selection must lock to origin widget during drag-select]].
- Repository context: this is a clean, isolated ticket worktree on
  `project-pipelines/ticket_1784307273_949764`. `crates/botster-tui/src/renderer.rs`
  is a thin re-export of `botster-tui-kit`; it does not own input routing.
  Production input follows `run_loop` -> `crossterm::event::read` ->
  `InputRouter::dispatch_event` -> `DogfoodApp::handle_dispatch`.
- Dependency context: the current kit pin `8b3ea35...` is the PR #10 merge.
  Activate-on-release landed in kit PR #12 at merge commit
  `fa0a728297c51d659b675f7d4201a3910a25bd82`. That revision uses the same
  `botster-core` revision already pinned by this repository, so no core or hub
  dependency movement is expected.
- Upstream contract: semantic Action, Field, and ListItem regions focus/capture
  on left Down and activate only on matching-node left Up. TerminalView is the
  deliberate exception: its focus/attach action remains left-Down-driven.
  At the exact PR #12 pin, a focused mouse-mode terminal forwards supported
  raw reports but ignores Up; complete SGR release forwarding belongs to the
  later PR #13 consumer update.
- Audit result: all four synthetic `MouseEventKind::Down` sites are in
  `crates/botster-tui/src/app.rs`. Three exercise semantic list/table behavior
  and must become Down+Up streams. One exercises TerminalView down-focus and
  should remain a Down-only contract proof.

## Scope

- Pin `botster-tui-kit` to exact merged PR #12 revision
  `fa0a728297c51d659b675f7d4201a3910a25bd82`, then refresh only the resulting
  lockfile entries.
- Update the README's documented kit revision and mouse-routing description so
  it matches the consumed contract.
- Update the three semantic-control synthetic mouse paths in `app.rs` to route
  Down followed by same-coordinate Up through the real kit `InputRouter`.
- Strengthen those tests so Down alone is observably non-activating where an
  action is expected, Up produces the one semantic action/selection mutation,
  and subsequent keyboard behavior still starts from the selected/focused row.
- Add a production-shaped redraw test: render frame N's hit map, dispatch Down,
  render frame N+1's hit map from the same stable app state, then dispatch Up
  against frame N+1 and prove the action still fires by stable `node_id`. Pin
  the complementary contract that a row reorder under the same coordinates
  cancels rather than activates the newly moved row.
- Preserve and strengthen TerminalView's intentional Down-driven attach: assert
  left Down focuses and emits `botster.terminal.focus`, feed it through
  `DogfoodApp::handle_dispatch`, then send the matching Up and prove the pair
  causes exactly one focus action and one attach. Add a test-local mouse-mode
  assertion that Up is `Ignored`, not a second focus action; complete release
  forwarding remains owned by the separate terminal-mouse consumer work.
- Run the repository's Rust checks and binary link/build smoke. Inspect the
  dependency diff to ensure only the intended kit revision moved. Runtime mouse
  evidence comes from production-shaped rendered-surface tests and the
  `run_loop` entry-point trace, not from `--smoke`.

Botster layer touched: Rust TUI client and its shared TUI-kit dependency
boundary. No plugin, Lua core, hub, session/client worker, SPA, Rails relay, MCP,
or plugin README behavior changes.

## Non-Scope

- Do not implement pointer capture, hit testing, or activate-on-release locally;
  those remain kit-owned.
- Do not add an activate-on-Down compatibility shim or a dual routing path.
- Do not change TerminalView raw mouse encoding, session transport, attach
  policy, or nested-TUI passthrough.
- Do not consume or lock mouse-mode SGR release bytes in this ticket; kit PR #13
  owns the later terminal-mouse encoding change. This consumer test asserts
  `Ignored` and absence of duplicate focus only.
- Do not implement text selection, drag thresholds, pointer-origin policy, or
  product-level selection behavior.
- Do not update to later kit main merely because it is newer; revisions after
  PR #12 contain unrelated wheel/trackpad and terminal-mouse work.
- Do not refactor unrelated app tests, renderer structure, or hub interaction
  code. A tiny test-only click helper is acceptable only if it directly replaces
  all repeated semantic Down+Up construction without obscuring per-event
  assertions.

## Assumptions And Unknowns

- Assumption: exact kit merge commit `fa0a728...` is the smallest authoritative
  dependency coordinate satisfying the closed dependency and the ticket's
  "capture PR rev or merged main" instruction.
- Assumption: real terminals emit both Down and Up under the already-enabled
  crossterm mouse capture mode. Production `run_loop` passes both events
  unchanged to the kit router, so no production app-code adaptation is needed.
- Assumption: stable hit-region `node_id` values in unchanged rendered surfaces
  satisfy the kit's same-node release identity rule. Implementation must prove
  that assumption across two independently rendered hit maps, matching the
  production frame boundary.
- Assumption: if hub polling reorders a session row beneath the pointer between
  Down and Up, canceling the activation is correct capture behavior; the newly
  moved row must never receive the old press. A negative redraw test records
  this intentional outcome.
- Assumption: TerminalView left Down must continue to dispatch
  `botster.terminal.focus`; changing it to release activation would violate the
  upstream exception and the ticket's attach/focus requirement.
- Unknown until the pin is resolved: the exact narrow `Cargo.lock` checksum/source
  diff. It must be inspected rather than accepted as generic dependency churn.
- Unknown until tests run against the new pin: whether any Down-only expectation
  is hidden behind a test name or helper that does not contain
  `MouseEventKind::Down`. A final repository-wide audit after resolution must
  confirm none remain outside the explicit TerminalView proof.
- Known follow-up: this exact pin intentionally trails kit main by PR #13 and
  PR #14, which belong to separate terminal-mouse and wheel/trackpad consumer
  work. Mouse-mode Up remains `Ignored` until that later consumer pin bump.
  Implement evidence must state that the lag is deliberate.
- Human answer `question_1784417476_725517` resolved a discovered plan conflict:
  keep exact PR #12, revise the test-local mouse-mode Up expectation to
  `Ignored`, and carry complete Up forwarding as residual risk for the later
  terminal-mouse consumer ticket.
- No further human question is required after
  `question_1784417476_725517`: its answer preserves the exact upstream pin
  while resolving the mouse-mode release expectation.

## Affected Surfaces And Files

- `crates/botster-tui/Cargo.toml`
  - Move only the `botster-tui-kit` git revision to `fa0a728...`.
- `Cargo.lock`
  - Resolve the new kit source revision; reject unrelated package movement.
- `crates/botster-tui/src/app.rs`
  - Convert composite-table selection/action, focused-session-list setup, and
    repeated exited-session mouse activation to Down+Up streams.
  - Exercise Down and Up against independently rendered hit maps, including
    stable-node activation and reordered-row cancellation.
  - Keep TerminalView attach on left Down, but send the complete pair and assert
    exactly one focus/attach; add a test-local mouse-mode Up `Ignored`
    assertion.
- `README.md`
  - Replace the stale kit pin and state semantic activate-on-release plus the
    TerminalView down-focus exception.
- `docs/plans/tui-kit-activate-on-release-mouse-consumption-plan.md`
  - Durable Plan artifact only; no runtime behavior.
- `crates/botster-tui/src/renderer.rs`
  - Expected unchanged. It should remain a thin kit adapter.

## Implementation Sequence

1. Update the exact kit pin and resolve `Cargo.lock`; inspect the lock diff before
   changing tests so dependency-caused failures are attributable.
2. Run the focused `app.rs` mouse tests against the new contract and record the
   expected Down-only failures.
3. Change only the three semantic-control streams to Down+Up. Assert intermediate
   Down results where needed so the tests would fail against activate-on-Down.
   In at least one action test, regenerate the rendered hit map between events
   and dispatch Up through the second map. Add the row-reorder negative case so
   the same coordinates cannot activate a different stable node.
4. Strengthen TerminalView coverage without converting attach to release:
   prove Down focuses, emits `botster.terminal.focus`, and reaches the existing
   `DogfoodApp` attach behavior; then dispatch Up and prove the pair produces no
   duplicate focus or attach. With a rendered `mouse_mode: true` TerminalView,
   assert Up is `Ignored` rather than a second focus action. Leave complete
   release forwarding and exact byte encoding to their owning kit consumer
   ticket. Retain terminal key-forwarding evidence.
5. Update the README pin/contract wording.
6. Run focused, full, lint, formatting, link/build smoke, and diff checks.
   Repeat the repository-wide synthetic mouse audit and confirm every semantic
   activation uses a complete pair. TerminalView tests may assert the Down
   result separately, but must also exercise the trailing Up.

## Risks

- A mechanical Down-to-Down+Up rewrite could break TerminalView's intentional
  focus/attach boundary. Mitigation: classify every synthetic site by hit role
  and retain an explicit Down-only terminal assertion.
- Tests could send Up but only assert the final result, allowing an accidental
  action on Down plus a duplicate action on Up. Mitigation: assert the Down
  dispatch is non-Action and state has not yet mutated for at least the direct
  semantic action/selection paths.
- Dropping the Down dispatch before Up can hide focus behavior needed by
  `DogfoodApp::sync_focused_session`. Mitigation: dispatch both events in order
  through the same router and current hit map, then assert selection/focus state.
- A release sent with different coordinates or a redrawn hit map will correctly
  cancel under kit capture semantics. Mitigation: prove stable-node activation
  across independently rendered maps and explicitly prove cancellation when a
  different row moves beneath the same coordinates.
- Production redraws and polls the hub between individual input events. A test
  that reuses one hit map would miss frame-boundary identity regressions.
  Mitigation: make a fresh-hit-map Down/frame/Up test a required acceptance
  check.
- Updating to current kit main would pull later unrelated behavior and widen
  regression scope. Mitigation: pin the exact PR #12 merge commit.
- Lockfile resolution can move transitive git packages unexpectedly. Mitigation:
  the upstream kit revision is verified to use this repo's existing core pin;
  inspect `git diff Cargo.lock` and reject unexplained movement.
- Static test changes alone would not prove the user path. Mitigation: trace and
  preserve production raw crossterm routing, exercise rendered HitMap ->
  InputRouter -> DogfoodApp handling across two rendered frames, and treat the
  production binary smoke only as a link/build check.

## Acceptance Checks And Tests

- Focused Rust tests in `app.rs` prove:
  - composite table left Down emits no `InputDispatch::Action` and does not
    select/activate the row;
  - matching left Up emits exactly the expected row action and payload, with
    selected-row state updated;
  - Down against frame N's hit map plus Up against a separately rendered frame
    N+1 hit map activates when the stable node remains under the pointer;
  - if a session-row reorder puts a different node under the same coordinates
    between frames, Up is ignored and neither row receives the old press;
  - session-list Down+Up selects/focuses the intended row before keyboard Down
    advances to the next row;
  - repeated exited-session attempts use a complete mouse click plus Enter,
    make no daemon attach request, and render one deduplicated error;
  - TerminalView left Down still focuses and emits
    `botster.terminal.focus`, that dispatch reaches the app's attach path, and
    the matching Up produces no second focus or attach;
  - a test-local rendered mouse-mode TerminalView routes the trailing Up as
    `Ignored`, not `Action`; complete SGR release forwarding remains deferred;
  - focused terminal key input still becomes `TerminalForward`.
- Repository audit:
  - `rg -n "MouseEventKind::Down|MouseEventKind::Up|InputDispatch::Action" crates`
    shows complete semantic Down+Up streams and only the intentional terminal
    Down-only proof;
  - no local activate-on-Down compatibility branch exists in app or renderer.
- Dependency/docs:
  - manifest, lockfile, and README agree on kit revision `fa0a728...`;
  - lockfile changes are limited to the kit source revision and necessary
    resolution metadata.
- Commands:
  - `script/fmt`
  - focused `cargo test -p botster-tui <mouse-or-terminal-test-name>` while
    iterating
  - `script/test`
  - `script/clippy`
  - `cargo run -p botster-tui -- --smoke` as a binary link/build check only
  - `git diff --check`
- Runtime-path proof: production `run_loop` continues to read real crossterm
  streams, redraws between loop iterations, and sends both Down and Up into the
  upgraded kit router. The rendered-surface tests mirror that boundary by
  rebuilding the hit map between events and exercise the same `HitMap`,
  `InputRouter`, and `DogfoodApp::handle_dispatch` boundaries; this ticket is
  not scaffold-only. The static `--smoke` message is not runtime mouse evidence.

## Pipeline Gates And Artifacts

- This document is the durable Plan artifact for `botster_plan_gate`.
- The run-scoped Plan workflow checklist records context loading, repository and
  upstream tracing, scope containment, runtime verification, and gate delivery.
- The run-scoped vault checklist records notes loaded, convention conflicts
  (none), verification evidence, and capture disposition.
- Plan Review should reject local capture logic, a dual activate-on-Down shim,
  a broad kit-main update without need, a semantic test that never asserts Down
  is non-activating, a click test that never crosses a render boundary, or
  TerminalView coverage that omits the trailing Up or rewrites attach as
  ordinary activate-on-release.
- Implement evidence must include the exact dependency diff, commands and
  results, final synthetic mouse audit, and the production runtime-path
  statement.

## Vault Gaps Worth Capturing

- The vault already records capture-oriented mouse behavior and TerminalView
  passthrough constraints, but it does not yet state the verified cross-client
  distinction that semantic TUI controls activate on matching release while
  TerminalView attach remains down-driven.
- Do not capture that as durable knowledge during Plan. After implementation and
  runtime verification, enrich an existing TUI routing note or capture one
  atomic note through the vault inbox pipeline if this distinction remains a
  stable public kit contract.
- No other durable gap is evident. Exact test helper mechanics, dependency SHAs,
  and ticket-local migration details belong in the repository artifact rather
  than the vault.
