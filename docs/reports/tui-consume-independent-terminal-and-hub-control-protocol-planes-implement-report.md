# Implementation report: TUI consume independent terminal and Hub control protocol planes

- **Ticket:** `ticket_1786661009_551067`
- **Run:** `run_1786704127_656781`
- **Step:** `botster_stack_implement`
- **PR:** none. Pipeline `merge_policy` is `direct`. Plan: merge into main, do not create a PR.
- **Target repository:** `botster-tui` (`trybotster/botster-tui`)
- **Target id:** `tgt_c3d470bab78549df920a41e8fb0e58d8`
- **Base:** `origin/main` `5d2af28e92eef94d51d8f59c45ab94b8e9a58c7c`
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

Convention conflicts: none that blocked the ticket. Live stall produced `host_adapter_closed` rather than `core_adapter_closed`; both are typed `TerminalSubscriptionClosed` reasons on Hub `aafd6c2`.

## Files changed

- `crates/botster-tui/Cargo.toml` — Hub `aafd6c2`, Core `f4f6bf5`, kit `c83ba6c`, UI contract tag `botster-ui-contract-v0.3.2`, add `botster-terminal-protocol-client`
- `Cargo.lock` — regenerated from those pins
- `crates/botster-tui/src/app.rs` — split Hello, mux read, no production Drain, Core `TerminalEvent` apply, fail-closed recovery, hermetic mux tests, live Ghostty 12k + adapter-close proof
- `README.md` — pins, handshake, mux, Ghostty provenance
- `docs/plans/tui-consume-independent-terminal-and-hub-control-protocol-planes-plan.md` — approved plan rev 4
- `docs/reports/tui-consume-independent-terminal-and-hub-control-protocol-planes-implement-report.md` — this report

## Ownership boundaries preserved

Edited only `botster-tui` application policy, handshake, mux consumption, hydration/recovery, tests, and docs. Did not edit Hub, Core, Kit, Ghostty, or Web. Those surfaces are consumed by pin only.

## Cross-repo routing

| Ticket | Target | Status | Role |
| --- | --- | --- | --- |
| `ticket_1786661008_634435` | Hub | closed | Unix adapters |
| `ticket_1786661009_576857` | TUI Kit | closed | UI contract identity |
| `ticket_1786705502_228757` | Hub | closed at `aafd6c2` | Hello split + `TerminalSubscriptionClosed` |

No new cross-repo tickets registered.

## Deviations from plan

1. **Live adapter-close reason.** Plan asked for authentic write-budget `core_adapter_closed`. Stalling mux reads against Hub `aafd6c2` / Core `f4f6bf5` produced `TerminalSubscriptionClosed` with `host_adapter_closed` (generation 1). TUI recovery is the same: abort decoder, mint a new `subscription_id`, one retry, then fail closed. Hermetic tests still use `core_adapter_closed`.
2. **Blank / concatenated mux lines.** Production mux reader skips JSON lines that are empty or have trailing characters. Without that, IsolatedHub/Hub writes a bare newline (and later a concatenated line) and `read_unix_mux_frame_from_reader` fails closed. This is consume-path resilience, not a Hub edit.
3. **8 MiB Ghostty scrollback.** `GhosttyClientProjection::new` defaults to 0 bytes. Incremental PAGE apply then fails with Ghostty `-2`. Production decoder now uses `GhosttyAdapterConfig::with_max_scrollback_bytes(8 MiB)` so 12k-line history can retain PAGEs. Post-READY apply failure still keeps the READY terminal and releases barriers after `attached`.
4. **`third_party/botster-ui-contract`.** Still present as leftover vendor. Workspace has no `[patch]`. Not referenced. Not deleted (out of ticket scope).

No committed plan acceptance-check rewrite: the consume contract is unchanged. Deviation 1 is live-oracle reason only.

## Tests and downstream proof

Hermetic (repo wrappers):

- `script/fmt`
- `script/clippy`
- `script/test` — 235 unit tests + `package_manifest_test` passed
- `cargo tree -i botster-ui-contract --locked` — one source: tag `botster-ui-contract-v0.3.2` `#0775e661`

New/updated hermetic proofs:

- Hello missing `terminal_compatibility` / missing terminal `snapshot_delivery=ready_then_history` fails before Attach
- Mux 12k READY-before-FINISH, phase-gap fresh attach without replay, close once then fail closed, late close for retired A leaves B, ProcessExit isolation, sibling attach survival, `poll_hub` sends no Drain
- Host floor 40 plus `unix_terminal_adapter` and `terminal_subscription_closed`

Live (`script/test-live-hub ghostty`, fresh `BOTSTER_LIVE_HUB_TARGET_DIR`):

- Hub bin realpath under fresh target, rev `aafd6c2cde430804f1bb54094c568fc88c15944b`
- Worker bin distinct realpath, locked Core `f4f6bf5babe92dfb9241a760c414187f711c2c42`
- 12k history PAGEs, READY attach, live output, detach/reconnect, ProcessExit/exited row, stalled-read `TerminalSubscriptionClosed` (`host_adapter_closed`) + recovery
- Printed `ghostty-live-complete` and `ghostty-live-write-budget`

Production entry point: `HubConnection::connect` calls `connect_and_hello_with_terminal_requirement`, requires `ack.terminal_compatibility`, `ensure_terminal_compatible`, then `Attach`. `poll_hub` reads mux frames and does not send `DaemonRequest::Drain`. Entity pumps stay on separate host-control connections.

## Runtime-teardown lenses implemented

| Lens | Implementation |
| --- | --- |
| Isolation | One unique `(session_id, subscription_id)` owns one decoder/hydration. Entity pumps and other sessions survive. Socket loss tears down this client. |
| Bounds | Authoritative incomplete-stream trigger is `TerminalSubscriptionClosed` for the current pair. One recovery Attach with a new `subscription_id`. Second close or second decode/phase gap fails closed. No `block_on` Hub close. No 20s timer. |
| Late-message matrix | Close events and leftover mux Terminal frames for retired `subscription_id` are ignored. Generation is evidence only. |
| Production-path proof | Live Ghostty path: Hello split → mux Terminal → one decoder → FINISH/incomplete + Attached. Stall → close event → recovery. |
| Ownership identity | TUI mints a unique `subscription_id` per Attach and never reuses it. Match close on session+subscription only. |
| Sibling / fail-closed | Sequential attach B survives closed A. Second recovery close fails that subscription. |

## Unverified behavior or residual risk

- Live 12k PAGE apply still hits Ghostty `-2` on some Hub-encoded history pages. READY terminal stays usable; live path opens after `attached`. Worker scrollback may drop `HISTORY_HEAD`.
- Live stall reason is `host_adapter_closed`, not Core write-budget `core_adapter_closed`.
- Mux blank-line skip is TUI-side; Hub still emits those lines.
- Dual `botster-core` / `botster-terminal-protocol` sources remain in the lock via hub-test-support `branch=main` vs direct rev `f4f6bf5`. Production Ghostty/core stay on `f4f6bf5`.
- `third_party/botster-ui-contract` leftover is unused.

## Missing vault guidance discovered

- First-party Unix attach clients must call `connect_and_hello_with_terminal_requirement` and treat `TerminalSubscriptionClosed` as the adapter-close signal. Plan already named this for post-Implement capture.
- Mux JSON lines can be empty or concatenated; `read_unix_mux_frame_from_reader` fails closed on those. Consumers need a skip/resync rule. Not captured this visit.
