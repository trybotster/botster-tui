# TUI Kit Full SGR Mouse Passthrough Pin Plan

Ticket: Bump botster-tui kit pin for full SGR mouse passthrough

## Context loaded

- Pipeline run `run_1784524134_763595`, Plan step `botster_plan`, and gate
  `botster_plan_gate`.
- The run has no prior artifacts, reviews, findings, questions, answers, or
  blocking dependencies.
- Planning authority: [[planner-playbook]], [[botster-planner-playbook]],
  [[botster-architecture]], [[cli-patterns]], and [[spa-patterns]].
- Ticket-specific vault constraints:
  [[focused mouse mode terminal passthrough needs complete sgr reports]],
  [[botster tui consumes tui kit through a thin app policy adapter]],
  [[terminal view prop contract is closed in botster core]],
  [[tui-mouse-event-dispatch-via-capture-semantics]],
  [[test script required for rust tests not cargo test]], and
  [[cli test sh filters match rust test names not filenames]].
- Repository state at planning time: the ticket branch and `origin/main` both
  point to `7c6bd154506c964f225b2e1970b06da181b43b29`; the worktree was clean before
  this plan artifact was added.
- Current dependency state:
  `crates/botster-tui/Cargo.toml`, `Cargo.lock`, and the README pin
  `botster-tui-kit` to PR #12 merge commit
  `fa0a728297c51d659b675f7d4201a3910a25bd82`.
- Upstream state verified from GitHub on 2026-07-19:
  `botster-tui-kit` has no release or tag, and current `main` is
  `bc066e2581b01fb9e5271794c9a67ba1ace36e42`. That revision includes:
  - PR #12, activate controls on mouse release (`fa0a728...`);
  - PR #13, complete terminal SGR mouse passthrough (`dfb8fa3...`);
  - PR #14, normalize wheel and trackpad scroll streams (`8ee28c8...`);
  - PR #15, add multi-click count and drag threshold (`51ace95...`);
  - PR #16, add `HitMap` occlusion barriers (`bc066e2...`).
- Production runtime trace:
  `run_loop` reads Crossterm events, calls the kit-owned
  `InputRouter::dispatch_event` with the production draw's `HitMap`, and passes
  the result to `DogfoodApp::handle_dispatch`.
  `InputDispatch::TerminalForward` then becomes
  `DaemonRequest::SendInput` for the currently attached session and
  subscription. This is an active runtime dependency update, not scaffold-only
  work.
- Existing regression state:
  `mouse_mode_terminal_release_is_ignored_without_duplicate_focus_action`
  creates a test-local `TerminalView` with renderer-local `mouse_mode: true`,
  proves Down focuses through the terminal action, and currently expects the
  trailing Up to be `InputDispatch::Ignored`. The adjacent
  `focused_terminal_mouse_pair_attaches_once_and_preserves_key_forwarding`
  already proves one Down-driven attach and key forwarding, but does not enable
  mouse mode or prove SGR release forwarding.

## Product decisions and assumptions

- Default: pin the exact current kit `main` commit
  `bc066e2581b01fb9e5271794c9a67ba1ace36e42`, not a floating branch. It is the
  most current immutable revision and satisfies the ticket preference for the
  already-merged mouse stack.
- Required behavior from that revision is PR #13's complete SGR routing. PRs
  #14-#16 may alter reusable router internals, but this consumer ticket does not
  need to expose new product controls for them.
- Preserve the established Down-driven terminal focus/attach contract. Once
  the router focuses the test-local mouse-mode terminal, its trailing left Up
  must be `TerminalForward` with SGR release bytes and must not produce another
  semantic Action.
- Treat the test-local `mouse_mode` prop as a renderer fixture only. Production
  `DogfoodApp::surface()` continues to validate the closed core
  `terminal_view` schema and must not gain a private `mouse_mode` prop.
- The implementation should set the app fixture's attached session and matching
  subscription after handling Down, then feed the returned Up dispatch through
  `DogfoodApp::handle_dispatch`. This proves the real consumer write boundary
  without inventing daemon state or changing attach lifecycle code.
- `botster-core`, `botster-hub-client`, and `botster-hub-test-support` pins
  remain unchanged unless Cargo demonstrates a concrete compatibility
  requirement. Any additional dependency movement is a scope expansion that
  must be explained from compiler or lockfile evidence.
- No human question is needed for the current plan. Ask before proceeding if
  current kit main no longer resolves to the verified commit, requires
  unrelated dependency upgrades, changes the Down-driven attach contract, or
  cannot express release forwarding without widening the core schema.

## Scope

- Replace the exact `botster-tui-kit` git revision with verified current main
  `bc066e2581b01fb9e5271794c9a67ba1ace36e42`.
- Regenerate `Cargo.lock` and inspect the resolved package diff so only the kit
  source revision and strictly necessary transitive resolution changes move.
- Update the mouse-mode terminal regression in `app.rs` so:
  - Down returns the existing `botster.terminal.focus` Action and causes one
    attach request;
  - after the test observes/installs matching attached-session state, Up returns
    `InputDispatch::TerminalForward` with the expected SGR release bytes;
  - handling Up reaches the app's `DaemonRequest::SendInput` observation;
  - the full Down+Up sequence produces exactly one attach and no second Action.
- Keep adjacent same-frame and cross-frame activate-on-release tests green,
  especially selection on stable release and cancellation when another row is
  redrawn under the pointer.
- Update the README Foundation section to name the selected revision, describe
  the full SGR release behavior, and state which later kit features are present
  but not yet explicitly consumed by app UI:
  - multi-click count is not displayed;
  - the optional scroll-normalizer poll/deadline clock is not driven;
  - no additional app-specific occlusion helpers are required beyond the kit's
    existing `HitMap` behavior.
- Preserve the thin `renderer.rs` adapter and the existing
  `run_loop -> InputRouter -> DogfoodApp::handle_dispatch` production path.

Botster layer touched: Rust TUI client, its pinned shared TUI-kit dependency,
unit/regression coverage, and dependency/runtime documentation.

## Non-scope

- Do not open the `botster-core` `terminal_view` schema for `mouse_mode`.
- Do not add a production `mouse_mode` prop that fails `UiNode::validate()`.
- Do not implement text selection or product UI for multi-click, scroll polling,
  drag thresholds, or occlusion.
- Do not copy or reimplement the kit's SGR encoder in `botster-tui`.
- Do not change hub/session transport ownership, terminal attach lifecycle, or
  the `TerminalForward -> DaemonRequest::SendInput` boundary.
- Do not refactor the renderer adapter, router, app state, or nearby mouse tests
  beyond changes required by the new upstream contract.
- Do not retrofit unrelated plans or docs merely because they cite older
  historical pins; update living README behavior and this ticket's artifact.
- Do not add gems, packages, abstractions, feature flags, or optional
  configuration.

## Assumptions and unknowns

- Assumption: exact commit `bc066e2...` remains the immutable implementation
  target even if upstream `main` advances during implementation.
- Assumption: PRs #13-#16 retain compatibility with this repo's current
  `botster-core`, Crossterm, and Ratatui versions because they are already on
  kit main; Cargo and tests must verify this rather than relying on the
  assumption.
- Assumption: the existing test-local node constructor intentionally bypasses
  core validation so renderer-local `mouse_mode` can exercise kit behavior
  without changing production schema.
- Assumption: exact SGR release bytes should be asserted at the router boundary;
  the app-level assertion may additionally verify the same bytes as the
  `SendInput.data` string.
- Unknown: the exact terminal-local coordinates encoded by the current rendered
  fixture. The implementation should derive the expected sequence from the
  chosen cell and assert a literal SGR release report, not duplicate the
  encoder algorithm in the test.
- Unknown: whether the lockfile refresh changes anything besides the kit source
  revision. Any broader change must be traced to Cargo resolution and minimized.
- Unknown: whether kit PRs #14-#16 expose unused public methods that trigger new
  consumer warnings or behavior. The strict clippy and full test gates decide;
  no speculative app integration is planned.

## Affected surfaces and files

- `crates/botster-tui/Cargo.toml`
  - change only the exact `botster-tui-kit` revision unless compilation proves
    another pin must move.
- `Cargo.lock`
  - resolve the selected immutable kit revision and audit the package source
    diff.
- `crates/botster-tui/src/app.rs`
  - revise/rename the focused mouse-mode terminal regression and, if clearest,
    consolidate it with the adjacent one-attach/key-forwarding proof while
    preserving each acceptance assertion.
- `README.md`
  - update the living Foundation pin/behavior and record intentionally unused
    later-kit facilities.
- `docs/plans/tui-kit-full-sgr-mouse-passthrough-pin-plan.md`
  - reviewable Plan-stage artifact and downstream acceptance map.
- `crates/botster-tui/src/renderer.rs`
  - expected unchanged; inspect only to confirm it remains a thin kit re-export.

## Implementation sequence

1. Update only the kit revision in `crates/botster-tui/Cargo.toml`, resolve the
   lockfile, and inspect the dependency diff before touching behavior.
2. Run the existing focused mouse/router tests against the new kit to confirm
   the intended stale expectation: test-local mouse-mode Up now produces
   `TerminalForward`.
3. Update the app-level regression to drive the complete Down+Up stream through
   the rendered `HitMap`, kit `InputRouter`, and `DogfoodApp::handle_dispatch`.
   Assert one Down-driven attach, exact Up SGR release bytes, one matching
   `SendInput`, and no second Action/attach.
4. Keep and rerun the stable-node and cross-frame/cross-node activate-on-release
   regressions to prove the later kit features did not change outer semantic
   capture.
5. Update the README pin, included PR list/behavior, and unused-feature
   disposition. Leave historical prior-ticket plan facts intact.
6. Run the repository's focused and full verification commands, inspect the
   complete diff for unrelated movement and path/PII leakage, and record exact
   executed test counts/results in the implementation report.

## Risks

- Dependency drift: current kit main includes four PRs after the existing pin.
  Mitigation: use the exact verified SHA, enumerate the included PRs, inspect
  the lock diff, and reject unexplained companion pin movement.
- Duplicate semantic activation: release forwarding could accidentally coexist
  with a second terminal focus Action. Mitigation: assert the exact Up variant,
  one attach across the pair, and one `SendInput`.
- False runtime proof: asserting only router bytes would not prove the consumer
  write path. Mitigation: pass the Up dispatch to `DogfoodApp::handle_dispatch`
  with matching attached state and assert the observed send-input request.
- Schema leakage: promoting test-local `mouse_mode` into the production surface
  would fail the closed core schema. Mitigation: leave `terminal_panel()` and
  `surface().validate()` unchanged.
- Coordinate brittleness: a raw byte expectation can drift if the fixture cell
  changes. Mitigation: choose a stable known terminal cell and document the
  literal 1-based terminal-local coordinate represented by the expected bytes.
- Later-kit behavior regression: scroll normalization, drag thresholds,
  multi-click tracking, or occlusion could alter existing semantic mouse paths.
  Mitigation: retain same-frame activation, cross-frame cancellation, terminal
  key forwarding, and full-suite coverage.
- Test-command false confidence: a name filter can execute zero tests.
  Mitigation: use actual Rust test-function substrings and record executed test
  counts; finish with the repository-wide script.

## Acceptance checks and tests

Focused checks, using the root wrapper so `BOTSTER_ENV=test` is set:

```sh
./test.sh mouse_mode_terminal
./test.sh focused_terminal_mouse_pair
./test.sh focused_session_list_row_updates_attach_selection
./test.sh session_click_cancels_when_redraw_reorders_another_row_under_release
```

If the implementation renames or consolidates tests, substitute the actual Rust
function-name substring and confirm at least one intended test executed.

Repository gates:

```sh
script/fmt
script/test
script/clippy
git diff --check origin/main...HEAD
```

Acceptance evidence must show:

- manifest, lockfile, and README agree on exact kit revision
  `bc066e2581b01fb9e5271794c9a67ba1ace36e42`;
- the selected revision is documented as containing kit PRs #12-#16, with PR
  #13 identified as the required SGR behavior;
- rendered test-local mouse-mode terminal Down produces the existing focus/attach
  Action exactly once;
- trailing Up is exact SGR `TerminalForward` data and reaches the app's
  send-input boundary, with no duplicate Action or attach;
- focused terminal key forwarding remains green;
- same-frame activation and cross-frame/cross-node cancellation remain green;
- the complete workspace test and strict clippy gates pass;
- `renderer.rs`, the production `terminal_view` schema, and unrelated dependency
  pins remain unchanged unless separately justified by concrete evidence.

No live-hub run is required by the ticket because the changed behavior is
Crossterm event routing before the already-covered `SendInput` boundary. If the
app-level observed-request proof cannot reach that boundary, escalate rather
than replacing it with a static source assertion.

## Pipeline gates and artifacts

- Plan artifact: this file, attached to the run.
- Plan gate: submit all required sections from this artifact to
  `botster_plan_gate`.
- Plan Review should verify the upstream SHA/PR ancestry, the closed-schema
  boundary, the exact runtime trace, and the no-second-Action acceptance rule.
- Implement should preserve the exact pin decision and attach a report with the
  lockfile audit, files changed, focused test counts, full script results, and
  unused-feature disposition.
- Review and Verify should compare the branch diff with this plan, run the same
  strict gates, and reject router-only evidence that never reaches
  `DogfoodApp::handle_dispatch`.

## Vault gaps worth capturing

- No blocking vault gap exists: the vault already records complete SGR routing,
  thin kit/app ownership, closed `terminal_view` schema, capture semantics, and
  repository test-wrapper behavior.
- Capture after implementation only if this consumer proves a durable pattern
  not already covered by those notes, such as a reusable downstream acceptance
  shape for “semantic Down followed by raw terminal Up” across the kit/app
  boundary.
- The fact that kit PRs #14-#16 are present but not product-consumed belongs in
  the README and implementation report for this ticket, not a new vault note
  unless the omission becomes a repeated cross-client planning constraint.
