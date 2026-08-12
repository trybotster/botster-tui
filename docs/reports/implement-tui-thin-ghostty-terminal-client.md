# Implementation report: Make TUI a thin Ghostty terminal client

- **Ticket:** `ticket_1786471490_592122`
- **Run:** `run_1786508115_389280`
- **Step:** `botster_stack_implement` (revisit after `review_1786518357_770541`)
- **PR:** https://github.com/trybotster/botster-tui/pull/51
- **Target:** `botster-tui` / `tgt_c3d470bab78549df920a41e8fb0e58d8`
- **Playbooks:** [[implementer-playbook]], [[botster-implementer-playbook]], [[botster-tui-playbook]]

## Open findings addressed

### High: green exact-bin test left matrix branches unproved (`finding_1786518358_440347`)

Hardened `headless_live_runtime_ghostty_install_scrollback_palette_and_mode_gated_input` so required branches **cannot soft-pass**:

| Oracle | Required proof |
| --- | --- |
| Scrollback | `TOP_MARKER` absent at live edge, present after `ScrollOp::Top`, present in **painted** frame |
| Palette index 1 | Exact RGB `(255,0,0)` from OSC 4 |
| Special foreground | Exact RGB `(0,255,0)` from OSC 10 / `COLOR_INDEX_FOREGROUND` |
| Styled cell | Projection `S` bold + truecolor `(0,128,0)`; painted terminal-region `S` bold + same RGB |
| Kitty + mouse | Controlled `enable-modes` forces `kitty_enabled` and `mouse_mode != 0` or test panics |
| Kitty input | Focused KeyEvent → `ModeGatedInput` CSI-u with freshness tokens (not conditional) |
| Mouse input | SGR → `ModeGatedInput` (not conditional) |
| Later live | Must appear in **painted Ratatui frame** (cache-only is insufficient) |
| Reconnect | Re-attach + `TOP_MARKER` in projection and paint |
| Silent no-history | Spawn with no pre-attach print; blank projection without history; paint immediate live output |

**Live run (passed):**
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
# ok — logged kitty_enabled=true mouse_mode=9 and ghostty-live-complete with both revs
```

Do **not** treat machine-local `/tmp/...` binary paths as durable artifacts; rebuild from the revs above.

### Low: PR/report durability (`finding_1786518358_398998`)

- Report updated with path-neutral live reproduction and current counts.
- PR body updated to match (205 unit tests + exact-bin live proof).

## Local gates
```sh
script/fmt     # 0
script/clippy  # 0
script/test    # 0 — 205 unit + package_manifest (live soft-skips without bins)
```

## Ownership
Unchanged TUI charter: paint/input policy only; Core projection; Hub ModeGatedInput; kit chrome.

## Residual risk
Default suite soft-skips live without bins. Exact live gate requires exported binaries and `BOTSTER_TUI_REQUIRE_HUB_TEST=1`.
