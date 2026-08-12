# Implementation report: Make TUI a thin Ghostty terminal client

- **Ticket:** `ticket_1786471490_592122`
- **Run:** `run_1786508115_389280`
- **Step:** `botster_stack_implement` (revisit after `review_1786516660_249798`)
- **Plan:** rev 7 `docs/plans/tui-thin-ghostty-terminal-client-plan.md`
- **Branch:** `project-pipelines/ticket_1786471490_592122`
- **PR:** https://github.com/trybotster/botster-tui/pull/51

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |

## Repository playbook and notes applied

1. [[implementer-playbook]]
2. [[botster-implementer-playbook]]
3. [[botster-tui-playbook]]
4. Targeted: Ghostty paint/input notes; kit `TerminalKeyEncoding`; human answers on GHOSTSNP authority
5. [[project-pipelines-playbook]] — not in scope

Addresses open findings from `review_1786516660_249798`.

## Files changed (this revisit)

| Path | Change |
| --- | --- |
| `crates/botster-tui/src/app.rs` | Kitty real KeyEvent path; ModeGated fail-closed for mouse without modes; attach opens live path immediately; ReadScreen non-authority; focus-gated scroll; expanded tests |
| `crates/botster-tui/src/renderer.rs` | Export `TerminalKeyEncoding` + `terminal_key_bytes_with` |
| `README.md` | Single Hub pin `89dae7e`; Kitty claim only with real encoder |
| `docs/plans/tui-thin-ghostty-terminal-client-plan.md` | Trailing whitespace stripped for `git diff --check` |

## Ownership boundaries preserved

- TUI re-encodes Kitty via kit public `terminal_key_bytes_with` (does not put Ghostty truth in kit)
- Core projection remains authority for cells/styles
- Hub ModeGatedInput + ModeFlags freshness for mode-dependent input

## Review finding disposition

| Finding | Fix |
| --- | --- |
| Kitty gates classic VT bytes | `handle_focused_terminal_key` uses `TerminalKeyEncoding::Kitty` then ModeGatedInput |
| ReadScreen primary path | `open_attach_live_path` on Attached; blank projection for no-history; ReadScreen diagnostic only |
| Live harness matrix | Expanded: ModeFlags required, resize, live output, reconnect clear; still soft-skips without bins |
| Plan whitespace | Stripped trailing spaces |
| Scroll steals focus | PageUp/Down/Ctrl-Home/End only when `tui-terminal` focused |
| README pin conflict | Live hub section now `89dae7e` |

## Deviations from plan

None material beyond prior viewport paint cache and live soft-skip without bins (`BOTSTER_TUI_REQUIRE_HUB_TEST` hard-fails).

## Tests and downstream proof

```sh
script/fmt     # 0
script/test    # 0 — 202 unit + package_manifest
script/clippy  # 0
```

New/updated proofs:
- `kitty_encoding_uses_real_key_path_and_mode_gated_input`
- `classic_encoding_remains_when_kitty_disabled`
- `mouse_report_without_mode_flags_fails_closed`
- `terminal_scroll_shortcuts_require_terminal_focus`
- ReadScreen non-authority + no-history immediate live path

## Unverified / residual risk

1. Live matrix with pin-matched Hub `89dae7e` + Core session-worker bins still not executed in this environment.
2. Full live reconnect re-attach with GHOSTSNP reinstall after mid-test `begin_attach_hydration` clear is only partially exercised (clears projection; does not re-complete second attach in same test).

## Missing vault guidance

- TUI Kitty re-encode after kit classic router
- Attach live-path open without ReadScreen readiness
