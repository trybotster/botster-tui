# Plan: TUI consume transient package events through the Hub control plane

Ticket: `ticket_1786663585_944018`
Run: `run_1787197986_912715`
Step: `botster_stack_plan`
Plan revision 2

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

## Plan Review corrections (review_1787199608_577018)

| Finding | Class | Fix in this revision |
| --- | --- | --- |
| `finding_1787199608_182920` no active workflow filter, playbook skipped | product / high | Loaded [[project-pipelines-playbook]]. Inbound handling keeps `run_id`, `ticket_id`, `step_id`. The notice filter applies human answer `question_1787199481_712019` precedence: active run, then active ticket, then active step. Device-wide notices are forbidden; no context match suppresses the notice. The production context source is the focused session mapped through `project-pipelines.run_step` and `project-pipelines.run` entity rows (Scope 5, 6). |
| `finding_1787199608_840819` durable question state unwired in production | product / high | Production wiring: `sync_entity_options_subscriptions` gains a first-party workflow-context demand set (`project-pipelines.question`, `project-pipelines.run`, `project-pipelines.run_step`) that is active whenever the Hub connection is up, independent of the open plugin surface. A durable `workspace-question-attention` band renders open-question state from entity rows only. Missed-event and reconnect lanes enter through this production path (Scope 6, checks 12–13). |
| `finding_1787199609_732423` false 32-frame claim, no numeric budgets | product / high | Design adds a hard per-tick apply bound that retains surplus frames in order (Scope 7). Exact numeric budgets published in Acceptance: ≤ 32 frames applied per tick, poll/apply tick < 200 ms, entity exact-row convergence ≤ 3,000 ms, terminal input echo ≤ 3,000 ms, terminal output progress ≤ 3,000 ms under flood. The flood lane measures each production oracle against these numbers (check 15). |
| `finding_1787199609_151364` SubscribeEvents response race unspecified | product / medium | Candidate/active subscription state machine specified (Scope 4). Unit tests cover PackageEvent and EventGap frames interleaved before `EventSubscribed`: response pairing preserved, buffered frames applied exactly once after promotion, `OperatorError` clears the candidate and drops its frames without touching durable or terminal planes (check 9). |
| `finding_1787199609_112946` EventGap must drop existing transient state | product / medium | `EventGap` now clears a currently visible matching notice in addition to creating none (Scope 5a). Unit and live checks cover notice clearing, unchanged durable state, no replay, no resubscribe, and later valid live events still producing notices (checks 10, 13). |
| `finding_1787199609_645540` empty structured completion evidence | process / info | This revision resubmits gate evidence with `plan_uri`, `artifact_id`, `checklist_id`, `target_id`, `target_repository`, and also passes the same structured evidence to `request_step_advance`. The existing artifact relation is preserved by adding a revision-2 artifact for the same plan path. |

Human decision recorded (`question_1787199481_712019`, answered): show a transient `question.opened` notice only when it matches the TUI's active workflow context — match the active run first; if the view has no run, match the active ticket; if the view has no ticket, match the active step context the TUI already owns. Device-wide notices are forbidden. Durable question and attention UI stays entity-driven. Reconnect must not replay notices.

## Repository playbook loaded

- [[botster-tui-playbook]]

## Other playbooks and notes loaded

Role / stack: [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[project-pipelines-playbook]] (loaded in revision 2: the plan consumes the PP package path and question contract).

TUI charter must-load notes applied here: [[botster tui consumes tui kit through a thin app policy adapter]], [[tui and browser are equal clients]], [[tui client attach uses hub protocol not session protocol]], [[first-party Unix attach clients use split Hello and subscription close events]], [[first-party clients put terminal mechanism tokens only in terminal compatibility]], [[Unix mux polling returns bounded complete-frame batches while input stays readable]], [[acceptance readiness requires the exact expected entity not any authoritative snapshot]], [[TUI live Ghostty has IsolatedHub ghostty plus attach-only ghostty-shared and ghostty-shared-exit]], [[TUI contract matrix headless echo can time out after successful Hello]], [[first-party Rust consumers pin the UI contract Git tag not a Hub rev]], [[Cargo Git URL and selector form are part of crate identity]], [[Git-consumed Hub members pin Core protocol by exact revision]], [[compatibility fixtures advertise every required optional feature]].

Task-surface event-plane notes: [[Client event subscriptions stay on the multiplexed host-control path]], [[Client event holders are connection-scoped]], [[Host package-event negotiation survives terminal admission rejection]], [[Fair host-control writing selects already-admitted frames]], [[exact owner plus name is the only package event subscription key]], [[Package-event subject filters are exact strings compiled at admission]], [[a transient package event cannot be the sole authority for a durable close]], [[additive daemon capabilities do not raise the default client requirement]], [[botster data plane bypasses the hub through session and client actors]], [[question opened clients subscribe with empty subjects]].

Not loaded, with reasons: [[botster runtime teardown lenses]] (teardown class does not apply, see table above), [[spa-patterns]] (no browser surface), [[cli-patterns]] (preserved mixed index; [[botster-architecture]] marks it non-authoritative for current ownership).

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
- Payload schema (`additionalProperties: false`): required `question_id` (≤128), `kind` (`human`|`agent`), `notice` (≤280 chars, the only human-presentable field); optional `blocking`, `run_id`, `step_id`, `ticket_id`. The schema has **no `subject` field**, so a non-empty `subjects` filter matches nothing. Clients subscribe with an empty subject list and filter workflow ids locally ([[question opened clients subscribe with empty subjects]]).
- Emit happens only after the durable question commit (`plugin.lua:1624` inside `record_question`), wrapped in `pcall`. The durable question row carries `id`, `run_id`, `ticket_id`, `step_id`, `kind`, `status`, `blocking`, `asked_by`, `question` (`plugin.lua:1585-1595`).
- Entity providers (`plugin.lua:3253-3273`, handler `:3277`): families include `project-pipelines.question`, `project-pipelines.run` (rows carry `ticket_id`), and `project-pipelines.run_step` (rows carry `run_id`, `step_id`, `agent_session_uuid`). Durable recovery uses these providers, never event replay (`docs/domain-contract.md:296-312`).

Current TUI base `dc7d600`:

- Pins: hub-client and hub-test-support at Hub `e864c3c8` (predates the event work — no event types exist at this pin); Core set at `fd66efd` across `botster-core`, `botster-terminal-ghostty`, `botster-terminal-protocol-client`, `botster-core-test-support`; UI contract tag `botster-ui-contract-v0.3.2`.
- Host control plane: `HubConnection` (`app.rs:8918`) with persistent `mux_buf`, `poll_mux_frames` (1 ms timeout, `MUX_POLL_BATCH_FRAMES = 32`), `decode_complete_mux_frames`, `parse_unix_mux_value`. One read can decode and append more than 32 frames to `pending_mux_frames`, and `take_pending_mux_frames` currently returns the full pending vector — the apply side, not the poll side, must own the per-tick bound. App fan-out: `TuiApp::apply_mux_frames` (`app.rs:3687`); the event branch `apply_mux_event` (`app.rs:3744`) today handles only `TerminalSubscriptionClosed`. `HubConnection::request` (`app.rs:8971`) pairs the first `Response` and parks interleaved frames in `pending_mux_frames`; `apply_pending_mux_frames` (`app.rs:3679`) applies them after the request returns.
- Hello: `tui_compatibility_requirement()` (`app.rs:9227`) with `MINIMUM_CONFORMANCE_FIXTURE_REVISION = 40` (`app.rs:92`); terminal requirement is separate (`app.rs:9282`); connect path is `connect_and_hello_with_terminal_requirement` (`app.rs:8936`).
- Reconnect: `try_connect` (`app.rs:2165`) re-runs `refresh_read_models`, `start_session_subscription`, `start_session_type_subscription_if_supported`; every entity resubscribe mints a fresh subscription id; there is no replay or cursor anywhere.
- Entity options production path: `sync_entity_options_subscriptions` (`app.rs:2955`) subscribes exactly the families demanded by the active plugin surface (`demanded_entity_option_families`), with per-family generation stores, `classify_delta` gap detection, `NeedsRecovery` fresh-generation resubscribe, and `heal_entity_options_subscriptions` retry. Today no surface demands a Project Pipelines family, so no production question state exists.
- Focused session identity: `selected_session` set by `sync_focused_session` (`app.rs:1581`) from the `tui-session-list` selection.
- Transient UI: none exists. `connection_alert()` (`app.rs:5204`) rendered as a one-row band in `draw_workspace_shell` (`app.rs:1157`) is the structural precedent for an `Option<UiNode>` band. The run loop polls every ≤100 ms (`app.rs:1118`).
- Boundary guard test: `tui_hub_boundary_uses_public_client_without_private_protocol_plumbing` (`app.rs:20039`).
- Live proof: `script/test-live-hub` modes map one exact test filter plus a completion sentinel; `IsolatedHubBuilder` drives hub/session-worker binaries; `skip_or_panic` gates on `BOTSTER_TUI_REQUIRE_HUB_TEST`.
- Docs: plans at `docs/plans/tui-<slug>-plan.md`, implement reports at `docs/reports/tui-<slug>-implement-report.md`; README pin table (README:30-40) moves in lockstep with `Cargo.toml`.

## Scope

Surgical TUI consumer change against Hub `7a09292` and PP `beaba94`. No Hub, Core, PP, or TUI Kit source changes.

1. **Pin roll (one lockstep set).** Bump `botster-hub-client` and `botster-hub-test-support` from `e864c3c8` to `7a09292cd518186e0def758c823c0841ee1cacf1`. Bump the Core set (`botster-core`, `botster-terminal-ghostty`, `botster-terminal-protocol-client`, `botster-core-test-support`) from `fd66efd` to `8fce2041b9fe742cb2a6df9e74cb262606672742` to match Hub member manifests ([[Git-consumed Hub members pin Core protocol by exact revision]]). Keep `botster-ui-contract` on tag `botster-ui-contract-v0.3.2`. Refresh `Cargo.lock`. Update the README pin table.
2. **Hello negotiation.** Add `FEATURE_PACKAGE_EVENT_SUBSCRIPTIONS` to `tui_compatibility_requirement()` required features and raise `MINIMUM_CONFORMANCE_FIXTURE_REVISION` from 40 to 44. Keep terminal compatibility untouched.
3. **Event subscription policy.** After a successful Hello in `try_connect`, send one `SubscribeEvents { subscription_id: "btui-events-{short_suffix()}", owner: "project-pipelines", name: "question.opened", subjects: vec![] }`. Reconnect naturally re-subscribes with a fresh id and gets no replay.
4. **Candidate/active subscription state (response race).** The minted id starts as **candidate** when the request is written. `HubConnection::request` keeps response pairing; interleaved `PackageEvent`/`EventGap` frames park in `pending_mux_frames` and apply after the request returns. On `EventSubscribed`, promote candidate → **active** before `apply_pending_mux_frames` runs, so parked frames for that id apply exactly once. On `OperatorError`, clear the candidate, drop any parked event frames carrying it, record a bounded diagnostic, and leave durable, entity, and terminal planes untouched. `apply_mux_event` accepts event frames only for the **active** id; frames for a cleared candidate or any foreign id drop silently.
5. **Inbound handling.**
   - `PackageEvent` (active id, owner `project-pipelines`, name `question.opened`): parse `notice` (required), `question_id`, `kind`, and keep `run_id`, `ticket_id`, `step_id`. Apply the **active workflow filter** (Scope 6). A matching event sets the transient notice (single slot, latest wins, O(1) per event). A non-matching event is suppressed. A payload without `notice` drops with a bounded diagnostic.
   - **(5a)** `EventGap` (active id): clear the currently visible matching transient notice, create none for missed events, keep every durable store unchanged, record one bounded gap diagnostic, and send no request — no replay, no resubscribe. A later valid live event still creates a notice.
6. **Active workflow context and production durable wiring.**
   - Add a first-party workflow-context demand set `WORKFLOW_CONTEXT_ENTITY_FAMILIES = ["project-pipelines.question", "project-pipelines.run", "project-pipelines.run_step"]`. `sync_entity_options_subscriptions` unions this set with surface-demanded families whenever the Hub connection is up, so the durable question plane is production-subscribed independent of any open plugin surface, reusing the existing generation stores, `classify_delta` gap recovery, and heal/retry paths. A subscribe failure for these families (for example, PP not installed) records a bounded diagnostic and degrades gracefully: durable UI shows nothing and all notices are suppressed (fail closed, no device-wide notices).
   - Derive the active context from production state: active run = the `run_id` of the `project-pipelines.run_step` row whose `agent_session_uuid` equals the focused `selected_session` (newest such row); active step = that row's `step_id`; active ticket = the matched run's `ticket_id` from `project-pipelines.run`.
   - Notice filter precedence (human answer `question_1787199481_712019`): compare at the highest level where both the TUI context and the payload carry the id — run first, then ticket, then step. A mismatch at that level suppresses. When no level has both ids, suppress. No device-wide notices.
   - Durable attention UI: a `workspace-question-attention` band (one-row `Option<UiNode>`, `connection_alert` precedent) rendered from `project-pipelines.question` entity rows only — open-question count for the active context plus the newest open question text. Entity rows are the sole authority; events never write it.
7. **Bounded per-tick apply with surplus retention.** Replace the unbounded `take_pending_mux_frames` drain with a bounded drain: `poll_and_apply_mux_frames` and `apply_pending_mux_frames` apply at most `MUX_APPLY_BATCH_FRAMES = 32` frames per tick and retain surplus frames in `pending_mux_frames` in order for the next tick. One read that decodes more than 32 frames therefore cannot extend a tick; surplus drains across subsequent ≤100 ms ticks.
8. **Reconnect and teardown hygiene.** `force_reconnect`, `apply_transport_failure`, and connection loss clear the candidate/active event subscription state and the transient notice. Workflow-context entity families recover through the existing entity generation machinery. A reconnected session shows no old notice.
9. **Transient notice UI (app policy only).** `transient_notice: Option<TransientNotice { text, question_id, kind, deadline: Instant }>` with `TRANSIENT_NOTICE_TTL` (proposed 5 s), rendered as the `workspace-transient-notice` band, expired by deadline check in `poll_hub` (the ≤100 ms tick). No TUI Kit change.
10. **Tests and live proof.** See Acceptance checks. Extend the boundary guard test vocabulary with the generated event types; add a `package-events` mode to `script/test-live-hub` with an exact filter and a `package-events-live: complete` sentinel.
11. **Docs.** Update README: pin table, foundation feature list, the optional-feature section, the live-lane list, and the "Not included yet" scope sentence that currently excludes Project Pipelines consumption. Keep this plan file current; add the implement report under `docs/reports/`.

## Non-scope

- No Hub, Core, or Project Pipelines source change. The producer contract and the host control surface are shipped.
- No TUI Kit change. The kit stays policy-free; no toast primitive is added to the kit.
- No question workbench: the durable attention band is a minimal entity-driven indicator, not answer/management UI. Answering questions stays on existing PP surfaces and MCP tools.
- No event replay, cursor, sequence, or history handling — the contract has none, and the TUI must not simulate one.
- No terminal-plane work: no Hub-specific terminal logic, no terminal frame scheduling, no terminal fairness. Event frames never touch `apply_unix_terminal_envelope`.
- No `pr_merged` consumption (audience is `["plugins"]` only — not client-visible).
- No subscription to additional owners/names, and no subject filters (the `question.opened` schema has no `subject` field).
- No WebRTC consumer work (that is the Web ticket `ticket_1786663584_427840`).

## Repository ownership boundaries and cross-repo dependencies

| Owner | Owns here | This ticket's stance |
| --- | --- | --- |
| botster-hub (`tgt_7e208a0c76a44980a83b63af976b1f22`) | Event admission, routing, egress bounds, gap semantics, fair host-control writing, generated client types | Consume at pinned `7a09292`. No change. |
| botster-project-pipelines (`tgt_a72ca1a83d504385b8648f71409119ab`) | `question.opened` contract, durable question records, `project-pipelines.question`/`run`/`run_step` entity providers | Consume at `beaba94`. No change. The active-context mapping uses fields these providers already publish (`agent_session_uuid`, `ticket_id`). |
| botster-core (`tgt_1f7bce66eb304881980f9b4a2a5ae3fe`) | Terminal plane, lifecycle journal | Pin follows Hub lockstep (`8fce204`). No change. |
| botster-tui-kit (`tgt_3dfae49c02454037bf13554f552baf7f`) | Reusable render/input mechanics | Unchanged; notice and attention bands are app-composed. |
| botster-tui (this repo) | Client event subscription policy, workflow-context filter, transient notice policy, durable attention band, reconnect behavior, live proof | All changes land here. |

Dependencies: all three ticket dependencies are closed; no new cross-repository prerequisite exists. If Implement finds that `project-pipelines.run_step` rows do not expose the agent session identity the TUI can match against its session list (U5), stop and register a dependency ticket against the PP target instead of widening this run.

## Assumptions and unknowns

Assumptions (Plan Review should challenge these):

- A1: Requiring `package_event_subscriptions` in the TUI's main Hello and raising the conformance floor to 44 is acceptable. The TUI is a first-party client whose README pin table moves in lockstep with the Hub; the repo already requires other optional daemon features. Consequence: shared live lanes (`ghostty-shared`, `ghostty-shared-exit`) need a caller Hub at ≥ `7a09292` after this change.
- A2: One event subscription (one owner+name) is enough for this ticket.
- A3: The transient notice is a single latest-wins slot with a fixed TTL. Multiple matching events within one TTL replace the visible notice; a notice queue is speculative.
- A4: The first-party workflow-context demand set is TUI app policy, not plugin policy: the ticket itself hard-codes the `project-pipelines`/`question.opened` consumption, and the demand set is the durable-plane mirror of that same product decision.
- A5: Fail-closed filtering is correct: when the workflow-context stores have not hydrated (startup, PP absent, or recovery in progress), notices are suppressed rather than shown device-wide. This follows the human answer's prohibition on device-wide notices.

Unknowns for Implement to resolve (not blockers unless noted):

- U1: The minimal PP call sequence that emits `question.opened` inside an IsolatedHub. `project_pipelines_ask_human` via `DaemonRequest::PluginMcpCallTool` is the expected trigger; `record_question` requires an existing run or ticket, so create the minimal project/ticket (and run, for run-precedence lanes) through PP MCP tools inside the test.
- U2: How the live lane locates the PP package checkout. Follow the installed-workspaces-driver precedent (caller-provided path environment variable, `EnablePackageLocalPath`, pin-floor check ≥ `beaba94`).
- U3: Whether the flood lane needs the synthetic `event-plane-producer` fixture (copy the Hub example package into TUI fixtures) or can reach flood/shed with PP alone plus `BOTSTER_HUB_TEST_CLIENT_EVENT_QUEUE_MAX`. Prefer the test knob with the real PP producer.
- U4: Exact placement of the notice-expiry check (`poll_hub` head versus immediately before draw). Either satisfies the ≤100 ms tick; pick one and test it.
- U5: Whether `project-pipelines.run_step.agent_session_uuid` values equal the Hub session ids the TUI holds in its session store (`selected_session`). Pipeline events show run_steps carrying `sess-...` UUIDs linked by session correlation; Implement must verify the exact field equality on a live IsolatedHub before relying on it. If no published field matches, register a PP dependency (see ownership table) — do not scrape or infer.

## Affected surfaces/files

- `crates/botster-tui/Cargo.toml`, `Cargo.lock` — pin roll (hub-client, hub-test-support, Core set).
- `crates/botster-tui/src/app.rs` —
  - `MINIMUM_CONFORMANCE_FIXTURE_REVISION` (`:92`) and `tui_compatibility_requirement()` (`:9227`): feature + floor;
  - `try_connect` (`:2165`): send `SubscribeEvents`, candidate state;
  - `apply_mux_event` (`:3744`): `PackageEvent` and `EventGap` arms with active-id and workflow filtering;
  - `poll_and_apply_mux_frames` (`:3665`) / `apply_pending_mux_frames` (`:3679`) / `take_pending_mux_frames` (`:9130` region): bounded per-tick drain with surplus retention;
  - `TuiApp` state (`:1356` region): `transient_notice`, candidate/active event subscription state, gap diagnostic;
  - `sync_entity_options_subscriptions` (`:2955`): first-party workflow-context demand set;
  - active-context derivation over `EntityOptionsStore` rows (`entity_options.rs` store read helpers as needed, no store mechanics change);
  - `poll_hub` (`:1594`): notice expiry tick;
  - `draw_workspace_shell` (`:1157`): `workspace-transient-notice` and `workspace-question-attention` bands;
  - `force_reconnect` (`:2146`) / `apply_transport_failure` (`:4198`): clear notice and event state;
  - boundary guard test (`:20039`): extend required vocabulary;
  - new unit and live tests in `mod tests`.
- `crates/botster-tui/src/entity_options.rs` — read-only helpers for workflow-context lookups if the store lacks them (no delta/gap mechanics change).
- `script/test-live-hub` — new `package-events` mode, filter, sentinel.
- `README.md` — pin table, feature list, optional-feature and live-lane docs, "Not included yet" correction.
- `docs/plans/tui-consume-transient-package-events-through-the-hub-control-plane-plan.md` — this plan.
- Possibly `crates/botster-tui/fixtures/` — only if U3 requires a synthetic producer package.

## Risks

- R1: The Core pin roll (`fd66efd` → `8fce204`) imports unrelated Core changes. Mitigation: run the full workspace gates and the existing Ghostty live lane on the rolled pins before the event work is judged; if an unrelated regression appears, register a separate blocker ticket instead of expanding this one.
- R2: Raising the Hello floor rejects daemons older than conformance 44. Accepted under A1; README documents the new floor; shared live lanes need a caller Hub ≥ `7a09292`.
- R3: Event flood could steal run-loop time from entity reconciliation or terminal I/O. Bounded by design: ≤ 32 frames applied per tick with surplus retention, O(1) latest-wins notice, nothing in the event path blocks or waits. The flood lane measures the published numeric budgets (check 15).
- R4: `EventGap` arrives before queued events and carries no count. The TUI must not treat it as an error or a resubscribe trigger; a wrong reaction here could churn subscriptions. Unit-tested (check 10).
- R5: A stale `PackageEvent` from a previous connection's subscription id could arrive interleaved around reconnect. The active-id generation match drops it; unit-tested (check 11).
- R6: `workspace_hides_transient_action_feedback` (`app.rs:11998`) asserts the workspace shell hides `action:` feedback text. The new bands are distinct, deliberate surfaces; verify the assertion still holds and intent stays distinguishable.
- R7: Known live-lane flake modes: headless echo can time out after successful Hello (use Ghostty live attach as the live-attach oracle; reproduce on base before attributing), and colon-free `CARGO_TARGET_DIR` is already forced by `test.sh` (this worktree path has no colon).
- R8: The always-on workflow-context subscriptions add three entity connections per TUI instance. Rows are bounded by PP state and the existing per-family pumps are already production mechanics; if PP is absent the subscriptions degrade to bounded diagnostics. If Implement measures meaningful idle cost, narrow the demand set (question only + lazy run/run_step) inside this ticket's scope rather than adding configurability.

## Acceptance checks/tests

Published numeric budgets for this ticket (used by checks 14–15):

| Budget | Limit | Oracle |
| --- | --- | --- |
| Frames applied per tick | ≤ 32 (`MUX_APPLY_BATCH_FRAMES`), surplus retained in order | unit test on the bounded drain |
| One `poll_and_apply_mux_frames` tick under flood | < 200 ms | `Instant` elapsed assertion (existing style, `app.rs:18586`) |
| Entity exact-row convergence under flood | ≤ 3,000 ms | converged store holds the exact expected row |
| Terminal input echo round-trip under flood | ≤ 3,000 ms | live attach echo through the Core terminal plane |
| Terminal output progress under flood | ≤ 3,000 ms | live attach output bytes advance within the window |

Repository gates (all must pass at Implement and again at Verify):

1. `script/fmt` (`cargo fmt --all -- --check`).
2. `script/clippy` (`-D warnings`).
3. `script/test` (`cargo test --workspace --all-targets`, `BOTSTER_ENV=test`).

Hermetic unit tests (in `app.rs` `mod tests`):

4. Hello composition: required features include `package_event_subscriptions`; floor is 44; terminal requirement unchanged.
5. Demux: `package_event` and `event_gap` JSON lines parse to `DaemonUnixMuxFrame::Event` and reach `apply_mux_event` through the production `apply_mux_frames` path.
6. Bounded drain: a pending vector of 100 decoded frames applies ≤ 32 per tick, retains surplus in order, and fully drains across ticks with order preserved.
7. Workflow filter: with production-shaped `run_step`/`run` rows and a focused session, a payload matching the active run shows; run mismatch suppresses; ticket-level match applies only when the payload has no `run_id`; step-level match applies only when the payload has neither `run_id` nor `ticket_id`; no shared id level suppresses; unhydrated context suppresses (fail closed).
8. Notice policy: matching `PackageEvent` sets one notice; a second matching event replaces it (latest wins); the notice expires after `TRANSIENT_NOTICE_TTL` via the production tick path; a payload without `notice` drops with a bounded diagnostic.
9. Response race: scripted frame sequences prove — parked `PackageEvent`/`EventGap` before `EventSubscribed` leave response pairing intact and apply exactly once after promotion; `OperatorError` clears the candidate, drops its parked frames, and leaves durable, entity, and terminal planes untouched.
10. Gap policy: matching `EventGap` clears a visible matching notice, creates none, clears no durable store, sends no request (no replay, no resubscribe), records one bounded diagnostic; a later valid live event still sets a notice.
11. Generation/foreign drop: frames with a cleared candidate id, a prior connection's id, a foreign owner, or a foreign name drop silently.
12. Production demand set: with the Hub connection up, `sync_entity_options_subscriptions` subscribes the three workflow-context families independent of any surface; surface-demanded families still compose; a subscribe failure records a bounded diagnostic and leaves the app functional.
13. Reconnect: teardown clears the notice and candidate/active state; the next `try_connect` sends `SubscribeEvents` with a fresh id (observable via `ObservedRequest`); durable attention rendering recovers from fresh entity baselines only.
    Boundary guard: `tui_hub_boundary_uses_public_client_without_private_protocol_plumbing` extended to require the generated `SubscribeEvents` vocabulary and still forbid private protocol plumbing.

Live Unix proof (`script/test-live-hub package-events`, IsolatedHub from pinned Hub `7a09292` binaries, real PP package ≥ `beaba94` enabled via `EnablePackageLocalPath`; authentic app-level connect through `connect_and_hello_with_terminal_requirement`, so the proof runs on the final independent Hub control and Core terminal planes; sentinel `package-events-live: complete`):

14. **Live notice through production paths:** create the minimal PP project/ticket/run (U1), focus the correlated session so the production active-context derivation matches, trigger `question.opened` through the PP MCP tool path; assert exactly one transient notice renders through the production apply path, and assert the durable question row is present through the **production workflow-context subscription** with the exact `question_id` and open state ([[acceptance readiness requires the exact expected entity not any authoritative snapshot]]); also assert a non-matching workflow id is suppressed while the durable row still arrives.
15. **Missed event keeps durable state:** with `BOTSTER_HUB_TEST_CLIENT_EVENT_QUEUE_MAX` forcing shed, trigger a question; assert the gap path (cleared/no notice) while the exact durable question row is present and the `workspace-question-attention` band renders it from entity state entering through the production subscription path.
16. **Reconnect without replay:** after a delivered notice, force reconnect; assert a fresh `SubscribeEvents` id, no replayed notice, and durable question recovery through the production entity baseline, not events.
17. **Flood within published budgets:** saturate events (U3) while a workflow-context entity family converges and a live terminal attach echoes; measure every budget in the table above against its production oracle; assert event traffic produces zero terminal-plane calls (request oracle and untouched terminal counters).

Downstream proof: none required beyond this repository — the TUI is the terminal consumer in this chain; Hub and PP contracts are pinned, not modified.

## Vault gaps

- Captured (inbox, 2026-08-19): `question.opened` clients subscribe with empty subjects; no hub-test-support event helpers or golden event fixture; `BOTSTER_HUB_TEST_CLIENT_EVENT_QUEUE_MAX` shed knob. Now also reflected in [[project-pipelines-playbook]] as [[question opened clients subscribe with empty subjects]].
- Capture after Implement: the TUI transient-notice + workflow-context-filter pattern (candidate/active subscription state, fail-closed context matching, entity-driven attention band) so the Web ticket reuses the shape.
- Capture after Implement: applied evidence instance of the hub-client-to-Core pin lockstep cascade (`fd66efd` → `8fce204`).
- Candidate Hub follow-up (not this run): a public test-support event emitter helper, once Web duplicates the TUI's scaffolding.
