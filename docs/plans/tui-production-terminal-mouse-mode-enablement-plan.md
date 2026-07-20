# Production Terminal Mouse-Mode Enablement Plan

Ticket: Production terminal mouse-mode enablement (client-owned, schema-valid)

## Context loaded

- Current pipeline assignment: ticket `ticket_1784521841_539739`, run
  `run_1784587522_866954`, Plan step `botster_plan`, run step
  `run_step_1784587523_601243`, and attestation gate `botster_plan_gate`.
- The fresh run has no artifacts, reviews, findings, or open questions. All six
  registered dependencies are closed. Earlier durable human answers remain
  binding: use authoritative session `ModeFlags` through a targeted probe; keep
  the shadow client-owned; do not add a pushed mode event, parse DECSET/DECRST
  in the TUI, or expand `terminal_view` props; and treat failure or unknown
  state as mouse mode off.
- `git pull origin main` reports this ticket worktree is up to date. The branch
  is bound to the botster-tui target
  `tgt_c3d470bab78549df920a41e8fb0e58d8`; implementation must remain in this
  worktree.
- Planning authority loaded: [[planner-playbook]],
  [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], and
  [[spa-patterns]]. The generic and Botster playbooks require the smallest
  existing-primitive change, explicit runtime-path proof, repo-authoritative
  artifact placement, exact target/worktree binding, and named Rust and live
  runtime harnesses.
- Ticket-specific vault constraints loaded:
  [[terminal view prop contract is closed in botster core]],
  [[nested rich tuis lose scrolling when botster consumes mouse reports or control keys]],
  [[focused mouse mode terminal passthrough needs complete sgr reports]],
  [[synced state types are allowed while pushed event variants are forbidden]],
  [[terminal accessory reattach must restore nested tui input passthrough]],
  [[pinned libghostty exposes synchronous exact mouse mode state]],
  [[botster tui consumes tui kit through a thin app policy adapter]], and
  [[botster tui uinode event routing captures hit regions during draw]].
- Merged substrate inspected from Project Pipelines artifacts and exact upstream
  source:
  - botster-core PR #106 merged as
    `7ce1f705952407a1e4f76bcc83cbc6da2efc7efb`; its real production route is
    `CoreDaemon::read_mode_flags` through local/worker runtime and the concrete
    Ghostty terminal query. Live and retained worker-backed reads proved
    `mouse_mode == 9`, while unsupported backends preserve typed failure.
  - botster-hub PR #149 merged as
    `06f1fa7a01a542eedc5c68a52cf9ecd05da9dabc`; hub-client now exposes
    `DaemonRequest::ReadModeFlags`, `DaemonResponseKind::ReadModeFlags`, and
    `DaemonModeFlags { session_id, mouse_mode: u8 }`. The merged external socket
    test proves the request reaches the real daemon path.
  - botster-tui-kit PR #17 merged as
    `16c6035dd06ffb5ec0704f1a3603c3e4bc5c81bf`; `HitMap` now exposes
    `set_terminal_mouse_mode(node_id, mode_bits)`. It enables the existing
    Boolean router flag only for tracking bits `1|2|4`; bit `8` alone is only
    SGR encoding. Rendering deliberately starts mode off, so clients must
    reapply their shadow after every render.
  - botster-web PR #65 merged as
    `2d52010f757ecd645e3d809391cf360d57c4033d`; it proves cross-client protocol
    representability but adds no browser caller. No botster-web work is needed
    here.
- Current botster-tui production entry point is `run_loop`: poll the hub, render
  `DogfoodApp::surface()` into a fresh `HitMap`, dispatch Crossterm input through
  kit `InputRouter`, then map `TerminalForward` to hub-client `SendInput` for the
  current attachment. `surface()` emits `dogfood-terminal` with only the valid
  `session_id` and `title` props and validates the tree before rendering.
- Current missing link is entirely in the consumer: Cargo pins predate the hub
  probe and kit hook; `DogfoodApp` owns no attachment-scoped mode shadow; and
  `run_loop` does not reapply mode to the newly rendered hit map. A test-only
  fixture still injects invalid `mouse_mode`; that fixture must move to the
  schema-valid hook.

## Scope

Botster layers touched: Rust TUI client policy/attachment state, hub-client and
TUI-kit dependency pins, client tests/live-hub acceptance, and local docs.

1. Pin `botster-hub-client` and `botster-hub-test-support` to merged hub commit
   `06f1fa7a01a542eedc5c68a52cf9ecd05da9dabc`, and pin
   `botster-tui-kit` to merged kit commit
   `16c6035dd06ffb5ec0704f1a3603c3e4bc5c81bf`; regenerate `Cargo.lock` without
   unrelated dependency updates. Keep botster-tui's direct botster-core pin
   unchanged because it is the UiNode type identity shared with the kit; the
   production hub build carries the authoritative core revision behind the
   hub-client boundary.
2. Add one attachment-scoped `u8` mouse-mode shadow to `DogfoodApp`, keyed or
   guarded by the current session/subscription generation. Keep `0`/absence as
   the safe-off state. Clear it before a new attach and on detach, process exit,
   reconnect/transport failure, session switch, stale identity, malformed
   response, or readback error.
3. Use the existing optional readback seam to issue
   `DaemonRequest::ReadModeFlags` for the attached session and accept only a
   `ReadModeFlags` body whose `session_id` still matches the active attachment.
   Probe when attach becomes active and after reattach hydration. A matching
   terminal-output drain marks the state for refresh; service that refresh at
   most once per second so child DECSET/DECRST output produces on/off
   transitions without issuing a query on every drain. Requests are synchronous
   today, so this cadence also prevents duplicate outstanding work without a new
   async abstraction.
4. After every production render and before input dispatch, call
   `HitMap::set_terminal_mouse_mode("dogfood-terminal", current_bits)`. The
   shared UiNode remains unchanged and the kit remains the sole owner of SGR
   mouse encoding and full-stream routing.
5. Replace the invalid test fixture prop with the public hit-map hook and add
   focused state/lifecycle, cadence, schema, and router integration coverage.
   Extend the existing isolated-hub acceptance to prove real Ghostty mode-on and
   mode-off readback reaches the same shadow and routing seam used by `run_loop`.
6. Update README ownership/pin documentation and this plan so neither still
   describes production enablement as deferred or a Boolean prop contract.

Every implementation line must trace to one of those six items. In particular,
prefer small methods on `DogfoodApp` over a new mode manager, service object,
event bus, or generic polling framework.

## Non-scope

- No botster-core UiNode schema change and no `mouse_mode` prop or slot on
  `terminal_view`.
- No TUI-local terminal escape parser and no pushed `ModeChanged`/mode event.
- No changes in botster-core, botster-hub, botster-tui-kit, botster-web, or the
  Ghostty fork; this ticket consumes their merged contracts only.
- No expansion of the established `u8` contract into a full xterm mode enum.
- No change to SessionIo byte ownership: encoded bytes still travel as
  `TerminalForward` -> hub-client `SendInput` -> the attached session.
- No text selection, browser UI, package/pipeline policy, generalized scheduler,
  adjacent dependency refresh, or cleanup of unrelated mouse tests.

## Assumptions and unknowns

- Binding mapping: DEC `1000 -> 1`, `1003 -> 2`, `1002 -> 4`, and `1006 -> 8`.
  This matches the shipped producer; it is not open to renumbering. The TUI
  stores the exact `u8`; the kit derives tracking-on from bits `1|2|4`, and bit
  `8` alone must remain off.
- The authoritative query is fallible. A missing body, wrong response kind,
  wrong session id, transport error, or daemon operator error clears/keeps the
  shadow off and may expose an existing diagnostic; none may fabricate default
  authority.
- The current hub-client request is synchronous, so stale replies cannot race a
  concurrent attach inside one call. Identity checks are still required to
  preserve the invariant if call structure changes and to reject malformed
  responses.
- DECSET/DECRST writes are terminal output, so a matching output event plus a
  bounded one-second refresh is the smallest dynamic transition mechanism.
  Immediate attach/rehydration probes cover already-active modes after restore.
- There is one production terminal node, `dogfood-terminal`, representing the
  current attachment. The kit hook deliberately no-ops for a missing id; a test
  must assert this production id is actually updated so string drift is visible.
- Unknown but non-blocking: exact helper names and whether attach-active and
  hydration-complete probes can share one method should follow nearby
  `ReadScreen`/`CaptureSnapshot` code during implementation. Do not widen scope
  if the smallest placement differs from this plan's pseudostructure.
- The pipeline worktree path contains a colon. Repository scripts can inherit a
  hostile DYLD fallback path, so verification must use a colon-free
  `CARGO_TARGET_DIR` where needed and record the exact result rather than
  classifying an environment abort as a code failure.

## Affected surfaces/files

- `crates/botster-tui/Cargo.toml` — exact merged hub and kit git revisions; keep
  direct core identity aligned with kit.
- `Cargo.lock` — resolved git sources for hub-client, hub test support, and kit,
  with only transitive changes required by those pins.
- `crates/botster-tui/src/app.rs` — attachment-owned `u8` shadow, bounded probe
  lifecycle, response validation, clear paths, post-render hook application,
  request observation fixture, unit/integration tests, and live-hub acceptance.
- `README.md` — exact pins, client-vs-kit-vs-core ownership, safe-off behavior,
  and production availability instead of the stale deferred/invalid-prop text.
- `docs/plans/tui-production-terminal-mouse-mode-enablement-plan.md` — this
  approved, now-actionable plan and final verification evidence if local
  convention records it here.
- `script/test-live-hub` only if the exact merged hub revision cannot be selected
  through the existing Cargo pin/build path. This is conditional and must not be
  touched merely for cleanup.

## Implementation sequence

1. Update the three hub/kit dependency coordinates and lockfile. Confirm Cargo
   resolves one compatible `botster-core` identity for app/kit UiNode values and
   that the isolated hub binaries build from the hub revision carrying core PR
   #106.
2. Introduce the smallest `DogfoodApp` mode-shadow fields and helpers. Centralize
   clearing so every attachment/connection terminal path uses the same safe-off
   invariant.
3. Add targeted probe scheduling and `ReadModeFlags` response handling beside
   the existing optional `ReadScreen` and `CaptureSnapshot` flow. Guard every
   write by current attachment identity and rate-limit output-driven refresh.
4. Reapply the exact shadow to `dogfood-terminal` immediately after each render,
   before `InputRouter::dispatch_event` receives that hit map.
5. Convert the invalid-prop fixture and add focused tests that ablate the hook,
   distinguish SGR bit-only from tracking bits, prove stale/error clearing, and
   exercise on -> off transitions through real renderer/router/app seams.
6. Extend live-hub acceptance with a child command that emits tracking + SGR
   DECSET, later emits matching DECRST, and prove the app observes `9`, forwards
   via the real focused terminal path, then observes `0` and restores outer
   routing. Keep the existing terminal input/reattach assertions green.
7. Update README, run the full gates below, inspect the final diff for invalid
   props, pushed events, local paths/PII, silent defaults, and unrelated churn,
   then attach exact results to the implementation artifact.

## Risks

- **Mode becomes stale after child transition.** Mitigation: output marks a
  bounded refresh due; unit and live-hub tests drive both DECSET and DECRST.
- **Query storm from high-volume PTY output.** Mitigation: at most one
  output-driven probe per attached session per second; verify request counts.
- **Old attachment controls the new terminal.** Mitigation: key shadow and
  response application to session/subscription identity and clear before every
  attach/reconnect path; exercise stale-response and stale-event cases.
- **Bit `8` is mistaken for tracking.** Mitigation: store exact bits and delegate
  the established `1|2|4` tracking test to the kit hook; explicitly test `8` as
  off and `9` as on.
- **Fresh render silently disables mode.** Mitigation: apply the shadow after
  every draw, and render twice in a regression test before dispatching.
- **String node-id drift silently no-ops.** Mitigation: assert the production
  `dogfood-terminal` region changes through the hook in app coverage.
- **Invalid prop survives in tests and is copied back into production.**
  Mitigation: remove the fixture and run a zero-hit scan for `mouse_mode` inside
  `terminal_view` JSON plus real `UiNode::validate()` coverage.
- **Dependency pin advances unrelated behavior.** Mitigation: pin exact reviewed
  merge commits, inspect lockfile delta, and run the full workspace and live-hub
  suite rather than only new tests.
- **Scaffold-only evidence is mistaken for runtime proof.** Mitigation: the
  isolated-hub command must drive real DECSET/DECRST through the production
  Ghostty/backend/hub-client path; unit tests alone do not satisfy acceptance.

## Acceptance checks/tests

1. `git diff --check` and an exact lockfile/source inspection show only the
   intended hub/client/test-support and kit pin changes, with no unrequested
   dependency refresh.
2. Focused Rust tests prove:
   - the production `DogfoodApp::surface()` validates with only `session_id` and
     `title` on `terminal_view`;
   - a matching `ReadModeFlags { mouse_mode: 9 }` owns only the current
     attachment, while `0`, bit `8` alone, errors, wrong ids, detach, exit,
     reconnect, and a new attach produce safe-off routing;
   - attach/rehydration and output-triggered cadence issue the expected requests
     without more than one output refresh per second;
   - rendering a schema-valid surface, applying `9`, and dispatching through the
     real `InputRouter` forwards complete SGR press/release/drag/move/wheel as
     the kit contract requires, while applying `0` restores non-passthrough
     routing and never duplicates the terminal focus/attach action;
   - rendering again requires and receives shadow reapplication.
3. `CARGO_TARGET_DIR=/tmp/botster-tui-ticket-mouse script/fmt` passes.
4. `CARGO_TARGET_DIR=/tmp/botster-tui-ticket-mouse script/test` passes the full
   workspace/all-target suite, including existing attach, reattach, terminal
   input, activate-on-release, and cross-frame regressions.
5. `CARGO_TARGET_DIR=/tmp/botster-tui-ticket-mouse script/clippy` passes with
   warnings denied.
6. `CARGO_TARGET_DIR=/tmp/botster-tui-ticket-mouse-live script/test-live-hub`
   passes against isolated hub/session-worker binaries built from the exact hub
   pin. The test must prove real DECSET `1000+1006` reaches app shadow `9`, the
   post-render production hit region routes an SGR mouse report through
   `TerminalForward`/`SendInput`, DECRST reaches shadow `0`, and reattach restores
   current authoritative mode rather than only visible screen bytes.
7. Final source scans find no production or test fixture that writes
   `mouse_mode` into a `terminal_view` prop, no pushed mode event, no TUI escape
   parser, no silent fallback that treats query failure as authoritative zero,
   and no absolute local path or PII in committed files.

If the native/live hub harness cannot execute for an environmental reason, the
implementation must report the exact command and failure; unit evidence cannot
waive runtime-path acceptance without a human decision.

## Pipeline gates and artifacts

- Plan gate: attach this document and structured evidence for context, scope,
  assumptions/unknowns, files, risks, checks, and vault gaps.
- Plan Review: verify exact merged APIs/pins, attachment identity/cadence, closed
  schema preservation, live-path acceptance, and absence of speculative
  abstractions before approval.
- Implement artifact: files changed, exact dependency revisions, applied
  playbooks, deviations, request-cadence evidence, test output, and residual
  risk.
- Review gate: inspect correctness, regressions, architecture fit, stale docs,
  invalid props, default fabrication, stale attachment state, dead/unwired code,
  and whether the live entry point applies the shadow after every render.
- Verify gate: independently run the focused, full, clippy/fmt, scan, and
  isolated-hub checks; resolve or explicitly retain each review finding.

## Vault gaps worth capturing

- Correct the existing [[terminal view prop contract is closed in botster core]]
  wording: verified kit review evidence shows `session_id` is required while
  `title` is allowed but optional. The closed allowlist/no-slots conclusion
  remains correct.
- If implementation proves the output-triggered, rate-limited authoritative
  probe is reusable beyond this client, capture one atomic pattern after runtime
  verification: restored terminal mode requires a client shadow to be
  invalidated by attachment identity and refreshed from authoritative emulator
  state after output. Do not capture it from this plan alone.
- No new architecture decision is otherwise needed: the authoritative Ghostty
  query, exact bit mapping, closed schema, synced-readback posture, and post-render
  kit hook are already durable vault knowledge.
