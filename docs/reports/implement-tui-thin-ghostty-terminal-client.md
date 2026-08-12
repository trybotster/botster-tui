# Implementation report: Make TUI a thin Ghostty terminal client

- **Ticket:** `ticket_1786471490_592122`
- **Run:** `run_1786508115_389280`
- **Step:** `botster_stack_implement` (revisit after `review_1786517457_348831`)
- **PR:** https://github.com/trybotster/botster-tui/pull/51
- **Target:** `botster-tui` / `tgt_c3d470bab78549df920a41e8fb0e58d8`
- **Playbook:** [[botster-tui-playbook]] (+ implementer / botster-implementer)

## Open findings addressed (`review_1786517457_348831`)

### 1. Unknown ModeFlags still permit classic key input
**Fix:** `handle_focused_terminal_key` now **consumes** all focused-terminal keys when attached. If ModeFlags are missing it probes once and **fails closed** (no classic SendInput). When known, encodes Classic or Kitty and sends SendInput or ModeGatedInput accordingly.

**Tests:**
- `unknown_mode_flags_fail_closed_for_focused_terminal_keys`
- `classic_encoding_remains_when_kitty_disabled` (consumes + classic SendInput)
- `kitty_key_path_covers_modifiers_repeat_and_release`
- `mode_gated_stale_token_reprobes_once_then_retries`
- `kitty_encoding_uses_real_key_path_and_mode_gated_input`

### 2. Exact-bin production matrix unproved
**Fix:** Expanded `headless_live_runtime_ghostty_install_scrollback_palette_and_mode_gated_input` and **ran it** with pin-matched binaries:

| Binary | Path | Provenance |
| --- | --- | --- |
| Hub | `/tmp/botster-hub-89dae7e-target/debug/botster-hub` | Hub rev **89dae7e15a844bcb7411b83b32581121720e23eb** |
| Session worker | `/tmp/botster-worker-2c5171a-target/debug/botster-session-worker` | Hub-locked Core **2c5171a6cb3b073c53620a9838d8b08480dd215c** |

**Live command (passed):**
```sh
export BOTSTER_HUB_BIN=/tmp/botster-hub-89dae7e-target/debug/botster-hub
export BOTSTER_SESSION_WORKER_BIN=/tmp/botster-worker-2c5171a-target/debug/botster-session-worker
export BOTSTER_HUB_BIN_REV=89dae7e15a844bcb7411b83b32581121720e23eb
export BOTSTER_SESSION_WORKER_BIN_REV=2c5171a6cb3b073c53620a9838d8b08480dd215c
export BOTSTER_TUI_REQUIRE_HUB_TEST=1
cargo test -p botster-tui -- --exact \
  app::tests::headless_live_runtime_ghostty_install_scrollback_palette_and_mode_gated_input --nocapture
# ok in 2.49s; printed ghostty-live-complete with both revs
```

**Matrix covered in live test:**
- Late attach with pre-attach history + GHOSTSNP install
- Full scrollback (`TOP_MARKER`) in projection **and painted Ratatui frame**
- OSC palette / special colors via `color_profile`
- Styled cells (bold/truecolor STYLED)
- ModeFlags required
- Resize 30×100
- Real focused key path (Kitty CSI-u ModeGatedInput or classic SendInput)
- Mouse SGR ModeGatedInput when mouse_mode != 0
- Later live output in paint/projection
- Reconnect re-attach reinstalls projection
- No-history second session: blank projection + immediate live output

## Local gates
```sh
script/fmt     # 0
script/clippy  # 0
script/test    # 0 — 205 unit + package_manifest (without live bins)
```

## Ownership
Unchanged: TUI policy/paint/input encoding choice; Core projection; Hub ModeGatedInput; kit TerminalView chrome.

## Residual risk
- Default `script/test` soft-skips live tests without bins (by design). Exact live gate requires exported bins + `BOTSTER_TUI_REQUIRE_HUB_TEST=1`.
- Live worker pin is hub-locked Core `2c5171a` (not TUI's direct Core pin `4d0d1d8`); that matches Hub 89dae7e's Cargo.lock tip for `botster-core-daemon`.

## Missing vault guidance
- Fail closed on unknown ModeFlags for focused terminal keys before classic fallthrough
- Exact-bin live provenance recording (hub rev + worker rev env)
