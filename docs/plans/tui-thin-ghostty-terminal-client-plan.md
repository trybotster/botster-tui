# Plan: Make TUI a thin Ghostty terminal client

Ticket: `ticket_1786471490_592122`  
Run: `run_1786508115_389280`  
Step: `botster_stack_plan` (visit 7 after Plan Review `review_1786514388_224203`)  
Plan **revision 7**

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`trybotster/botster-tui`) |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Base | `origin/main` **`abc804e19bc3e01465cd308c11de5f4292331c3d`** |
| Branch | `project-pipelines/ticket_1786471490_592122` |
| `teardown_class_applies` | **false** |

## Repository playbook loaded

- [[botster-tui-playbook]]

## Other playbooks and notes loaded

- [[planner-playbook]], [[botster-planner-playbook]]
- [[botster-architecture]], [[cli-patterns]]
- [[botster-core-playbook]], [[botster-terminal-ghostty-playbook]], [[botster-tui-kit-playbook]], [[botster-hub-client-playbook]]
- [[tui and browser are equal clients]]
- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[tui client attach uses hub protocol not session protocol]]
- [[botster hub client crate is the external client boundary]]
- [[focused mouse mode terminal passthrough needs complete sgr reports]]
- [[botster rust consumers that share ui contract must pin one hub revision]]
- [[renderer acceptance tests must drive real frame backend]]
- [[external client hub tests use subprocess spawned hub test support]]
- [[test script required for rust tests not cargo test]]
- Human answers: `question_1786508781_856951`, `question_1786508866_164600`
- Reviews: `review_1786508972_267861`, `review_1786513598_944097`

## Context loaded

### Closed dependencies (correct targets)

| Ticket | Pin / artifact |
| --- | --- |
| Hub Ghostty contract `ticket_1786471489_718500` | Hub **`89dae7e15a844bcb7411b83b32581121720e23eb`** |
| Kit UI-contract `ticket_1786509045_506152` | Kit **`32d804e3bbcb982e77113d5df12374baa8e9a2fa`** (ui-contract → Hub `89dae7e…`) |
| Core projection `ticket_1786509045_208932` | Core **`4d0d1d8832d19352454a0789419a3e31e67d50df`** — `GhosttyClientProjection` |

### Plan Review corrections (rev 6 → rev 7)

| Finding | Fix |
| --- | --- |
| UI contract selector `@0.1.0` wrong | Selector **`botster-ui-contract@0.3.2`** (matches current lock) |
| `cargo fetch -p botster-tui` invalid | Removed; use **`cargo build -p botster-tui`** to admit new terminal-ghostty package |
| Prior paint / ambiguous update | Retained from rev 6 |

### Current baseline (`abc804e`) production facts

- Pins: hub `891cc796`, kit `9d4a566`, core `16bf08f` — **all superseded by pin table below**
- Terminal: `TerminalView` + Text child from `terminal_output` string; kit paints plain `Paragraph`
- Input: all `TerminalForward` → `SendInput`
- Hydration: ReadScreen primary
- Workspace draw: `draw_workspace_shell` renders focused session UiNode into `terminal_area` via kit `render_node_with_presentation_state`; hit id `tui-terminal`

### Core API (pin surface)

```rust
// botster-terminal-ghostty, feature = "libghostty-vt"
GhosttyClientProjection::new(size)?
install_ghostsnp(&snapshot.history.decoded_bytes()?)?  // GHOSTSNP only
apply_terminal_output(live_utf8_or_bytes)
project_viewport() -> ViewportProjection { cols, rows, cells, cursor }
scrollbar() / scroll(ScrollOp::{Top,Bottom,Delta})
color_profile() / mode_flags() / dimensions() / resize()
```

No PTY ownership; no OSC answers. Never `install_ghostsnp` Scrollback.

## Product architecture

```text
Hub Snapshot.history.decoded_bytes() ──► GhosttyClientProjection::install_ghostsnp
Hub TerminalOutput.data ───────────────► apply_terminal_output
project_viewport / scrollbar / color_profile
        │
        ▼ TUI-owned paint (locked seam below)
kit TerminalView hit region + title chrome
ModeGatedInput for Kitty/mouse; ReadScreen optional diagnostic only
```

### Locked production paint seam (findings `_552747`, `_641936`)

**Decision:** TUI owns styled-cell paint. Kit does **not** gain Ghostty truth.

**Ratatui 0.30.2 constraint (verified):** `Frame` does **not** expose `buffer_mut()`. Paint goes through `frame.render_widget(widget, area)` where the widget implements `ratatui::widgets::Widget` and receives `&mut Buffer` in `Widget::render`.

**Locked production sequence** (workspace shell and any path that draws the focused session terminal):

1. Keep `UiNodeKind::TerminalView` node id **`tui-terminal`** so kit still:
   - registers `HitRole::TerminalView` on the real terminal chrome
   - accepts `HitMap::set_terminal_mouse_mode("tui-terminal", …)`
   - draws border/title chrome (and monochrome Text child placeholder — not authority)
2. Call kit `render_node_with_presentation_state(...)` for the focused-session panel into the workspace pane as today.
3. **Locate the real terminal rectangle only after kit draw**, via public kit API:
   ```text
   hit_map.regions().iter().rev()
     .find(|r| r.node_id == "tui-terminal" && r.role == HitRole::TerminalView)
     .map(|r| r.rect)
   ```
   Do **not** paint into the outer workspace `terminal_area` pane. That pane can be larger/taller than the terminal when `focused_session_panel` also renders `workspace-error` (or other siblings) above the `tui-terminal` node.
4. If no `tui-terminal` TerminalView region exists (error-only panel, detached copy-only, etc.): **do not** paint projection cells; real-frame tests must cover this error-row case as a negative (no projection paint / no crash).
5. When the region exists, paint with a **TUI-local** type, e.g. `ProjectionWidget<'a> { projection: &'a ViewportProjection }` implementing `ratatui::widgets::Widget`:
   ```text
   let outer = tui_terminal_region.rect;           // from HitMap
   let inner = botster_tui_kit::terminal_inner_rect(outer);  // public kit helper
   frame.render_widget(ProjectionWidget { .. }, inner);
   ```
   Inside `Widget::render`, write each `ProjectedCell` into the provided `Buffer` (fg/bg/modifiers), and paint cursor when `cursor.in_viewport`.
6. Empty / detached / non-attach states: kit Text placeholders or no region; no fake GHOSTSNP.
7. Scroll keys/wheel when terminal focused: `ScrollOp` on projection, then repaint.

**Real-frame proof must include:** styled cells, cursor, palette/special colors, scroll position, later live output, **and** the workspace-error sibling case (region missing or non-full pane).

### Attach / reconnect H0–H5

| Step | Behavior |
| --- | --- |
| H0 | Attach; clear projection + presentation |
| H1 | Buffer live TerminalOutput |
| H2 | Snapshot → `decoded_bytes` → `install_ghostsnp` (fail closed); never Scrollback |
| H3 | Attached readiness after install |
| H4 | `project_viewport` + TUI paint; scroll via Core scrollbar |
| H4b | Hub `ReadModeFlags` for ModeGatedInput freshness |
| H4c | ReadScreen optional diagnostic only |
| H5 | `apply_terminal_output` buffered then live |
| Reconnect | Full cycle |

## Exact Cargo changes (`crates/botster-tui/Cargo.toml`)

Replace dependency pins with **exactly**:

```toml
[dependencies]
botster-core = { git = "https://github.com/trybotster/botster-core.git", rev = "4d0d1d8832d19352454a0789419a3e31e67d50df", default-features = false }
botster-terminal-ghostty = { git = "https://github.com/trybotster/botster-core.git", rev = "4d0d1d8832d19352454a0789419a3e31e67d50df", features = ["libghostty-vt"] }
botster-hub-client = { git = "https://github.com/trybotster/botster-hub.git", rev = "89dae7e15a844bcb7411b83b32581121720e23eb" }
botster-tui-kit = { git = "https://github.com/trybotster/botster-tui-kit.git", rev = "32d804e3bbcb982e77113d5df12374baa8e9a2fa" }
botster-ui-contract = { git = "https://github.com/trybotster/botster-hub.git", rev = "89dae7e15a844bcb7411b83b32581121720e23eb" }
crossterm = "0.29.0"
ratatui = "0.30.2"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

[dev-dependencies]
botster-core-test-support = { git = "https://github.com/trybotster/botster-core.git", rev = "4d0d1d8832d19352454a0789419a3e31e67d50df", default-features = false }
botster-hub-test-support = { git = "https://github.com/trybotster/botster-hub.git", rev = "89dae7e15a844bcb7411b83b32581121720e23eb" }
jsonschema = { version = "0.49.2", default-features = false }
```

### Lockfile refresh (executable on current dual-core lock)

**Do not run** bare `cargo update -p botster-core` — on the current lock it exits **101** (ambiguous: `branch=main` via hub-test-support **and** direct `rev=16bf08f…`). Verified:

```text
cargo update -p botster-core --dry-run
# error: There are multiple `botster-core` packages… specification is ambiguous
```

**Required sequence after writing the Cargo.toml block above:**

```sh
# Source-qualified updates using package IDs present on current abc804e lock.
# Cargo resolves each package to the NEW rev required by the edited Cargo.toml.
# Verified dry-run form for the direct core rev package succeeds on this worktree.

# Verified dry-run exit 0 on base abc804e lock (ui-contract version is 0.3.2, not 0.1.0).
cargo update \
  -p 'git+https://github.com/trybotster/botster-core.git?rev=16bf08f29ec723c70c290cf995745ccbf79d4f05#botster-core@0.1.0' \
  -p 'git+https://github.com/trybotster/botster-core.git?rev=16bf08f29ec723c70c290cf995745ccbf79d4f05#botster-core-test-support@0.1.0' \
  -p 'git+https://github.com/trybotster/botster-hub.git?rev=891cc796faeab51ee4bee1a0e8494562b233036e#botster-hub-client@0.1.0' \
  -p 'git+https://github.com/trybotster/botster-hub.git?rev=891cc796faeab51ee4bee1a0e8494562b233036e#botster-hub-test-support@0.1.0' \
  -p 'git+https://github.com/trybotster/botster-hub.git?rev=891cc796faeab51ee4bee1a0e8494562b233036e#botster-ui-contract@0.3.2' \
  -p 'git+https://github.com/trybotster/botster-tui-kit.git?rev=9d4a566f309e9d848771b5448764a87f4721468e#botster-tui-kit@0.1.0'

# botster-terminal-ghostty is NEW (not on current lock).
# Do NOT run `cargo fetch -p botster-tui` — cargo fetch does not accept `-p` (exits 1).
# After the source-qualified update, admit the new git package with:
cargo build -p botster-tui
# (refreshes lock for botster-terminal-ghostty + new revs; requires Zig/submodule once libghostty-vt builds)
```

**Note:** `botster-hub-test-support@89dae7e` still depends on `botster-core` **branch=main** (dev-only transitive). That secondary source may remain in the lock. Production direct deps must resolve to **`4d0d1d8…`**. cargo-tree gates below enforce the production identity; do not require eliminating the hub-test-support branch source unless tree shows botster-tui linking the wrong core for non-dev edges.

If source-qualified selectors drift after a partial edit, re-read `Cargo.lock` package `source =` and package **version** lines and re-run with the **current** unique IDs — never bare `-p botster-core`, and never invent a package version (ui-contract is **0.3.2** on current lock).

### Cargo-tree / identity checks (required)

```sh
# single ui-contract source
cargo tree -p botster-tui -i botster-ui-contract
# expect one git rev 89dae7e…

# production Ghostty client source
cargo tree -p botster-tui -i botster-terminal-ghostty
# expect rev 4d0d1d8… with feature libghostty-vt

# test support same core rev
cargo tree -p botster-tui --edges normal,dev -i botster-core-test-support
# expect rev 4d0d1d8…
```

Fail Implement if two ui-contract revs appear or terminal-ghostty lacks `libghostty-vt`.

### libghostty-vt build path

`botster-terminal-ghostty` with `libghostty-vt` needs vendored Ghostty + Zig 0.16 (Core README). For the TUI consumer:

```sh
# once per machine/worktree that builds native Ghostty
# Cargo will fetch botster-core git sources; initialize submodule inside the
# resolved botster-terminal-ghostty package source (Cargo git checkout):
#   $CARGO_HOME/git/checkouts/botster-core-*/4d0d1d8/crates/botster-terminal-ghostty
# or use a path override only for local debug — production pin is git rev.
git -C "$(cargo metadata --format-version=1 | jq -r '.packages[] | select(.name=="botster-terminal-ghostty") | .manifest_path' | xargs dirname)" \
  submodule update --init vendor/ghostty
# Zig 0.16.0 available as zig / BOTSTER_ZIG / mise
cargo build -p botster-tui --locked
```

Document Zig 0.16 + submodule init in README under Commands / Live hub.

Compatibility: require `FEATURE_MODE_GATED_INPUT`, conf floor **34**, protocol 6 via hub-client.

## Scope

1. Exact Cargo pins above + lock + README pin narrative  
2. Attachment-scoped `GhosttyClientProjection`; H0–H5 hydration  
3. **Locked paint seam** after kit TerminalView chrome  
4. ModeGatedInput + full ModeFlags freshness; Kitty when `kitty_enabled`  
5. Scroll → `ScrollOp`; no OSC color answers; no Scrollback install  
6. Tests + gates in Acceptance  

## Non-scope

- Kit/core/hub product reimplementation  
- Moving GHOSTSNP decode into kit  
- Pushed mode events; control-path GHOSTSNP  
- Expanding TerminalView props with Ghostty cell arrays (kit truth leak)  

## Ownership

| Layer | Owner |
| --- | --- |
| GhosttyClientProjection | core `botster-terminal-ghostty@4d0d1d8` |
| ModeGatedInput / Snapshot wire | hub-client `@89dae7e` |
| TerminalView hit/chrome/input encode | kit `@32d804e` |
| Install policy, ModeGatedInput dispatch, **styled paint** | **botster-tui** |

## Assumptions and unknowns

- Human product: install+render GHOSTSNP; ReadScreen diagnostic only  
- Default `GhosttyAdapterConfig` (max_scrollback 0) honors snapshot producer policy  
- Kit TerminalView remains monochrome Paragraph for Text children — paint overlay is required, not optional  
- Unknown residual: exact scroll keybinding mapping (reuse existing terminal focus wheel/key paths if already forwarded; otherwise map focused-terminal wheel/page keys to `ScrollOp` without stealing outer UI)  

## Affected surfaces

- `crates/botster-tui/Cargo.toml`, `Cargo.lock`  
- `crates/botster-tui/src/app.rs` — projection, H0–H5, ModeGatedInput, paint helper, tests  
- `crates/botster-tui/src/renderer.rs` — only if paint helper lives there as TUI-local code  
- `README.md` — pins, Zig/submodule, live binary pin `89dae7e`  
- `script/*` — only if new filter env needed; prefer existing `script/test` / `script/test-live-hub`  

### Production entry points

1. Handshake: conf ≥ 34 + `mode_gated_input`  
2. Attach: Snapshot → `install_ghostsnp` before live apply  
3. `draw_workspace_shell` / `draw`: kit render → **HitMap `tui-terminal` region** → `frame.render_widget(ProjectionWidget, terminal_inner_rect(region))`  
4. Input: ModeGatedInput for Kitty/mouse; `set_terminal_mouse_mode("tui-terminal", …)`  
5. Resize: hub Resize + `projection.resize`  

## Risks

| Risk | Mitigation |
| --- | --- |
| Dual ui-contract | cargo-tree check; exact pins |
| Ambiguous `cargo update -p botster-core` | source-qualified IDs only |
| libghostty-vt/Zig/submodule | documented build path; build fails hard without Zig |
| Wrong paint rect / unavailable API | HitMap region + Widget::render only; error-row negative test |
| Soft-skip live | explicit BOTSTER_*_BIN required; script already exits 1 if missing |
| ReadScreen primary regress | tests fail if install path skipped |

## Acceptance checks / tests (executable)

### Local workspace gates (always)

```sh
cd "$TUI_WORKTREE"   # pipeline worktree
script/fmt
script/test
script/clippy
```

(`script/test` = `cargo test --workspace --all-targets`.)

### Pin / tree gates

```sh
cargo tree -p botster-tui -i botster-ui-contract | tee /tmp/tui-ui-contract.tree
# must contain only rev 89dae7e15a844bcb7411b83b32581121720e23eb
cargo tree -p botster-tui -i botster-terminal-ghostty | tee /tmp/tui-ghostty.tree
# must contain rev 4d0d1d8832d19352454a0789419a3e31e67d50df
cargo tree -p botster-tui --edges normal,dev -i botster-core-test-support | tee /tmp/tui-core-test.tree
# must contain rev 4d0d1d8832d19352454a0789419a3e31e67d50df
```

### Focused unit/integration tests (exact names to add in `app.rs` tests)

```sh
script/test -- --exact app::tests::ghostty_install_snapshot_before_live_applies_output
script/test -- --exact app::tests::ghostty_scrollback_event_never_calls_install_ghostsnp
script/test -- --exact app::tests::ghostty_paint_real_frame_shows_styled_cells_cursor_and_palette
script/test -- --exact app::tests::ghostty_scroll_op_moves_viewport_to_pre_attach_history_marker
script/test -- --exact app::tests::mode_gated_kitty_and_mouse_use_freshness_with_single_reprobe
script/test -- --exact app::tests::read_screen_is_non_authoritative_when_projection_installed
```

**Real-frame paint proof** (`ghostty_paint_real_frame_…`): drive production draw path that (1) kit-renders focused session, (2) resolves `tui-terminal` HitMap region, (3) `frame.render_widget(ProjectionWidget, inner)`; assert styled fg/bg, cursor, palette-driven colors, scroll-moved history marker, later live cell change; **plus** workspace-error sibling case with no false paint into outer pane. `color_profile()` alone is insufficient.

### Live hub gates (fail closed)

Build pin-matched binaries from Hub **`89dae7e15a844bcb7411b83b32581121720e23eb`** (session worker package `botster-core-daemon` at the Core rev locked by that Hub checkout — re-verify lock at Implement; expected Core for Ghostty contract family is `2c5171a…` or the lock tip that Hub `89dae7e` records). Set:

```sh
export BOTSTER_HUB_BIN=/absolute/path/to/89dae7e/target/.../botster-hub
export BOTSTER_SESSION_WORKER_BIN=/absolute/path/to/89dae7e/target/.../botster-session-worker
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/botster-tui-live-ghostty-target}"
# fail closed: script/test-live-hub exits 1 if bins missing/non-executable
test -x "$BOTSTER_HUB_BIN" && test -x "$BOTSTER_SESSION_WORKER_BIN"
```

Contract-matrix live (existing entrypoint — still required regression):

```sh
export BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE=/path/to/hub@89dae7e/packages/hub-test-support/fixtures/plugin-contract-matrix
script/test-live-hub
```

Ghostty-specific live proof (extend headless live runtime; exact test name):

```sh
script/test -- --exact app::tests::headless_live_runtime_ghostty_install_scrollback_palette_and_mode_gated_input
```

That live test **must**:

1. Use `BOTSTER_HUB_BIN` / `BOTSTER_SESSION_WORKER_BIN` (or hub-test-support spawn of those exact bins) — abort if unset  
2. Spawn session, produce pre-attach history including a marker that ends **outside** the default viewport  
3. Late attach: observe Snapshot install via production path; assert marker reachable after scroll through **painted** frame  
4. OSC palette/special mutation then assert painted colors / `color_profile` agreement  
5. Mode-gated mouse or kitty path admitted after ReadModeFlags  
6. Later live output appears in projection paint  
7. Soft residual / skip without bins = **failure**

Optional: wrap the new live test in `script/test-live-hub` only if existing modes cannot host it; default is exact `cargo test` filter via `script/test -- --exact …` with bins exported.

### Summary gate set for Review/Verify

1. `script/fmt`  
2. `script/test`  
3. `script/clippy`  
4. cargo-tree identity checks  
5. Exact unit tests listed above  
6. Live with pin-matched hub + session-worker bins  

## Implementation sequence

1. Apply exact Cargo.toml pins; source-qualified `cargo update` (ui-contract@0.3.2); `cargo build -p botster-tui`; cargo-tree checks  

2. Submodule/Zig for `libghostty-vt` until `cargo build -p botster-tui` succeeds  
3. Projection state + H0–H5  
4. `ProjectionWidget` via `frame.render_widget` on HitMap `tui-terminal` inner rect after kit render  

5. ModeGatedInput + ModeFlags  
6. Exact tests + live; README pins  

## Finding disposition

| Finding | Status in rev 7 |
| --- | --- |
| Prior product findings (GHOSTSNP, paint API, gates) | Resolved in rev4–6 |
| UI contract `@0.1.0` selector | **Fixed to `@0.3.2`**; multi-selector dry-run exit 0 |
| Invalid `cargo fetch -p` | **Removed**; **`cargo build -p botster-tui`** admits new package |
| Checklist / artifact_id | Reuse checklist; full evidence on gate+advance |

## Vault gaps

Post-implement: TUI paint-after-TerminalView pattern; pin table for hub/kit/core Ghostty stack.
