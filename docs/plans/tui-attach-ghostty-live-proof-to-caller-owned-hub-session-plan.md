# Plan: Attach Ghostty live proof to a caller-owned Hub session

Ticket: `ticket_1786868597_171437`
Run: `run_1786868609_472512`
Step: `botster_stack_plan` (visit 4)
Plan revision: 4

Parent Hub integration: `ticket_1786661010_115885` / finding `finding_1786868395_783448`.
Sibling Web ticket: `ticket_1786868596_331812` (do not edit Web).
Plan Review: `review_1786869808_756114`, `review_1786870297_420927` (`changes_required`).
Hub occupancy dependency: `ticket_1786870433_515008` **closed**, merge `c72712e2606b8abe77e1b91c2a736791036fadd8`.

## Plan Review corrections

### Rev 1 → rev 2

| Finding | Severity | Fix |
| --- | --- | --- |
| `finding_1786869808_306108` Detach teardown has no real time bound | high | Bounded Detach path with a 2s hard stop. Stub Hub withholds Detach response. |
| `finding_1786869809_718838` Acceptance does not drive actual connection loss | high | Live case cuts the TUI Unix socket. Drive `poll_hub` → `ClientDisconnected`. Sibling + host survive. |
| `finding_1786869809_964237` Required session exit observation is optional | high | First run leaves the session up. Second run stays attached while the caller ends the session. TUI must observe ProcessExited or exact entity `exited`/`failed`. |
| `finding_1786869809_602529` Shared profile still requires unrelated Hub binaries | medium | `ghostty-shared` and `ghostty-shared-exit` skip Hub/worker binary resolution. Wrapper fail-closed on connection + session id only. |

### Rev 2 → rev 3

| Finding | Severity | Fix |
| --- | --- | --- |
| `finding_1786870297_867987` Detach deadline does not bound the write | high | Set `set_write_timeout` from the same 2s deadline **before** `write_frame`. Close the stream on write timeout. Add a peer that saturates the send path. |
| `finding_1786870298_283386` Socket-loss proof still cannot prove server-side release | high | Consumed `botster-hub-client` at Hub `4f30d695` has no exact-pair occupancy oracle. Registered blocking Hub ticket `ticket_1786870433_515008` on `tgt_7e208a0c76a44980a83b63af976b1f22`. |

### Rev 3 → rev 4

| Trigger | Fix |
| --- | --- |
| Hub `ticket_1786870433_515008` merged at `c72712e` | Pin `botster-hub-client` and `botster-hub-test-support` to that revision. Require host feature `attach_occupancy`. After socket EOF, sibling `Status.live_attach_occupancy` must omit the exact old `(session_id, subscription_id)` **and** the Status compatibility must advertise `attach_occupancy`. Empty occupancy without that advertised token is not absence proof. |

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`trybotster/botster-tui`) |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Spawn target path | resolved from `list_spawn_targets`, not from the process working directory |
| Worktree branch | `project-pipelines/ticket_1786868597_171437` |
| Base HEAD | `fc1ff6238ae707c355febbc03eeab5130cccf91c` |
| `teardown_class_applies` | **yes** — multi-client attach, Detach vs ShutdownSession, socket EOF, bounded teardown, ProcessExited / session-entity exit |

Do not infer the repository from the ambient checkout. The ticket `target_id` maps to spawn target `botster-tui`.

## Repository playbook loaded

- [[botster-tui-playbook]]

## Other role and surface playbooks and atomic notes loaded

Role overlays:

- [[planner-playbook]]
- [[botster-planner-playbook]]
- [[botster-architecture]]
- [[cli-patterns]]
- [[identity]]
- [[goals]]

Consumed-kit overlay (TUI charter Must Load; this ticket does not change kit):

- [[botster-tui-kit-playbook]]

Runtime-teardown class (applies):

- [[botster runtime teardown lenses]]
- [[mux envelope delivery does not prove Hub route ownership]]
- [[daemon socket attach must detach subscriptions on disconnect and exit]]
- [[WebRTC DataChannel local close uses the peer close bound before cleanup]] (bound-then-cleanup shape only; this ticket is Unix, not WebRTC)

Targeted notes:

- [[tui client attach uses hub protocol not session protocol]]
- [[botster hub client crate is the external client boundary]]
- [[first-party Unix attach clients use split Hello and subscription close events]]
- [[first-party clients put terminal mechanism tokens only in terminal compatibility]]
- [[TUI contract matrix headless echo can time out after successful Hello]]
- [[canceling incremental attach aborts the decoder and sends Detach]]
- [[pre READY attach failed ends client hydration]]
- [[unix socket ipc for tui detach reattach]]
- [[tui and browser are equal clients]]
- [[tui and socket terminal streams use clientworker transport adapters]]
- [[incremental GHOSTSNP uses one decoder per subscription]]
- [[incremental GHOSTSNP attach streams READY history pages and FINISH]]
- [[Unix mux polling returns bounded complete-frame batches while input stays readable]]
- [[Core terminal subscription ownership is session, subscription, and generation]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]
- [[first-party Rust consumers pin the UI contract Git tag not a Hub rev]]
- [[test script required for rust tests not cargo test]]
- [[plan agents must author vault context as wikilinks not home paths]]
- [[pipeline vault checklists must cite exact resolvable note titles]]
- [[vault example paths are not repository placement conventions]]
- [[colon worktree paths break cargo dyld library paths]]

Not loaded:

- [[project-pipelines-playbook]] — this ticket does not change Project Pipelines package or plugin paths.
- [[botster-hub-playbook]] — Hub is not the target. Do not use it as a substitute charter.
- [[botster-web-playbook]] — Web is a sibling consumer, not this target.
- [[botster-terminal-ghostty-playbook]] — Ghostty decode stays in the pinned Core crate. This ticket consumes it.

## Context loaded

### Ticket intent

`script/test-live-hub ghostty` always starts `IsolatedHubBuilder` and spawns its own session. The north-star parent must attach authentic Web and TUI to one live session. This ticket adds a TUI-owned attach-only live profile that joins a caller-owned Hub session.

### Current production facts (`fc1ff62`)

- Pins today: Hub client / test-support `4f30d6952f9a29541ab3a670a54bf5e136b8eb8e`; Core / Ghostty / protocol-client `f4f6bf5babe92dfb9241a760c414187f711c2c42`; UI contract tag `botster-ui-contract-v0.3.2`; kit `c83ba6c518e2324e34ce24c7abe5a8a05e56293c`.
- **Required pin bump (this ticket):** `botster-hub-client` and `botster-hub-test-support` to Hub `c72712e2606b8abe77e1b91c2a736791036fadd8`. That revision ships `FEATURE_ATTACH_OCCUPANCY`, `DaemonCompatibilityRequirement::for_attach_occupancy()`, and `DaemonStatus.live_attach_occupancy: Vec<DaemonAttachOccupancy { session_id, subscription_id, generation }>`. Hub `CONFORMANCE_FIXTURE_REVISION` is **43**.
- Host Hello must add `FEATURE_ATTACH_OCCUPANCY`. Keep terminal mechanism tokens only on `terminal_compatibility`. Bump TUI `MINIMUM_CONFORMANCE_FIXTURE_REVISION` to **43** so the occupancy matrix is required.
- Production connect is `HubConnection::connect` → `connect_and_hello_with_terminal_requirement`.
- Host Hello is `tui_compatibility_requirement()`: protocol 7, conformance floor 40, host-plane tokens only.
- Terminal Hello is `TerminalCompatibilityRequirement::for_ready_then_history_attach()` with `client_name = "botster-tui"`.
- Attach uses `DaemonRequest::Attach` with a minted `subscription_id` and one incremental Ghostty decoder.
- `detach_attached` and `recover_current_subscription` send `Detach` and abort the decoder.
- `HubConnection::request` calls `set_read_timeout(None)` and waits forever for a response.
- `force_reconnect` and `record_transport_error` drop the client and clear attach state. They do **not** send `Detach`. They do **not** send `ShutdownSession`.
- `poll_hub` on a readable EOF maps to `DaemonTransportError::ClientDisconnected` and `record_transport_error`.
- Existing live Ghostty oracles live in `headless_live_runtime_ghostty_install_scrollback_palette_and_mode_gated_input`. That test starts IsolatedHub, spawns sessions, and calls `ShutdownSession`.
- `script/test-live-hub` resolves `BOTSTER_HUB_BIN` and `BOTSTER_SESSION_WORKER_BIN` before every mode, including modes that should only attach.
- Caller-owned Hub injectors already exist for Workspaces acceptance. There is no `BOTSTER_SHARED_SESSION_ID` and no `ghostty-shared` profile.
- Consumed `botster-hub-client` at `4f30d695` public surface: `DaemonRequest` has Attach/Detach/Drain/Status, not a list of attach routes. `DaemonStatus.lifecycle_counters.live_attach_subscriptions` is a `u64` counter. `TerminalSubscriptionClosed` is documented as **not** emitted on connection death, Detach, process exit, or session removal. `live_attach_routes` and `list_terminal_subscriptions()` are Hub-internal.

### Parent contract (do not implement Hub here)

Parent plan rev 2 locks session id `north-star-shared` unless the Hub coordinator prints another exact id. Parent owns the shared producer. Parent will call TUI `ghostty-shared` and the Web caller-owned terminal lane against that id.

Human answer `question_1786867995_904640`: “Clients pin protocol versions, not Hub Git revisions” applies to **terminal-plane compatibility identity**. TUI may keep Cargo Git revisions for `botster-hub-client` and `botster-hub-test-support` as build provenance. No Hub Git SHA may gate terminal compatibility.

### Session-type eligibility parent rules

Those rules do **not** apply to this ticket.

This ticket attaches to a caller-owned session id. It must not spawn. It must not call `list_session_types_for_target`. It must not filter by client `target_id` equality. Session-type Option A stays on the Hub parent and the Web sibling.

### Worktree hygiene

- Tracked `.gitignore` has 7 lines. Do not truncate it.
- This worktree path has no `:`. Do not invent a `CARGO_TARGET_DIR` for colon hygiene. Keep the existing live-hub temp target-dir pattern.

## Scope

Add attach-only live profiles and the smallest production teardown fixes required to prove them.

1. Add `script/test-live-hub ghostty-shared` and `script/test-live-hub ghostty-shared-exit`.
   - Require only `BOTSTER_HUB_CONNECTION` and `BOTSTER_SHARED_SESSION_ID`.
   - Fail closed in the wrapper when either is missing, empty, or malformed.
   - Do **not** resolve `BOTSTER_HUB_BIN` or `BOTSTER_SESSION_WORKER_BIN`.
   - Do **not** export those binaries into the test environment.
   - Do not start IsolatedHub.
   - Do not spawn a session.
   - Do not send `ShutdownSession` from the TUI.
   - Keep IsolatedHub `ghostty` binary resolution unchanged.

2. Add production-path tests that parse `BOTSTER_HUB_CONNECTION` through `parse_hub_connection` / `AppArgs`, connect with `HubConnection::connect`, wait for the **exact** session entity `BOTSTER_SHARED_SESSION_ID`, and attach with the production Ghostty decoder.

3. Bound Detach on the production teardown path.
   - Constant: `DETACH_ON_DISCONNECT_BOUND = Duration::from_secs(2)`.
   - Do not send teardown Detach through unbounded `HubConnection::request`.
   - Add `request_with_deadline(request, deadline)` used by disconnect, `force_reconnect`, cancel, and `record_transport_error` when a live owner pair exists and the stream is still writable.
   - Before `write_frame`, call `set_write_timeout(Some(remaining))` from the same deadline. After the write, use `set_read_timeout(Some(remaining))` for the response. Do not start the read deadline only after an unbounded write.
   - When write or read deadline fires: `shutdown(Shutdown::Both)`, drop the client, abort the decoder, retire the subscription locally, fail closed. Never `ShutdownSession`.
   - Prove withheld **response**: TUI-owned Unix stub completes Hello and Attach, accepts the Detach write, and withholds the response. Production handler returns in less than 3s.
   - Prove blocked **write**: after Hello and Attach, the stub stops reading. The test fills the TUI send buffer on the same `UnixStream` until write would block, then drives the production teardown Detach. Production handler returns in less than 3s, fail-closed, no ShutdownSession. A stub that accepts the small Detach write is not this test.

4. Drive **actual** TUI connection loss on the live caller-owned Hub.
   - After attach, keep a sibling `HubConnection` attached to the same session.
   - Cut the TUI Unix socket (peer `shutdown` / close the TUI fd). Do not call a clean `detach_attached` first.
   - Drive `poll_hub` so production `ClientDisconnected` → `record_transport_error` runs.
   - **Release proof (required, Hub-visible, exact pair):** after EOF, the sibling issues production `DaemonRequest::Status` on a connection whose Hello required `attach_occupancy`. Assert:
     1. `status.compatibility.features` contains `attach_occupancy` (empty occupancy without this advertised token is **not** absence proof).
     2. `status.live_attach_occupancy` contains **no** row whose `(session_id, subscription_id)` equals the dead TUI pair.
     3. `status.live_attach_occupancy` still contains the sibling pair (isolation).
     4. `generation` on remaining rows is identity evidence; do not treat a later generation of the same subscription id as the dead owner.
   - `Status.lifecycle_counters.live_attach_subscriptions` is a counter only and is not sufficient. Local `retired_subscription_ids`, sibling send-and-echo, and a new Attach with a new `subscription_id` are isolation/reconnect oracles, not release proof.
   - Isolation oracles still required: sibling send+echo after the cut; exact session entity still running; TUI sent no ShutdownSession; a new TUI attach with a new `subscription_id` reaches `attached`.
   - Hub `ticket_1786870433_515008` is **closed**. Implement pins `c72712e` and consumes this oracle now.

5. Require session-exit observation on the second invocation.
   - `ghostty-shared` leaves the host session running.
   - `ghostty-shared-exit` attaches to the same id, stays connected, and waits for the **caller** to end that session.
   - TUI must observe ProcessExited **or** the exact session entity becoming `exited` or `failed`.
   - TUI observed requests must not include `ShutdownSession`.
   - The caller ends the session through Hub control that the TUI does not send.

6. Document both wrapper modes and the caller end-session step in `README.md`.

7. Fail-closed unit coverage that does not start a Hub: missing connection, missing session id, malformed JSON.

## Non-scope

- Do not edit `botster-hub` or `botster-web`.
- Do not change IsolatedHub `ghostty` oracles, flood / write-budget, or silent-session ShutdownSession coverage.
- Do not spawn, create, or replace the shared session from the TUI profile.
- Do not use a Hub Git SHA as the terminal compatibility gate.
- Do not republish Hub crates only to remove Cargo revision pins.
- Do not change kit, Ghostty crate internals, host `PROTOCOL_VERSION`, or host feature tokens.
- Do not bound every `HubConnection::request` (Status, Attach, SendInput). Bound teardown Detach only.
- Do not add Workspaces, session-types, or contract-matrix work to this profile.
- Do not treat IsolatedHub restart or `force_reconnect` without a socket cut as connection-loss proof.
- Do not add optional flags that skip oracles.
- Do not create a pull request. Merge directly into `main`.

## Repository ownership boundaries and cross-repo dependencies

| Owner | Responsibility |
| --- | --- |
| **botster-tui** (this ticket) | Caller-owned Ghostty attach profiles, wrapper fail-closed injectors, bounded Detach, production socket-loss handler, TUI oracles, README caller contract. |
| **botster-hub** (`tgt_7e208a0c76a44980a83b63af976b1f22`) | Long-lived Hub, session `north-star-shared`, shared producer, admission, grants, adapters, host Hello, caller-side session end. Occupancy oracle shipped by closed `ticket_1786870433_515008` at `c72712e`. Parent ticket `ticket_1786661010_115885` already depends on this TUI ticket. |
| **botster-web** (`tgt_40abcf71ccf049f4ac0c99953a799869`) | Sibling caller-owned terminal lane. Parallel. Do not edit. |
| **botster-core / terminal-ghostty** | Decoder, READY/PAGE/FINISH, `decoded_bytes()`. Consume the existing pin. |
| **botster-tui-kit** | Hit map and TerminalView chrome. Do not change. |
| **botster-hub-client** | Public Attach / Detach / Hello / entity DTOs. Consume the existing pin. |

Registered dependency `dependency_1786870438_296010` is **closed**. Consume Hub `c72712e` in this TUI ticket. Do not edit Hub in this TUI worktree.

## Product architecture

```text
Caller owns Hub + session BOTSTER_SHARED_SESSION_ID + producer
        │
        │ BOTSTER_HUB_CONNECTION + BOTSTER_SHARED_SESSION_ID
        │ (no BOTSTER_HUB_BIN)
        ▼
script/test-live-hub ghostty-shared          # run 1: attach oracles + socket cut
script/test-live-hub ghostty-shared-exit     # run 2: attach, then caller ends session
        │
        ▼
TuiApp production path
  parse_hub_connection → HubConnection::connect (split Hello)
  exact session-entity id → Attach + one Ghostty decoder
  run 1: cut TUI socket → poll_hub ClientDisconnected
         sibling stays live; new TUI attach; no ShutdownSession
  run 2: stay attached → observe ProcessExited or entity exited/failed
```

### Locked caller contract

The TUI profile never creates this session. The caller must supply it.

| Injector | Rule |
| --- | --- |
| `BOTSTER_HUB_CONNECTION` | Required. Existing production JSON descriptor. Wrapper fail-closed if missing. |
| `BOTSTER_SHARED_SESSION_ID` | Required. Default parent id is `north-star-shared`. TUI must not rewrite it. |
| Hub / worker binaries | **Forbidden** as a `ghostty-shared` prerequisite. IsolatedHub modes may still resolve them. |
| History token | Caller must write `NORTH_STAR_HISTORY` before the first TUI attach. |
| Input echo | Session must echo a TUI `SendInput` line so the TUI can observe `NORTH_STAR_TUI_<suffix>`. |
| Run 1 lifetime | Session must stay running across the socket cut and until run 2 attaches. |
| Run 2 end | After run 2 is attached, the **caller** ends that session. TUI does not send ShutdownSession. |

`BOTSTER_HUB_DATA_DIR` is not required for this attach profile.

### Locked TUI oracles (one session identity)

All oracles use `BOTSTER_SHARED_SESSION_ID`. Do not attach the first listed session.

**Run 1 — `ghostty-shared` (session stays alive)**

1. **Attach chronology.** READY, later PAGE history when present, FINISH or `snapshot_history_incomplete`, then `attached`. One decoder.
2. **Exact bytes.** `decoded_bytes()` into `apply_terminal_output`. After TUI sends `NORTH_STAR_TUI_<suffix>\n`, applied payloads contain those exact ASCII bytes (or the echo form). No UTF-8 repair to `U+FFFD`.
3. **Late-attach / history.** First attach is late. ScrollOp::Top / viewport shows `NORTH_STAR_HISTORY`.
4. **Resize.** Production `TerminalResize` sends `DaemonRequest::Resize`. Hub accepts. Local viewport updates. Worker Snapshot dimensions if present.
5. **Input.** Production SendInput / mode-gated input. Live marker above.
6. **Cancellation.** `detach_attached` aborts the decoder and sends bounded Detach. No ShutdownSession. Session entity stays running. Later Attach with a new `subscription_id` succeeds.
7. **Actual connection loss.** Sibling `HubConnection` is attached first. Cut the TUI Unix socket. Drive `poll_hub` until production `ClientDisconnected` / `record_transport_error` runs. Prove:
   - **exact old `(session_id, subscription_id)` is absent** from sibling `Status.live_attach_occupancy` while `attach_occupancy` is advertised (not a TUI-local map; not the occupancy counter)
   - sibling still receives a later live frame **and** a later sibling `SendInput` echoes
   - exact session entity is still running
   - TUI observed requests contain no `ShutdownSession`
   - a new TUI connection attaches to the same session with a new `subscription_id`
8. **Clean reconnect after loss.** The new TUI attach still shows `NORTH_STAR_HISTORY` and a later live marker.

Print `ghostty-shared-complete`. Leave the host session running.

**Run 2 — `ghostty-shared-exit` (caller ends the session)**

1. Connect and attach the same `BOTSTER_SHARED_SESSION_ID`.
2. Print `ghostty-shared-exit-attached` so the caller can end the session.
3. Stay connected. Do not send `ShutdownSession`.
4. Observe ProcessExited **or** the exact session entity lifecycle `exited` or `failed`.
5. Print `ghostty-shared-exit-complete`.

A running entity after run 1 is **not** the exit oracle. IsolatedHub `ghostty` ShutdownSession ProcessExit is **not** a substitute for run 2.

### Production teardown path

Two production entries must be distinct and both proven:

| Entry | How it is driven | Required behavior |
| --- | --- | --- |
| Clean cancel / `detach_attached` | Test calls production detach | Bounded Detach, abort decoder, session lives |
| Actual connection loss | Cut Unix socket, then `poll_hub` | `ClientDisconnected` handler. If the stream is still writable, bounded Detach. If not, skip send, hard-stop locally. Never hang. Never ShutdownSession. |

`force_reconnect` must use the same bounded Detach-if-writable helper before it drops the client. Rev 1 “send Detach then drop” is not connection-loss proof.

### Bounded Detach (locked)

```text
DETACH_ON_DISCONNECT_BOUND = 2 seconds
deadline = Instant::now() + bound

request_with_deadline(Detach, deadline):
  remaining = deadline.saturating_duration_since(now)
  stream.set_write_timeout(Some(remaining))
  write_frame(Detach)   # write timeout must fire here if the peer does not read
  loop until response or deadline
    remaining = deadline.saturating_duration_since(now)
    stream.set_read_timeout(Some(remaining))
    read / decode complete mux frames
  on write timeout, read timeout, or deadline:
    stream.shutdown(Both)
    return timeout error
  caller:
    abort decoder
    retire subscription
    client = None
    no ShutdownSession
```

Do not use `set_read_timeout(None)` or an unbounded write on this path.

## Runtime-teardown lens answers

| Field | Answer |
| --- | --- |
| `teardown_class_applies` | yes. Multi-client attach. Socket EOF vs clean Detach. Bounded teardown. Caller-ended ProcessExited / entity exit. |
| `teardown_isolation` | One TUI subscription dies: that `(session_id, subscription_id, generation)` only. Sibling Unix attach and the host session stay up after TUI socket cut. |
| `teardown_bounds` | Teardown Detach uses `DETACH_ON_DISCONNECT_BOUND` (2s) on **write and read**. Deadline closes the stream. No unbounded `request()` or unbounded `write_frame` on this path. Withheld-response stub and saturated-send stub must each return in < 3s. |
| `late_message_matrix` | See table below. |
| `production_path_proof` | Live `ghostty-shared` cuts the TUI socket and drives `poll_hub` → `ClientDisconnected`. Sibling `Status` advertises `attach_occupancy` and `live_attach_occupancy` omits the exact old pair while keeping the sibling pair. Sibling send+echo after the cut. New TUI attach. Live `ghostty-shared-exit` observes caller-ended ProcessExited or exact entity exit. Stubs prove 2s write-bound and read-bound Detach hard stops. IsolatedHub `ghostty` remains write-budget proof only. |
| `ownership_identity` | Core owner is `(session_id, subscription_id, generation)`. TUI lookup is `(session_id, subscription_id)`. `generation` is close evidence only. New attach after loss mints a new subscription id. Delayed close for a retired id must not kill the new owner. Sibling ownership is a separate pair. Mux delivery alone does not prove that pair. |
| `sibling_fail_closed_policy` | On TUI socket cut or Detach timeout: sibling and host session keep working. On ultimate TUI close failure: still no ShutdownSession; fail the TUI client closed; host session stays up. |

### Late-message matrix

| Message | Tag | After TUI terminal / disconnect | Sweep |
| --- | --- | --- | --- |
| `Attach` | new `subscription_id` + session id | Reject if exact entity is not attachable. | Retired ids stay in `retired_subscription_ids`. |
| `Detach` | same pair | Required when stream is writable. Bounded to 2s. | Abort decoder. Drop local frames. Close stream on timeout. |
| `SendInput` / `ModeGatedInput` / `Resize` | attached pair + mode freshness | Ignore after detach or ClientDisconnected. | Pending input/resize die with hydration. |
| `SubscribeEntities` (session) | TUI entity subscription id | Stays up across terminal Detach. Dropped on full client disconnect. Used in run 2 to observe exit. | Existing pump Drop. |
| `UnsubscribeEntities` | same entity subscription | Used on full reconnect cleanup. | Existing invalidate helpers. |
| Hello / host Status | connection | Host plane stays readable after terminal Detach. Dead after socket cut. | No session teardown. |
| `TerminalSubscriptionClosed` | `(session_id, subscription_id)`; generation is evidence | One recovery Attach, then fail closed. | Retire old subscription. |
| Socket EOF / `ClientDisconnected` | this Unix client | Drive `record_transport_error`. Bounded Detach if writable. Else local hard-stop. Never ShutdownSession. | Host session remains. Sibling pair remains. |
| `ShutdownSession` | host policy | **Forbidden** from TUI shared profiles and from TUI disconnect. Caller may end the session from Hub control during run 2. | IsolatedHub `ghostty` may still send it. |
| ProcessExited / entity `exited`/`failed` | exact `BOTSTER_SHARED_SESSION_ID` | Required observation in run 2 after the caller ends the session. | TUI detaches locally. No ShutdownSession. |

## Assumptions and unknowns

Assumptions:

- Parent / Verify starts the Hub and session **outside** these profiles. IsolatedHub as that external caller is allowed. IsolatedHub **inside** `ghostty-shared` is not.
- Caller writes `NORTH_STAR_HISTORY` and echoes TUI input.
- Run 1 never ends the session. Run 2 never sends ShutdownSession. The caller ends the session after `ghostty-shared-exit-attached`.
- Cargo Hub revision pins stay as build provenance. Terminal acceptance uses protocol version and feature tokens only.
- Existing `BOTSTER_HUB_CONNECTION` JSON shape is the contract.
- A TUI-owned Unix stub is valid production-path proof for the Detach **time** bound because it drives the same `request_with_deadline` + disconnect handler. It is not a substitute for live socket-cut **release** proof.
- Hub `c72712e` is the occupancy pin. Caller-owned live Hub for `ghostty-shared` must advertise `attach_occupancy`.

Unknowns:

- Whether the parent producer is a shell echo loop. Locked tokens remove that ambiguity for TUI.
- Zig / Ghostty native build environment.

Convention conflicts: none after the Plan Review corrections.

## Affected surfaces / files

- `crates/botster-tui/Cargo.toml` and `Cargo.lock` — pin `botster-hub-client` and `botster-hub-test-support` to `c72712e2606b8abe77e1b91c2a736791036fadd8`. Keep UI contract on tag `botster-ui-contract-v0.3.2`. Keep Core / Ghostty pins unless the Hub pin forces a compile break.
- `script/test-live-hub` — `ghostty-shared` and `ghostty-shared-exit`; skip binary resolution; wrapper fail-closed on connection + session id; grep `ghostty-shared-complete` and `ghostty-shared-exit-complete`.
- `crates/botster-tui/src/app.rs` — add `FEATURE_ATTACH_OCCUPANCY` to host Hello; bump conformance floor to 43; shared-profile tests; `request_with_deadline` write+read; production Detach-on-teardown; sibling Status occupancy assertions; IsolatedHub `ghostty` unchanged except compile/fixture floor if required by the pin.
- `README.md` — caller injectors, no Hub binaries, two wrapper modes, caller end-session step, occupancy pin.
- `docs/plans/tui-attach-ghostty-live-proof-to-caller-owned-hub-session-plan.md` — this plan.

## Risks

- Treating IsolatedHub `ghostty` as the shared-session proof.
- Treating `force_reconnect` without a socket cut as connection-loss proof.
- Treating a still-running entity as ProcessExited.
- Treating local retirement, sibling echo, replacement Attach, or `live_attach_subscriptions` as exact-pair release.
- Requiring Hub binaries and silently keeping an IsolatedHub-shaped gate.
- Bounding only the Detach read and leaving `write_frame` unbounded.
- Attaching the first listed session.
- Using a Hub Git SHA as a terminal compatibility assert.
- Soft-skipping live proof when injectors are missing.
- Pinning Hub client without requiring `attach_occupancy` on Hello, then treating empty occupancy as release.
- Dual-pipelining teardown into a second TUI ticket. One TUI Plan → Implement path. Hub occupancy is a registered dependency ticket, not a second TUI pipeline.

## Acceptance checks / tests

Fail-closed (no Hub):

- Missing `BOTSTER_HUB_CONNECTION` fails in the wrapper and in the test.
- Missing `BOTSTER_SHARED_SESSION_ID` fails with an explicit message.
- Malformed connection JSON fails closed.
- Wrapper for `ghostty-shared` / `ghostty-shared-exit` does not call `resolve_binary` and does not require `BOTSTER_HUB_BIN`.

Bounded Detach stubs (no live Hub):

- Unix stub completes Hello + Attach, accepts Detach, withholds the **response**. Production handler returns in less than 3 seconds.
- After Hello + Attach, stub stops reading. Test fills the TUI send buffer until write would block, then drives production teardown Detach. Production handler returns in less than 3 seconds.
- Both stubs: TUI is fail-closed. No `ShutdownSession`.

Live caller-owned Hub (caller supplies connection + session id only):

```sh
export BOTSTER_HUB_CONNECTION='{"transport":{"type":"unix_socket","path":"<hub.sock>"}}'
export BOTSTER_SHARED_SESSION_ID=north-star-shared
export BOTSTER_TUI_REQUIRE_HUB_TEST=1
# Do not set BOTSTER_HUB_BIN or BOTSTER_SESSION_WORKER_BIN.

script/test-live-hub ghostty-shared
# session must still be running

# Start the exit profile, wait for ghostty-shared-exit-attached, then
# end the session from the caller Hub control plane (not from TUI).
script/test-live-hub ghostty-shared-exit
```

`ghostty-shared` must print `ghostty-shared-complete` and prove chronology, exact bytes, history, resize, input, cancel, **socket cut through `poll_hub`**, sibling `Status` advertises `attach_occupancy`, **exact old pair absent from `live_attach_occupancy`**, sibling pair still present, sibling send+echo, new TUI attach, and a still-running exact session entity. Zero TUI `ShutdownSession`. Caller Hub binaries for this profile are still forbidden; the **TUI crate** pins Hub client `c72712e`.

`ghostty-shared-exit` must print `ghostty-shared-exit-attached` then `ghostty-shared-exit-complete`. It must observe ProcessExited or exact entity `exited`/`failed`. Zero TUI `ShutdownSession`.

Repo gates:

- `script/fmt`
- `script/clippy`
- `script/test`
- `script/test-live-hub ghostty` still prints `ghostty-live-complete` (IsolatedHub profile unchanged; binaries still required there)

Downstream:

- After merge, Hub `ticket_1786661010_115885` can attach Web and TUI to `north-star-shared`. This ticket does not perform that joint proof.

## Vault gaps worth capturing

- After Implement proves both profiles, capture one note: TUI live Ghostty has IsolatedHub `ghostty`, attach-only `ghostty-shared`, and `ghostty-shared-exit`. Do not capture during Plan.
- No Plan-time inbox capture.
- Vault checklist already exists for this ticket (`checklist_1786869035_240652`). This visit skips a duplicate.

## Implement sequence

1. Restore `.gitignore` from HEAD if a later step wipes it. Never truncate.
2. Pin `botster-hub-client` and `botster-hub-test-support` to `c72712e2606b8abe77e1b91c2a736791036fadd8`. Add `FEATURE_ATTACH_OCCUPANCY` to host Hello. Set `MINIMUM_CONFORMANCE_FIXTURE_REVISION` to 43.
3. Add `request_with_deadline` with write timeout then read timeout. Add withheld-response and saturated-send stub tests.
4. On `record_transport_error` / `force_reconnect`, Detach-if-writable with the 2s write+read bound before dropping the client.
5. Add fail-closed session-id + connection checks.
6. Add `ghostty-shared` live test: exact-id attach, oracles, sibling, socket cut, `poll_hub`, Hub occupancy exact-absence, new attach.
7. Add `ghostty-shared-exit` live test: attach, wait for caller-ended exit observation.
8. Wire both wrapper modes. Skip binary resolution. Update README.
9. Keep IsolatedHub `ghostty` intact.
10. Run the acceptance commands against a caller-owned Hub.
11. Merge to `main`. Do not open a PR.

## Implement adaptations

- IsolatedHub `ghostty` flood Status is issued immediately after sibling attach, before the poll loop. Hub `c72712e` can emit `core_adapter_closed` on the first `poll_hub`, which skipped the host-Status oracle when it lived inside the loop. Flood/write-budget assertions are otherwise unchanged.
- `script/test-live-hub ghostty-shared` and `ghostty-shared-exit` stream cargo test output. Buffering until process exit hid `ghostty-shared-exit-attached` from the caller and deadlocked run 2.
- Hub client `c72712e` no longer re-exports terminal mechanism tokens. TUI tests import those tokens from `botster-terminal-protocol-client`.
