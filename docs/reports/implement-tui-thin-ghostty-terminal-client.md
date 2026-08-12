# Implementation report: Make TUI a thin Ghostty terminal client

- **Ticket:** `ticket_1786471490_592122`
- **Run:** `run_1786508115_389280`
- **Step:** `botster_stack_implement` (`run_step_1786514671_320050`)
- **Plan:** rev 7 `docs/plans/tui-thin-ghostty-terminal-client-plan.md` (approved)
- **Branch:** `project-pipelines/ticket_1786471490_592122`
- **Base:** `abc804e19bc3e01465cd308c11de5f4292331c3d`

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Routing verification | Ticket, run, and approved plan rev7 all resolve to the same target; ambient worktree matches |

## Repository playbook and other playbooks/notes applied

### Required load order
1. [[implementer-playbook]]
2. [[botster-implementer-playbook]]
3. [[botster-tui-playbook]] (ownership charter)
4. Targeted atomics (below)
5. [[project-pipelines-playbook]] — not loaded (no package/plugin policy paths in scope)

### Targeted notes / related charters
- [[botster-terminal-ghostty-playbook]] (Core package boundary; consumed only)
- [[tui and browser are equal clients]]
- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[tui client attach uses hub protocol not session protocol]]
- [[botster hub client crate is the external client boundary]]
- [[focused mouse mode terminal passthrough needs complete sgr reports]]
- [[botster rust consumers that share ui contract must pin one hub revision]]
- [[renderer acceptance tests must drive real frame backend]]
- [[external client hub tests use subprocess spawned hub test support]]
- [[test script required for rust tests not cargo test]]
- [[cli-patterns]], [[botster-architecture]]
- Human answers: `question_1786508781_856951`, `question_1786508866_164600`
- Runtime-teardown class: **false** (no teardown lenses)

## Files changed

| Path | Role |
| --- | --- |
| `crates/botster-tui/Cargo.toml` | Exact pins: Core/terminal-ghostty `4d0d1d8` + `libghostty-vt`, Hub/ui-contract `89dae7e`, kit `32d804e` |
| `Cargo.lock` | Source-qualified update + build admit of `botster-terminal-ghostty` |
| `crates/botster-tui/src/projection_paint.rs` | **New** TUI-owned `ProjectionWidget` + HitMap `tui-terminal` paint helper |
| `crates/botster-tui/src/app.rs` | H0–H5 attach/install, ModeGatedInput, scroll, paint integration, conf 34, tests |
| `crates/botster-tui/src/main.rs` | Module registration |
| `crates/botster-tui/src/renderer.rs` | Re-export `HitRole` for tests |
| `README.md` | Pin table, Zig/submodule, ModeGatedInput + paint seam narrative |
| `docs/plans/tui-thin-ghostty-terminal-client-plan.md` | Approved plan (artifact) |

## Ownership boundaries preserved

- **TUI owns:** attach policy, Snapshot→install order, ModeGatedInput dispatch, styled paint overlay, scroll key mapping.
- **Core owns:** `GhosttyClientProjection` / GHOSTSNP decode / viewport truth (`botster-terminal-ghostty@4d0d1d8`).
- **Hub owns:** wire Snapshot/TerminalOutput/ModeGatedInput/ModeFlags (`89dae7e`).
- **Kit owns:** TerminalView chrome, HitMap, input encode; **no** Ghostty cell arrays added to kit props.
- No PTY ownership, no OSC color answers, no Scrollback→install path.

## Cross-repo dependencies or separately routed work

| Dependency | Status |
| --- | --- |
| Hub Ghostty contract `ticket_1786471489_718500` | closed (pin `89dae7e`) |
| Core projection `ticket_1786509045_208932` | closed (pin `4d0d1d8`) |
| Kit UI-contract repin `ticket_1786509045_506152` | closed (pin `32d804e`) |

No additional cross-repo tickets opened. Consumed APIs only.

## Deviations from plan

1. **Viewport paint cache:** production draw still calls kit then `ProjectionWidget` on HitMap inner rect, but `project_viewport` is refreshed into `ghostty_viewport_cache` before immutable draw (Ratatui draw borrows `&TuiApp`). Same production paint path; avoids `&mut` through the whole workspace shell.
2. **Live ghostty test soft-skip:** `headless_live_runtime_ghostty_install_scrollback_palette_and_mode_gated_input` uses `skip_or_panic` so default `script/test` stays green without bins; with `BOTSTER_TUI_REQUIRE_HUB_TEST=1` missing bins is a hard failure (plan live-gate intent). Pin-matched hub/session-worker bins were not available in this environment, so live production attach with real Hub Snapshot was **not** executed here.
3. **Non-GHOSTSNP Snapshot bodies:** conformance opaque fixtures that are not `GHOSTSNP` magic fail closed without install and without hard UI error (diagnostic ReadScreen path remains); real GHOSTSNP decode failures still hard-error.

No plan acceptance-check rewrites required beyond residual live-bin proof.

## Tests and downstream proof run

### Local gates (passed)
```sh
script/fmt     # exit 0
script/test    # exit 0 — 198 unit tests + package_manifest
script/clippy  # exit 0
```

### Cargo-tree identity (passed)
```sh
cargo tree -p botster-tui -i botster-ui-contract
# sole rev 89dae7e15a844bcb7411b83b32581121720e23eb

cargo tree -p botster-tui -i botster-terminal-ghostty
# rev 4d0d1d8832d19352454a0789419a3e31e67d50df (libghostty-vt)

cargo tree -p botster-tui --edges normal,dev -i botster-core-test-support
# rev 4d0d1d8832d19352454a0789419a3e31e67d50df
```

### Plan-named unit tests (passed)
- `ghostty_install_snapshot_before_live_applies_output`
- `ghostty_scrollback_event_never_calls_install_ghostsnp`
- `ghostty_paint_real_frame_shows_styled_cells_cursor_and_palette` (real TestBackend frame + workspace-error sibling)
- `ghostty_scroll_op_moves_viewport_to_pre_attach_history_marker`
- `mode_gated_kitty_and_mouse_use_freshness_with_single_reprobe`
- `read_screen_is_non_authoritative_when_projection_installed`

### Live gate
- Test present: `headless_live_runtime_ghostty_install_scrollback_palette_and_mode_gated_input`
- Not executed with pin-matched bins in this session (bins unset → soft-skip). Residual for Review/Verify.

### Production entry points wired
1. Handshake: conf ≥ 34 + `FEATURE_MODE_GATED_INPUT`
2. Attach: Snapshot → `install_ghostsnp` before live apply; Scrollback never installs
3. Draw: kit `tui-terminal` → HitMap region → `ProjectionWidget` on `terminal_inner_rect`
4. Input: ModeGatedInput when kitty_enabled or mouse SGR reports with ModeFlags freshness
5. Resize: hub Resize + projection.resize + cache refresh

## Unverified behavior or residual risk

1. **Live Hub attach with real session-worker GHOSTSNP** not proven in this environment (bins absent).
2. Kit still ignores wheel-on-terminal when mouse mode is off; local history scroll uses PageUp/PageDown (and ScrollOp API). Mouse-mode-on still SGR-forwards wheel via kit.
3. ModeGatedInput unit path without a live client records the request then transport-errors (clears connection); live admission/rejection is residual without bins.
4. Dual `botster-core` sources: production direct pin `4d0d1d8`; hub-test-support may still pull branch=main as dev transitive — cargo-tree production identity is enforced for direct ghostty/core-test-support.

## Missing vault guidance discovered

- No durable note for “TUI paint-after-TerminalView via HitMap + ProjectionWidget” (plan residual).
- No durable pin table note for hub/kit/core Ghostty multipath stack (README updated; vault capture deferred to post-implement).
