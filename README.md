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
BOTSTER_HUB_SOCKET=/path/to/botster.sock cargo run -p botster-tui
```

The interactive renderer opens the alternate terminal screen and exits with
`q`, `Esc`, or `Ctrl-C`.

This standalone crate does not start a hub daemon. For local dogfood, start an
installed Botster hub separately and pass its socket path into the TUI:

```sh
botster start --headless
botster status
BOTSTER_HUB_SOCKET=/path/from/status/or/runtime-artifact.sock cargo run -p botster-tui
```

The TUI socket client uses the existing attach protocol: it sends `hello`,
waits for `hello_ack`, subscribes to the `hub` channel as `tui_hub`, and then
requests core entity snapshots with `hub:entities` for `hub`,
`connection_code`, `session`, `session_action`, `workspace`, `spawn_target`,
and `worktree`. Path existence or Unix connect success alone is not treated as
a usable hub connection.

Dogfood checks in the UI:

- Leave the prompt blank and activate `Spawn and attach` to see local validation.
- Provide a branch or issue plus a prompt to send a `create_agent` command over
  the hub socket.
- Once a session entity is present, activate `Attach first session`; terminal
  bytes and scrollback delivered by the hub are shown below the `terminal_view`,
  while keyboard input and resize dispatch back to the socket client.

## Scope

Included now:

- Root Cargo workspace.
- One binary client crate at `crates/botster-tui`.
- A real binary entry point with a noninteractive `--smoke` path.
- A production dogfood surface for session spawn/attach that renders through the
  shared `UiNode` renderer instead of the old demo fixture.
- A small local hub socket adapter for the installed Botster attach protocol:
  length-prefixed JSON/PTY frames, `hello`/`hello_ack`, hub subscribe, explicit
  core entity snapshot pull, `create_agent`, terminal subscribe, PTY input, and
  resize.
- A first ratatui renderer registry for shared `botster-core` `UiNode`
  primitives: stack/inline/panel/scroll_area/text/badge/status_dot/empty_state,
  list/list_item, table-as-list fallback, button actions, form inputs,
  field errors, dialog, and safe unsupported fallback.
- Core UI renderer conformance fixture coverage through
  `botster-core-test-support` with `default-features = false`.
- A runtime draw path that renders the dogfood session surface through the
  production renderer and dispatches terminal events through the renderer hit map.
- Deterministic format, test, and clippy scripts.

Not included yet:

- Starting or provisioning the hub daemon from this repo.
- Pairing, auth, socket discovery, plugin entity-bound lists, or browser routes.
- Plugin execution, Project Pipelines policy, browser surfaces, or hub/core
  runtime policy.

## License

Botster is released under the [O'Saasy License Agreement](LICENSE).
