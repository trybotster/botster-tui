# Plan: TUI consume transient package events through the Hub control plane

Ticket: `ticket_1786663585_944018`
Run: `run_1787197986_912715`
Step: `botster_stack_plan`
Plan revision 5

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
| Implement blocked on | **none.** All four dependencies are closed: Hub `ticket_1786663583_640263` (Hub `7a09292`), PP emitter `ticket_1786663583_568924`, TUI protocol split `ticket_1786661009_551067`, and PP mutation publishing `ticket_1787200699_360898` (merged; PP `origin/main` tip `cd7c2f926fcead78e15e7a9c713ad26dfe883914`). Live checks 14–17 are unblocked. |

## Plan Review corrections — third review (findings 1787244867_*)

| Finding | Class | Fix in this revision |
| --- | --- | --- |
| `finding_1787244867_694865` wrong transport for provider failure, no retry bound | product / high | Confirmed both halves. `subscribe_entities` maps any non-`EntitySubscribed` response to `DaemonTransportError::Protocol` (hub-client `lib.rs:362-366`), so the fail-closed provider surfaces at **admission**, not as `DaemonEntityFrame::Error`; revision 4's A7 and check 7a tested the wrong path. Separately, `heal_entity_options_subscriptions` (`app.rs:3092`) retries every missing demanded family on every poll with no bound — harmless while families were surface-demanded, but this plan's always-on set would drive ~20 connect attempts per second forever when PP is absent. Scope 6a now specifies both paths with per-family exponential backoff (750 ms → 30 s cap, reset on success), keeps fresh-generation recovery for genuine in-stream errors while charging it to the same budget, and check 7a asserts a request bound across repeated poll ticks. New risk R12 records that this plan's own change created the exposure. |
| `finding_1787244867_388404` unreachable ticket-only and step-only tiers | product / high | Confirmed: PP writes `run_id`, `step_id`, and `ticket_id` on every spawned `session_request` row (`plugin.lua:1507-1517`), so a matched row always owns a run, and the TUI has no other workflow-context state. Asked the human rather than choosing silently; answer `question_1787244996_447177` selected **run matching only**. Scope 6 now filters on `run_id` alone, check 7 drops the impossible tier fixtures, and the rationale is recorded so the tiers are not "restored" later. |
| `finding_1787244867_231121` attention band has no authoritative order | product / medium | Confirmed and sharper than reported: PP question rows carry no `created_at` or sequence, `next_id` yields `question_<counter>` (`plugin.lua:1046-1049`), callers may supply ids, and `EntityOptionsStore` keys a `BTreeMap` by id (`entity_options.rs:31`) — so `question_10` sorts before `question_2` and a "newest" rendering would be actively wrong. The band now shows only an authoritative **count** of open rows whose `run_id` equals the active run. New check 12a covers multiple matching and non-matching rows plus an answer update. No cross-repository order field is requested. |

## Revision 4: dependency landed, contract verified against the merged surface

PP dependency `ticket_1787200699_360898` merged. An agent message reported the merge at `d42ab56`; independent verification against fetched `origin/main` shows that SHA is an ancestor but **not** the tip — three later commits belong to the same delivery (`85b62f0` run_step `agent_session_uuid`, `41f8d6b` activation session-binding proof, `cd7c2f9` merge). The live-lane PP pin floor is therefore `cd7c2f926fcead78e15e7a9c713ad26dfe883914`, not the reported SHA.

Facts verified at PP `cd7c2f9` (each replaces an assumption with a source citation):

| Fact | Evidence | Effect on this plan |
| --- | --- | --- |
| Live-published families are exactly `project-pipelines.question` and `project-pipelines.session_request` | `ENTITY_MUTATION.order` / `.families`, `plugin.lua:297-312` | Confirms the revision-3 demand set unchanged. |
| Baseline and deltas share **one durable monotonic counter per family** | `provider_snapshot` CAS-allocates `last_seq + 1` (`plugin.lua:423-455`); mutations allocate `last_seq + index` (`plugin.lua:956`) from the same `plugin_db` seq key | `snapshot_seq` is contiguous across the baseline→delta boundary, so the TUI's existing `classify_delta` Accept rule (`snapshot_seq == current + 1`) works unmodified. Retires the resubscribe-loop risk. The in-memory counter at `plugin.lua:3641` applies only to non-mutation families. |
| `run_step.agent_session_uuid` now exists but `run_steps` is **not** live-published | added by `85b62f0` (`plugin.lua:1893`); `run_steps` is absent from `ENTITY_MUTATION.families` and is reconciled only inside the non-mutation snapshot branch (`reconcile_run_step_session_bindings`, `plugin.lua:3643-3650`) | `session_request.session_id` remains the correct context source: it converges live, while `run_step` context would go stale between snapshots. Do not "upgrade" this to `agent_session_uuid`. |
| Providers fail closed on incomplete family load | `d42ab56` | A provider failure surfaces as `DaemonEntityFrame::Error` rather than a partial snapshot. The TUI treats it as no context (fail-closed suppression) plus a bounded diagnostic — new assumption A7, check 7a. |
| Published upsert shape matches the client DTO | PP frame `{type, entity_type, snapshot_seq, id, entity}` (`plugin.lua:953-960`) versus `DaemonEntityFrame::Upsert { subscription_id, entity_type, snapshot_seq, id, entity }` (hub-client `lib.rs:2564`); snapshots carry `items` matching `Snapshot` (`lib.rs:2556`) | No TUI parse change beyond existing store mechanics. |

## Plan Review corrections — second review (findings 1787200489_*)

| Finding | Class | Fix in this revision |
| --- | --- | --- |
| `finding_1787200489_228841` cited absent `agent_session_uuid` | product / high | Corrected to a field `beaba94` actually publishes: `session_request.session_id` is set from the spawn response (`plugin.lua:1531`, `session_id or session_uuid`), and the same row carries `run_id`, `step_id`, `ticket_id` (`plugin.lua:1507-1517`); `run.session_id` mirrors it (`plugin.lua:1539`). The workflow-context demand set shrinks to two families: `project-pipelines.question` and `project-pipelines.session_request` (`run`/`run_step` dropped). Live check 14 asserts equality between `session_request.session_id` and the TUI-held session id before any notice claim; the former U5 deferral is removed. |
| `finding_1787200489_338068` no live mutation producer for durable rows | product / high | Confirmed at `beaba94`: providers and the `entities` tool are pull-only; no mutation path calls `botster.entity_publish` (mechanism shipped: Hub `package_entity_fanout`, used by botster-workspaces `plugin.lua:382`). Registered cross-repo dependency `ticket_1787200699_360898` on the PP target (`tgt_a72ca1a83d504385b8648f71409119ab`): publish `question` and `session_request` upserts after each durable `save_state`. Dependency edge `dependency_1787200707_907098` added; run `run_1787200746_538984` started on `botster_stack_delivery`. Live checks 14–16 now require create **and answer** convergence on one existing subscription (baseline + published upserts), never event replay. |
| `finding_1787200489_134286` fallback keyed on payload omission | product / high | Fallback now keys on what the active TUI view owns (human answer `question_1787199481_712019`): a view that owns a run compares only `payload.run_id` — a ticket-only payload is suppressed even when its ticket matches; the ticket tier applies only when the view owns a ticket without a run; the step tier applies only when the view owns a step without a ticket. Check 7 enumerates positive and negative tests for each tier, including view-with-run versus ticket-only event. |
| `finding_1787200489_114580` unstable wall-clock unit oracle | product / medium | Unit oracles are now deterministic work bounds only (≤ 32 frames applied per tick, ordered surplus retention, drain-across-ticks). Elapsed limits move to the controlled live lane as production observations with stated isolation, timeout role, diagnostics, and ambient-load rerun policy (Acceptance, budgets section). Exact numeric entity and terminal limits remain published. |

## Plan Review corrections — first review (review_1787199608_577018, all resolved in revision 2)

| Finding | Fix (revision 2, refined by revision 3 where noted) |
| --- | --- |
| `finding_1787199608_182920` workflow filter | Filter keeps `run_id`/`ticket_id`/`step_id`; precedence per human answer; [[project-pipelines-playbook]] loaded. Context source corrected in revision 3 to `session_request.session_id`. |
| `finding_1787199608_840819` durable production wiring | First-party workflow-context demand set through `sync_entity_options_subscriptions` plus entity-driven `workspace-question-attention` band. Revision 3 adds the PP mutation-publishing dependency so the production subscription actually converges live. |
| `finding_1787199609_732423` bounds and budgets | Bounded per-tick apply with surplus retention; numeric budgets published. Revision 3 moves elapsed assertions to the live lane. |
| `finding_1787199609_151364` response race | Candidate/active subscription state machine; check 9. |
| `finding_1787199609_112946` gap drops transient state | `EventGap` clears the visible matching notice; checks 10, 15. |
| `finding_1787199609_645540` structured evidence | Gate evidence and `request_step_advance` evidence both carry the required fields. |

Human decisions recorded, in order:

1. `question_1787199481_712019` (answered): show a transient `question.opened` notice only when it matches the TUI's active workflow context, preferring run, then ticket, then step. Device-wide notices are forbidden. Durable question and attention UI stays entity-driven. Reconnect must not replay notices.
2. `question_1787244996_447177` (answered, **current**): after Plan Review showed the ticket and step tiers are unreachable in this client, the filter is **run matching only** — show only when `payload.run_id` equals the focused session's active run; suppress on mismatch, on a missing `run_id`, on an unmatched `session_request`, and when there is no focused workflow context. Do not add ticket-only or step-only view state in this ticket. Remove tests for the unreachable tiers. The broader event contract may still carry other workflow identities; this consumer owns only the complete `session_request` context. Durable question state remains entity-driven.

Decision 2 narrows decision 1 for this client. Both prohibit device-wide notices.

## Repository playbook loaded

- [[botster-tui-playbook]]

## Other playbooks and notes loaded

Role / stack: [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[project-pipelines-playbook]] (the plan consumes the PP package path and question contract).

TUI charter must-load notes applied here: [[botster tui consumes tui kit through a thin app policy adapter]], [[tui and browser are equal clients]], [[tui client attach uses hub protocol not session protocol]], [[first-party Unix attach clients use split Hello and subscription close events]], [[first-party clients put terminal mechanism tokens only in terminal compatibility]], [[Unix mux polling returns bounded complete-frame batches while input stays readable]], [[acceptance readiness requires the exact expected entity not any authoritative snapshot]], [[TUI live Ghostty has IsolatedHub ghostty plus attach-only ghostty-shared and ghostty-shared-exit]], [[TUI contract matrix headless echo can time out after successful Hello]], [[first-party Rust consumers pin the UI contract Git tag not a Hub rev]], [[Cargo Git URL and selector form are part of crate identity]], [[Git-consumed Hub members pin Core protocol by exact revision]], [[compatibility fixtures advertise every required optional feature]].

Task-surface event-plane notes: [[Client event subscriptions stay on the multiplexed host-control path]], [[Client event holders are connection-scoped]], [[Host package-event negotiation survives terminal admission rejection]], [[Fair host-control writing selects already-admitted frames]], [[exact owner plus name is the only package event subscription key]], [[Package-event subject filters are exact strings compiled at admission]], [[a transient package event cannot be the sole authority for a durable close]], [[additive daemon capabilities do not raise the default client requirement]], [[botster data plane bypasses the hub through session and client actors]], [[question opened clients subscribe with empty subjects]], [[botster plugin entities are canonical for plugin-owned dynamic state]].

Timing-oracle notes applied to budgets: [[wall-clock MAX_OWNER_TURN_MS assertions flake under default-concurrency lib load]], [[wall-clock ready-operation bounds through a daemon child are ambient-load-sensitive]], [[conformance harnesses gate on deterministic invariants not timing]].

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
- Entity providers (`plugin.lua:3253-3273`, handler `:3277`): families include `project-pipelines.question` and `project-pipelines.session_request`. `session_request` rows carry `id`, `run_id`, `step_id`, `ticket_id`, `session_type_id`, `status`, and `session_id` set from the spawn response (`plugin.lua:1507-1517`, `:1531`); `run.session_id` mirrors the correlation (`plugin.lua:1539`). `run_step` rows carry only `id`, `run_id`, `step_id`, `status`, `sequence` — **no session identity** — so `run_step`/`run` are not context sources.
- Durable recovery uses entity state, never event replay (`docs/domain-contract.md:296-312`).

Merged PP mutation surface at `cd7c2f926fcead78e15e7a9c713ad26dfe883914` (closed dependency `ticket_1787200699_360898`) — the former live-mutation gap is closed. `ENTITY_MUTATION.publish_frame` calls `botster.entity_publish` (`plugin.lua:347`) after the durable save, publishing `entity_upsert` frames for the two families this plan consumes. Publish failure degrades to a bounded `entity_publish_degraded` diagnostic (`plugin.lua:996-1004`) and never rolls back the committed mutation, preserving [[a transient package event cannot be the sole authority for a durable close]]. See the revision-4 table above for the sequence, identity, and frame-shape facts.

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

Surgical TUI consumer change against Hub `7a09292` and PP `cd7c2f9` (the `question.opened` contract from `beaba94` plus the merged mutation-publishing surface). No Hub, Core, PP, or TUI Kit source changes.

1. **Pin roll (one lockstep set).** Bump `botster-hub-client` and `botster-hub-test-support` from `e864c3c8` to `7a09292cd518186e0def758c823c0841ee1cacf1`. Bump the Core set (`botster-core`, `botster-terminal-ghostty`, `botster-terminal-protocol-client`, `botster-core-test-support`) from `fd66efd` to `8fce2041b9fe742cb2a6df9e74cb262606672742` to match Hub member manifests ([[Git-consumed Hub members pin Core protocol by exact revision]]). Keep `botster-ui-contract` on tag `botster-ui-contract-v0.3.2`. Refresh `Cargo.lock`. Update the README pin table. For live lanes, the PP checkout floor is `cd7c2f926fcead78e15e7a9c713ad26dfe883914` (the merged dependency tip, which also carries the run_step binding commits). PP is a Lua package loaded by path, not a Cargo pin, so this floor is a live-lane admission check, not a manifest change.
2. **Hello negotiation.** Add `FEATURE_PACKAGE_EVENT_SUBSCRIPTIONS` to `tui_compatibility_requirement()` required features and raise `MINIMUM_CONFORMANCE_FIXTURE_REVISION` from 40 to 44. Keep terminal compatibility untouched.
3. **Event subscription policy.** After a successful Hello in `try_connect`, send one `SubscribeEvents { subscription_id: "btui-events-{short_suffix()}", owner: "project-pipelines", name: "question.opened", subjects: vec![] }`. Reconnect naturally re-subscribes with a fresh id and gets no replay.
4. **Candidate/active subscription state (response race).** The minted id starts as **candidate** when the request is written. `HubConnection::request` keeps response pairing; interleaved `PackageEvent`/`EventGap` frames park in `pending_mux_frames` and apply after the request returns. On `EventSubscribed`, promote candidate → **active** before `apply_pending_mux_frames` runs, so parked frames for that id apply exactly once. On `OperatorError`, clear the candidate, drop any parked event frames carrying it, record a bounded diagnostic, and leave durable, entity, and terminal planes untouched. `apply_mux_event` accepts event frames only for the **active** id; frames for a cleared candidate or any foreign id drop silently.
5. **Inbound handling.**
   - `PackageEvent` (active id, owner `project-pipelines`, name `question.opened`): parse `notice` (required), `question_id`, `kind`, and `run_id`. Apply the **run-only workflow filter** (Scope 6). A matching event sets the transient notice (single slot, latest wins, O(1) per event). A non-matching event is suppressed. A payload without `notice` drops with a bounded diagnostic.
   - **(5a)** `EventGap` (active id): clear the currently visible matching transient notice, create none for missed events, keep every durable store unchanged, record one bounded gap diagnostic, and send no request — no replay, no resubscribe. A later valid live event still creates a notice.
6. **Active workflow context and production durable wiring.**
   - First-party workflow-context demand set `WORKFLOW_CONTEXT_ENTITY_FAMILIES = ["project-pipelines.question", "project-pipelines.session_request"]`. `sync_entity_options_subscriptions` unions this set with surface-demanded families whenever the Hub connection is up, so the durable question plane is production-subscribed independent of any open plugin surface, reusing the existing generation stores, `classify_delta` gap recovery, and heal/retry paths. Live convergence after mutation is supplied by the merged PP publish path; baseline and delta share one contiguous per-family sequence, so no `classify_delta` change is required.

   **(6a) Two distinct failure modes, with a request bound on the always-on families.** Making these families always-on removes the surface-demand guard that previously limited retry attempts, so the retry policy is part of this change, not an afterthought.
   - **Admission failure** (`subscribe_entities` returns `Err`): the provider fail-closed throw surfaces here, not as an in-stream frame. Hub converts the initial provider failure into an `OperatorError` `SubscribeEntities` response, and hub-client turns any non-`EntitySubscribed` response into `DaemonTransportError::Protocol("entity subscription was not accepted")` (`lib.rs:362-366`). Connect and Hello failures land here too. Policy: per-family consecutive-failure count plus a next-attempt deadline with exponential backoff (750 ms, 1.5 s, 3 s, 6 s, 12 s, cap 30 s), reset on success, one bounded diagnostic per transition rather than per attempt. Backoff caps the request rate without ever giving up permanently, so enabling PP later still recovers without a restart.
   - **In-stream `DaemonEntityFrame::Error`**: preserve the existing proven behavior — drop the generation and resubscribe with a fresh id. That resubscribe is itself an admission attempt and therefore consumes the same per-family backoff budget, which is what prevents an error-then-resubscribe loop.
   - `heal_entity_options_subscriptions` (`app.rs:3092`) currently returns early when no plugin surface exists and retries every missing demanded family on every poll. It must handle the always-on families regardless of surface presence and must skip any family whose next-attempt deadline has not passed.
   - Scope note: this bound applies to the workflow-context families this plan makes always-on. Surface-demanded families keep their current behavior unchanged; their unbounded heal is pre-existing and is not this ticket's to alter.
   - Active context derivation: find the `project-pipelines.session_request` row whose `session_id` equals the focused `selected_session` (newest such row). That row supplies the active run (`run_id`), active step (`step_id`), and active ticket (`ticket_id`) directly.
   - **Notice filter: run matching only** (human answer `question_1787244996_447177`, superseding the tiered reading of `question_1787199481_712019`). Show the notice only when `payload.run_id` equals the focused session's active `run_id`. Suppress when the payload carries a different `run_id`, when the payload has no `run_id`, when no `session_request` row matches the focused session, and when the TUI has no focused workflow context at all. No device-wide notices.
     - Rationale, recorded so a later reader does not "restore" the tiers: PP writes `run_id`, `step_id`, and `ticket_id` on every spawned `session_request` row (`plugin.lua:1507-1517`), so a matched row always owns a run. The TUI has exactly two states — a focused session yielding a complete `(run, step, ticket)` triple, or no context. Ticket-only and step-only view states do not exist in this client, so those tiers are unreachable here and their tests are deliberately not written. The wider event contract still admits other workflow identities; this consumer owns only the complete `session_request` context and filters on `run_id`.
   - Durable attention UI: a `workspace-question-attention` band (one-row `Option<UiNode>`, `connection_alert` precedent) rendered from `project-pipelines.question` entity rows only. It shows an **authoritative count of open questions whose `run_id` equals the active run** — no "newest question" text. PP question rows carry no `created_at`, sequence, or order field; `next_id` produces `question_<counter>` (`plugin.lua:1046-1049`), callers may supply arbitrary ids, and `EntityOptionsStore` holds rows in a `BTreeMap` keyed by id (`entity_options.rs:31`), so store order is lexical, not creation order — `question_10` sorts before `question_2`. A "newest" claim would therefore be wrong, not merely unordered. A count is order-free and needs no new cross-repository field. Entity rows are the sole authority; events never write this band. Answering a question drops it from the count when its row status changes, via the published upsert.
7. **Bounded per-tick apply with surplus retention.** Replace the unbounded `take_pending_mux_frames` drain with a bounded drain: `poll_and_apply_mux_frames` and `apply_pending_mux_frames` apply at most `MUX_APPLY_BATCH_FRAMES = 32` frames per tick and retain surplus frames in `pending_mux_frames` in order for the next tick. One read that decodes more than 32 frames therefore cannot extend a tick; surplus drains across subsequent ≤100 ms ticks.
8. **Reconnect and teardown hygiene.** `force_reconnect`, `apply_transport_failure`, and connection loss clear the candidate/active event subscription state and the transient notice. Workflow-context entity families recover through the existing entity generation machinery. A reconnected session shows no old notice.
9. **Transient notice UI (app policy only).** `transient_notice: Option<TransientNotice { text, question_id, kind, deadline: Instant }>` with `TRANSIENT_NOTICE_TTL` (proposed 5 s), rendered as the `workspace-transient-notice` band, expired by deadline check in `poll_hub` (the ≤100 ms tick). No TUI Kit change.
10. **Tests and live proof.** See Acceptance checks. Extend the boundary guard test vocabulary with the generated event types; add a `package-events` mode to `script/test-live-hub` with an exact filter and a `package-events-live: complete` sentinel.
11. **Docs.** Update README: pin table, foundation feature list, the optional-feature section, the live-lane list, and the "Not included yet" scope sentence that currently excludes Project Pipelines consumption. Keep this plan file current; add the implement report under `docs/reports/`.

## Non-scope

- No Hub, Core, PP, or TUI Kit source change. PP entity mutation publishing was delivered by the now-closed `ticket_1787200699_360898` on the PP target; this run consumes it.
- No question workbench: the durable attention band is a minimal entity-driven indicator, not answer/management UI. Answering questions stays on existing PP surfaces and MCP tools.
- No event replay, cursor, sequence, or history handling — the contract has none, and the TUI must not simulate one.
- No terminal-plane work: no Hub-specific terminal logic, no terminal frame scheduling, no terminal fairness. Event frames never touch `apply_unix_terminal_envelope`.
- No `pr_merged` consumption (audience is `["plugins"]` only — not client-visible).
- No subscription to additional owners/names, and no subject filters (the `question.opened` schema has no `subject` field).
- No WebRTC consumer work (that is the Web ticket `ticket_1786663584_427840`).

## Repository ownership boundaries and cross-repo dependencies

| Owner | Owns here | This ticket's stance |
| --- | --- | --- |
| botster-hub (`tgt_7e208a0c76a44980a83b63af976b1f22`) | Event admission, routing, egress bounds, gap semantics, fair host-control writing, generated client types, package entity fanout | Consume at pinned `7a09292`. No change. |
| botster-project-pipelines (`tgt_a72ca1a83d504385b8648f71409119ab`) | `question.opened` contract, durable question/session_request records and providers, live mutation publishing | Consume only. **Dependency `ticket_1787200699_360898` closed** (edge `dependency_1787200707_907098`, run `run_1787200746_538984`); merged at PP `cd7c2f9`, which supplies the `entity_publish` upserts TUI live checks 14–16 consume. No PP change in this run. |
| botster-core (`tgt_1f7bce66eb304881980f9b4a2a5ae3fe`) | Terminal plane, lifecycle journal | Pin follows Hub lockstep (`8fce204`). No change. |
| botster-tui-kit (`tgt_3dfae49c02454037bf13554f552baf7f`) | Reusable render/input mechanics | Unchanged; notice and attention bands are app-composed. |
| botster-tui (this repo) | Client event subscription policy, workflow-context filter, transient notice policy, durable attention band, reconnect behavior, live proof | All changes land here. |

## Assumptions and unknowns

Assumptions (Plan Review should challenge these):

- A1: Requiring `package_event_subscriptions` in the TUI's main Hello and raising the conformance floor to 44 is acceptable. The TUI is a first-party client whose README pin table moves in lockstep with the Hub; the repo already requires other optional daemon features. Consequence: shared live lanes (`ghostty-shared`, `ghostty-shared-exit`) need a caller Hub at ≥ `7a09292` after this change.
- A2: One event subscription (one owner+name) is enough for this ticket.
- A3: The transient notice is a single latest-wins slot with a fixed TTL. Multiple matching events within one TTL replace the visible notice; a notice queue is speculative.
- A4: The first-party workflow-context demand set is TUI app policy, not plugin policy: the ticket itself hard-codes the `project-pipelines`/`question.opened` consumption, and the demand set is the durable-plane mirror of that same product decision.
- A5: Fail-closed filtering is correct: when the workflow-context stores have not hydrated (startup, PP absent, recovery in progress) or the focused session maps to no `session_request` row, notices are suppressed rather than shown device-wide.
- A6: The spawn response `session_id` stored on `session_request` rows (`plugin.lua:1531`) is the same Hub session identity the TUI session store holds. Live check 14 asserts this equality before any notice claim; a failure there is a contract defect to route to the owning repository, not a reason to widen matching.
- A7 (corrected): the fail-closed provider surfaces at **subscribe admission**, not as an in-stream frame — Hub returns `OperatorError` and hub-client maps any non-`EntitySubscribed` response to `DaemonTransportError::Protocol` (`lib.rs:362-366`). Both that admission failure and a genuine in-stream `DaemonEntityFrame::Error` mean no usable context: the TUI suppresses every notice and renders no attention state until a healthy generation recovers. The two paths differ in retry handling, per Scope 6a.
- A8: Exponential backoff, not a permanent stop, is the right bound for the always-on families. A user can install or repair PP while the TUI runs, so the policy must cap the request rate while still recovering without a restart.
- A9: An authoritative open-question **count** is sufficient attention UI for this ticket. PP publishes no order field, so any "newest" rendering would be a guess; a count is exact and needs no cross-repository change.

Unknowns for Implement to resolve (not blockers unless noted):

- U1: The minimal PP call sequence that emits `question.opened` inside an IsolatedHub. `project_pipelines_ask_human` via `DaemonRequest::PluginMcpCallTool` is the expected trigger; `record_question` requires an existing run or ticket, so create the minimal project/ticket (and run with a spawned session for run-precedence lanes) through PP MCP tools inside the test.
- U2: How the live lane locates the PP package checkout. Follow the installed-workspaces-driver precedent (caller-provided path environment variable, `EnablePackageLocalPath`, asserted pin floor ≥ `cd7c2f9`).
- U3: Whether the flood lane needs the synthetic `event-plane-producer` fixture (copy the Hub example package into TUI fixtures) or can reach flood/shed with PP alone plus `BOTSTER_HUB_TEST_CLIENT_EVENT_QUEUE_MAX`. Prefer the test knob with the real PP producer.
- U4: Exact placement of the notice-expiry check (`poll_hub` head versus immediately before draw). Either satisfies the ≤100 ms tick; pick one and test it.

## Affected surfaces/files

- `crates/botster-tui/Cargo.toml`, `Cargo.lock` — pin roll (hub-client, hub-test-support, Core set).
- `crates/botster-tui/src/app.rs` —
  - `MINIMUM_CONFORMANCE_FIXTURE_REVISION` (`:92`) and `tui_compatibility_requirement()` (`:9227`): feature + floor;
  - `try_connect` (`:2165`): send `SubscribeEvents`, candidate state;
  - `apply_mux_event` (`:3744`): `PackageEvent` and `EventGap` arms with active-id and workflow filtering;
  - `poll_and_apply_mux_frames` (`:3665`) / `apply_pending_mux_frames` (`:3679`) / `take_pending_mux_frames` (`:9130` region): bounded per-tick drain with surplus retention;
  - `TuiApp` state (`:1356` region): `transient_notice`, candidate/active event subscription state, gap diagnostic;
  - `sync_entity_options_subscriptions` (`:2955`): first-party workflow-context demand set (two families);
  - active-context derivation over `EntityOptionsStore` rows (`entity_options.rs` read helpers as needed, no store mechanics change);
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
- R3: Event flood could steal run-loop time from entity reconciliation or terminal I/O. Bounded by design: ≤ 32 frames applied per tick with surplus retention, O(1) latest-wins notice, nothing in the event path blocks or waits. The flood lane measures the published budgets (check 17).
- R4: `EventGap` arrives before queued events and carries no count. The TUI must not treat it as an error or a resubscribe trigger; a wrong reaction here could churn subscriptions. Unit-tested (check 10).
- R5: A stale `PackageEvent` from a previous connection's subscription id could arrive interleaved around reconnect. The active-id generation match drops it; unit-tested (check 11).
- R6: `workspace_hides_transient_action_feedback` (`app.rs:11998`) asserts the workspace shell hides `action:` feedback text. The new bands are distinct, deliberate surfaces; verify the assertion still holds and intent stays distinguishable.
- R7: Known live-lane flake modes: headless echo can time out after successful Hello (use Ghostty live attach as the live-attach oracle; reproduce on base before attributing), and colon-free `CARGO_TARGET_DIR` is already forced by `test.sh` (this worktree path has no colon).
- R8: The always-on workflow-context subscriptions add two entity connections per TUI instance. Rows are bounded by PP state and the existing per-family pumps are already production mechanics; if PP is absent the subscriptions degrade to bounded diagnostics. If Implement measures meaningful idle cost, narrow the demand set inside this ticket's scope rather than adding configurability.
- R9 (retired): the PP dependency merged at `cd7c2f9` with the family set, frame shape, and sequence semantics this plan assumed. Verified in the revision-4 table; no plan change was required.
- R10: A future PP change could add `run_steps` to the live-published families or move workflow identity, making `run_step.agent_session_uuid` the better context source. This plan deliberately uses `session_request.session_id` because `run_steps` is snapshot-only today. Implement must not switch sources without re-verifying `ENTITY_MUTATION.families` at the pinned PP revision.
- R11: The durable per-family sequence is allocated by CAS with a bounded retry (`provider_snapshot`, 8 attempts). Under heavy concurrent mutation a snapshot can error rather than return a stale sequence. That surfaces as an admission failure and is handled by the Scope 6a backoff, not as a gap requiring resubscribe churn.
- R12: Making the workflow-context families always-on removes the surface-demand guard that previously limited retry attempts. Without the Scope 6a bound, PP being absent or unhealthy would drive a fresh connect, Hello, and `SubscribeEntities` on every ≤100 ms poll tick for two families — roughly twenty attempts per second, indefinitely. The per-family backoff and the check-7a request bound exist specifically to close the defect this plan's own change would otherwise introduce.

## Acceptance checks/tests

### Published budgets

Deterministic unit oracles (work bounds — no wall-clock assertions in unit tests, per [[wall-clock MAX_OWNER_TURN_MS assertions flake under default-concurrency lib load]]):

| Bound | Oracle |
| --- | --- |
| ≤ 32 frames applied per tick (`MUX_APPLY_BATCH_FRAMES`) | unit: 100 pending frames apply ≤ 32 per tick, surplus retained in order, full ordered drain across ticks |
| Notice application O(1), single slot | unit: state inspection under repeated events |

Live-lane production observations (exact numeric limits, measured only in the controlled `package-events` lane): one isolated IsolatedHub per lane run; the Cargo test timeout is the harness backstop, never the pass condition; every measurement is recorded in the test output as a diagnostic; an over-limit result under suspected ambient load is rerun once in isolation and, if it passes isolated but fails under default concurrency, is classified per [[wall-clock ready-operation bounds through a daemon child are ambient-load-sensitive]] with the measured values attached rather than silently retried.

| Budget | Limit | Production oracle |
| --- | --- | --- |
| One `poll_and_apply_mux_frames` tick under flood | < 200 ms | `Instant` elapsed around the production tick call |
| Entity exact-row convergence under flood | ≤ 3,000 ms | converged workflow-context store holds the exact expected row |
| Terminal input echo round-trip under flood | ≤ 3,000 ms | live attach echo through the Core terminal plane |
| Terminal output progress under flood | ≤ 3,000 ms | live attach output bytes advance within the window |

### Repository gates (at Implement and again at Verify)

1. `script/fmt` (`cargo fmt --all -- --check`).
2. `script/clippy` (`-D warnings`).
3. `script/test` (`cargo test --workspace --all-targets`, `BOTSTER_ENV=test`).

### Hermetic unit tests (in `app.rs` `mod tests`)

4. Hello composition: required features include `package_event_subscriptions`; floor is 44; terminal requirement unchanged.
5. Demux: `package_event` and `event_gap` JSON lines parse to `DaemonUnixMuxFrame::Event` and reach `apply_mux_event` through the production `apply_mux_frames` path.
6. Bounded drain: a pending vector of 100 decoded frames applies ≤ 32 per tick, retains surplus in order, and fully drains across ticks with order preserved (work-bound oracle only).
7. Workflow filter (run matching only), with production-shaped `session_request` rows carrying `run_id`, `step_id`, and `ticket_id`:
   - focused session matches a row + payload with equal `run_id` → show;
   - focused session matches a row + payload with a different `run_id` → suppress;
   - focused session matches a row + payload whose `ticket_id` matches but whose `run_id` differs or is absent → suppress (run identity alone decides);
   - no `session_request` row matches the focused session → suppress;
   - no focused session, or unhydrated stores, or PP absent → suppress.
   No ticket-only or step-only tier tests are written: those view states cannot occur in this client, and fixtures for them would assert record shapes PP never produces.
7a. Provider admission failure and in-stream error are distinct (6a):
   - a `subscribe_entities` `Err` (including `Protocol("entity subscription was not accepted")` from the fail-closed provider's `OperatorError`) suppresses every notice, renders no attention state, records one bounded diagnostic per state transition, and schedules the next attempt by backoff;
   - **request bound:** with PP absent, N poll ticks produce at most the backoff-scheduled number of subscribe attempts, not one per tick — the deterministic regression oracle for the always-on demand set;
   - backoff resets after a successful subscribe, so enabling PP later recovers without restart;
   - an in-stream `DaemonEntityFrame::Error` still drops the generation and resubscribes with a fresh id, and that attempt consumes the same per-family backoff budget.
8. Notice policy: matching `PackageEvent` sets one notice; a second matching event replaces it (latest wins); the notice expires after `TRANSIENT_NOTICE_TTL` via the production tick path (deadline injected or clock-controlled, not slept); a payload without `notice` drops with a bounded diagnostic.
9. Response race: scripted frame sequences prove — parked `PackageEvent`/`EventGap` before `EventSubscribed` leave response pairing intact and apply exactly once after promotion; `OperatorError` clears the candidate, drops its parked frames, and leaves durable, entity, and terminal planes untouched.
10. Gap policy: matching `EventGap` clears a visible matching notice, creates none, clears no durable store, sends no request (no replay, no resubscribe), records one bounded diagnostic; a later valid live event still sets a notice.
11. Generation/foreign drop: frames with a cleared candidate id, a prior connection's id, a foreign owner, or a foreign name drop silently.
12. Production demand set: with the Hub connection up, `sync_entity_options_subscriptions` subscribes both workflow-context families independent of any surface; surface-demanded families still compose and keep their existing unbounded-heal behavior; a subscribe failure records a bounded diagnostic and leaves the app functional.
12a. Attention band from entity rows only: with several open questions across two runs plus one answered question, the band shows the exact count of open rows whose `run_id` equals the active run; non-matching-run rows and answered rows are excluded; an answer upsert decrements the count. No ordering or "newest" assertion exists, and the band renders no question text.
13. Reconnect: teardown clears the notice and candidate/active state; the next `try_connect` sends `SubscribeEvents` with a fresh id (observable via `ObservedRequest`); durable attention rendering recovers from fresh entity baselines only.
    Boundary guard: `tui_hub_boundary_uses_public_client_without_private_protocol_plumbing` extended to require the generated `SubscribeEvents` vocabulary and still forbid private protocol plumbing.

### Live Unix proof

Lane: `script/test-live-hub package-events`, IsolatedHub from pinned Hub `7a09292` binaries, real PP package at ≥ `cd7c2f926fcead78e15e7a9c713ad26dfe883914` enabled via `EnablePackageLocalPath` (the lane asserts this floor with `git merge-base --is-ancestor`, following the claim-driver pin-ledger precedent); authentic app-level connect through `connect_and_hello_with_terminal_requirement`, so the proof runs on the final independent Hub control and Core terminal planes; sentinel `package-events-live: complete`. All checks are unblocked.

14. **Live notice through production paths:** create the minimal PP project/ticket/run with a spawned session (U1); assert the `session_request` row's `session_id` equals the TUI-held session id (A6), then focus that session so the production active-context derivation matches; trigger `question.opened` through the PP MCP tool path; assert exactly one transient notice renders through the production apply path, and assert the new question row arrives **as a published upsert on the already-open production workflow-context subscription** with the exact `question_id` and open state ([[acceptance readiness requires the exact expected entity not any authoritative snapshot]]); also assert a non-matching workflow id is suppressed while its durable row still arrives.
15. **Missed event keeps durable state:** with `BOTSTER_HUB_TEST_CLIENT_EVENT_QUEUE_MAX` forcing shed, trigger a question; assert the gap path (cleared/no notice) while the exact durable question row still converges on the existing production subscription and the `workspace-question-attention` band renders it from entity state.
16. **Reconnect without replay, and answer convergence:** after a delivered notice, force reconnect; assert a fresh `SubscribeEvents` id, no replayed notice, and durable question recovery through the production entity baseline; then answer the question through the PP MCP tool and assert the attention band clears via the published status-change upsert on the existing subscription — never via events.
16a. **Contiguous sequence across the baseline boundary:** across checks 14–16, assert the workflow-context families never emit a `NeedsRecovery` resubscribe caused by the baseline→delta sequence step. This is the live regression oracle for the shared durable counter; a spurious gap here means PP changed its sequence source and must be routed to the owning repository, not absorbed by loosening `classify_delta`.
17. **Flood within published budgets:** saturate events (U3) while a workflow-context entity family converges and a live terminal attach echoes; measure every live-lane budget in the table above against its production oracle under the stated isolation/diagnostics/rerun policy; assert event traffic produces zero terminal-plane calls (request oracle and untouched terminal counters).

Downstream proof: none required beyond this repository and its registered PP dependency — the TUI is the terminal consumer in this chain; Hub and Core are pinned, not modified.

## Vault gaps

- Captured (inbox, 2026-08-19): `question.opened` clients subscribe with empty subjects; no hub-test-support event helpers or golden event fixture; `BOTSTER_HUB_TEST_CLIENT_EVENT_QUEUE_MAX` shed knob. Now also reflected in [[project-pipelines-playbook]] as [[question opened clients subscribe with empty subjects]].
- Ready to capture now (the dependency shipped): a package that publishes live entity mutations must allocate baseline and delta `snapshot_seq` from **one durable per-family counter**, or a client's contiguous-delta rule turns every first delta into a false gap and a resubscribe loop. PP `cd7c2f9` does this by CAS-allocating the snapshot sequence from the same `plugin_db` key the mutation path uses, while non-mutation families keep a separate in-memory counter. This is the general rule behind [[botster plugin entities are canonical for plugin-owned dynamic state]] and belongs next to it.
- Ready to capture now: live-published family membership, not field presence, decides whether an entity field is a usable live context source. PP publishes `run_step.agent_session_uuid` but does not live-publish `run_steps`, so `session_request.session_id` is the correct correlation source despite the more obvious-looking field.
- Capture after Implement: the TUI transient-notice pattern (candidate/active subscription state, run-only fail-closed matching, bounded-backoff always-on entity families, count-only entity-driven attention band) so the Web ticket reuses the shape.
- Ready to capture now: a client filter tier is only real if some client view state can produce it. The tiered run/ticket/step rule was sound as a contract but unreachable in a client whose only context source always supplies all three ids. Verify the producing state exists before planning tests for a fallback tier.
- Capture after Implement: applied evidence instance of the hub-client-to-Core pin lockstep cascade (`fd66efd` → `8fce204`).
- Candidate Hub follow-up (not this run): a public test-support event emitter helper, once Web duplicates the TUI's scaffolding.
