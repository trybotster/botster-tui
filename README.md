# botster-tui

`botster-tui` is Botster's first-party daily-use terminal workspace. It presents
authoritative hub sessions, keeps selection separate from terminal attachment,
and adapts from wide split panes to a compact stacked layout. It is a hub client
over core APIs and the shared TUI renderer kit, not a runtime policy owner.

## Role

This crate consumes the shared Botster UI contract from `botster-core` and the
reusable Ratatui/Crossterm mechanics from `botster-tui-kit`:

- Render `UiNode` trees with kit-owned ratatui widgets.
- Route keyboard, mouse, form, list, and terminal input through the kit-owned
  input router.
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

The workspace uses `botster-tui-kit` pinned to revision
`fb0fdcb87d102232cb015b6da782a971903b4190` and its compatible UI-contract
revision `7d52fb78024b45764d6830cf4c6b131f13a83e62`. Runnable-entrypoint connection
decoding and validation consume `botster-core` revision
`16bf08f29ec723c70c290cf995745ccbf79d4f05`. The kit supplies semantic
viewport layouts, state-aware rendering, scroll areas, toolbar overflow,
focus reconciliation, complete terminal SGR mouse reports, and `HitMap`
occlusion barriers. The kit owns reusable
Ratatui/Crossterm `UiNode` rendering, hit maps, form/list routing, and terminal
input forwarding. Semantic controls focus and capture on left Down, then
activate only on matching-node left Up; `terminal_view` deliberately keeps its
left-Down focus/attach behavior and forwards the trailing SGR release when
mouse mode is focused.

The app does not yet display multi-click counts or drive the optional scroll
normalizer poll/deadline clock, and it needs no app-specific occlusion helpers
beyond the kit's `HitMap` behavior. Production mouse-mode ownership is split
deliberately: core keeps the closed `terminal_view` contract (`session_id` plus
optional `title`), the hub exposes authoritative emulator mode flags, this client
keeps an attachment-scoped `u8` shadow and reapplies it after every render, and
the kit converts tracking bits `1|2|4` into full-stream SGR routing. Bit `8` alone
selects SGR encoding but does not enable tracking. Failed, malformed, stale, or
detached readback clears the client shadow to safe-off.
`botster-tui` owns the first-party hub client app, including workspace
composition, hub connection setup, session presentation, packages, installed
apps, marketplace diagnostics, and terminal attach/input/resize/drain behavior.

## Session workspace

The default surface is session-first:

- A compact status line distinguishes connected, unavailable, disconnected,
  and reconnecting hub state.
- The session navigator distinguishes local pending spawns, authoritative
  running/failed/exited lifecycle, local selection, and the attached stream.
- The focused pane explains why attachment is available or disabled and shows
  terminal content only for the explicit attachment.
- The contextual toolbar keeps Spawn and Attach reachable, moves secondary
  actions into kit-owned overflow, and requires confirmation before Shutdown or
  Remove.
- System details contains package, app, plugin, compatibility, configuration,
  diagnostics, and command editing in a scrollable secondary surface.

Expanded (`>=120` columns) and regular (`80..119`) terminals use side-by-side
navigation and focus panes. Compact terminals (`<80`) stack them vertically.
Tab and Shift-Tab move focus; arrows navigate focused controls; Enter or Space
activates; PageUp/PageDown and the mouse wheel scroll; `Esc` cancels an open
confirmation or exits otherwise. `q` and `Ctrl-C` also exit.

## Commands

From a fresh clone:

```sh
script/fmt
script/test
script/clippy
cargo run -p botster-tui -- --smoke
cargo run -p botster-tui
```

The interactive renderer opens the alternate terminal screen and uses the
workspace shortcuts documented above.

## Live hub verification

The session workspace uses the authoritative external hub client protocol
from `botster-hub-client`, pinned to botster-hub revision
`02bffebd0e29cb69a8e1e639e01f704f6dfffe48`. The protocol source is
`crates/botster-hub-client/src/lib.rs` in that repository; it owns the daemon
handshake, request/response frames, session spawn/attach, input, resize, and
drain events. `botster-tui` does not implement a private socket protocol.

Run against a separately started isolated hub:

```sh
hub_dir="$(mktemp -d /tmp/botster-tui-hub.XXXXXX)"
botster-hub start --data-dir "$hub_dir"
BOTSTER_HUB_CONNECTION="{\"transport\":{\"type\":\"unix_socket\",\"path\":\"$hub_dir/botster-hub.sock\"}}" \
BOTSTER_HUB_DATA_DIR="$hub_dir" \
  cargo run -p botster-tui
botster-hub shutdown --data-dir "$hub_dir"
```

The headless live-runtime path proves the same client/app surface without opening the
alternate screen:

```sh
BOTSTER_HUB_CONNECTION="{\"transport\":{\"type\":\"unix_socket\",\"path\":\"$hub_dir/botster-hub.sock\"}}" \
BOTSTER_HUB_DATA_DIR="$hub_dir" \
  cargo run -p botster-tui -- --headless-live-runtime
```

The visible System details diagnostics are intentionally local-client
diagnostics, not private hub probes. They distinguish:

- missing, malformed, or invalid `BOTSTER_HUB_CONNECTION` configuration;
- local hub unavailable, disconnected, or reconnecting;
- compatibility mismatch and unsupported feature diagnostics from the
  `botster-hub-client` compatibility handshake;
- observed daemon compatibility descriptor values from status, including
  protocol, protocol version, feature list, conformance fixture revision, and
  status schema version;
- package registry state from public status/list responses, including installed
  package count, enabled package count, package name, version, classification,
  package state, requested capabilities, provider profile admission, package
  availability, dependency availability, feature availability, and hub-supplied
  blocked reason/action rows;
- installed app rows from public app registry responses, including package id,
  app id, entrypoint id, app kind, launch mode, lifecycle state, blocked reasons,
  diagnostics, hub-provided action descriptors, web app local URLs, and terminal
  app launchability;
- marketplace available package rows from public package lifecycle responses,
  including entry id, source labels, first-party state, compatibility results,
  requested capabilities, pin metadata, install plans, update status, and package
  action decisions;
- package compatibility failures and package errors through public diagnostics,
  including diagnostic operation and feature fields for package registry work;
- package configuration schema and sanitized values from public package rows,
  including string, boolean, select, multiline text, and secret-placeholder
  fields, required/missing state, package-level diagnostics, and update
  submission through the hub daemon;
- plugin app/settings route rows from public package/app DTOs, and hub-delivered
  plugin surface/action responses rendered through the shared TUI `UiNode`
  renderer path;
- connected, terminal stream unavailable, action failure, and startup
  diagnostics from public `DaemonDiagnostic` rows on status, response, operator
  error, and compatibility error payloads;
- action or validation failures that stay visible after unrelated successful
  refreshes.

Package install, enable, disable, remove, entrypoint, and update flows remain
owned by hub package operations rather than private TUI-only controls. The TUI
renders hub-resolved dependency/auth/update state and does not infer it from
package configuration, capabilities, or local registry paths. Configuration
edits submit the hub-owned package configuration value shape; secret fields
render only state markers and never raw secret material.

Installed app rows are also hub-owned. `web_app` rows show only the
hub-provided `local_url` and copy/open instructions; if the hub omits a URL, the
TUI keeps the row visible with blocked reasons or diagnostics instead of
guessing a port. `terminal_app` rows show launchability from lifecycle, blocked
reasons, diagnostics, and action descriptors; app action descriptors are
display-only in this client path.

The focused terminal distinguishes selected session from attached stream.
Selecting a row changes the attach target; terminal input is sent only after an
attach state is observed for that stream. Until then, the pane reports terminal
stream unavailable rather than silently treating selection as an attached PTY.

The session navigator opens one explicit `session` entity subscription per hub
connection. Its authoritative snapshot and strictly ordered upsert, patch, and
remove frames drive the visible rows; normal synchronization does not poll a
session list. Spawn adds an immediate client-local pending row, then the matching
authoritative entity replaces it. Spawn, selection, and terminal attachment are
separate actions, so neither appearance nor reconnect automatically attaches a
PTY. A reconnect discards the prior subscription generation and waits for the
fresh generation's snapshot before accepting deltas.

The TUI uses a deliberately narrowed compatibility requirement for the live-runtime
terminal surface: sessions, session entity subscriptions, terminal streaming,
terminal readback, package navigation, and resize. It does not require
plugin surface render/action capabilities for this path. A running but
incompatible hub is reported as a compatibility mismatch instead of being
collapsed into the generic unavailable/reconnecting state.

The live-hub smoke also runs the hub-owned plugin contract matrix harness from
`botster-hub-test-support`, then independently requests the real fixture's
app, empty, and settings surfaces through `botster-hub-client`. Those delivered
surface bodies are deserialized to `botster_core::ui::UiNode`, validated against
the core contract, checked against TUI renderer capabilities, and rendered with
the production TUI kit. Unsupported client primitives fail with the
capability-validation diagnostic, including the node id and primitive, instead
of being treated as a passing render.

## Local Package

`botster-tui` declares a first-party local package manifest in
`botster-package.json`. The package exposes one runnable entrypoint, `tui`, as a
`terminal_app` with `foreground_stdio` launch mode. It is a foreground terminal
client contract, not a background supervised web process.

For source-checkout live-runtime, build the binary and install the checkout as a
local package:

```sh
cargo build -p botster-tui
botster-hub packages install --data-dir "$hub_dir" --path <botster-tui checkout>
botster-hub packages enable --data-dir "$hub_dir" botster-tui
```

The manifest command is `target/debug/botster-tui` relative to the package root,
so source-checkout installs must build or stage that debug binary before
opening the app. `script/test-live-hub` does this staging when it uses an
external `CARGO_TARGET_DIR`.

The app-open flow launches the checked-in runnable entrypoint through the
hub-resolved foreground terminal contract. The hub supplies canonical
foreground launch environment `BOTSTER_HUB_CONNECTION` and
`BOTSTER_HUB_DATA_DIR`, and the TUI consumes both launch inputs. The connection
value is Core's validated JSON descriptor; the data directory is runtime
context and is never used to infer an endpoint:

```sh
botster-hub apps open --data-dir "$hub_dir" botster-tui
```

For lower-level client debugging, the direct foreground live-runtime command remains
available:

```sh
BOTSTER_HUB_CONNECTION="{\"transport\":{\"type\":\"unix_socket\",\"path\":\"$hub_dir/botster-hub.sock\"}}" \
BOTSTER_HUB_DATA_DIR="$hub_dir" \
  cargo run -p botster-tui
```

There is also an automated isolated-Hub test using
`botster-hub-test-support`. The wrapper accepts explicit matching
`botster-hub` and `botster-session-worker` binaries, or resolves those command
names from `PATH`; it does not discover or build a sibling Hub checkout. It
starts an isolated daemon, runs the TUI live-runtime path, runs the
revision-16 session lifecycle subscription conformance runner and plugin
contract matrix conformance harness, renders the delivered fixture
surfaces through the TUI renderer, and tears the daemon down. The renderer
coverage includes the composite application primitive fixture for `metric_grid`,
`table`, `toolbar`, `status_badge`, `section`, `empty_state`, enhanced
panel/list semantics, and form/action feedback. It also
installs/enables this checkout as a local package and opens `botster-tui`
through `botster-hub apps open` with a headless live-runtime env switch so the
foreground app exits cleanly under automation:

```sh
BOTSTER_HUB_BIN=/path/to/botster-hub \
BOTSTER_SESSION_WORKER_BIN=/path/to/botster-session-worker \
CARGO_TARGET_DIR=/tmp/botster-tui-live-target \
  script/test-live-hub
```

Under the hood, the Rust harness accepts explicit `BOTSTER_HUB_BIN` and
`BOTSTER_SESSION_WORKER_BIN` paths because `botster-tui` does not own those
binaries. If a variable is omitted, the wrapper looks up the corresponding
command on `PATH` and fails with a setup diagnostic if it is unavailable.
`CARGO_TARGET_DIR` is optional; omitting it creates and cleans up a fresh
temporary target. `BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE` may point to an
explicit Hub contract-matrix fixture directory to include live plugin-surface
proof. Normal unit tests skip the isolated runtime when the binary variables are
absent; the wrapper sets `BOTSTER_TUI_REQUIRE_HUB_TEST=1`, so missing binaries
cannot silently pass. The live-Hub test also asserts non-default compatibility
descriptor values from the isolated daemon and exercises a compatibility
mismatch through `connect_and_hello_with_requirement` with an unsatisfied
required feature.

## Scope

Included now:

- Root Cargo workspace.
- One binary client crate at `crates/botster-tui`.
- A real binary entry point with a noninteractive `--smoke` path.
- Consumption of `botster-tui-kit` for shared `botster-core` `UiNode`
  rendering and input routing mechanics.
- A state-aware runtime draw path that renders the responsive session workspace
  as shared `UiNode`, reconciles focus against each new hit map, routes semantic
  actions through the kit input router, reflects visible form drafts, and
  displays terminal bytes inside `terminal_view`.
- Push-driven hub session snapshot/delta reconciliation, pending spawn feedback,
  explicit selection/attach, terminal input, resize, drain, reconnect, and
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
