# Implementation report: TUI consume byte-faithful Hub terminal output

- **Ticket:** `ticket_1786562566_712634`
- **Run:** `run_1786568426_623487`
- **Step:** `botster_stack_implement`
- **PR:** https://github.com/trybotster/botster-tui/pull/53
- **Target repository:** `botster-tui` (`trybotster/botster-tui`)
- **Target id:** `tgt_c3d470bab78549df920a41e8fb0e58d8`
- **Base:** `f2bc11fc8c0b14b57ebcf9b6ace4f1d80565720f`
- **teardown_class_applies:** false

## Playbooks and notes applied

Repository charter: [[botster-tui-playbook]]

Role / stack:

- [[implementer-playbook]]
- [[botster-implementer-playbook]]
- [[botster-architecture]]
- [[cli-patterns]]
- [[spa-patterns]]

Consumed (not edited):

- [[botster-tui-kit-playbook]]
- [[botster-hub-client-playbook]]
- [[botster-terminal-ghostty-playbook]]

TUI charter must-load notes:

- [[tui and browser are equal clients]]
- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[tui client attach uses hub protocol not session protocol]]
- [[tui and socket terminal streams use clientworker transport adapters]]
- [[botster tui uinode event routing captures hit regions during draw]]
- [[tui error dedup tests must drive real input handlers]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]
- [[incomplete repo local session types drop the hub client connection]]
- [[deleting a waiver proof test can drop unrelated coverage in its tail]]

Task-surface notes:

- [[live terminal output base64 envelopes carry renderable bytes]]
- [[botster rust consumers that share ui contract must pin one hub revision]]
- [[botster hub client crate is the external client boundary]]
- [[cold turkey migrations eliminate dual code paths and version suffixes]]
- [[external client hub tests use subprocess spawned hub test support]]
- [[test script required for rust tests not cargo test]]
- [[live hub proof records distinct hub and locked core binary provenance]]
- [[live hub target dirs can cache stale same version client schema]]
- [[renderer acceptance tests must drive real frame backend]]
- [[coredaemon attached follows initial snapshots before live terminal output]]
- [[botster clients restore visible terminal state from readscreen before buffered live output]]
- [[hub replays full history on every attach so clients must clear per cycle]]
- [[pipeline vault checklists must cite exact resolvable note titles]]
- [[implementation artifacts must match actual git state]]
- [[implement gate must verify committed work and pr link before review]]
- [[implementation steps must persist report artifacts for review]]

Convention conflicts: none new. Shipped TUI restore stays GHOSTSNP-first; ReadScreen remains diagnostic only.

## Files changed

- `crates/botster-tui/Cargo.toml` — Hub `7499c161` + kit `c07f793` pin set
- `Cargo.lock` — regenerated; one `botster-ui-contract` at Hub `7499c161`
- `crates/botster-tui/src/app.rs` — protocol 7 / floor 36, `payload.decoded_bytes()`, `Vec<u8>` buffer, byte-faithful unit + live proofs, C2.5 rustfmt
- `script/test-live-hub` — `ghostty` mode
- `README.md` — pin table, protocol 7 / floor 36, `script/test-live-hub ghostty`
- `docs/plans/tui-consume-byte-faithful-hub-terminal-output-plan.md` — approved plan
- `docs/reports/tui-consume-byte-faithful-hub-terminal-output-implement-report.md` — this report

## Ownership boundaries preserved

Edited only `botster-tui` policy, handshake, live-byte apply, tests, and docs. Did not edit kit, Hub, Core, Ghostty backend, or Web/Restty. Kit and Hub are consumed by pin only.

## Cross-repo routing

| Ticket | Target | Status | Role |
| --- | --- | --- | --- |
| `ticket_1786562565_286591` | hub `tgt_7e208a0c76a44980a83b63af976b1f22` | closed / merge `7499c161` | parent contract |
| `ticket_1786568835_840471` | kit `tgt_3dfae49c02454037bf13554f552baf7f` | closed / merge `c07f793` | ui-contract identity pin |
| `ticket_1786562565_267926` | web `tgt_40abcf71ccf049f4ac0c99953a799869` | open | sibling; not implemented |

## Production entry points

1. `HubConnection::connect` → `tui_compatibility_requirement()` (protocol 7 / floor 36)
2. `run_loop` → `poll_hub` / `request_and_apply` → `TuiApp::apply_response` → `apply_response_state`
3. `DaemonEvent::TerminalOutput` → `payload.decoded_bytes()` → H1 `Vec<u8>` buffer or H5 `apply_live_terminal_output(&[u8])` → `GhosttyClientProjection::apply_terminal_output`
4. `open_attach_live_path` flushes buffered bytes through the same apply path
5. `draw_workspace_shell` still paints `ProjectionWidget` from the projection

## Pins

| Crate | Pin |
| --- | --- |
| `botster-hub-client` / `botster-ui-contract` / `botster-hub-test-support` | Hub `7499c1615078069ba391489b20c6f39c55c2d4c6` |
| `botster-tui-kit` | `c07f793fb9ac46c24dcf1688881cd08be18ebc27` |
| `botster-core` / `botster-terminal-ghostty` | Core `4d0d1d8832d19352454a0789419a3e31e67d50df` |

`cargo tree -i botster-ui-contract --locked` resolved one git source at `7499c161`. Lock still has a second `botster-core` via hub-test-support `branch=main` at `5a993837`; production Ghostty/core remain on `4d0d1d88`.

## Deviations from plan

Not product-scope changes:

- Hub pin required a `DaemonDiagnosticKind::WorkerCompatibility` match arm (new enum variant).
- Shared late-attach fixture at Hub `7499c161` now ships GHOSTSNP goldens. Tests consume that fixture instead of dummy `[0, 255, 71, 84, 89, 1]`.
- Live split-UTF-8 barrier uses `stty -echo` plus command tokens so the producer writes exact `[0xE2]` then `[0x82, 0xAC]`. Kernel echo of the command line otherwise split the euro sequence.
- Live NUL/ESC marker uses a complete `\033[0m` before `BYTEFAITH` so an incomplete ESC does not eat the marker.
- Kit fetch needed `CARGO_NET_GIT_FETCH_WITH_CLI=true` (libgit2 auth failed).

## Tests and downstream proof

```sh
script/fmt      # 0
script/test     # 0 — 220 unit + package_manifest
script/clippy   # 0
cargo tree -i botster-ui-contract --locked
# one source: Hub 7499c1615078069ba391489b20c6f39c55c2d4c6
```

Live gate (not `script/test`):

```sh
# Hub checkout 7499c161; worker from that lock's Core 5a993837
# Built with cargo build --locked --bin botster-hub and
# cargo build --locked -p botster-core-daemon --bin botster-session-worker
# into a fresh target dir.
export BOTSTER_HUB_BIN=<fresh-hub-target>/debug/botster-hub
export BOTSTER_SESSION_WORKER_BIN=<fresh-hub-target>/debug/botster-session-worker
export BOTSTER_HUB_BIN_REV=7499c1615078069ba391489b20c6f39c55c2d4c6
export BOTSTER_SESSION_WORKER_BIN_REV=5a9938377b492ee1fa3acfb31365ebbebccc2a96
script/test-live-hub ghostty
# ok — ghostty-live-complete with both revs
```

GHOSTSNP install succeeded against worker `5a993837` with local Ghostty pin `4d0d1d88`. No Core retarget.

Ablation: temporarily applied `String::from_utf8_lossy` before `apply_terminal_output`.

- `apply_response_preserves_split_utf8_live_bytes` went red: first payload became `[0xEF, 0xBF, 0xBD]` instead of `[0xE2]`
- `apply_response_preserves_invalid_nul_and_escape_live_bytes` went red: `[0x00, 0x1b, 0xff, 0xc0]` became `[0x00, 0x1b, 0xEF, 0xBF, 0xBD, 0xEF, 0xBF, 0xBD]`

Bytes path restored; both tests green.

## Unverified behavior or residual risk

- Default `script/test` still soft-skips the Ghostty live test without binaries. The fail-closed gate is `script/test-live-hub ghostty`.
- Workspaces / session-types / contract-matrix live lanes were not rerun; they are separately owned and not this ticket's gate.
- Dual `botster-core` lock sources remain: production `4d0d1d88` plus hub-test-support `branch=main` at `5a993837`.

## Missing vault guidance

None required. Optional later capture of the `stty -echo` / incomplete-ESC live-barrier gotcha is not needed to start Review.
