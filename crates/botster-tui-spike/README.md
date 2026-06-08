# Botster TUI Spike

This crate is a scaffold-only proof for the ratatui + crossterm foundation ticket.
The current worktree has no production `botster-tui` crate, so the proof models the
adapter boundary rather than wiring a production entry point.

Run it with:

```bash
./test.sh -p botster-tui-spike
CARGO_TARGET_DIR=/tmp/botster-tui-spike-target cargo run -p botster-tui-spike
```

The explicit `CARGO_TARGET_DIR` avoids macOS dynamic-library path failures when
this Botster session worktree path contains a `:` from the Git remote name.

The tests exercise:

- split-pane, list, form, dialog, and terminal-view rendering through `UiNode`-like fixtures
- crossterm hover, click, scroll, and key translation
- stable-node-id hit map lookup
- semantic action emission through an `ActionBindingV1`-shaped envelope
- terminal-view focus/input forwarding as a separate path from ordinary widget actions
- bounded redraw scheduling under frequent terminal output
