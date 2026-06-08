# ADR 0001: Ratatui + Crossterm TUI Renderer Foundation

## Status

Accepted. The foundation proof has been ported into the production
`botster-tui` renderer scaffold.

## Context

This repository started as a scaffold-only architecture spike. It now contains a
production `botster-tui` crate with a ratatui renderer entry point that consumes
the shared `botster-core` UI contract.

The Botster architecture is not greenfield. The production TUI direction is an
adapter over the existing Rust render tree:

- `stack(direction=horizontal)` maps to `HSplit`
- `stack(direction=vertical)` maps to `VSplit`
- `panel` maps to `BlockConfig`
- `list` maps to `WidgetType::List`
- `text_input` maps to `WidgetType::Input`
- `terminal_view` maps to `WidgetType::Terminal`

The spike tests that ratatui + crossterm fit the missing foundation pieces around
that adapter: split rendering, crossterm input capture, stable-node hit maps,
semantic action emission, client-local presentation state, terminal input
forwarding, and bounded redraws under frequent terminal output.

## Decision

Use ratatui + crossterm as the Botster TUI foundation.

Ratatui is the renderer foundation because it provides composable Rust-native
terminal widgets, deterministic test rendering through `TestBackend`, and a
shape that maps cleanly onto Botster's existing render tree. Crossterm is the
platform adapter foundation because it supplies cross-platform terminal events,
including mouse move, click, scroll, and key events that the renderer can translate
into Botster actions.

Production integration wires in at the adapter from shared `UiNode` snapshots
into ratatui widgets. The current scaffold renders a core-derived sample through
`crates/botster-tui/src/app.rs` and tests against the bundled
`botster-core-test-support` renderer conformance fixtures. Live entity stores are
still future transport work.

## State And Action Boundary

Renderer-local interaction state stays local:

- hover target
- captured pointer target
- focused field or terminal panel
- modal/dialog open state through the production pattern of `ui.local_state(...)`
  and `botster.presentation.{set,clear,toggle}`

Cross-process work leaves the renderer as semantic action requests:

- ordinary shared UI actions use an `ActionBindingV1`-shaped envelope
- action ids must stay finite and semantic, such as `botster.session.select`, not
  DOM or terminal event names
- terminal panels are special: focus can be semantic, but child terminal input is
  forwarded as PTY bytes through the terminal data plane rather than converted into
  ordinary widget actions

The renderer maps these concepts to:

- core `UiActionRequest` values for shared semantic actions
- renderer-local event dispatch for hover, scroll, and ignored input
- terminal input forwarding as PTY bytes, separate from ordinary widget actions

## Runtime Evidence

The `botster-tui` crate contains tests for:

- core renderer conformance fixtures from `botster-core-test-support`
- stack/inline/panel/text/badge/status_dot/list/list_item/table-as-list/button,
  form, dialog, empty-state, and unsupported fallback rendering
- form values, field errors, select/checkbox/text input, and read-only textarea
- crossterm hover, click, scroll, and terminal key event translation
- stable-node hit map lookup independent of raw screen-only coordinates
- semantic action emission through the core `UiActionRequest` envelope
- terminal-view focus and input forwarding as separate paths
- frequent output coalesced by a redraw budget instead of an unbounded tick loop

Run evidence:

```bash
script/fmt
script/test
script/clippy
cargo run -p botster-tui -- --smoke
```

In Botster session worktrees whose path contains `:`, macOS Rust test execution
can fail while constructing `DYLD_FALLBACK_LIBRARY_PATH`. Use any colon-free
`CARGO_TARGET_DIR` for those session-only runs.

## Alternatives

OpenTUI is not selected. It is promising for rich terminal UI work, but this ticket
needs a Rust-native adapter over the existing Botster TUI render tree. Adopting
OpenTUI would add a second renderer model before proving a blocker in the current
Rust path.

Bubble Tea is not selected. Its Elm-style architecture is excellent in Go programs,
but Botster's TUI runtime, PTY/session data plane, and shared renderer direction are
Rust-native. Using Bubble Tea would introduce a language/runtime boundary that does
not help the existing adapter path.

## Consequences

Positive:

- Botster can preserve TUI/browser parity through one semantic UI contract with
  renderer-specific adapters.
- The TUI foundation remains Rust-native and testable without a terminal emulator.
- Crossterm event capture can be translated into stable-node hit testing and
  semantic action requests.
- Terminal passthrough remains explicit, which protects nested rich TUIs from
  outer-widget mouse and control-key capture.

Risks:

- The real renderer must consume both `ui_tree_snapshot` and entity stores; rendering
  only the structural tree can produce connected-empty plugin surfaces.
- The semantic action vocabulary must remain additive and finite. Free-form spike
  action names would drift from the shared contract.
- Terminal mode fidelity still needs production verification with nested mouse-mode
  TUIs after the adapter is wired to `ClientWorker` and `SessionIo`.

## Assumptions

- Dependency versions were checked at implementation time: ratatui `0.30.1` and
  crossterm `0.29.0`.
- `botster-core` is consumed as a contract-only dependency with
  `default-features = false` at revision `8f2f4acf`.
