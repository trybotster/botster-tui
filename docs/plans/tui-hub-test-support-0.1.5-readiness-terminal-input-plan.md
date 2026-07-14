# TUI Hub Test Support 0.1.5 Readiness And Terminal Input Plan

## Context loaded

- Pipeline run `run_1784048122_692804`, Plan step `botster_plan`, and gate `botster_plan_gate` were loaded through `project_pipelines_current_context`. The upstream release dependency is closed. There are no prior artifacts, reviews, findings, questions, or answers to reconcile.
- The closed dependency publishes `@trybotster/hub-test-support` 0.1.5 with conformance fixture revision 12. The corresponding merged Rust hub revision is `a9b8c637682e7bd5862b041b471bf660d91a895c`; its typed fixture orders history as `attaching`, renderable snapshot/scrollback, `attached`, then live output, and its idle fixture orders `attaching`, optional initial state, `attached`, then live output without fabricated scrollback.
- Repo state started at `87dc529` with `botster-hub-client` and `botster-hub-test-support` pinned to `333b75fc66de7eda521e05bea5dcc5eb91b8884c`. At the human's explicit direction during Plan, both pins and `Cargo.lock` were updated to `a9b8c637682e7bd5862b041b471bf660d91a895c`.
- Production runtime trace: `run_loop` routes Crossterm events through `botster_tui_kit::InputRouter`; `DogfoodApp::handle_dispatch` converts `InputDispatch::TerminalForward` into public `DaemonRequest::SendInput` only for `attached_session`; `apply_response` owns `AttachState` projection; `poll_hub` keeps the bounded hydration drain active.
- Existing live proof in `assert_live_attach_history_readback` bypasses the TUI input owner by issuing `DaemonRequest::SendInput` from a separate direct daemon connection. It proves output observation, but not TUI input forwarding.
- Vault authority loaded: [[identity]], [[goals]], [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[botster hub client crate is the external client boundary]], [[tui client attach uses hub protocol not session protocol]], [[terminal subscribe readiness gates on sessionio initial snapshot delivery]], [[initial terminal snapshots must precede live output activation]], [[lifecycle guards evaluated before the reconciling drain are one call stale]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], [[botster pipeline needs continuous product owner between agent steps]], [[plan agents must author vault context as wikilinks not home paths]], [[plan steps need reviewable plan artifacts]], and [[project pipelines checklist worker timeouts require artifact evidence fallback]].
- Project Pipelines checklist instructions were loaded. Both standard vault-checklist creation and a custom Plan checklist timed out in the plugin worker, including a sequential retry. Per the documented fallback, this artifact records notes read, convention fit, verification evidence, and capture disposition.

## Scope

- Keep `botster-hub-client` and `botster-hub-test-support` pinned together at merged revision `a9b8c637682e7bd5862b041b471bf660d91a895c`, which exposes conformance fixture revision 12.
- Update the shared late-attach tests to assert event meaning and relative order instead of old positional indices:
  - history case: `Attaching < Snapshot/Scrollback with renderable data < Attached < TerminalOutput`;
  - idle case: `Attaching < optional initial state < Attached < TerminalOutput`, with no fabricated scrollback.
- Preserve the existing bounded hydration drain, history-first display, deadline fallback, reconnect reset, and event-order rendering behavior. `Attached` establishes input ownership; it does not turn opaque snapshot byte metadata into visible history or remove the defensive deadline.
- Extend the isolated live-hub smoke so it waits until the TUI observes `Attached`, forwards a unique marker through `DogfoodApp::handle_dispatch(InputDispatch::TerminalForward { ... })`, and then observes that marker in PTY echo/output through the TUI's normal drain/render state.
- If that live proof fails, make the smallest correction to `attached_session`/input ownership in the existing `DogfoodApp` response and dispatch path, with a focused regression test. Do not add a parallel input state machine.
- Update the README's pinned hub revision/readiness description where the touched behavior is currently stale.

## Non-scope

- No hub/core/session-worker changes; revision 12 is the upstream authority being adopted.
- No new protocol, private socket plumbing, alternate terminal data plane, TUI-owned scrollback store, VT parser, or terminal truth cache.
- No change to plugin, SPA, Rails relay, MCP, Project Pipelines policy, package workflows, renderer primitives, or general TUI navigation.
- No removal of the five-second hydration deadline, `ReadScreen` fallback, `CaptureSnapshot` diagnostics, reconnect hydration, or bounded drain merely because corrected `Attached` readiness is now available.
- No claim that opaque blank `Snapshot.bytes` is renderable history; only non-empty `Snapshot.data`/`Scrollback.data` may satisfy visible-history evidence.
- No broad cleanup of existing terminal tests or app state beyond lines required by the revised fixture and live proof.

## Assumptions and unknowns

- Assumption: hub commit `a9b8c637682e7bd5862b041b471bf660d91a895c` is the Rust dependency coordinate corresponding to the published Node package 0.1.5/revision 12; upstream metadata and `CONFORMANCE_FIXTURE_REVISION` confirm this.
- Assumption: the production terminal-input boundary worth proving headlessly is the same `DogfoodApp::handle_dispatch` branch called by `run_loop` after kit routing. Constructing `InputDispatch::TerminalForward` in the live harness avoids terminal UI automation while still exercising TUI ownership and public `SendInput` request construction.
- Assumption: terminal line discipline or the spawned fixture command will make the unique input marker observable as PTY echo/output. The acceptance check must fail if no marker returns; it must not infer success from a successful request response alone.
- Assumption: `Attached` may arrive while the defensive hydration cycle remains pending. Input can become available at `Attached`, while the bounded drain continues looking for renderable history or its deadline.
- Unknown to resolve during implementation: whether revision 12's live isolated hub consistently emits `AttachState::Attached` in the first attach response or a later drain. The smoke must poll with a deadline rather than assume one response boundary.
- Unknown to resolve only if the live marker fails: whether ownership is missing because `apply_response` does not retain `Attached`, because the event's session/subscription identity is mismatched, or because the input dispatch is not reaching `SendInput`. Diagnose before changing state.

## Affected surfaces/files

- `crates/botster-tui/Cargo.toml` — paired hub client/test-support pins; already updated at the human's direction.
- `Cargo.lock` — exact paired git sources; already refreshed.
- `crates/botster-tui/src/app.rs` — fixture expectation tests, isolated live attach/input proof, and only if the proof fails, the existing `AttachState`/`TerminalForward` ownership path.
- `README.md` — correct stale hub revision and describe readiness-gated TUI input/live proof.
- `script/test-live-hub` — expected to remain structurally unchanged because it already builds the pinned hub/session worker and runs the isolated test; touch only if the revised smoke needs an explicit environment input that cannot live in the Rust harness.
- This plan artifact — reviewable Plan-stage evidence.

Botster layers touched: Rust TUI and the public Rust hub-client/test-support dependency boundary. The run remains bound to target `tgt_c3d470bab78549df920a41e8fb0e58d8` and this ticket worktree; no ambient hub checkout is an implementation target.

## Implementation sequence

1. Keep the paired hub pins/lockfile at `a9b8c637682e7bd5862b041b471bf660d91a895c` and confirm the dependency descriptor reports conformance revision 12.
2. Rewrite the two shared-fixture tests around semantic event lookup/order. Feed responses in boundaries that prove empty drain and `attaching` do not finish hydration, renderable history does, `attached` establishes the attached session, live bytes render after readiness, and idle/no-history live output remains valid without fabricated history.
3. Add explicit coverage that an empty/opaque snapshot (`data.is_empty()` with byte metadata) neither populates terminal output nor counts as visible-history completion. Preserve the deadline path.
4. In `assert_live_attach_history_readback`, wait with a bounded loop for `app.attached_session` to match the selected session. Send a unique marker through `app.handle_dispatch(InputDispatch::TerminalForward { ... })`, then poll until the marker appears in `app.terminal_output` and rendered terminal content. Replace the direct-daemon `SendInput` shortcut for this assertion.
5. If step 4 fails, trace the real `AttachState` event identity through `apply_response` and correct only the existing attached-session ownership branch; add a focused regression that fails without the correction.
6. Update README dependency/readiness/input proof text, then run focused, workspace, lint, and live-hub verification.

## Risks

- Positional-fixture risk: merely changing `[1]` to `[2]` would remain brittle and could omit the readiness contract. Mitigation: locate/assert event kinds, states, identities, and relative order.
- False input-proof risk: direct `HubConnection::request(SendInput)` bypasses the production TUI gate. Mitigation: invoke the same `handle_dispatch` branch reached from `run_loop` and require returned PTY bytes.
- Premature-input risk: selection or `Attaching` could be mistaken for ownership. Mitigation: wait for matching `Attached` and retain the existing pre-attach rejection test.
- Opaque-history risk: a blank snapshot with non-zero metadata bytes could suppress fallback or be rendered as history. Mitigation: visible-history evidence remains non-empty renderable `data`, with an explicit regression.
- Drain regression risk: treating `Attached` as complete hydration could remove the defensive history/fallback window. Mitigation: keep `attach_hydration`, deadline, and drain tests separate from `attached_session` input readiness.
- Live-smoke timing risk: daemon event batching varies. Mitigation: bounded polling against state/output, never fixed sleeps as the success condition.
- Dependency-coupling risk: updating only one hub crate can produce mixed protocol fixtures. Mitigation: paired pins and lockfile assertions.
- Environment risk: this worktree path contains `:`, which makes Cargo's default target path invalid in `DYLD_FALLBACK_LIBRARY_PATH` on macOS. Verification must set `CARGO_TARGET_DIR` to a colon-free temporary path.

## Acceptance checks/tests

- Dependency evidence:
  - `cargo metadata --format-version 1 --locked` resolves both hub crates at `a9b8c637682e7bd5862b041b471bf660d91a895c`.
  - a focused assertion or existing compatibility descriptor confirms conformance fixture revision 12.
- Focused Rust tests in `crates/botster-tui/src/app.rs` prove:
  - history ordering is `Attaching < renderable Snapshot/Scrollback < Attached < TerminalOutput`;
  - idle ordering is `Attaching < optional initial state < Attached < TerminalOutput` without fabricated scrollback;
  - an opaque/empty snapshot is not visible-history evidence;
  - terminal input before `Attached` remains rejected;
  - matching `Attached` owns input without ending the bounded hydration drain;
  - the unique terminal marker traverses TUI dispatch and returns through PTY output.
- Commands, using a colon-free target directory:
  - `CARGO_TARGET_DIR=/tmp/botster-tui-ticket-1783965015 script/fmt`
  - `CARGO_TARGET_DIR=/tmp/botster-tui-ticket-1783965015 cargo test -p botster-tui shared_`
  - `CARGO_TARGET_DIR=/tmp/botster-tui-ticket-1783965015 script/test`
  - `CARGO_TARGET_DIR=/tmp/botster-tui-ticket-1783965015 script/clippy`
  - `CARGO_TARGET_DIR=/tmp/botster-tui-ticket-1783965015 script/test-live-hub`
- Runtime acceptance for `script/test-live-hub`: the isolated TUI observes matching `Attached`, forwards a per-run unique marker through TUI terminal dispatch, and later observes that marker in its terminal output/rendered surface. Request success without returned bytes is a failure.
- Regression evidence must show the production entry point remains `run_loop -> InputRouter::dispatch_event -> DogfoodApp::handle_dispatch -> DaemonRequest::SendInput`; fixture-only state mutation is insufficient.
- Current Plan evidence after pin update: `CARGO_TARGET_DIR=/tmp/botster-tui-plan-target cargo test -p botster-tui shared_` compiles revision 12 and fails both old positional tests, at `app.rs:4353` (index 2 is now `Attached`, not live output) and `app.rs:4379` (the old idle slice no longer contains live output). These are expected ticket-related failures, not accepted final failures.

## Pipeline gates and artifacts

- Plan artifact: this file plus a Project Pipelines `plan` artifact carrying the required gate fields and explicit assumptions.
- Plan gate: submit `botster_plan_gate` with context, scope, assumptions/unknowns, affected files, risks, acceptance checks, and vault gaps.
- Plan Review should reject any plan/implementation that removes the bounded drain, counts blank snapshot metadata as history, retains the direct-daemon input shortcut as proof, or changes hub/core instead of consuming revision 12.
- Implementation/verification artifacts should attach the paired dependency resolution, focused test output, full repo checks, and the standalone live marker evidence.
- Checklist API status: creation timed out repeatedly in the Project Pipelines plugin worker. This artifact is the required durable fallback evidence; later agents should retry checklist creation/update if the worker recovers.

## Convention fit

- No convention conflicts. The plan consumes the public hub client/test-support boundary, uses the existing TUI model and callbacks, introduces no abstraction or alternate protocol, and limits changes to the dependency, affected tests/runtime seam, and stale documentation.
- The human-directed pin update is a cold, paired dependency move rather than a dual-path compatibility layer.
- Rails conventions are not applicable because no Rails surface is touched.

## Vault gaps worth capturing

- Candidate durable pattern after runtime proof: a first-party terminal client smoke must send input through the client's production ownership/dispatch boundary; direct protocol `SendInput` proves the hub but not the client. Capture through the vault inbox/pipeline only after the revised live smoke validates the claim.
- No new readiness-order note is needed: [[terminal subscribe readiness gates on sessionio initial snapshot delivery]], [[initial terminal snapshots must precede live output activation]], and the existing shared-fixture notes already hold the durable contract.
- Preserve the already-known operational gap [[project pipelines checklist worker timeouts require artifact evidence fallback]]; this run reproduced it, but repetition alone does not require a duplicate note.
