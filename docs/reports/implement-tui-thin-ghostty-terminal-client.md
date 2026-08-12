# Implementation report: Make TUI a thin Ghostty terminal client

- **Ticket:** `ticket_1786471490_592122`
- **Run:** `run_1786508115_389280`
- **Step:** `botster_stack_implement` (revisit after `review_1786518945_388315` / `finding_1786518945_991739`)
- **PR:** https://github.com/trybotster/botster-tui/pull/51
- **Target:** `botster-tui` / `tgt_c3d470bab78549df920a41e8fb0e58d8`
- **Playbooks:** [[implementer-playbook]], [[botster-implementer-playbook]], [[botster-tui-playbook]]

## Open findings addressed

### High: resize proof stopped at client-owned dimensions (`finding_1786518945_991739`)

The live test previously only asserted `terminal_viewport_size` immediately after `InputDispatch::TerminalResize`. That field is set client-side before `DaemonRequest::Resize` is sent, so the assertion could pass if Hub rejected or the worker ignored resize.

**Fix in** `headless_live_runtime_ghostty_install_scrollback_palette_and_mode_gated_input`:

| Oracle | Required proof |
| --- | --- |
| Hub Resize success | After production `TerminalResize` → `DaemonRequest::Resize`, `app.error` must be `None` |
| Client projection size | Post-resize `ghostty_projection.dimensions()` is 30×100 |
| Worker-applied size | After reconnect installs a fresh GHOSTSNP Snapshot, projection dimensions, `terminal_viewport_size`, and viewport cache are **30×100** (dimensions come from decoded Snapshot / session worker, not the local handler alone) |
| History still restored | `TOP_MARKER` still required after reconnect (proves Snapshot install, not blank local projection) |

### Prior high/low findings (still held)

Hard matrix oracles from `finding_1786518358_440347` remain: exact palette/special RGB, painted STYLED, forced Kitty+mouse, ModeGatedInput key/mouse, later live in painted frame, reconnect history, silent no-history. PR/report durability from `finding_1786518358_398998` remains path-neutral.

## Live run (passed)

```sh
# Build Hub @ 89dae7e and session-worker from that Hub lock's Core tip (2c5171a).
export BOTSTER_HUB_BIN=<path-to-hub-binary>
export BOTSTER_SESSION_WORKER_BIN=<path-to-session-worker-binary>
export BOTSTER_HUB_BIN_REV=89dae7e15a844bcb7411b83b32581121720e23eb
export BOTSTER_SESSION_WORKER_BIN_REV=2c5171a6cb3b073c53620a9838d8b08480dd215c
export BOTSTER_TUI_REQUIRE_HUB_TEST=1
cargo test -p botster-tui -- --exact \
  app::tests::headless_live_runtime_ghostty_install_scrollback_palette_and_mode_gated_input \
  --nocapture
# ok — ghostty-live-modes kitty_enabled=true mouse_mode=9; ghostty-live-complete with both revs
```

Do **not** treat machine-local `/tmp/...` binary paths as durable artifacts; rebuild from the revs above.

## Local gates

```sh
script/fmt     # 0
script/clippy  # 0
script/test    # 0 — 205 unit + package_manifest (live soft-skips without bins)
```

## Files changed (this revisit)

- `crates/botster-tui/src/app.rs` — resize Hub + reconnect Snapshot dimension oracles
- `docs/reports/implement-tui-thin-ghostty-terminal-client.md` — this report

## Ownership

Unchanged TUI charter: paint/input policy only; Core projection; Hub ModeGatedInput; kit chrome. No cross-repo code changes.

## Deviations from plan

None. Test-hardening only against open Review finding; product path for TerminalResize was already correct.

## Residual risk

Default suite soft-skips live without bins. Exact live gate requires exported binaries and `BOTSTER_TUI_REQUIRE_HUB_TEST=1`.

## Missing vault guidance

None discovered for this revisit.
