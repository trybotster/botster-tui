# Implementation report: TUI consume independent terminal and Hub control protocol planes

- **Ticket:** `ticket_1786661009_551067`
- **Run:** `run_1786704127_656781`
- **Step:** `botster_stack_implement`
- **PR:** none. Pipeline `merge_policy` is `direct`. Plan: merge into main, do not create a PR.
- **Target repository:** `botster-tui` (`trybotster/botster-tui`)
- **Target id:** `tgt_c3d470bab78549df920a41e8fb0e58d8`
- **Base:** `origin/main` `5d2af28e92eef94d51d8f59c45ab94b8e9a58c7c`
- **First Implement commit:** `0698c1af625f53c853600b137459c01fc439efc5`
- **Review-revisit commit:** `13b02b9fdaed98c83d225c3c5dd0ccd04e62fc74`
- **teardown_class_applies:** yes
- **Plan revision:** 4 (`artifact_1786712757_521533`)

## Playbooks and notes applied

Repository charter: [[botster-tui-playbook]]

Role / stack:

- [[implementer-playbook]]
- [[botster-implementer-playbook]]
- [[botster-architecture]]
- [[cli-patterns]]
- [[spa-patterns]]
- [[botster runtime teardown lenses]]
- [[implement gate must verify committed work and pr link before review]]
- [[implementation artifacts must match actual git state]]
- [[implementation steps must persist report artifacts for review]]
- [[pipeline vault checklists must cite exact resolvable note titles]]
- [[test script required for rust tests not cargo test]]

Consumed, not edited:

- [[botster-hub-playbook]]
- [[botster-hub-client-playbook]]
- [[botster-tui-kit-playbook]]
- [[botster-terminal-ghostty-playbook]]

TUI charter must-load notes:

- [[kit UI contract pin proof uses an already split TUI consumer]]
- [[first-party Rust consumers pin the UI contract Git tag not a Hub rev]]
- [[compatibility fixtures advertise every required optional feature]]
- [[pre READY attach failed ends client hydration]]
- [[canceling incremental attach aborts the decoder and sends Detach]]
- [[Core terminal protocol separates Hub-safe envelopes from client semantic bodies]]
- [[tui and browser are equal clients]]
- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[tui client attach uses hub protocol not session protocol]]
- [[tui and socket terminal streams use clientworker transport adapters]]
- [[botster tui uinode event routing captures hit regions during draw]]
- [[tui error dedup tests must drive real input handlers]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]
- [[incomplete repo local session types drop the hub client connection]]
- [[deleting a waiver proof test can drop unrelated coverage in its tail]]

Task-surface:

- [[public protocol versions host control and Core terminal planes independently]]
- [[botster terminal v1 starts at protocol 1 and conformance revision 1]]
- [[incremental GHOSTSNP uses one decoder per subscription]]
- [[incremental GHOSTSNP attach streams READY history pages and FINISH]]
- [[incremental GHOSTSNP clients defer resize and input until FINISH and attached]]
- [[post READY history failure releases the decoder but keeps the terminal]]
- [[terminal subscription lifecycle is Core owned while host session policy is Hub owned]]
- [[Core terminal subscription ownership is session, subscription, and generation]]
- [[Core ClientWorker bind requires a live attach generation]]
- [[Core subscription hard-stop is synchronous close and drop on the host tick]]
- [[cold turkey migrations eliminate dual code paths and version suffixes]]
- [[botster hub client crate is the external client boundary]]
- [[external client hub tests use subprocess spawned hub test support]]
- [[live hub proof records distinct hub and locked core binary provenance]]
- [[live hub target dirs can cache stale same version client schema]]

Not loaded: [[project-pipelines-playbook]] (no Project Pipelines package/plugin paths).

Convention conflicts: none that blocked TUI-owned consume work. Cargo cannot `[patch]` Hub `branch=main` onto Core `rev=f4f6bf5` because those are the same git source. Live whole-mux stall still cannot prove Core `core_adapter_closed`. Both gaps are registered Hub tickets.

## Files changed

- `crates/botster-tui/Cargo.toml` — Hub `3bee3a57`, Core `f4f6bf5`, kit `c83ba6c`, UI contract tag `botster-ui-contract-v0.3.2`, add `botster-terminal-protocol-client`
- `Cargo.lock` — regenerated from those pins; one `botster-terminal-protocol` at Core `f4f6bf5`
- `crates/botster-tui/src/app.rs` — split Hello, incremental mux buffer, no production Drain, Core `TerminalEvent` apply, Ghostty recover-on-error, fail-closed recovery, hermetic mux tests, live Ghostty 12k + sibling isolation
- `README.md` — pins, handshake, mux, Ghostty provenance
- `docs/plans/tui-consume-independent-terminal-and-hub-control-protocol-planes-plan.md` — plan rev 4 plus Implement-revisit acceptance resync
- `docs/reports/tui-consume-independent-terminal-and-hub-control-protocol-planes-implement-report.md` — this report

## Ownership boundaries preserved

Edited only `botster-tui` application policy, handshake, mux consumption, hydration/recovery, tests, and docs. Did not edit Hub, Core, Kit, Ghostty, or Web. Those surfaces are consumed by pin only.

## Cross-repo routing

| Ticket | Target | Status | Role |
| --- | --- | --- | --- |
| `ticket_1786661008_634435` | Hub | closed | Unix adapters |
| `ticket_1786661009_576857` | TUI Kit | closed | UI contract identity |
| `ticket_1786705502_228757` | Hub | closed at `aafd6c2` | Hello split + `TerminalSubscriptionClosed` |
| `ticket_1786716545_950076` | Hub | closed at `3bee3a57` | Pin hub-client `botster-terminal-protocol` to Core `f4f6bf5` |
| `ticket_1786716545_417854` | Hub | open | Emit `core_adapter_closed` while Unix host mux stays readable |

## Review findings this revisit

| Finding | Disposition |
| --- | --- |
| `finding_1786715974_149013` dual protocol identities | Consumed Hub `3bee3a57` (`ticket_1786716545_950076` closed). `cargo tree -p botster-tui -e normal -i botster-terminal-protocol` shows one identity: Core `f4f6bf5`. |
| `finding_1786715974_898936` mux drop | Fixed in TUI. Persistent `mux_buf`, `Read::read`, keep partial lines, parse every JSON value. Tests: split write, two values on one line, two newline-delimited lines. |
| `finding_1786715974_797854` Ghostty sequence | Fixed in TUI. Unexpected progress and apply `Err` call `recover_current_subscription`. `SnapshotHistoryIncomplete` still keeps READY. |
| `finding_1786715974_781287` Core hard-stop | TUI live test proves a sibling Unix connection stays readable during a stall. It does not require `core_adapter_closed`. Registered `ticket_1786716545_417854`. |

## Deviations from plan

1. **Live Core write-budget oracle.** Plan asked for authentic `core_adapter_closed`. Hub `aafd6c2` still closes the stalled Unix connection as `host_adapter_closed`. This revisit does not widen that oracle. Formal owner is Hub `ticket_1786716545_417854`. Plan acceptance checks now match.
2. **One terminal-protocol identity.** Consumed. Hub `3bee3a57` pins hub-client to Core `f4f6bf5`. `cargo tree` shows one identity.
3. **8 MiB Ghostty scrollback.** `GhosttyClientProjection::new` defaults to 0 bytes. Production decoder uses `GhosttyAdapterConfig::with_max_scrollback_bytes(8 MiB)`.
4. **`third_party/botster-ui-contract`.** Leftover unused vendor. Not deleted.

## Tests and downstream proof

Hermetic (repo wrappers):

- `script/fmt`
- `script/clippy`
- `script/test` — 238 unit tests + `package_manifest_test` passed
- `cargo tree -i botster-ui-contract --locked` — one source: tag `botster-ui-contract-v0.3.2` `#0775e661`
- `cargo tree -p botster-tui -e normal -i botster-terminal-protocol` — one identity: `git+https://github.com/trybotster/botster-core.git?rev=f4f6bf5babe92dfb9241a760c414187f711c2c42`

New/updated hermetic proofs:

- Hello missing `terminal_compatibility` / missing terminal `snapshot_delivery=ready_then_history` fails before Attach
- Mux decoder keeps a partial line and emits concatenated JSON values
- Ghostty unexpected progress and post-READY apply error start one fresh Attach without replay
- Mux 12k READY-before-FINISH, close once then fail closed, late close for retired A leaves B, ProcessExit isolation, sibling attach survival, `poll_hub` sends no Drain
- Host floor 40 plus `unix_terminal_adapter` and `terminal_subscription_closed`

Live (`script/test-live-hub ghostty` on Hub `3bee3a57`):

- Hub bin realpath `/private/tmp/botster-tui-live-hub-3bee3a57.pin/release/botster-hub`, rev `3bee3a57cc7a031b93c6c63d8e9f267d6a9e0c79`
- Worker bin distinct realpath `/private/tmp/botster-tui-live-core-f4f6bf5.pin/botster-session-worker`, locked Core `f4f6bf5babe92dfb9241a760c414187f711c2c42`
- 12k history PAGEs, READY attach, live output, detach/reconnect, ProcessExit/exited row
- Sibling second Unix connection received 81 terminal frames during the 20s stall (`ghostty-live-sibling`)
- Stalled flood close reason `host_adapter_closed` generation 1 (`ghostty-live-write-budget`). Not treated as Core 512-tick proof. `ticket_1786716545_417854` remains open.
- Printed `ghostty-live-complete`

Production entry point: `HubConnection::connect` calls `connect_and_hello_with_terminal_requirement`, requires `ack.terminal_compatibility`, `ensure_terminal_compatible`, then `Attach`. `poll_hub` reads mux frames and does not send `DaemonRequest::Drain`. Entity pumps stay on separate host-control connections.

## Runtime-teardown lenses implemented

| Lens | Implementation |
| --- | --- |
| Isolation | One unique `(session_id, subscription_id)` owns one decoder/hydration. Entity pumps and other sessions survive. Socket loss tears down this client. |
| Bounds | Authoritative incomplete-stream trigger is `TerminalSubscriptionClosed` for the current pair. One recovery Attach with a new `subscription_id`. Second close or second decode/phase gap fails closed. No `block_on` Hub close. No 20s timer. |
| Late-message matrix | Close events and leftover mux Terminal frames for retired `subscription_id` are ignored. Generation is evidence only. |
| Production-path proof | Live Ghostty path: Hello split → mux Terminal → one decoder → FINISH/incomplete + Attached. Sibling Unix connection stays readable during a stall. Authentic `core_adapter_closed` is Hub `ticket_1786716545_417854`. |
| Ownership identity | TUI mints a unique `subscription_id` per Attach and never reuses it. Match close on session+subscription only. |
| Sibling / fail-closed | Sequential attach B survives closed A. Live second connection receives sibling frames during a stall. Second recovery close fails that subscription. |

## Unverified behavior or residual risk

- Live 12k PAGE apply can still hit Ghostty `-2` on some Hub-encoded history pages. READY terminal stays usable.
- Live stall reason on Hub `aafd6c2` is host egress, not Core 512-tick `core_adapter_closed`.
- Authentic held-open `core_adapter_closed` proof remains Hub `ticket_1786716545_417854`. This pin does not claim that proof.
- `third_party/botster-ui-contract` leftover is unused.

## Missing vault guidance discovered

- First-party Unix attach clients must call `connect_and_hello_with_terminal_requirement` and treat `TerminalSubscriptionClosed` as the adapter-close signal.
- Cargo cannot `[patch]` a git `branch=` dependency onto a `rev=` of the same repository URL. Consumers need the producer to pin one identity.
