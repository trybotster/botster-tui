# botster-tui

`botster-tui` is Botster’s first-party terminal client. It connects to a running
Botster hub and provides session terminals, app and package management, and
operational diagnostics.

## Run it

The normal entrypoint is the hub-managed app:

```sh
botster-hub apps open --data-dir <hub-data-dir> botster-tui
```

From a source checkout, connect directly to a running hub socket:

```sh
cargo build -p botster-tui
BOTSTER_HUB_SOCKET=/path/to/botster-hub.sock cargo run -p botster-tui
```

Run `botster-tui --help` for all command-line options. Unknown options and
missing option values fail without opening the interactive screen.

## Use it

The client has three focused views:

- **Sessions** starts, selects, attaches, and detaches terminal sessions. New
  sessions default to the user’s `$SHELL`.
- **Apps & Packages** shows installed and available software, configuration,
  blocked reasons, and actions that apply to the current lifecycle state.
- **Diagnostics** contains connection, compatibility, and transport details
  that are useful when something is wrong.

Use Tab and Shift-Tab to move between controls, arrow keys to select rows, and
Enter or Space to activate a control. While the terminal is focused, keys are
sent to the attached process; Shift-Tab returns to the surrounding UI.

Press `Ctrl-\` to exit Botster. Ordinary terminal keys—including `q`, Escape,
Tab, and `Ctrl-C`—are not client quit shortcuts.

Package removal and update application require confirmation. Secret settings
are never displayed or accepted as ordinary form values; configure them through
the hub credential flow.

## Develop it

```sh
script/fmt
script/test
script/clippy
cargo run -p botster-tui -- --smoke
```

The full isolated-hub acceptance path builds matching hub binaries, starts an
isolated daemon, exercises terminal and plugin contracts, installs this checkout
as a local package, and tears the daemon down:

```sh
CARGO_TARGET_DIR=/tmp/botster-tui-impl-target script/test-live-hub
```

For a manual isolated environment:

```sh
hub_dir="$(mktemp -d /tmp/botster-tui-hub.XXXXXX)"
botster-hub start --data-dir "$hub_dir"
BOTSTER_HUB_SOCKET="$hub_dir/botster-hub.sock" cargo run -p botster-tui
botster-hub shutdown --data-dir "$hub_dir"
```

## Architecture

The client consumes the public hub protocol from `botster-hub-client`, shared
`UiNode` contracts from `botster-core`, and reusable Ratatui/Crossterm mechanics
from `botster-tui-kit`. It emits semantic requests and renders hub-owned state;
workflow policy, package policy, terminal truth, and process supervision remain
owned by the hub and core crates.

The local package declaration is in `botster-package.json`. It exposes the `tui`
entrypoint as a foreground `terminal_app`; source-checkout installs must build
`target/debug/botster-tui` before opening it through the hub.

## License

Botster is released under the [O’Saasy License Agreement](LICENSE).
