# TUI: send terminal input through the Core duplex adapter

Ticket: `ticket_1787603674_865638`
Run: `run_1788280083_197023`
Pipeline: Botster Stack Delivery
Base ref: `main` (TUI `3b84d57`)

## Target repository

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`trybotster/botster-tui`) |
| `target_id` | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Repository playbook | [[botster-tui-playbook]] |

The `target_id` resolves to `botster-tui` through `list_spawn_targets`. The plan
does not infer the repository from the process working directory.

## Playbooks and notes loaded

Role and repository:

- [[planner-playbook]]
- [[botster-planner-playbook]]
- [[botster-tui-playbook]]
- [[botster runtime teardown lenses]] (runtime-teardown class applies)

Targeted atomic notes:

- [[core owns duplex terminal transport while Hub stays content blind]]
- [[every TerminalInputResult must stamp the live subscription id]]
- [[core default requirement includes duplex binary again]]
- [[first-party Unix attach clients use split Hello and subscription close events]]
- [[first-party clients put terminal mechanism tokens only in terminal compatibility]]
- [[Core terminal subscription ownership is session, subscription, and generation]]
- [[terminal subscription lifecycle is Core owned while host session policy is Hub owned]]
- [[Core terminal protocol separates Hub-safe envelopes from client semantic bodies]]
- [[Git-consumed Hub members pin Core protocol by exact revision]]
- [[Cargo Git URL and selector form are part of crate identity]]
- [[pin rolls update live lane provenance defaults and README pin prose]]
- [[TUI live Ghostty has IsolatedHub ghostty plus attach-only ghostty-shared and ghostty-shared-exit]]
- [[TUI contract matrix headless echo can time out after successful Hello]]
- [[tui and browser are equal clients]]
- [[vault example paths are not repository placement conventions]]

[[project-pipelines-playbook]] is not loaded. This ticket changes no Project
Pipelines package or plugin path.

## Context loaded

Target repository code:

- `crates/botster-tui/src/app.rs` — attach hydration, mux frame apply, terminal
  event apply, input dispatch, mode shadow, resize, live tests.
- `crates/botster-tui/Cargo.toml` — Hub and Core pins.
- `README.md` — the pin table and terminal contract prose.
- `script/test-live-hub` — live Hub lanes (`ghostty`, `ghostty-shared`,
  `ghostty-shared-exit`, `package-events`, `contract-matrix`).
- `script/test`, `script/fmt`, `script/clippy`, `test.sh`.

Dependency repositories (read only):

- `botster-hub` at `main` `b4020a976010f4ec495c89efd6ea66271e02712f`.
- `botster-core` at `main` `e5a927c31d5b7d0b0f4b198e5e556ed75d53ddf1`.

Key facts confirmed by reading the dependency sources:

1. Hub `main` deleted `DaemonRequest::SendInput`, `DaemonRequest::ModeGatedInput`,
   and `DaemonRequest::Resize`. The cold cut merged at `b1aab5d`. The current TUI
   Hub pin `baeb04d` is 208 commits behind and still has those requests.
2. Hub `main` exposes the client ingress seam:
   `DaemonConnection::send_terminal_frame(session_id, subscription_id, frame_bytes)`,
   which writes one `DaemonUnixTerminalEnvelope` and expects no paired response.
   `DaemonUnixTerminalEnvelope::from_frame_bytes` is public, and
   `botster_hub_client::write_frame` is generic over `Serialize`.
3. Hub `src/transport/unix/connection.rs` routes an inbound terminal envelope to
   `mux.live_handle(session_id, subscription_id).push_ingress(bytes)`. A missing
   live handle drops the envelope silently. A full ingress queue or an invalid
   base64 payload closes the subscription. Hub never decodes the payload.
4. Core `botster-terminal-protocol-client` owns the semantic input encoder:
   `TerminalInputCommand::{Input, ModeGatedInput, Resize}` and
   `encode_terminal_input` produce an opaque `TerminalInputFrame`.
   `TerminalInputCommand::Input` and `ModeGatedInput` carry `Vec<u8>`, not `String`.
5. Core adds `TerminalEvent::InputResult(TerminalInputResult)`. The result carries
   `subscription_id`, `kind`, `admitted`, `bytes_written`, `mode_generation`,
   `mode_revision`, a complete `TerminalModeFlags`, and an optional
   `TerminalInputRejection` of `StaleMode`, `PartialWrite`, `Timeout`, or
   `SessionNotWritable`.
6. Hub `main` reports protocol version 8 and conformance fixture revision 47.
   The TUI floor constant is currently 44 and its Hub pin reports protocol 7.
7. Hub `main` pins Core `e5a927c31d5b7d0b0f4b198e5e556ed75d53ddf1` in every
   member manifest, including `botster-hub-client` and `botster-hub-test-support`.
8. `DaemonRequest::ReadModeFlags` and `DaemonModeFlags` survive the cold cut, so
   the existing freshness probe stays available.

Vault and project context:

- Project `project_1787600579_585482` freezes the ownership split and states
  "Browser and TUI input use the duplex Core adapter" as an acceptance item.
- The project forbids fallbacks and dual active terminal routes.
- Dependency `ticket_1787894427_525056` (Hub cold cut) is closed.
- Dependency `ticket_1787603671_590198` is closed as superseded.

## Scope

In scope, all inside `botster-tui`:

1. Roll the Hub pin to a `botster-hub` `main` commit that contains the cold cut,
   and roll the Core pins to the exact Core revision that Hub commit pins.
   Candidate pins: Hub `b4020a976010f4ec495c89efd6ea66271e02712f`, Core
   `e5a927c31d5b7d0b0f4b198e5e556ed75d53ddf1`, `@trybotster/hub-test-support@0.1.42`.
2. Add one TUI-owned duplex input path. The path encodes a
   `TerminalInputCommand` with `encode_terminal_input`, wraps the frame bytes in
   `DaemonUnixTerminalEnvelope::from_frame_bytes`, and writes it on the live
   attached Unix mux connection.
3. Route typed keys, pasted bytes, mouse reports, and mode-gated input through
   that path as `Input` or `ModeGatedInput`.
4. Route terminal resize through that path as `TerminalInputCommand::Resize`.
5. Delete every TUI use of `DaemonRequest::SendInput`,
   `DaemonRequest::ModeGatedInput`, and `DaemonRequest::Resize`, including the
   `ObservedRequest` test variants and the live-test call sites.
6. Replace the synchronous `apply_mode_gated_input_response` flow with an
   event-driven `TerminalEvent::InputResult` handler that refreshes the mode
   shadow and reacts to `StaleMode`.
7. Carry terminal input as `Vec<u8>` from `InputDispatch::TerminalForward` to the
   encoder, and delete the `String::from_utf8` gate that only existed to fill a
   JSON request field.
8. Raise `MINIMUM_CONFORMANCE_FIXTURE_REVISION` to the revision reported by the
   chosen Hub pin, and update the Hello protocol assertions that name protocol 7
   and revision 44.
9. Update `README.md` pin prose, the pin table, and the Ghostty live provenance
   defaults in `app.rs` in the same commit as the pin roll.
10. Add the tests listed under Acceptance checks.

Out of scope:

- Any change in `botster-hub`, `botster-core`, `botster-tui-kit`, or
  `botster-web`.
- A JSON terminal input fallback, a feature flag, or a compatibility shim. The
  ticket and the project both forbid a second active route.
- WebRTC transport work. The TUI is a Unix client.
- Terminal rendering, projection, scrollback, selection, or Ghostty decode
  changes beyond what the byte-type change forces.
- Package-event, entity, notice, workspace, or session-type behavior.
- Broad refactors of `app.rs` structure.

## Repository ownership boundaries and cross-repository dependencies

| Concern | Owner |
| --- | --- |
| Terminal subscription identity, generations, attach phases, ordering, pressure, gated input scheduling, input results | `botster-core` |
| Semantic input encoding and event decoding contract | `botster-core` (`botster-terminal-protocol-client`) |
| Admission, grants, Unix mux framing, ingress routing, subscription close events | `botster-hub` |
| Client input policy, key and mouse encoding, mode shadow, resize ownership, retry policy above the wire | `botster-tui` (this run) |
| Reusable renderer and input routing mechanics | `botster-tui-kit` |

Boundary rules this plan keeps:

- The TUI never asks Hub to interpret terminal bytes. It sends opaque frames.
- The TUI does not re-implement Core's input encoding. It calls
  `encode_terminal_input`.
- The TUI keeps Core mechanism tokens in the terminal compatibility requirement
  only, never in host `required_features`.

Cross-repository dependencies:

- `ticket_1787894427_525056` (`botster-hub`, closed) delivered the Hub cold cut
  and the client ingress seam. It is already registered as a dependency.
- `ticket_1787603671_590198` (`botster-hub`, closed, superseded) is already
  registered.
- No new dependency ticket is required. Both open sibling Hub and Core tickets
  (`ticket_1788206393_323469`, `ticket_1788112223_631570`,
  `ticket_1787894967_973951`) are follow-ups on top of the merged cold cut. This
  run pins an exact commit and does not wait for them.

## Assumptions and unknowns

Assumptions:

1. The TUI may write client-to-Hub terminal envelopes directly on its own
   `HubConnection` Unix stream with `botster_hub_client::write_frame`. The TUI
   does not use `DaemonConnection`, and `send_terminal_frame` is only the
   reference implementation of the same wire shape.
2. Hub silently drops an envelope whose `(session_id, subscription_id)` has no
   live handle. The TUI therefore gates every write on the live attached
   subscription and treats absence of an `input_result` as normal, not as an
   error.
3. Input becomes fire-and-forget at the wire. The user-visible outcome moves from
   a synchronous response to an asynchronous `input_result` frame plus echoed
   output. The TUI keeps at most one stale-mode retry per submitted command.
4. `ReadModeFlags` remains the initial freshness probe. `input_result` refreshes
   the shadow afterwards.
5. Raising the conformance floor to the pinned Hub's revision is correct. The
   protocol-version equality gate already rejects the older Hub, so the floor
   raise adds no new exclusion.

Unknowns for Implement to resolve, with the default the plan assumes:

1. Exact Hub and Core pins. Implement re-checks that the candidate pins are still
   `main` ancestors and that the Hub commit pins the Core revision verbatim. If
   Hub `main` has moved, Implement rolls both pins forward in lockstep and
   records the exact SHAs.
2. Whether `PartialWrite` deserves a retry of the unwritten tail. Default: report
   it as an error and do not retry, because Core owns write budgets and the
   ticket does not ask for a resend policy.
3. Whether the pending-input queue held during attach hydration should keep
   command identity (`Input` versus `ModeGatedInput`). Default: queue raw bytes
   and classify at release time, which is the current behavior.

If Implement finds that a duplex write cannot be gated on a live subscription
without a new Hub or Core surface, Implement must ask a human rather than add a
JSON fallback.

## Affected surfaces and files

| File | Change |
| --- | --- |
| `crates/botster-tui/Cargo.toml` | Hub and Core pin roll in lockstep. |
| `Cargo.lock` | Regenerated by the pin roll. |
| `crates/botster-tui/src/app.rs` | Duplex input path, resize path, `InputResult` handler, mode shadow refresh, byte-typed input, deleted JSON request paths, floor and protocol assertions, live-lane provenance defaults, new and migrated tests. |
| `README.md` | Pin table, live Hub prose, terminal input contract prose. |
| `docs/plans/tui-send-terminal-input-through-the-core-duplex-adapter-plan.md` | This plan. |
| `docs/reports/…-implement-report.md` | Implement evidence report, per repository prior art. |

Named code sites in `app.rs`:

- `HubConnection` — add one `send_terminal_input_frame` method.
- `InputDispatch::TerminalForward` — stop converting bytes to `String`.
- `InputDispatch::TerminalResize` — replace `DaemonRequest::Resize`.
- `handle_focused_terminal_key` — replace the `SendInput` branch.
- `forward_terminal_input`, `forward_mode_gated_input`,
  `apply_mode_gated_input_response` — rewrite as encode plus write, and move
  outcome handling to the event path.
- `apply_terminal_event` — add the `TerminalEvent::InputResult` arm.
- `open_attach_live_path` — release the queued resize and queued input through
  the duplex path.
- `AttachHydration.pending_input` — hold `Vec<u8>`.
- `ObservedRequest` — delete `SendInput`, `ModeGatedInput`, and `Resize`; add an
  observed duplex-frame record so tests can assert the encoded command.
- `MINIMUM_CONFORMANCE_FIXTURE_REVISION` and the protocol 7 / revision 44 tests.
- The live tests near `headless_live_runtime_ghostty_…`, the shared Ghostty
  lanes, and the socket-cut sibling test.

## Risks

| Risk | Mitigation |
| --- | --- |
| A 208-commit Hub pin roll changes unrelated DTOs and breaks compilation or tests far from terminal input. | Roll the pin first as its own commit, run the full gate set, and fix fallout before the input change. Keep the two concerns in separate commits. |
| Input silently disappears when the subscription is not live, because Hub drops the envelope without a response. | Gate every write on `attached_session` plus `attached_subscription_id == self.subscription_id`, surface a client error when the gate fails, and prove the stale case with a test. |
| Losing the synchronous error surface degrades user feedback for rejected input. | Handle `input_result` with `admitted == false`, map each `TerminalInputRejection` to a distinct message, and assert those messages in tests. |
| A stale-mode retry loop, because the retry itself can be rejected. | Allow at most one re-probe and one retry per submitted command, exactly as the current `allow_reprobe` flag does. Assert the bound in a test. |
| Byte-type change breaks paste fidelity or the queued-input order. | Keep one ordered queue, keep bytes unmodified end to end, and assert exact bytes for a multi-byte paste. |
| `TerminalInputResult` has no `session_id`, so a result cannot be matched by session. | Match on `subscription_id` alone against the live attached subscription, and ignore results for retired subscription ids. |
| Live Ghostty lanes fail for environment reasons and hide a real regression. | Use the IsolatedHub `ghostty` lane as the primary live oracle and require the printed completion markers, per the repository charter. |
| Hub `main` moves before Implement, so the plan's SHAs go stale. | Implement re-verifies ancestry and records the exact SHAs it used in the Implement report. |
| Resize regression, because resize now rides the terminal plane rather than a request with a response. | Keep the client-side size owner unchanged, keep the "latest queued resize only" rule during hydration, and prove the applied geometry through the worker PTY echo in the live lane. |

## Runtime-teardown class answers

`teardown_class_applies`: yes. The ticket changes the live terminal data plane,
subscription-scoped ownership, stale subscription closure, and the divergence
between client terminal state and the live runtime.

`teardown_isolation`: the ownership set is one `(session_id, subscription_id)`
pair on one Unix mux connection. A rejected or dropped input affects only that
subscription. Hub closes only the offending subscription handle when ingress
overflows; other subscriptions and the host control plane on the same connection
keep working. The TUI holds exactly one terminal subscription at a time, so it
has no terminal siblings of its own, and it must not tear down the host control
connection when a terminal input fails.

`teardown_bounds`: a duplex input write is a bounded socket write on the existing
connection. There is no `block_on` and no new thread. The TUI keeps its existing
detach deadline for control requests. A write error maps to the existing
transport-error path, which already closes the connection hard. Stale-mode
handling is bounded to one re-probe plus one retry. No unbounded wait is added,
and no code waits for an `input_result` that may never arrive.

`late_message_matrix`:

| Message | Direction | Owner tag | Rejection after terminal failure | Residual sweep |
| --- | --- | --- | --- | --- |
| `Input` frame | TUI to Hub | `(session_id, subscription_id)` in the envelope | TUI refuses to write when the pair is not the live attached pair; Hub drops an envelope with no live handle | none needed; the frame creates no durable client state |
| `ModeGatedInput` frame | TUI to Hub | same pair plus `mode_generation` and `mode_revision` | Core rejects a stale generation or revision with `StaleMode` | the mode shadow is cleared on detach and on subscription close |
| `Resize` frame | TUI to Hub | same pair | same live-pair gate | the queued resize is dropped when hydration is abandoned |
| `input_result` | Hub to TUI | `subscription_id` | ignored when the id is retired or is not the live attached subscription | `retired_subscription_ids` already suppresses late frames |
| `TerminalSubscriptionClosed` | Hub to TUI | `(session_id, subscription_id)` | already handled as the adapter-close signal | retires the subscription id and clears the mode shadow |
| `Attach` and `Detach` | TUI to Hub | control plane, unchanged | unchanged | unchanged |

`production_path_proof`: the exact path is real `KeyEvent` or paste bytes to
`InputDispatch::TerminalForward`, to `forward_terminal_input`, to
`encode_terminal_input`, to the `DaemonUnixTerminalEnvelope` write on the live
Unix stream, to Hub `push_ingress`, to the Core adapter, to the PTY, and back as
`TerminalOutput`. The live oracle is `script/test-live-hub ghostty`, which drives
real input handlers against an IsolatedHub and asserts the echoed marker in the
Ghostty viewport cache. Hermetic tests must drive `handle_dispatch` and
`handle_focused_terminal_key`, not inner helpers, per the repository charter.

`ownership_identity`: the owner id is the `(session_id, subscription_id)` pair for
writes, and `subscription_id` alone for `input_result`, because Core does not
stamp a session on that event. A reconnect mints a new subscription id, so a late
`input_result` from a retired id must not update the mode shadow of a live
subscription. This is proven by a test that feeds a retired-id `input_result`
after a reconnect and asserts the live shadow and the live error state are
unchanged.

`sibling_fail_closed_policy`: on a successful terminal-subscription close, the
host control connection and every non-terminal subscription keep working. On an
input write failure, the TUI records a transport error through the existing path
and does not shut down the session or other subscriptions. The TUI never sends
`ShutdownSession` in response to an input failure. The existing socket-cut test
already asserts that a sibling client keeps echoing after the TUI connection
dies; that test migrates to the duplex path and keeps its assertion.

## Acceptance checks and tests

Repository gates:

1. `./script/fmt` — clean.
2. `./script/clippy` — clean, strict.
3. `./script/test` — full workspace tests pass.
4. `cargo build -p botster-tui --locked` — a separate production build gate, run
   after the pin roll. `cargo test --no-run` is not production build evidence.
5. `cargo tree` shows one Core revision and one Hub revision across direct and
   indirect paths.
6. A repository-wide search for the old revisions `baeb04d` and `7eafa47` returns
   only historical report text, never a live pin, default, or README claim.

New hermetic tests in `crates/botster-tui/src/app.rs`:

1. Typing through the real focused-key handler writes one duplex envelope whose
   decoded frame is `TerminalInputCommand::Input` with the exact bytes, and
   records no `DaemonRequest`.
2. A Kitty-enabled session writes `TerminalInputCommand::ModeGatedInput` with the
   shadow `mode_generation` and `mode_revision`.
3. A mouse report without ModeFlags freshness does not fall through to a plain
   `Input` command.
4. Paste of a multi-byte, non-UTF-8-safe sequence writes the exact bytes with no
   replacement characters.
5. `InputDispatch::TerminalResize` writes `TerminalInputCommand::Resize` with the
   exact rows and columns, and updates the local projection.
6. Attach hydration queues input and only the latest resize, then releases them
   in order through the duplex path once READY, FINISH, and `attached` are all
   seen.
7. An `input_result` with `rejection: StaleMode` triggers exactly one re-probe and
   one retry, and a second `StaleMode` produces a user-visible error with no
   further writes.
8. An `input_result` with `admitted: true` refreshes the mode shadow from
   `TerminalModeFlags`, `mode_generation`, and `mode_revision`.
9. An `input_result` for a retired subscription id is ignored and does not change
   the live mode shadow or the error state.
10. A write attempted while the current subscription is not the attached
    subscription produces the existing client error and writes nothing.
11. `TerminalSubscriptionClosed` for the live pair retires the subscription,
    clears the mode shadow, and blocks later writes for that pair.
12. Reconnect mints a new subscription id and the first input after reconnect
    uses that new id.
13. The Hello test asserts the new protocol version and the new conformance floor,
    and rejects a Hub that reports a lower revision.
14. No test and no production path constructs `DaemonRequest::SendInput`,
    `DaemonRequest::ModeGatedInput`, or `DaemonRequest::Resize`. These variants
    no longer exist at the new Hub pin, so this is compiler-enforced; the
    Implement report states that explicitly.

Live proof, per the repository charter:

1. `./script/test-live-hub ghostty` prints `ghostty-live-complete` and its
   provenance line names the new Hub and Core revisions. This lane must cover
   typing, the Kitty branch, the mouse branch, resize with an echoed geometry
   check, and the socket-cut plus reconnect sequence.
2. `./script/test-live-hub ghostty-shared` prints `ghostty-shared-complete`.
3. `./script/test-live-hub ghostty-shared-exit` prints
   `ghostty-shared-exit-attached` before the caller ends the shared session, then
   `ghostty-shared-exit-complete`.
4. `./script/test-live-hub package-events` prints `package-events-live: complete`,
   because the pin roll moves the Hub that owns package-event projection.
5. `./script/test-live-hub contract-matrix` runs as the Hello and compatibility
   lane. A headless echo timeout after a successful Hello is a known lane
   limitation; Ghostty remains the live-attach oracle when that lane matches base.

Independent base re-verification: before attributing any failure to this change,
Implement runs the same gate on `main` at `3b84d57` with the old pins and records
both results.

## Vault gaps worth capturing

1. The Unix client-to-Hub terminal ingress contract from a client's view: Hub
   drops an envelope with no live handle, closes the subscription on ingress
   overflow, and never sends a paired response. A first-party client must
   therefore gate every write on the live attached pair and must not wait for an
   acknowledgment. No current note states the client-side consequence.
2. `TerminalInputResult` carries no `session_id`, so client correlation is
   subscription-only. [[every TerminalInputResult must stamp the live subscription id]]
   states the producer duty; the consumer duty is not captured.
3. Duplex terminal input removes the synchronous request-response error surface
   from first-party clients, so client input error reporting becomes event
   driven and bounded to one stale-mode retry. This is a client-policy decision
   worth a note once it ships in both the TUI and the Web client.

Implement should capture these only after the change is proven, and should record
the capture path in the vault checklist.
