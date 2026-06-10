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

## Local Hub Dogfood

The dogfood session surface uses the authoritative external hub client protocol
from `botster-hub-client`, pinned to botster-hub revision
`24453ef448fb4c89ed63e784ed518de7ca301cd7`. The protocol source is
`crates/botster-hub-client/src/lib.rs` in that repository; it owns the daemon
handshake, request/response frames, session spawn/attach, input, resize, and
drain events. `botster-tui` does not implement a private socket protocol.

Run against a separately started isolated hub:

```sh
hub_dir="$(mktemp -d /tmp/botster-tui-hub.XXXXXX)"
botster-hub start --data-dir "$hub_dir"
BOTSTER_HUB_SOCKET="$hub_dir/botster-hub.sock" cargo run -p botster-tui
botster-hub shutdown --data-dir "$hub_dir"
```

The headless dogfood path proves the same client/app surface without opening the
alternate screen:

```sh
BOTSTER_HUB_SOCKET="$hub_dir/botster-hub.sock" \
  cargo run -p botster-tui -- --headless-dogfood
```

The visible diagnostics are intentionally local-client diagnostics, not private
hub probes. The hub panel distinguishes:

- missing socket configuration (`--hub-socket` or `BOTSTER_HUB_SOCKET` needed);
- local hub unavailable, disconnected, or reconnecting;
- compatibility mismatch and unsupported feature diagnostics from the
  `botster-hub-client` compatibility handshake;
- observed daemon compatibility descriptor values from status, including
  protocol, protocol version, feature list, conformance fixture revision, and
  status schema version;
- package registry state from public status/list responses, including installed
  package count, enabled package count, package name, version, classification,
  enabled/disabled/error state, requested capabilities, and provider profile
  admission;
- connected, terminal stream unavailable, action failure, and startup
  diagnostics from public `DaemonDiagnostic` rows on status, response, operator
  error, and compatibility error payloads;
- action or validation failures that stay visible after unrelated successful
  refreshes.

Package registry display is read-only in this dogfood slice. Local package
install, enable, disable, and remove flows remain owned by hub package
operations rather than private TUI-only controls.

The terminal panel distinguishes selected session from attached stream. Selecting
a row changes the attach target; terminal input is sent only after an attach
state is observed for that stream. Until then, the panel reports terminal stream
unavailable rather than silently treating selection as an attached PTY.

The TUI uses a deliberately narrowed compatibility requirement for the dogfood
terminal surface: sessions, terminal streaming, and resize. It does not require
plugin surface render/action capabilities for this path. A running but
incompatible hub is reported as a compatibility mismatch instead of being
collapsed into the generic unavailable/reconnecting state.

There is also an automated isolated-hub test using the merged
`botster-hub-test-support` crate. The preferred command builds matching
`botster-hub` and `botster-session-worker` binaries from the pinned git
dependencies, starts an isolated daemon, runs the TUI dogfood path, and tears
the daemon down:

```sh
CARGO_TARGET_DIR=/tmp/botster-tui-impl-target script/test-live-hub
```

Under the hood, the Rust harness accepts explicit `BOTSTER_HUB_BIN` and
`BOTSTER_SESSION_WORKER_BIN` paths because `botster-tui` does not own those
binaries. The wrapper script supplies those paths internally. Without the two
binary path variables, the test is skipped during the normal unit test suite.
With `BOTSTER_TUI_REQUIRE_HUB_TEST=1`, missing binaries fail the test instead of
silently skipping it. The live-hub test also asserts non-default compatibility
descriptor values from the isolated daemon and exercises a compatibility
mismatch through `connect_and_hello_with_requirement` with an unsatisfied
required feature.

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
- A runtime draw path that renders the session dogfood surface as shared
  `UiNode`, routes semantic actions through the renderer hit map, reflects
  visible form drafts, and displays terminal bytes inside `terminal_view`.
- Hub session spawn, attach, terminal input, resize, drain, reconnect, and
  validation/error states through `botster-hub-client`.
- Automated isolated-hub bring-up and teardown coverage when matching hub
  binaries are supplied to the test harness.
- Deterministic format, test, and clippy scripts.

Not included yet:

- Pairing, remote auth, or hub provisioning inside this crate.
- Entity-store hydration for bound plugin lists or owner-routed plugin action
  execution.
- Plugin execution, Project Pipelines policy, browser surfaces, or hub/core
  runtime policy.

## License

Botster is released under the [O'Saasy License Agreement](LICENSE).
