# botster-tui

`botster-tui` is Botster's first-party terminal client. Like the web client, it
is an application ecosystem around a Hub rather than a single terminal
workspace. It presents authoritative hub state through app-owned navigation and
pages, and uses the shared TUI renderer kit for controls, input, and responsive
rendering. It is a hub client over core APIs, not a runtime policy owner.

## Role

This crate consumes the Hub-owned `botster-ui-contract` and the
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
`551feb151f531d59d362efdae0cc7d3a34d8e311` and the kit, Hub client, and this
crate share `botster-ui-contract`
from botster-hub revision
`302190ec2acc5ecee744432a6c9ffd1f040ebe01`. Runnable-entrypoint connection
decoding and validation consume `botster-core` revision
`16bf08f29ec723c70c290cf995745ccbf79d4f05`. The live-Hub dev harness also
receives a branch-tracked `botster-core` through `botster-hub-test-support`;
`Cargo.lock` pins that dev-only source at
`e36435f2cb583c344d6f6ba2d62c39da324c7a64`. The kit supplies semantic
viewport layouts, state-aware rendering, scroll areas, toolbar overflow,
focus reconciliation, complete terminal SGR mouse reports, and `HitMap`
occlusion barriers. The kit owns reusable
Ratatui/Crossterm `UiNode` rendering, hit maps, form/list routing, and terminal
input forwarding. Semantic controls focus and capture on left Down, then
activate only on matching-node left Up; `terminal_view` keeps left-Down focus
behavior and forwards the trailing SGR release when mouse mode is focused.

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
The application owns its outer Ratatui shell geometry and places contract-owned
`UiNode` app and plugin content into kit-rendered regions.

## Session workspace

The default surface is session-first:

- A compact status line distinguishes connected, unavailable, disconnected,
  and reconnecting hub state.
- The session navigator distinguishes local pending spawns, authoritative
  running/failed/exited lifecycle, local selection, and the attached stream.
- The focused pane explains why attachment is available or disabled and shows
  terminal content only for the explicit attachment.
- The contextual toolbar promotes one relevant action—Spawn, Attach, or
  Detach—keeps valid alternatives inline until width pressure moves them into
  kit-owned overflow, and requires confirmation before Shutdown or Remove.
- System details contains Hub software identity, package, app, plugin, Session types management (list/detail/create/edit/delete via Hub authoring reads), target-first spawn, and
  compatibility, configuration, diagnostics, and command editing in a scrollable
  secondary surface.

Expanded (`>=120` columns) terminals use a fixed 40-column session navigator;
regular (`80..119`) terminals use a 40/60 split. Compact terminals (`<80`)
stack a content-sized navigator above the terminal.
Tab and Shift-Tab move focus; arrows navigate focused controls; Enter or Space
activates; PageUp/PageDown and the mouse wheel scroll; `Esc` cancels an open
confirmation, returns from plugin-owned content to the System shell, or exits
from the base shell. `q` and `Ctrl-C` also exit.

Activating a running session row attaches that session. Moving selection with
the keyboard does not attach until Enter or Space activates the row. Clicking
the terminal only focuses an already attached terminal; it never initiates an
attachment.

## Application architecture

The target shell mirrors the web client's information architecture while
remaining native to a terminal:

- **Home** is the landing page for recent sessions and common next actions.
- **Apps** browses and launches installed apps and hosts plugin-owned surfaces.
- **Hub settings** owns General, Spawn points, Session types, Extensions, and
  Support sections. **Session types currently ships under System details** and
  moves into Hub settings when that multi-page shell exists; the aspirational
  IA above is not the shipped navigation path yet.
- **Session** is a focused terminal workspace reached by activating a session.
- Hub-admitted plugin navigation extends the app shell without taking ownership
  of it.

The application owns routes, navigation, page composition, and shell geometry.
`UiNode` descriptors define app and plugin content inside those pages, while
`botster-tui-kit` owns reusable rendering, focus, input routing, and genuine
width-constrained overflow.

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
`302190ec2acc5ecee744432a6c9ffd1f040ebe01`. The protocol source is
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
diagnostics, not private hub probes.

### Session types (System details)

Session types are authoritative Hub descriptors consumed through the
`session_type` entity subscription (`session_type_entity_subscriptions`).

- Rows group by Hub `source` and render Hub labels/roles/traits/lifecycle as
  delivered; package rows are read-only when `editable == false`.
- Edit is **lossless**: the TUI opens the editor only after
  `ShowSessionTypeDefinition` and submits wholesale `UpdateSessionType`
  definitions. Entity rows are never used as edit seeds (they omit relative
  working-directory path and environment).
- Product launch is **target-first** `SpawnSessionType` only, opened as a
  dialog from the toolbar (not buried under System details). The first step
  lists **enabled admitted** Hub spawn targets only (no client synthesis of
  `device:local` / `package:<name>`). Picking admitted target `T` **synchronously**
  calls Hub `ListSessionTypesForTarget { target_id: T }` and renders the returned
  available winners (including device Globals projected for `T`). Spawn carries
  the Hub effective `session_type_id` with `request.target_id = T`. Freeform
  `DaemonRequest::Spawn { command }` is not a product affordance (headless /
  Workspaces harness seeding may still use raw Spawn).
- Client handshake keeps `MINIMUM_CONFORMANCE_FIXTURE_REVISION = 33` and does
  **not** require `session_type_entity_subscriptions` globally; when the feature
  is missing, Session types shows a surface-local unsupported notice.
- Pins: Hub crates `cb93df53d66fead323973b5233d4589562cf57b1`, kit
  `902650dfbd56a5bdc99c1e88c04ba2e62442f703`. Cargo.lock must keep a single
  `botster-ui-contract` (workspace `[patch]` path-unifies kit’s older Hub rev
  onto the Hub pin’s byte-identical contract under
  `third_party/botster-ui-contract`) and dual `botster-core` sources
  (direct `16bf08f2…` plus `branch=main` at `ff115694…` via hub-test-support).

Live proof (independent of contract-matrix):

```sh
# Prefer Hub + session-worker binaries built from >= cb93df5 (exact pin preferred).
# In pipeline worktrees whose path contains `:`, set a colon-free target dir:
export CARGO_TARGET_DIR="/tmp/botster-tui-cargo-tgt-session-types"
export BOTSTER_HUB_BIN=/path/to/pin-matched/botster-hub
export BOTSTER_SESSION_WORKER_BIN=/path/to/pin-matched/botster-session-worker
script/test-live-hub session-types
```

The profile fail-closes when the live handshake reports conformance &lt; 33 or
missing `session_type_entity_subscriptions`. It proves product launch through
list-for-target for a real admitted spawn point `T` (not `device:local`).

### Workspaces live-acceptance lanes

`script/test-live-hub workspaces installed-driver`, `plumbing`, and `lifecycle`
are the repository-owned runtime proof that the installed Workspaces package,
including the spawn-form `session_type_id` field and lifecycle bindings, works
against a protocol-6 Hub. They require pin-matched Hub binaries (the revision
this crate pins, currently `cb93df53d66fead323973b5233d4589562cf57b1`) and an
explicit clean post-migration `botster-workspaces` package path via
`BOTSTER_WORKSPACES_PACKAGE_PATH`. A hermetic source-scan under `script/test`
also pins the acceptance driver field key so a silent `template_id` revert
cannot hide behind env-gated live tests.

`ticket_1786036326_597046` owns restoring and proving these three lanes. Open
sibling `ticket_1786038825_352271` owns the separate `contract-matrix` live
failure when that lane is red; it is not the Workspaces proof path.

## Caller-owned Workspaces Spawn acceptance

The installed TUI exposes one deterministic file-based acceptance mode for a
caller that already owns a shared Hub, package setup, Git fixtures, sequencing,
and cleanup. Set both paths when launching the installed package:

```sh
BOTSTER_TUI_ACCEPTANCE_SCENARIO=/path/to/scenario.json \
BOTSTER_TUI_ACCEPTANCE_EVIDENCE=/path/to/new-evidence.jsonl \
  botster-hub apps open --data-dir /path/to/shared-hub botster-tui
```

The v1 schema and consumer fixtures live under
`crates/botster-tui/fixtures/workspaces-spawn-driver-v1.*`. The scenario names
the existing workspace and the three assigned target/branch cases; it never
contains node ids or action payloads. The evidence path must not exist and must
be distinct from the scenario path. Each JSONL record is flushed as a complete
line, and exactly one `complete` or bounded `failure` record terminates a run.
Stdout remains the foreground terminal application's unstructured UI channel.
The JSON Schema pins the structural scenario matrix and every event payload;
the binary additionally rejects duplicate case ids, surrounding whitespace,
and expected target/branch values that do not equal their requested values,
because standard JSON Schema cannot express those cross-field equalities.
The two acceptance paths reach the installed child through the current
`botster-hub apps open` caller-environment inheritance behavior; the package
manifest declares only its normal Hub connection and data-directory injections.
If Hub launch policy later replaces inherited caller environment with declared
passthrough inputs, this contract must be routed to the Hub owner and migrated.

This mode uses the same presentation-aware production UiNode tree and realized
`HitMap` as interactive drawing at a fixed production-sized viewport. It finds
the detail Spawn opener by the producer-authored semantic action
`botster_workspaces.open_spawn`, independent of its visible label, and dispatches
the exact node id, action id, surface, kind, and payload read back from that frame.
Every control is reached with bounded Tab traversal; values are selected and typed
with key events, and submission happens only through `InputRouter`. It
opens the Workspaces surface once initially and once after keyboard-activating
the rendered Reconnect control. After that barrier, pushed session entity frames
must update subsequent renders without `ListSessions`, polling reads, list
refreshes, or synchronization surface renders.
Acceptance mode replaces the interactive event loop; it does not instrument
`run_loop`. Its request ledger therefore proves that pushed entity frames alone
update the acceptance driver's production tree and that the driver's own request
stream contains exactly two surface renders and no session-list reads. The
zero-session-list counter is a regression tripwire for a request variant the
interactive TUI does not currently construct. Scenario resolution classes are
carried into evidence for the caller's independent Git/worktree verification;
the TUI verifies returned target, branch, worktree, action, and entity facts but
does not infer the producer's Git resolution class.

The repository-owned installed-binary proof is:

```sh
BOTSTER_HUB_BIN=/path/to/botster-hub \
BOTSTER_SESSION_WORKER_BIN=/path/to/botster-session-worker \
BOTSTER_WORKSPACES_PACKAGE_PATH=/clean/path/to/botster-workspaces \
  script/test-live-hub workspaces installed-driver
```

That test alone owns an isolated Hub and Git matrix so it can execute the exact
installed package through `apps open` before merge. Production acceptance mode
never starts or stops a Hub, installs or enables packages, creates fixtures, or
performs shared cleanup. The downstream Workspaces integration remains
responsible for independently proving Hub, Git, worktree, package, membership,
and session truth from its one long-lived shared Hub.

The System details diagnostics distinguish:

- missing, malformed, or invalid `BOTSTER_HUB_CONNECTION` configuration;
- local hub unavailable, disconnected, or reconnecting;
- compatibility mismatch and unsupported feature diagnostics from the
  `botster-hub-client` compatibility handshake;
- authoritative Hub software identity from `DaemonStatus.software`, including
  product name, product id, version, and build revision when the Hub reports
  one. Hub identity is never derived from an installed package row, and an
  absent build revision renders as absent rather than as a placeholder;
- observed daemon compatibility descriptor values from status, including
  protocol, protocol version, feature list, conformance fixture revision, and
  status schema version;
- package registry state from public status/list responses, including installed
  package count, enabled package count, package name, version, classification,
  package state, requested capabilities, provider profile admission, package
  availability, dependency availability, feature availability, and hub-supplied
  blocked reason/action rows. Each admitted package surface renders its shared
  contract id, typed kind, title, and supported operations; the package `Show`
  control requests the Hub-owned detail projection without deriving manifest
  policy in the TUI;
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
Focusing a row changes the attach target; activating it explicitly requests the
attachment. Terminal input is sent only after an attach state is observed for
that stream. Until then, the pane reports terminal stream unavailable rather
than silently treating focus as an attached PTY.

The session navigator opens one explicit `session` entity subscription per hub
connection. Its authoritative snapshot and strictly ordered upsert, patch, and
remove frames drive the visible rows; normal synchronization does not poll a
session list. Target-first SpawnSessionType adds an immediate client-local pending
row, then the matching authoritative entity replaces it. Spawn, selection, and terminal attachment are
separate actions, so neither appearance nor reconnect automatically attaches a
PTY. A reconnect discards the prior subscription generation and waits for the
fresh generation's snapshot before accepting deltas.

The TUI uses a deliberately narrowed compatibility requirement for the live-runtime
terminal surface: sessions, session entity subscriptions, terminal streaming,
terminal readback, package navigation, resize, and plugin surface render/action.
A running but incompatible hub is reported as a compatibility mismatch instead
of being collapsed into the generic unavailable/reconnecting state.
The daemon protocol version is matched **exactly**, not as a floor: the client
requires protocol version 6 and refuses any other, so a newer Hub is rejected as
firmly as an older one. The conformance fixture revision keeps minimum
semantics with a floor of 31. Fixture revisions 16–30 and every protocol version
other than 6 fail through the structured compatibility diagnostic; there is no
fallback path.

When the Hub delivers a plugin surface, the TUI keeps a stable client-owned
status/navigation shell and makes the plugin tree the interactive content
owner. Button and form requests retain the kit-authored request, surface,
action, node, kind, values, and payload identity and route through
`PluginSurfaceAction`. Matching typed results apply accepted presentation
operations and content replacements through the kit; rejected results retain
the original content, router drafts, focus, and visible field/form feedback.
Unknown or colliding plugin action IDs never enter the built-in
`botster.tui.*` switch, and no plugin action dispatches without a matching
active owner.

Plugin surfaces use the identity-matched `ui_tree_snapshot.body` renderer
entrypoint and project canonical `/session` `bind_list`, item-relative
`$bind`, and `bind_if` values from the TUI's existing authoritative session
subscription. Snapshot, upsert, patch, remove, and reconnect baselines affect
the next frame and hit map directly; the client does not poll or refresh the
surface, derive lifecycle classes, or keep a second session store. Missing
references select the authored empty template, while malformed or unsupported
bindings render a diagnostic instead of masquerading as unavailable. The TUI
resolves producer-authored item-relative row IDs while each `bind_list` row is
in context, then realizes every producer-keyed descendant with the canonical
Hub contract helper before handing distinct literal IDs to TUI-kit. Bound
required labels remain authored binding sentinels until the same materialization
pass. Duplicate realized IDs that can coexist in one materialized render fail
visibly before focus or action routing, including collisions between a bound
row and a static sibling; mutually exclusive responsive alternatives may reuse
an ID.

The live-hub smoke also runs the hub-owned plugin contract matrix harness from
`botster-hub-test-support`, then independently requests the real fixture's
package list, package detail, navigation, app, empty, settings, and
session-binding surfaces through `botster-hub-client`. It renders the same typed
package descriptors after List and Show, activates Show and resolved navigation
Open through the production frame hit map and Crossterm input router, and
restores the complete package list through Refresh. Those delivered surface
bodies arrive as typed `botster_ui_contract::UiNode`, are validated against the
Hub-owned contract, checked against TUI renderer capabilities, and rendered
with the production TUI kit. The session-binding proof drives a canonical
descendant `rename` by keyboard and `remove` by mouse, then requires the live
Hub results to echo each exact control ID and row payload. An arbitrary
delivered plugin control is likewise activated through the real hit map/input
path, and its typed action result must
change visible presentation. Unsupported client primitives fail with the
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
hub-resolved foreground terminal contract. The hub supplies
`BOTSTER_HUB_CONNECTION` as the canonical foreground launch environment and
`BOTSTER_HUB_DATA_DIR` as package storage context. The TUI decodes and validates
the Core-owned connection descriptor, shows whether storage context was supplied
in System details, and never uses storage context to infer an endpoint:

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
revision-25 session lifecycle/presentation conformance runner and plugin
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
BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE=/path/to/hub/packages/hub-test-support/fixtures/plugin-contract-matrix \
CARGO_TARGET_DIR=/tmp/botster-tui-live-target \
  script/test-live-hub
```

The wrapper also exposes one production-shaped Workspaces mode with an explicit
profile. It never discovers a sibling checkout: the caller supplies a clean
`botster-workspaces` package path, and the harness validates the package
manifest before starting an isolated Hub.

```sh
BOTSTER_HUB_BIN=/path/to/botster-hub \
BOTSTER_SESSION_WORKER_BIN=/path/to/botster-session-worker \
BOTSTER_WORKSPACES_PACKAGE_PATH=/path/to/clean/botster-workspaces \
CARGO_TARGET_DIR=/tmp/botster-tui-workspaces-plumbing-target \
  script/test-live-hub workspaces plumbing
```

The `plumbing` profile installs, enables, and reloads the real package through
public Hub requests; opens admitted Workspaces navigation; renders the
owner-authored index and detail; and routes both mouse and keyboard actions
from the production frame and hit map using the exact delivered node, action,
and payload identity. It exits zero only after its named completion ledger and
isolated-Hub cleanup are complete. This is package plumbing and generic TUI
action proof, not Workspaces lifecycle product proof.

The strict-superset lifecycle command is the downstream consumer gate:

```sh
BOTSTER_HUB_BIN=/path/to/botster-hub \
BOTSTER_SESSION_WORKER_BIN=/path/to/botster-session-worker \
BOTSTER_WORKSPACES_PACKAGE_PATH=/path/to/clean/botster-workspaces \
CARGO_TARGET_DIR=/tmp/botster-tui-workspaces-lifecycle-target \
  script/test-live-hub workspaces lifecycle
```

That profile requires 16 retained references and the producer-authored
`/session` binding contract: exact `session_uuid` plus `lifecycle_class`
filters for `current`, `ended`, and `indeterminate`, plus a separate exact-UUID
absence binding. Before spawning its two controlled lifecycle sessions, the
harness establishes the TUI-owned subscription and authoritative baseline and
proves both UUIDs are absent. It then requires both exact rows to become
authoritative `current` rows before opening the Workspaces surface. The
subsequent `current` -> `ended` -> removal barriers also match the exact UUID
and state; an empty snapshot alone is never readiness for an expected row.
Timeout diagnostics retain only the active subscription id, snapshot state and
sequence, expected UUID/state, and that row's last observed state or absence.
It joins the delivered descriptors to realized
`item_template`/`empty_template` roots, so it does not depend on headings,
incidental node-id spelling, prose, or geometric renderer position. It also
requires an individual group's realized roots to preserve the
retained-reference order in structural render traversal, including when
producer-authored wrappers sit between the group and its roots; lifecycle class
is never inferred from that order. The profile further requires an
entity-driven current-to-ended transition without a new surface request, an
absent/deleted historical reference, inert presence-detection templates,
unique canonical realized identity, real membership removal, a fresh
reconnect subscription/snapshot, explicit surface reopen, historical
rehydration, stale-generation rejection, and clean shutdown.

Producer lifecycle bindings for current/ended/unavailable groups shipped on
`botster-workspaces` (closed `ticket_1785296184_677408`). The TUI lifecycle
consumer gate is proven by a green `script/test-live-hub workspaces lifecycle`
against a pin-matched Hub and a real `botster-workspaces` checkout; a fixture
or a composed summary cannot replace that combined consumer proof.

Under the hood, the Rust harness accepts explicit `BOTSTER_HUB_BIN` and
`BOTSTER_SESSION_WORKER_BIN` paths because `botster-tui` does not own those
binaries. If a variable is omitted, the wrapper looks up the corresponding
command on `PATH` and fails with a setup diagnostic if it is unavailable.
`CARGO_TARGET_DIR` is optional; omitting it creates and cleans up a fresh
temporary target. In the default `contract-matrix` mode,
`BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE` is required and must name a Hub
contract-matrix fixture directory containing `botster-package.json` and
`plugin.lua`; the parent acceptance run uses the extracted
`package/fixtures/plugin-contract-matrix` directory from public
`@trybotster/hub-test-support@0.1.18`. The wrapper fails before building when
the selected mode's fixture/package path or Workspaces profile is missing or
invalid. Normal unit tests skip the isolated runtime when the required live
inputs are absent; the wrapper sets
`BOTSTER_TUI_REQUIRE_HUB_TEST=1`, so missing
binaries or plugin-surface proof cannot silently pass. The live-Hub test also
asserts non-default compatibility descriptor values from the isolated daemon
and exercises a compatibility mismatch through
`connect_and_hello_with_requirement` with an unsatisfied required feature.

## Scope

Included now:

- Root Cargo workspace.
- One binary client crate at `crates/botster-tui`.
- A real binary entry point with a noninteractive `--smoke` path.
- Consumption of `botster-tui-kit` for shared `botster-ui-contract` `UiNode`
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
- Generic owner-routed plugin action execution, accepted presentation and
  replacement transitions, rejected form retention, and stable-shell `Esc`
  navigation.
- Deterministic format, test, and clippy scripts.

Not included yet:

- Pairing, remote auth, or hub provisioning inside this crate.
- Plugin execution, Project Pipelines policy, browser surfaces, or hub/core
  runtime policy.

## License

Botster is released under the [O'Saasy License Agreement](LICENSE).
