# botster-tui

`botster-tui` is the first-party Rust terminal client scaffold for Botster.
It is a renderer/client over hub and core APIs, not a policy owner.

## Role

This crate consumes the shared Botster UI contract from `botster-core`:

- Render `UiNode` trees with ratatui widgets.
- Emit semantic action requests instead of owning workflow behavior.
- Consume entity frames for client-visible model state.
- Display `terminal_view` output and forward terminal input through the shared
  terminal data plane.

The TUI does not own plugin policy, workflow policy, hub orchestration,
authoritative terminal truth, or terminal scrollback. Terminal truth and
passthrough fidelity belong to the shared SessionIo/ClientWorker/backend
contracts. Future terminal_view work must preserve nested TUI mouse reports and
control-key input passthrough across attach and reattach paths.

## Foundation

This scaffold uses ratatui `0.30.1` and crossterm `0.29.0` on Rust 2024 with
`rust-version = "1.88.0"`. Project Pipelines dependency ticket
`ticket_1780941198_279684` was closed with no known blocker in the current
pipeline context, so ratatui plus crossterm is the accepted foundation for this
scaffold.

## Commands

From a fresh clone:

```sh
script/fmt
script/test
script/clippy
cargo run -p botster-tui -- --smoke
cargo run -p botster-tui
```

The interactive renderer opens the alternate terminal screen and exits with
`q`, `Esc`, or `Ctrl-C`.

## Scope

Included now:

- Root Cargo workspace.
- One binary client crate at `crates/botster-tui`.
- A real binary entry point with a noninteractive `--smoke` path.
- A first ratatui renderer registry for shared `botster-core` `UiNode`
  primitives: stack/inline/panel/scroll_area/text/badge/status_dot/empty_state,
  list/list_item, table-as-list fallback, button actions, form inputs,
  field errors, dialog, and safe unsupported fallback.
- Core UI renderer conformance fixture coverage through
  `botster-core-test-support` with `default-features = false`.
- A runtime draw path that renders a core-derived `UiNode` sample through the
  production renderer and dispatches terminal events through the renderer hit map.
- Deterministic format, test, and clippy scripts.

Not included yet:

- Hub connection, pairing, auth, socket attach, or terminal subscription.
- Entity-store hydration for bound plugin lists, owner-routed action execution,
  or live `terminal_view` subscription/input forwarding.
- Plugin execution, Project Pipelines policy, browser surfaces, or hub/core
  runtime policy.

## License

Botster is released under the [O'Saasy License Agreement](LICENSE).
