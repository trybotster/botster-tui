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
   Candidate pins after both prerequisites closed: Hub `bb1a330` (`main` HEAD,
   which pins Core `48a437032791e678010254708259568ce4ad02bf`), that same Core
   revision, and `@trybotster/hub-test-support@0.1.43`. Hub reports protocol 8
   and conformance fixture revision 48. Implement re-verifies ancestry and
   lockstep at the moment it rolls the pins.
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
   shadow and reacts to `StaleMode`, correlated through the bounded in-flight
   queue described under "Input result correlation".
6a. Bound every duplex write with `TERMINAL_INPUT_WRITE_BOUND`, and hard-close
   plus report a transport error on timeout.
7. Carry terminal input as `Vec<u8>` from `InputDispatch::TerminalForward` to the
   encoder, and delete the `String::from_utf8` gate that only existed to fill a
   JSON request field.
7a. Route every bracketed paste through the published `encode_paste` transaction
   helper, correlate its one authoritative result by `operation_id`, allow one
   in-flight paste per subscription with a single safe retry, and map the four
   added rejections. Check non-paste payloads against the imported single-frame
   ceilings before encode, write, and queue insertion. Add no client chunk policy.
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

- **`ticket_1788287678_207209` (`botster-core`, closed).** Published the bounded
  atomic terminal input transaction and its Rust and TypeScript helpers.
- **`ticket_1788313897_932611` (`botster-hub`, closed).** Pinned the paste frame
  kinds so Hub ingress accepts them, and proved live multi-frame paste over the
  Unix and WebRTC adapters.
- Both prerequisites are closed, so this ticket is unblocked.

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
3. Input becomes fire-and-forget at the wire, but not uncorrelated. The
   user-visible outcome moves from a synchronous response to an asynchronous
   `input_result` frame plus echoed output. The TUI correlates each result to a
   submitted command through the bounded in-flight queue, and keeps at most one
   stale-mode retry per command. The write itself is deadline bounded.
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
- Three new constants beside `DETACH_ON_DISCONNECT_BOUND`:
  `TERMINAL_INPUT_WRITE_BOUND` of two seconds,
  `TERMINAL_INPUT_INFLIGHT_CAPACITY` of 64, and
  `TERMINAL_INPUT_INFLIGHT_BYTES` of 262,144.
- Imported Core ceilings `MAX_INPUT_DATA_BYTES` and `MAX_MODE_GATED_DATA_BYTES`
  from `botster-terminal-protocol-client`, never hardcoded.
- A new bounded in-flight input queue field, cleared with the mode shadow on
  detach, close, and reconnect.
- `MINIMUM_CONFORMANCE_FIXTURE_REVISION` and the protocol 7 / revision 44 tests.
- The live tests near `headless_live_runtime_ghostty_…`, the shared Ghostty
  lanes, and the socket-cut sibling test.

## Risks

| Risk | Mitigation |
| --- | --- |
| A 208-commit Hub pin roll changes unrelated DTOs and breaks compilation or tests far from terminal input. | Roll the pin first as its own commit, run the full gate set, and fix fallout before the input change. Keep the two concerns in separate commits. |
| Input silently disappears when the subscription is not live, because Hub drops the envelope without a response. | Gate every write on `attached_session` plus `attached_subscription_id == self.subscription_id`, surface a client error when the gate fails, and prove the stale case with a test. |
| Losing the synchronous error surface degrades user feedback for rejected input. | Handle `input_result` with `admitted == false`, map each `TerminalInputRejection` to a distinct message, and assert those messages in tests. |
| A stale-mode retry loop, because the retry itself can be rejected. | Allow at most one re-probe and one retry per submitted command. Assert the bound in a test. |
| An asynchronous `input_result` cannot name the command it answers, so a stale-mode retry could resend the wrong bytes. | Keep a bounded ordered in-flight queue per live subscription and correlate the head entry, with a `kind` cross-check and a fail-closed path on mismatch. See "Input result correlation" below. |
| Byte-type change breaks paste fidelity or the queued-input order. | Keep one ordered queue, keep bytes unmodified end to end, and assert exact bracketed-paste bytes driven from a real `Event::Paste`. Kit `dispatch_paste` wraps a Rust `String`, so paste bytes are always valid UTF-8 at the kit boundary; the byte-typed pipeline matters for key and mouse encoding, not for reaching non-UTF-8 paste input. |
| `TerminalInputResult` has no `session_id`, so a result cannot be matched by session. | Match on `subscription_id` alone against the live attached subscription, and ignore results for retired subscription ids. |
| Live Ghostty lanes fail for environment reasons and hide a real regression. | Use the IsolatedHub `ghostty` lane as the primary live oracle and require the printed completion markers, per the repository charter. |
| Hub `main` moves before Implement, so the plan's SHAs go stale. | Implement re-verifies ancestry and records the exact SHAs it used in the Implement report. |
| The duplex write timeout leaks onto the shared stream and silently bounds later control-plane writes. | Restore `set_write_timeout(None)` on the success path and rely on `hard_close` for the failure path. Test 20 asserts the restored state. |
| The timeout restore itself fails, leaving the shared control stream in an uncertain state that is then reported as success. | Treat a failed restore as a write failure: `hard_close` and record a transport error. Test 21 asserts it. |
| The entry count alone does not bound memory, because each entry retains its exact submitted bytes. | Add `TERMINAL_INPUT_INFLIGHT_BYTES` of 256 KiB as the binding limit against a 4,194,240-byte worst case, enforced before the write and the queue insertion. Test 22 asserts it. |
| Large paste behavior depends on a contract this repository does not own. | Consume the published `encode_paste` helper and correlate by `operation_id`. Core validates the whole operation before delivery and delivers zero PTY bytes on failure, so a partial bracketed paste cannot originate here. Tests 23 to 30 assert the consumption, not a local implementation. |
| The client in-flight bound drifts above Core's `INPUT_QUEUE_CAPACITY`, so Core hard-stops the subscription before the client fails soft. | Keep `TERMINAL_INPUT_INFLIGHT_CAPACITY` at 64 against Core's 256, and require Implement to re-read Core's constant at the chosen pin. Test 19 proves the soft-fail path. |
| Resize regression, because resize now rides the terminal plane rather than a request with a response. | Keep the client-side size owner unchanged, keep the "latest queued resize only" rule during hydration, and prove the applied geometry through the worker PTY echo in the live lane. |

## Input result correlation

`TerminalInputResult` carries no command id and no sequence number. It carries
`subscription_id`, `kind`, `admitted`, `bytes_written`, `mode_generation`,
`mode_revision`, `mode_flags`, and an optional `rejection`. A retry policy that
reacts to `StaleMode` therefore needs a correlation rule, because more than one
command can be in flight on one subscription.

Correlation rule:

1. The TUI keeps one ordered in-flight queue for each live subscription. It
   pushes an entry on every successful duplex write. The entry holds the command
   kind, the exact submitted bytes, the freshness tokens used, and a retry flag.
2. Each `input_result` for the live subscription pops the head entry.
3. The TUI cross-checks `result.kind` against the head entry's kind. On a
   mismatch the TUI does not retry. It clears the whole in-flight queue, records
   a distinct client error, and requires a fresh ModeFlags probe before the next
   gated write. Fail closed beats resending the wrong bytes.
4. Only a popped entry whose `retry` flag is unused may be resent, and only once,
   after one ModeFlags re-probe.
5. The queue is bounded by two limits, both enforced before the socket write and
   before queue insertion. An entry count limit,
   `TERMINAL_INPUT_INFLIGHT_CAPACITY`, set to 64; and a retained-byte limit,
   `TERMINAL_INPUT_INFLIGHT_BYTES`, set to 262,144 (256 KiB). When either limit
   would be exceeded the TUI refuses the write, records a distinct back-pressure
   error, and drops the input rather than losing correlation.

   The byte limit is the binding one, and the count alone is not sufficient. Each
   entry retains the exact submitted bytes so a stale-mode retry can resend them.
   `MAX_INPUT_DATA_BYTES` is 65,535 at the pinned Core contract, so 64 maximum
   `Input` payloads would retain up to 4,194,240 bytes with the count limit alone.
   256 KiB is far above any realistic in-flight burst and well below that worst
   case.

   The entry count of 64 stays far below Core's `INPUT_QUEUE_CAPACITY`, which is
   256 at the candidate pin and hard-stops the owner when exceeded, so the client
   fails soft before Core closes the subscription. Core does not re-export
   `INPUT_QUEUE_CAPACITY` from `botster_core::engine` at the candidate pin, so the
   TUI keeps a local constant with a comment naming Core's value, and Implement
   re-reads Core's constant at the chosen pin to confirm the client bound stays
   lower.

6. The queue is cleared on detach, on `TerminalSubscriptionClosed`, on reconnect,
   and whenever the live subscription id changes.

This rule rests on one stated Core assumption, verified by reading Core
`managed_session_runtime.rs` and `client_worker.rs` at the candidate pin:

- Core enqueues exactly one `input_result` per admitted command onto one ordered
  per-owner egress queue, so results arrive in submission order for one
  subscription.
- A result is never silently lost while the subscription stays live. Every
  `enqueue_input_result` failure path calls `detach_live` or
  `owner_apply_teardown_outcome`, so a lost result becomes a subscription close.
  `Malformed` and `QueueOverflow` are deliberately unpublished for the same
  reason: close is the report.
- A gated command parks later input for the same owner, so one owner has at most
  one outstanding gated wait.

Implement must prove the assumption rather than trust it. The `kind` cross-check
in step 3 is the guard that keeps the client correct if Core ever reorders or
drops a result, and Implement must add a test that drives a mismatched result and
asserts the fail-closed path.

## Oversized input and the paste ceiling

Core caps one frame body. `MAX_INPUT_DATA_BYTES` is 65,535 and
`MAX_MODE_GATED_DATA_BYTES` is 65,519, both re-exported from
`botster-terminal-protocol-client`, so the TUI imports them and must never
hardcode either value.

The TUI must check the payload against the matching ceiling before it encodes,
writes, or enqueues. `encode_terminal_input` also returns `PayloadTooLarge`, but
the client check must come first so an oversized payload never reaches the socket
or the in-flight queue.

The ceiling is reachable, not theoretical. Today a large paste travels as one
JSON `SendInput` with no client-side size limit, so a paste above 64 KiB works.

### Decision: Core owns the transaction, the TUI consumes the published helper

Revision 4 of this plan put a chunk policy in the TUI. Revision 5 offered the
human three client-side policies. The answer to `question_1788282946_225545`
rejected all of them and settled ownership instead. The durable rule is
[[core owns bounded atomic terminal input transactions across clients]].

Both prerequisites have since closed:

- `ticket_1788287678_207209` (`botster-core`) published the transaction.
- `ticket_1788313897_932611` (`botster-hub`) pinned the paste frame kinds and
  proved live multi-frame paste over the Unix and WebRTC adapters. Hub ingress
  validates frame headers against the pinned protocol crate, so this pin was
  required before any client could send the new kinds.

### The published contract

`botster-terminal-protocol-client` at Core `48a4370` exposes:

- `encode_paste(operation_id, mode_generation, mode_revision, data)` returning an
  ordered `Vec<TerminalInputFrame>`: one `PasteBegin`, the `PasteChunk` frames,
  then one `PasteCommit`.
- `encode_paste_abort(operation_id)` returning one frame.
- `MAX_PASTE_BYTES` of 1,048,576, `MAX_PASTE_CHUNK_DATA_BYTES`, and
  `MAX_PASTE_CHUNKS`.
- `TerminalInputKind::Paste`, and `TerminalInputResult.operation_id:
  Option<u32>`, so one authoritative result identifies its operation.
- Four added rejections: `OperationInFlight`, `OperationOutOfBounds`,
  `OperationIncomplete`, and `Aborted`.

Core validates the complete operation before any PTY delivery and delivers zero
PTY bytes on every failure, so a partial bracketed paste is impossible by
construction. Hub stays content blind.

### How the TUI consumes it

1. **Every bracketed paste uses the transaction.** The TUI does not branch on
   size. One path removes the threshold, and the result's `operation_id` gives
   explicit correlation for small and large pastes alike. The TUI writes the
   frames the helper returns, in order, on the live subscription, and defines no
   chunk policy, no scheduling, and no reassembly of its own.
2. **Paste correlation is by `operation_id`, not by queue position.** A paste
   in-flight entry records its `operation_id`, and a `Paste` result pops that
   entry by id match. Non-paste commands keep the ordered head correlation and
   the `kind` cross-check from the correlation section above.
3. **One paste in flight per subscription.** Core publishes `OperationInFlight`,
   so the TUI refuses a second paste while one is outstanding and reports
   back-pressure rather than sending a guaranteed rejection.
4. **Paste retry is now safe, and is kept to one attempt.** Because every failure
   path delivers zero PTY bytes, a `StaleMode` paste may be retried once with
   refreshed mode tokens and a **new** `operation_id`. This is strictly safer than
   the withdrawn chunk design, where a retry could reorder bytes at the PTY.
5. **Abort.** When the TUI abandons an in-flight paste while the subscription is
   still live, it sends `encode_paste_abort(operation_id)` and expects the
   `Aborted` rejection as the authoritative result. It sends no abort when the
   subscription is already retired, because the operation dies with the owner.
6. **Payloads above `MAX_PASTE_BYTES`.** `encode_paste` returns `PayloadTooLarge`.
   The TUI reports that ceiling as Core's declared contract. This is not a client
   policy and not a temporary compatibility rejection.
7. **Bounds.** A paste is accounted separately from the non-paste in-flight
   budget: at most one in-flight paste retaining at most `MAX_PASTE_BYTES` for its
   single retry, alongside `TERMINAL_INPUT_INFLIGHT_BYTES` of 262,144 for
   non-paste commands. The worst-case retained total is therefore bounded and
   stated rather than derived at runtime.
8. **Rejection reporting.** `OperationInFlight`, `OperationOutOfBounds`,
   `OperationIncomplete`, and `Aborted` each map to a distinct client message,
   alongside the four pre-existing rejections.

### Retained invariant

The framing invariant is now a Core guarantee rather than TUI logic: the opening
bracketed-paste marker never reaches the PTY unless the closing marker does,
because Core validates the whole operation before delivery and delivers zero
bytes on failure. The TUI relies on that guarantee and adds no framing logic.

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

`teardown_bounds`: a duplex input write must carry an explicit deadline. A plain
`write_frame` on the TUI `HubConnection` stream inherits `set_write_timeout(None)`
from the request path, so a Hub that stops reading can block the TUI event loop
forever once the socket send buffer fills. The plan therefore adds a
`TERMINAL_INPUT_WRITE_BOUND` constant of two seconds, matching the existing
`DETACH_ON_DISCONNECT_BOUND`, and sets a write timeout before every duplex write.

The stream is shared with the control plane, so the write timeout must not leak.
`HubConnection::request` sets only a read timeout and never a write timeout, so a
duplex write that left `set_write_timeout(Some(..))` in place would silently bound
every later control-plane write. The duplex write therefore restores
`set_write_timeout(None)` on the success path, exactly as `request_with_deadline`
already does. On the failure path it calls the existing
`HubConnection::hard_close`, which already clears both timeouts, and records a
transport error.

The restore call is itself fallible. If `set_write_timeout(None)` returns an
error after an otherwise successful write, the shared control stream is left in
an uncertain timeout state, so the method must not report success. It calls
`hard_close` and records a transport error, the same as a failed write. A stream
whose timeout state cannot be trusted is not usable for later control-plane
requests. This mirrors `DETACH_ON_DISCONNECT_BOUND` and
`request_with_deadline`, which already bound the Detach path. There is no
`block_on` and no new thread. Stale-mode handling is bounded to one re-probe plus
one retry per submitted command. No code waits for an `input_result` that may
never arrive; a missing result is reported by subscription close, never by a
blocking wait.

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
4. Paste drives the real kit path: the test calls
   `InputRouter::dispatch_event(Event::Paste(text), &hit_map)` with the terminal
   region focused, feeds the returned `InputDispatch` to `handle_dispatch`, and
   asserts one `TerminalInputCommand::Input` whose bytes are exactly
   `\x1b[200~` plus the multi-byte UTF-8 payload plus `\x1b[201~`. The test must
   not fabricate an `InputDispatch::TerminalForward` value.
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
15. Two commands in flight: a `StaleMode` result pops and retries the correct
    head entry, and the second command's later result is applied to the second
    entry. This proves the correlation rule rather than assuming it.
16. A result whose `kind` does not match the head entry clears the in-flight
    queue, records the distinct fail-closed error, and performs no retry.
17. Detach, `TerminalSubscriptionClosed`, and reconnect each clear the in-flight
    queue, so no entry survives into a new subscription.
18. A saturated socket bounds the duplex write. The test reuses the existing
    `spawn_detach_bound_stub(DetachStubMode::StopReadingAfterAttach)` plus
    `HubConnection::fill_send_buffer_until_blocked` pattern, drives a real key
    through `handle_dispatch`, and asserts the call returns inside
    `TERMINAL_INPUT_WRITE_BOUND` with the connection hard-closed and a transport
    error recorded. This is the same oracle as
    `bounded_detach_returns_when_peer_stops_reading`.
19. A full in-flight queue fails soft. The test submits
    `TERMINAL_INPUT_INFLIGHT_CAPACITY` commands with no `input_result` returned,
    then drives one more real key, and asserts no further duplex write, the
    distinct back-pressure error, an unchanged queue length, and a live
    subscription that is neither detached nor closed. It then delivers one
    `input_result`, drives another key, and asserts the write resumes.
20. A successful duplex write leaves no write timeout on the shared stream. The
    test drives a real key, then asserts the stream's write timeout is `None` so
    a later control-plane `request` is not silently bounded.
21. A failed timeout restore is not reported as success. The test forces
    `set_write_timeout(None)` to fail after a good write and asserts the
    connection is hard-closed and a transport error is recorded.
22. The retained-byte budget binds before the entry count. The test submits a few
    large `Input` payloads whose total exceeds `TERMINAL_INPUT_INFLIGHT_BYTES`
    while the entry count stays under `TERMINAL_INPUT_INFLIGHT_CAPACITY`, and
    asserts the back-pressure error, no further write, and a live subscription.
23. Every bracketed paste travels as one Core transaction. The test drives
    `Event::Paste` through `InputRouter::dispatch_event` with the terminal region
    focused, and asserts the written frames are exactly the ordered
    `encode_paste` output: one `PasteBegin` carrying the live mode tokens and the
    total length, the `PasteChunk` frames in index order, then one `PasteCommit`.
    It asserts the TUI builds no frames of its own.
24. A paste above one frame body uses the same single path. The test uses a
    payload above `MAX_PASTE_CHUNK_DATA_BYTES`, asserts the concatenated chunk
    payloads equal the original bracketed-paste bytes exactly, and asserts the
    TUI branches on no size threshold.
25. Paste results correlate by `operation_id`, not by queue position. The test
    puts a paste and a later key in flight, delivers the `Paste` result with its
    `operation_id`, and asserts it pops the paste entry while the key entry stays
    pending.
26. One paste in flight per subscription. A second paste while one is outstanding
    is refused with the back-pressure error, writes nothing, and leaves the
    subscription live.
27. A `StaleMode` paste result is retried exactly once, with refreshed mode tokens
    and a new `operation_id`. A second `StaleMode` reports the error with no
    further writes.
28. Each added rejection maps to a distinct message: `OperationInFlight`,
    `OperationOutOfBounds`, `OperationIncomplete`, and `Aborted`.
29. Abandoning an in-flight paste on a live subscription writes
    `encode_paste_abort(operation_id)` and treats the `Aborted` result as
    authoritative. Abandoning it on a retired subscription writes nothing.
30. A payload above `MAX_PASTE_BYTES` reports Core's `PayloadTooLarge` ceiling and
    writes zero frames.

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
3. [[core owns bounded atomic terminal input transactions across clients]] is the
   durable rule settled by `question_1788282946_225545`. Payloads larger than one
   frame body are a Core-owned bounded atomic transaction with one authoritative
   result, not a client chunk policy, and every failure delivers zero PTY bytes.
   The Core ticket owns the capture once the contract ships.
4. Duplex terminal input removes the synchronous request-response error surface
   from first-party clients, so client input error reporting becomes event
   driven and bounded to one stale-mode retry. This is a client-policy decision
   worth a note once it ships in both the TUI and the Web client.

Implement should capture these only after the change is proven, and should record
the capture path in the vault checklist.
