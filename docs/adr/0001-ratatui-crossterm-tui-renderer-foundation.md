# ADR 0001: Ratatui + Crossterm TUI Renderer Foundation

## Status

Accepted for the scaffold proof.

## Context

This repository started as a scaffold-only architecture spike: it had no production
`botster-tui` crate or Rust renderer entry point. The proof therefore cannot wire a
real production path in this worktree. Its job is to make the intended production
boundary concrete and testable.

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

Production integration should wire in at the adapter from the shared `UiNodeV1`
snapshot plus live entity stores into the existing Rust render tree. The scaffold
crate models that boundary with `UiNode` fixtures and an `adapter_mapping()` test;
it does not replace the production tree.

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

The spike maps these concepts to:

- `UiActionRequest::Semantic`
- `UiActionRequest::LocalPresentation`
- `UiActionRequest::TerminalForward`

## Runtime Evidence

The scaffold crate contains tests for:

- split panes plus list, form, dialog, and terminal-view rendering
- crossterm hover, click, and scroll event translation
- stable-node hit map lookup independent of raw screen-only coordinates
- semantic selection action emission
- dialog dismissal through client-local presentation state
- terminal-view focus and `ctrl+j` input forwarding as separate paths
- frequent output coalesced by a redraw budget instead of an unbounded tick loop
- adapter mapping onto the existing Rust render tree names

Run evidence:

```bash
./test.sh -p botster-tui-spike
cargo run -p botster-tui-spike
```

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

- This proof is scaffold-only until applied to the production `botster-tui` crate.
- The real renderer must consume both `ui_tree_snapshot` and entity stores; rendering
  only the structural tree can produce connected-empty plugin surfaces.
- The semantic action vocabulary must remain additive and finite. Free-form spike
  action names would drift from the shared contract.
- Terminal mode fidelity still needs production verification with nested mouse-mode
  TUIs after the adapter is wired to `ClientWorker` and `SessionIo`.

## Assumptions

- The empty worktree is intentional for this architecture spike.
- The production renderer entry point exists outside this scaffold and should receive
  the adapter, not a rewrite.
- Dependency versions were checked at implementation time: ratatui `0.30.1` and
  crossterm `0.29.0`.
