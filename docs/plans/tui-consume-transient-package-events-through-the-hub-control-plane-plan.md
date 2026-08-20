# Plan: TUI consume transient package events through the Hub control plane

Ticket: `ticket_1786663585_944018`
Run: `run_1787197986_912715`
Step: `botster_stack_plan`
Plan revision 1

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`trybotster/botster-tui`) |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Resolved from | `list_spawn_targets` (`name=botster-tui`, `repo_name=trybotster/botster-tui`) |
| Ambient worktree | pipeline worktree for this run; routing follows the ticket target, not the process cwd |
| Base | `origin/main` `dc7d6002c90dc6c565168df6328a032b640e9b48` |
| Branch | `project-pipelines/ticket_1786663585_944018` |
| `teardown_class_applies` | **no** — this ticket adds host-control event consumption and a transient notice. It does not change WebRTC/peer lifecycle, SessionIo/ClientWorker teardown, multi-peer ownership, FD/CPU spin paths, or terminal-state versus live-runtime divergence. |
| Session-type eligibility consumer | **false** — the parents are the Hub client-event ticket, the Project Pipelines emitter ticket, and the TUI protocol-split ticket, not Hub session-type eligibility. |
| Implement blocked on | none. All three dependencies are closed: Hub `ticket_1786663583_640263` (merged as Hub `7a09292`), Project Pipelines `ticket_1786663583_568924` (on PP main `beaba94`), TUI `ticket_1786661009_551067` (protocol split on TUI main). |

## Repository playbook loaded

- [[botster-tui-playbook]]

## Other playbooks and notes loaded

Role / stack: [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]].

TUI charter must-load notes applied here: [[botster tui consumes tui kit through a thin app policy adapter]], [[tui and browser are equal clients]], [[tui client attach uses hub protocol not session protocol]], [[first-party Unix attach clients use split Hello and subscription close events]], [[first-party clients put terminal mechanism tokens only in terminal compatibility]], [[Unix mux polling returns bounded complete-frame batches while input stays readable]], [[acceptance readiness requires the exact expected entity not any authoritative snapshot]], [[TUI live Ghostty has IsolatedHub ghostty plus attach-only ghostty-shared and ghostty-shared-exit]], [[TUI contract matrix headless echo can time out after successful Hello]], [[first-party Rust consumers pin the UI contract Git tag not a Hub rev]], [[Cargo Git URL and selector form are part of crate identity]], [[Git-consumed Hub members pin Core protocol by exact revision]], [[compatibility fixtures advertise every required optional feature]].

Task-surface event-plane notes: [[Client event subscriptions stay on the multiplexed host-control path]], [[Client event holders are connection-scoped]], [[Host package-event negotiation survives terminal admission rejection]], [[Fair host-control writing selects already-admitted frames]], [[exact owner plus name is the only package event subscription key]], [[Package-event subject filters are exact strings compiled at admission]], [[a transient package event cannot be the sole authority for a durable close]], [[additive daemon capabilities do not raise the default client requirement]], [[botster data plane bypasses the hub through session and client actors]].

Not loaded, with reasons: [[project-pipelines-playbook]] (no PP package or workflow-policy change in scope; PP is only the producer of an already-shipped contract), [[botster runtime teardown lenses]] (teardown class does not apply, see table above), [[spa-patterns]] (no browser surface), [[cli-patterns]] (preserved mixed index; [[botster-architecture]] marks it non-authoritative for current ownership).

## Context loaded

Shipped Hub contract at `7a09292cd518186e0def758c823c0841ee1cacf1` (merge of closed `ticket_1786663583_640263`; protocol 7, conformance fixture revision 44, `@trybotster/hub-test-support@0.1.39`):

- Feature token: `FEATURE_PACKAGE_EVENT_SUBSCRIPTIONS = "package_event_subscriptions"` (hub-client `lib.rs:67`). The daemon advertises it under `supported_features`; `DaemonCompatibilityRequirement::current()` does not require it. Helpers: `for_package_event_subscriptions()` (`lib.rs:837`, floor = revision 44), `hello_requires_package_event_subscriptions` (`lib.rs:969`).
- Requests: `DaemonRequest::SubscribeEvents { subscription_id, owner, name, subjects }` (`lib.rs:1031`) and `DaemonRequest::UnsubscribeEvents { subscription_id }` (`lib.rs:1038`). `subjects` is skipped on the wire when empty.
- Responses: `DaemonResponseKind::EventSubscribed` / `EventUnsubscribed` with empty bodies. The client-chosen `subscription_id` is the identity; failures return `OperatorError` with typed codes (`too_many_event_subscriptions`, `duplicate_event_subscription`, `package_event_subscriptions_not_negotiated`, `rejected_audience`, `shed_busy`, ...).
- Inbound frames: `DaemonEvent::PackageEvent { subscription_id, owner, name, payload }` and `DaemonEvent::EventGap { subscription_id, owner, name }` (`lib.rs:2971`, `lib.rs:2978`). `payload` is the producer's schema-validated JSON. There is no sequence, cursor, timestamp, replay, or history field. `EventGap` is a coalesced sticky bit, written before queued events, with no count.
- Unix demux: `parse_unix_mux_value` at this revision routes `package_event` and `event_gap` to `DaemonUnixMuxFrame::Event`. Event delivery stays on the multiplexed host-control connection; the exclusive entity subscription socket rejects `subscribe_events`.
- Hub admission needs the feature in Hello `compatibility.required_features` on the same connection. Negotiation survives an independent terminal-compatibility rejection. Holders are `(connection_id, subscription_id)`; connection cleanup drops only its own holders; reconnect gets no replay.
- Bounds: 64 subscriptions per connection, 16 subjects per subscription, per-connection consumer queue 128 events / 2 MiB, queue age 1,000 ms. Test knob: `BOTSTER_HUB_TEST_CLIENT_EVENT_QUEUE_MAX` (honored only under `BOTSTER_ENV=test`).
- Hub `7a09292` pins Core at `8fce2041b9fe742cb2a6df9e74cb262606672742` in every member manifest. The `botster-ui-contract-v0.3.2` tag remains the UI-contract identity.

Shipped Project Pipelines contract at `beaba94a8cb311c5138f6b7499915642fc6abfa2` (closed `ticket_1786663583_568924`, package `project-pipelines` 0.3.0):

- Event: owner `project-pipelines`, name `question.opened`, audience `["clients","plugins"]` (`botster-package.json:127-143`).
- Payload schema (`additionalProperties: false`): required `question_id` (≤128), `kind` (`human`|`agent`), `notice` (≤280 chars, the only human-presentable field); optional `blocking`, `run_id`, `step_id`, `ticket_id`. The schema has **no `subject` field**, so a non-empty `subjects` filter matches nothing. Clients must subscribe with an empty subject list and filter client-side.
- Emit happens only after the durable question commit (`plugin.lua:1624` inside `record_question`), wrapped in `pcall`. Durable recovery uses the `project-pipelines.question` entity provider, never event replay (`docs/domain-contract.md:296-312`).

Current TUI base `dc7d600`:

- Pins: hub-client and hub-test-support at Hub `e864c3c8` (predates the event work — no event types exist at this pin); Core set at `fd66efd` across `botster-core`, `botster-terminal-ghostty`, `botster-terminal-protocol-client`, `botster-core-test-support`; UI contract tag `botster-ui-contract-v0.3.2`.
- Host control plane: `HubConnection` (`app.rs:8918`) with persistent `mux_buf`, `poll_mux_frames` (1 ms timeout, 32-frame batch), `decode_complete_mux_frames`, `parse_unix_mux_value`. App fan-out: `TuiApp::apply_mux_frames` (`app.rs:3687`); the event branch `apply_mux_event` (`app.rs:3744`) today handles only `TerminalSubscriptionClosed` and ignores other events.
- Hello: `tui_compatibility_requirement()` (`app.rs:9227`) with `MINIMUM_CONFORMANCE_FIXTURE_REVISION = 40` (`app.rs:92`); terminal requirement is separate (`app.rs:9282`); connect path is `connect_and_hello_with_terminal_requirement` (`app.rs:8936`).
- Reconnect: `try_connect` (`app.rs:2165`) re-runs `refresh_read_models`, `start_session_subscription`, `start_session_type_subscription_if_supported`; every entity resubscribe mints a fresh subscription id; there is no replay or cursor anywhere.
- Transient UI: none exists. No toast, timer, or expiry mechanism. `connection_alert()` (`app.rs:5204`) rendered as a one-row band in `draw_workspace_shell` (`app.rs:1157`) is the structural precedent for an `Option<UiNode>` band. The run loop polls every ≤100 ms (`app.rs:1118`).
- Questions/attention: no question entity family, no Project Pipelines reference in Rust source.
- Boundary guard test: `tui_hub_boundary_uses_public_client_without_private_protocol_plumbing` (`app.rs:20039`) requires generated hub-client vocabulary and forbids private protocol plumbing.
- Live proof: `script/test-live-hub` modes map one exact test filter plus a completion sentinel; `IsolatedHubBuilder` drives hub/session-worker binaries; `skip_or_panic` gates on `BOTSTER_TUI_REQUIRE_HUB_TEST`.
- Docs: plans at `docs/plans/tui-<slug>-plan.md`, implement reports at `docs/reports/tui-<slug>-implement-report.md`; README pin table (README:30-40) moves in lockstep with `Cargo.toml`.

## Scope

Surgical TUI consumer change against Hub `7a09292` and PP `beaba94`. No Hub, Core, PP, or TUI Kit source changes.

1. **Pin roll (one lockstep set).** Bump `botster-hub-client` and `botster-hub-test-support` from `e864c3c8` to `7a09292cd518186e0def758c823c0841ee1cacf1`. Bump the Core set (`botster-core`, `botster-terminal-ghostty`, `botster-terminal-protocol-client`, `botster-core-test-support`) from `fd66efd` to `8fce2041b9fe742cb2a6df9e74cb262606672742` to match Hub member manifests ([[Git-consumed Hub members pin Core protocol by exact revision]]). Keep `botster-ui-contract` on tag `botster-ui-contract-v0.3.2`. Refresh `Cargo.lock`. Update the README pin table.
2. **Hello negotiation.** Add `FEATURE_PACKAGE_EVENT_SUBSCRIPTIONS` to `tui_compatibility_requirement()` required features and raise `MINIMUM_CONFORMANCE_FIXTURE_REVISION` from 40 to 44. Keep terminal compatibility untouched ([[first-party clients put terminal mechanism tokens only in terminal compatibility]]). Hub admission reads Hello `required_features`, so requiring the token on the main mux connection is the only way this connection can subscribe.
3. **Event subscription policy.** After a successful Hello in `try_connect`, send one `SubscribeEvents { subscription_id: "btui-events-{short_suffix()}", owner: "project-pipelines", name: "question.opened", subjects: vec![] }`. Store the id as the current event generation. Accept `EventSubscribed`; on `OperatorError`, record a bounded diagnostic and continue — event consumption is best-effort and must not fail the connection or the durable planes. Reconnect naturally re-subscribes with a fresh id and gets no replay.
4. **Inbound handling.** Extend `apply_mux_event` with two arms. `PackageEvent`: require an exact match on the current event subscription id, owner, and name; drop foreign-generation frames silently; parse the payload minimally (`notice` required; keep `question_id` and `kind`); set the transient notice (single slot, latest wins, O(1) per event). `EventGap`: require the exact id match; drop nothing durable and fabricate nothing transient; record a bounded gap diagnostic; do not request replay and do not resubscribe. Durable question and attention state stays on the package entity plane ([[a transient package event cannot be the sole authority for a durable close]]).
5. **Transient notice UI (app policy only).** New `transient_notice: Option<TransientNotice { text, question_id, kind, deadline: Instant }>` on `TuiApp` with a `TRANSIENT_NOTICE_TTL` constant (proposed 5 s). Render as an `Option<UiNode>` one-row `workspace-transient-notice` band in `draw_workspace_shell`, following the `connection_alert()` precedent, validated against `node.validate()` and `tui_capabilities().validate_node`. Expire by deadline check inside `poll_hub` (the ≤100 ms run-loop tick), not inside an event handler. No TUI Kit change; the band composes existing `UiNode` kinds through the thin adapter ([[botster tui consumes tui kit through a thin app policy adapter]]).
6. **Reconnect and teardown hygiene.** `force_reconnect`, `apply_transport_failure`, and connection loss clear the current event subscription generation and the transient notice. A reconnected session shows no old notice.
7. **Tests and live proof.** See Acceptance checks. Extend the boundary guard test vocabulary with the generated event types; add a `package-events` mode to `script/test-live-hub` with an exact filter and a `package-events-live: complete` sentinel.
8. **Docs.** Update README: pin table, foundation feature list, the optional-feature section, the live-lane list, and the "Not included yet" scope sentence that currently excludes Project Pipelines consumption. Add this plan file and, at Implement, the implement report under `docs/reports/`.

## Non-scope

- No Hub, Core, or Project Pipelines source change. The producer contract and the host control surface are shipped.
- No TUI Kit change. The kit stays policy-free; no toast primitive is added to the kit.
- No durable question or attention UI build-out. The durable plane already reaches the TUI through generic entity subscriptions and plugin surfaces; this ticket proves the durable row stays visible, it does not design a question workbench.
- No event replay, cursor, sequence, or history handling — the contract has none, and the TUI must not simulate one.
- No terminal-plane work: no Hub-specific terminal logic, no terminal frame scheduling, no terminal fairness. Event frames never touch `apply_unix_terminal_envelope`.
- No `pr_merged` consumption (audience is `["plugins"]` only — not client-visible).
- No subscription to additional owners/names, and no subject filters (the `question.opened` schema has no `subject` field).
- No WebRTC consumer work (that is the Web ticket `ticket_1786663584_427840`).

## Repository ownership boundaries and cross-repo dependencies

| Owner | Owns here | This ticket's stance |
| --- | --- | --- |
| botster-hub (`tgt_7e208a0c76a44980a83b63af976b1f22`) | Event admission, routing, egress bounds, gap semantics, fair host-control writing, generated client types | Consume at pinned `7a09292`. No change. |
| botster-project-pipelines (`tgt_a72ca1a83d504385b8648f71409119ab`) | `question.opened` contract, durable question records, `project-pipelines.question` entity provider | Consume at `beaba94`. No change. |
| botster-core (`tgt_1f7bce66eb304881980f9b4a2a5ae3fe`) | Terminal plane, lifecycle journal | Pin follows Hub lockstep (`8fce204`). No change. |
| botster-tui-kit (`tgt_3dfae49c02454037bf13554f552baf7f`) | Reusable render/input mechanics | Unchanged; notice band is app-composed. |
| botster-tui (this repo) | Client event subscription policy, transient notice policy, reconnect behavior, live proof | All changes land here. |

Dependencies: all three ticket dependencies are closed; no new cross-repository prerequisite exists. If Implement finds a missing Hub or PP seam, stop and register a dependency ticket against that repository's target instead of widening this run.

## Assumptions and unknowns

Assumptions (Plan Review should challenge these):

- A1: Requiring `package_event_subscriptions` in the TUI's main Hello and raising the conformance floor to 44 is acceptable. The TUI is a first-party client whose README pin table moves in lockstep with the Hub; the repo already requires other optional daemon features (`terminal_subscription_closed`, `session_entity_subscriptions`). The alternative — a degraded no-events mode against older daemons — adds a dual path this ticket does not need. Consequence: shared live lanes (`ghostty-shared`, `ghostty-shared-exit`) need a caller Hub at ≥ `7a09292` after this change.
- A2: One event subscription (one owner+name) is enough for this ticket. The concrete case is `question.opened`; generic multi-subscription plumbing is speculative.
- A3: The transient notice is a single latest-wins slot with a fixed TTL. The ticket asks for "one transient notice" per live event; a notice queue is speculative.
- A4: The durable-question proof may use the existing generic entity subscription path (`start_entity_options_subscription` family plumbing) against `project-pipelines.question` in the live test, per [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]. No new durable product UI is required by the ticket text.

Unknowns for Implement to resolve (not blockers):

- U1: The minimal PP call sequence that emits `question.opened` inside an IsolatedHub. `project_pipelines_ask_human` via `DaemonRequest::PluginMcpCallTool` is the expected trigger; if it needs a project/ticket row first, create the minimal rows through PP MCP tools inside the test.
- U2: How the live lane locates the PP package checkout. Follow the installed-workspaces-driver precedent (caller-provided path environment variable, `EnablePackageLocalPath`, pin-floor check ≥ `beaba94`).
- U3: Whether the flood lane needs the synthetic `event-plane-producer` fixture (copy the Hub example package into TUI fixtures) or can reach flood/shed with PP alone plus `BOTSTER_HUB_TEST_CLIENT_EVENT_QUEUE_MAX`. Prefer the test knob with the real PP producer; add the synthetic fixture only if PP cannot emit fast enough.
- U4: Exact placement of the notice-expiry check (`poll_hub` head versus immediately before draw). Either satisfies the ≤100 ms tick; pick one and test it.

## Affected surfaces/files

- `crates/botster-tui/Cargo.toml`, `Cargo.lock` — pin roll (hub-client, hub-test-support, Core set).
- `crates/botster-tui/src/app.rs` —
  - `MINIMUM_CONFORMANCE_FIXTURE_REVISION` (`:92`) and `tui_compatibility_requirement()` (`:9227`): feature + floor;
  - `try_connect` (`:2165`): send `SubscribeEvents`, store the event generation;
  - `apply_mux_event` (`:3744`): `PackageEvent` and `EventGap` arms;
  - `TuiApp` state (`:1356` region): `transient_notice`, event subscription id, gap diagnostic;
  - `poll_hub` (`:1594`): notice expiry tick;
  - `draw_workspace_shell` (`:1157`): `workspace-transient-notice` band;
  - `force_reconnect` (`:2146`) / `apply_transport_failure` (`:4198`): clear notice and event generation;
  - boundary guard test (`:20039`): extend required vocabulary;
  - new unit and live tests in `mod tests`.
- `script/test-live-hub` — new `package-events` mode, filter, sentinel.
- `README.md` — pin table, feature list, optional-feature and live-lane docs, "Not included yet" correction.
- `docs/plans/tui-consume-transient-package-events-through-the-hub-control-plane-plan.md` — this plan.
- Possibly `crates/botster-tui/fixtures/` — only if U3 requires a synthetic producer package.

## Risks

- R1: The Core pin roll (`fd66efd` → `8fce204`) imports unrelated Core changes. Mitigation: run the full workspace gates and the existing Ghostty live lane on the rolled pins before the event work is judged; if an unrelated regression appears, register a separate blocker ticket instead of expanding this one.
- R2: Raising the Hello floor rejects daemons older than conformance 44. Accepted under A1; README documents the new floor; shared live lanes need a caller Hub ≥ `7a09292`.
- R3: Event flood could steal run-loop time from entity reconciliation or terminal I/O. Bounded by design: `poll_mux_frames` already caps batches at 32 frames; notice application is O(1) latest-wins; nothing in the event path blocks or waits. The flood lane must prove the existing published test budgets still hold (poll elapsed bound, entity convergence, live terminal echo within the live-gate timeout).
- R4: `EventGap` arrives before queued events and carries no count. The TUI must not treat it as an error or a resubscribe trigger; a wrong reaction here could churn subscriptions. Unit-test the gap arm explicitly.
- R5: A stale `PackageEvent` from a previous connection's subscription id could arrive interleaved around reconnect. The exact-id generation match drops it; unit-test the foreign-id case.
- R6: `workspace_hides_transient_action_feedback` (`app.rs:11998`) asserts the workspace shell hides `action:` feedback text. The new notice band is a distinct, deliberate surface; verify the new band does not violate that test's assertion and adjust the new node id/text so intent stays distinguishable.
- R7: Live-lane flake modes known from the charter: headless echo can time out after successful Hello (use Ghostty live attach as the live-attach oracle; reproduce on base before attributing), and colon-free `CARGO_TARGET_DIR` is already forced by `test.sh` (this worktree path has no colon).

## Acceptance checks/tests

Repository gates (all must pass at Implement and again at Verify):

1. `script/fmt` (`cargo fmt --all -- --check`).
2. `script/clippy` (`-D warnings`).
3. `script/test` (`cargo test --workspace --all-targets`, `BOTSTER_ENV=test`).

Hermetic unit tests (in `app.rs` `mod tests`):

4. Hello composition: required features include `package_event_subscriptions`; floor is 44; terminal requirement unchanged.
5. Demux: `package_event` and `event_gap` JSON lines parse to `DaemonUnixMuxFrame::Event` and reach `apply_mux_event` through the production `apply_mux_frames` path.
6. Notice policy: matching `PackageEvent` sets one notice; a second event replaces it (latest wins); the notice expires after `TRANSIENT_NOTICE_TTL` via the production tick path; foreign `subscription_id`, foreign owner, and foreign name are dropped; a payload without `notice` is dropped with a bounded diagnostic.
7. Gap policy: matching `EventGap` produces no notice, clears no durable store, sends no request, and records one bounded diagnostic.
8. Reconnect: teardown clears the notice and the event generation; the next `try_connect` sends `SubscribeEvents` with a fresh id (observable through the test request oracle `ObservedRequest`).
9. Subscribe failure: an `OperatorError` response to `SubscribeEvents` leaves the connection, entity subscriptions, and terminal path fully functional.
10. Boundary guard: `tui_hub_boundary_uses_public_client_without_private_protocol_plumbing` extended to require the generated `SubscribeEvents` vocabulary and still forbid private protocol plumbing.

Live Unix proof (`script/test-live-hub package-events`, IsolatedHub from pinned Hub `7a09292` binaries, real PP package enabled from a caller-provided checkout at ≥ `beaba94`; authentic app-level connect through `connect_and_hello_with_terminal_requirement`, so the proof runs on the final independent Hub control and Core terminal planes; sentinel `package-events-live: complete`):

11. **Live notice:** trigger `question.opened` through the PP MCP tool path (U1); assert exactly one transient notice renders through the production apply path (state plus `render_to_lines` band assertion), and assert the durable question row is present via the `project-pipelines.question` entity family with the exact `question_id` and expected state ([[acceptance readiness requires the exact expected entity not any authoritative snapshot]]).
12. **Missed event keeps durable state:** with `BOTSTER_HUB_TEST_CLIENT_EVENT_QUEUE_MAX` forcing shed, trigger a question; assert an `EventGap` (or absence of the notice) while the exact durable question row is still present and rendered state is driven by the entity plane.
13. **Reconnect without replay:** after a delivered notice, force reconnect; assert a fresh `SubscribeEvents` id, no replayed notice, and durable question recovery through the entity baseline, not events.
14. **Flood within budgets:** saturate events (U3) while an entity family converges and a live terminal attach echoes; assert the existing published budgets hold — bounded `poll_mux_frames` elapsed (existing 200 ms-style bound), exact-row entity convergence, and terminal echo inside the live-gate timeout. Event traffic must produce no terminal-plane calls (assert via the request oracle and the untouched terminal counters).

Downstream proof: none required beyond this repository — the TUI is the terminal consumer in this chain; Hub and PP contracts are pinned, not modified.

## Vault gaps

- `question.opened` has no `payload.subject`, so client subject filters match nothing; client-side filtering on `run_id`/`ticket_id` is the only narrowing. Capture as an atomic note once the consumer ships.
- Neither hub-test-support (Rust or npm) ships an event emitter helper or a golden package-event conformance fixture; consumers copy the Hub's `pub(crate)` test helpers or the `examples/event-plane-producer` package. Capture as a gap note; a Hub follow-up ticket may be worth registering if a second consumer (Web) duplicates the same scaffolding.
- The TUI transient-notice policy (single slot, TTL, gap-drop) will be the first first-party transient UI mechanism; capture the pattern after Implement so Web's equivalent ticket reuses the shape.
- Evidence note candidate: repinning hub-client forces the Core pin set to move in lockstep (`fd66efd` → `8fce204` here) — an instance of [[Git-consumed Hub members pin Core protocol by exact revision]] worth recording as applied evidence.
