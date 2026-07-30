# TUI Hub-owned package surface contract repin plan

## Target and context

- Target repository: `botster-tui`
- Target id: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Pipeline run: `run_1785384358_879548`
- Base: `main` at `0d26ce0`
- Repository playbook: [[botster-tui-playbook]]
- Role and surface playbooks loaded: [[planner-playbook]],
  [[botster-planner-playbook]], [[botster-tui-kit-playbook]],
  [[botster-hub-client-playbook]], [[botster-runtime-reviewer-playbook]],
  [[botster-runtime-verifier-playbook]], and
  [[project-pipelines-playbook]].
- Atomic notes loaded: [[botster package surface semantics live in ui contract while hub owns admission]],
  [[package navigation entries declare discoverability not host placement]],
  [[plugin surface requests require a declared id and operation]],
  [[botster hub client crate is the external client boundary]],
  [[botster hub client compatibility descriptors belong in client crate]],
  [[tui client attach uses hub protocol not session protocol]],
  [[tui and browser are equal clients]],
  [[botster tui consumes tui kit through a thin app policy adapter]],
  [[botster tui uinode event routing captures hit regions during draw]],
  [[tui error dedup tests must drive real input handlers]],
  [[renderer acceptance tests must drive real frame backend]],
  [[external client hub tests use subprocess spawned hub test support]],
  [[adding a hub client feature constant is a three site change]],
  [[live hub target dirs can cache stale same version client schema]],
  [[plan steps need reviewable plan artifacts]], and
  [[project pipelines checklist worker timeouts require artifact evidence fallback]].
- Repository context inspected: `README.md`, workspace and crate manifests,
  `Cargo.lock`, `crates/botster-tui/src/app.rs`,
  `crates/botster-tui/src/renderer.rs`, repository gate scripts, the existing
  package/navigation/plugin action plans, and the packaged live-Hub contract
  matrix path.
- Pipeline context inspected: ticket, run, gate, closed dependency edges,
  open same-target project tickets, artifacts, findings, reviews, questions,
  and question answers through `project_pipelines_current_context`.

The authoritative target came from the ticket target id and admitted spawn
target registry, not from the process directory. The only other open
same-target ticket concerns canonical session entity binding materialization;
it does not own package descriptor, navigation, show, or plugin action scope.

## Dependency facts

The required Hub extraction merged in `trybotster/botster-hub` as
`b403bb72c1065f633ae59fd876b13024e2ab54a7` (PR 176). That revision:

- defines `PackageSurfaceDescriptor`, `PackageSurfaceKind`,
  `PackageSurfaceOperation`, `PackageNavigationEntry`,
  `PackageNavigationTarget`, and package-presentation validation in
  `botster-ui-contract`;
- changes `DaemonPackage.surfaces` to
  `Vec<botster_ui_contract::PackageSurfaceDescriptor>`;
- deletes `DaemonPackageSurfaceDescriptor` from `botster-hub-client`;
- raises the public conformance fixture revision from 19 to 24; and
- extends the packaged plugin contract-matrix conformance flow across install,
  list, show, navigation, render, and action requests.

The required `botster-tui-kit` repin merged as
`c66f1ae60235d7d0ce0993f4e9ed89068a12b7d2` (PR 25). Its workspace pins
`botster-ui-contract` to the same Hub merge commit `b403bb72...`.

The current TUI instead pins `botster-hub-client`,
`botster-hub-test-support`, and its direct `botster-ui-contract` dependency to
Hub revision `3d3623f...`, and pins `botster-tui-kit` to `22df686...`.
`TuiApp` already lists packages and navigation, renders Hub-delivered plugin
surfaces through the kit, and routes plugin actions, but package rows expose
only `surfaces=<count>` and there is no TUI `ShowPackage` action.

## Scope

1. Cold-repin all Hub-owned dependencies used by this crate to the one merged
   Hub revision `b403bb72c1065f633ae59fd876b13024e2ab54a7`:
   `botster-hub-client`, direct `botster-ui-contract`, and
   `botster-hub-test-support`. Repin `botster-tui-kit` to its merged
   `c66f1ae60235d7d0ce0993f4e9ed89068a12b7d2` revision, whose own contract pin
   is the same Hub commit. Regenerate `Cargo.lock` narrowly.
2. Update the TUI compatibility floor to conformance fixture revision 24 and
   make only the mechanical fixture-literal additions required by the fresh
   public Hub-client DTOs (`lifecycle_class`, lifecycle counters, optional
   worker counters, or equivalent fields actually required by the merged
   structs).
3. Import and consume the canonical
   `botster_ui_contract::{PackageSurfaceDescriptor, PackageSurfaceKind,
   PackageSurfaceOperation}` types directly. Render each package's admitted
   surface descriptors with stable id, kind, title, and supported operations,
   rather than proving only a surface count. Use the public resolved
   `DaemonPackageNavigationEntry` projection for runtime navigation; do not
   mirror raw manifest navigation locally.
4. Add one TUI-owned package `Show` control. Route its rendered semantic action
   through the existing input/dispatch seam to
   `DaemonRequest::ShowPackage`, apply the public `Packages` response, and show
   the same canonical descriptor details. Keep the change inside the existing
   System details package surface.
5. Extend the existing isolated-Hub contract-matrix proof so the packaged Hub
   fixture is installed/enabled and the real TUI path:
   - applies `ListPackages` and renders canonical descriptor kind/operation
     values;
   - activates the rendered package Show control through the kit hit map and a
     production Crossterm input event, observes `ShowPackage` at the public
     client seam, applies the real Hub response, and renders the same
     descriptors;
   - applies `ListPackageNavigation`, activates the rendered Open control
     through the real input router, and receives the fixture surface through
     `PluginSurfaceRender`; and
   - activates an arbitrary delivered plugin control through the real
     TestBackend/hit-map/InputRouter path, sends the exact canonical
     `UiActionRequest` through `PluginSurfaceAction`, and renders the typed
     `UiActionResult` presentation/replacement outcome.
6. Update `README.md` with the exact merged pins, revision-24 compatibility
   floor, package descriptor/show behavior, and the strengthened live-Hub
   proof. Keep the existing ownership language: Hub admits and projects;
   TUI presents and dispatches.

Every implementation line must trace to the cold repin, compilation against
the merged DTOs, package descriptor/show presentation, production interaction
proof, or the documentation made stale by those changes.

## Non-scope

- No edits to `botster-hub`, `botster-hub-client`, `botster-ui-contract`,
  `botster-tui-kit`, `botster-core`, browser code, plugin Lua, or Project
  Pipelines policy from this run.
- No local DTOs, type aliases, serde compatibility fields, copied enums,
  stringly replacement surface kinds/operations, Core package-surface imports,
  mixed Hub revisions, `[patch]` entries, path dependencies, or
  sibling-worktree overrides.
- No new navigation framework, placement/order policy, package registry
  authority, manifest parsing, admission logic, renderer primitive, or broad
  System-details redesign.
- No canonical session binding/entity-store work owned by open ticket
  `ticket_1785298229_854008`.
- No browser parity implementation. This run does not modify the shared
  contract; generated TypeScript and browser/package parity remain evidence
  owned by the merged Hub dependency. The TUI supplies the required downstream
  consumer proof.

## Ownership boundaries and cross-repository dependencies

- `botster-ui-contract` owns renderer-neutral package surface and manifest
  navigation vocabulary.
- `botster-hub` owns manifest parsing, admission, registry persistence,
  resolved navigation/routes, render/action enforcement, and sanitized
  projection.
- `botster-hub-client` owns the public daemon request/response/DTO boundary
  consumed here.
- `botster-hub-test-support` owns the packaged contract-matrix fixture and
  subprocess live-Hub conformance flow.
- `botster-tui-kit` owns generic Ratatui/Crossterm rendering, hit maps, and
  input routing.
- `botster-tui` owns package presentation, the Show/Open controls, active
  plugin surface identity, request correlation, public Hub dispatch, and
  visible result handling.

Cross-repository prerequisites are already registered against their
authoritative targets and closed:

- Hub ticket `ticket_1785294387_531161`, target
  `tgt_7e208a0c76a44980a83b63af976b1f22`, merged as `b403bb72...`.
- TUI-kit ticket `ticket_1785295913_493655`, target
  `tgt_3dfae49c02454037bf13554f552baf7f`, merged as `c66f1ae...`.

If implementation finds the named public types, packaged fixture behavior, or
revision-24 report fields absent at those exact commits, stop and ask a human;
do not substitute a later Hub commit or reimplement the dependency.

## Assumptions and unknowns

- Assumption: “the same merged Hub commit” means exact dependency merge
  `b403bb72...`, not the later current Hub main. The closed TUI-kit dependency
  independently confirms that commit by pinning it.
- Assumption: `DaemonPackageNavigationEntry` remains the correct sanitized,
  host-resolved runtime navigation projection. The raw
  `PackageNavigationEntry`/`PackageNavigationTarget` types are manifest
  declarations and do not replace the daemon projection in TUI state.
- Assumption: the existing `renderer::render_to_lines*` helpers remain valid
  real-frame proof because they delegate to the kit's Ratatui
  `Terminal<TestBackend>::draw` production renderer. Tests must pair them with
  hit-map and `InputRouter::dispatch_event` evidence; semantic tree inspection
  alone is insufficient.
- Assumption: a Show response may replace the visible package vector with the
  one authoritative package returned by Hub; the existing Refresh action
  restores the full list. Avoid a new detail-cache abstraction unless the
  actual response behavior makes this unusable.
- Unknown until compilation: which unrelated additive public DTO fields
  between revisions require fixture literal updates. Keep those changes
  mechanical and do not expose unrelated new UI.
- Unknown until live proof: the exact action-result replacement/presentation
  fields added to revision 24. Read them from the public report and delivered
  node; do not duplicate fixture expectations.

No loaded engineering convention conflicts with this plan.

## Affected surfaces and files

- `crates/botster-tui/Cargo.toml`
  - three exact Hub pins and the exact TUI-kit pin.
- `Cargo.lock`
  - one resolved `botster-ui-contract` source at `b403bb72...`, updated
    Hub-client/test-support and kit sources, and only their transitive lock
    movement.
- `crates/botster-tui/src/app.rs`
  - canonical package surface imports and rendering;
  - Show semantic action/request observation;
  - compatibility floor and mechanical fresh-DTO test fixtures;
  - unit and isolated-Hub real frame/input/public-protocol proof.
- `README.md`
  - dependency provenance, compatibility floor, package surface/show
    presentation, and live verification contract.
- `docs/plans/tui-hub-owned-package-surface-contract-repin-plan.md`
  - this reviewable plan and later accepted implementation deviations, if any.

`crates/botster-tui/src/renderer.rs` and `script/test-live-hub` should remain
unchanged unless the existing helpers cannot expose the already-owned real
frame/input evidence. Any change there must be minimal and justified in the
implementation report.

## Risks

- Mixed Git identities can make nominally identical Rust types incompatible.
  Mitigation: update all Hub pins together, consume the merged kit revision,
  and prove one lockfile source before behavior edits.
- A later Hub main can hide whether the ticket's exact merged dependency is
  sufficient. Mitigation: compile and run against `b403bb72...` artifacts,
  with distinct recorded Hub and lockfile-pinned Core worker provenance.
- Reused Hub target artifacts can retain the prior same-version client schema.
  Mitigation: build the Hub and TUI live proof in fresh target directories.
- A source-only or helper-only test can falsely prove the user path.
  Mitigation: packaged fixture, public client requests, real Ratatui
  TestBackend draw, hit regions, production Crossterm events, and observed Hub
  results must form one evidence chain.
- Directly calling `handle_dispatch` in the live proof would skip the requested
  input backend. Mitigation: obtain controls from the rendered hit map and
  dispatch Down/Up or focus/Enter through `InputRouter` before handing the
  resulting request to `TuiApp`.
- Rendering only a descriptor count would compile while leaving the new typed
  contract unproved. Mitigation: assert visible id/kind/support values for the
  packaged fixture after both List and Show.
- Treating raw package navigation declarations as placement policy would cross
  ownership boundaries. Mitigation: keep runtime navigation on Hub-resolved
  `DaemonPackageNavigationEntry` and preserve client-owned placement.
- A broad dependency bump can introduce unrelated lifecycle/UI work.
  Mitigation: use the exact two merge commits and restrict fresh DTO changes to
  compilation/test literals unless ticket behavior requires them.

## Acceptance checks and tests

### Static dependency and source proof

- `cargo tree -p botster-tui -d` shows no duplicate
  `botster-ui-contract`.
- `cargo tree -p botster-tui -i botster-ui-contract` shows the TUI, Hub client,
  Hub test support, and TUI kit converging on the same contract package.
- A lockfile scan finds exactly one
  `name = "botster-ui-contract"` package and its source is Hub revision
  `b403bb72c1065f633ae59fd876b13024e2ab54a7`.
- Source scans find no `DaemonPackageSurfaceDescriptor`, local
  `PackageSurface*`/navigation structs or aliases, Core package-surface import,
  mixed Hub revision, Cargo patch, or path/worktree override.
- The direct Hub dependencies resolve to `b403bb72...`; the kit resolves to
  `c66f1ae...`, and inspection of that kit artifact confirms its contract
  dependency resolves to `b403bb72...`.

### Repository gates

Run from the assigned `botster-tui` worktree and record commands plus exits:

```sh
script/fmt
script/test
script/clippy
cargo run -p botster-tui -- --smoke
```

Strict Clippy means the repository command exactly:
`cargo clippy --workspace --all-targets --all-features -- -D warnings`.

### Targeted TUI proof

- Unit coverage renders package descriptor ids, typed kinds, and typed
  operation support from `DaemonPackage.surfaces`.
- A real rendered package Show control produces
  `DaemonRequest::ShowPackage` only after production key or mouse routing and
  renders the Hub response.
- A real rendered navigation Open control produces
  `PluginSurfaceRender` with the Hub-projected package/surface identity.
- A delivered arbitrary plugin button or form produces
  `PluginSurfaceAction` with exact request/surface/action/node/kind/value/payload
  identity through the real input router; typed success and failure results
  preserve the existing replacement, presentation, feedback, and error rules.
- Negative controls prove no plugin action without an active matching owner,
  blocked/unsupported navigation does not open, and no built-in action id
  branch handles arbitrary plugin actions.

### Downstream live-Hub proof

Use fresh TUI and Hub target directories. Build/use:

- `botster-hub` from exact Hub merge `b403bb72...`;
- `botster-session-worker` from the distinct `botster-core` revision pinned by
  that Hub lockfile; and
- the contract-matrix fixture copied from the same Hub revision.

Record both source SHAs and binary realpaths, then run:

```sh
BOTSTER_HUB_BIN=<exact merged Hub binary> \
BOTSTER_SESSION_WORKER_BIN=<Hub-lockfile Core worker binary> \
BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE=<same-revision packaged fixture> \
CARGO_TARGET_DIR=<fresh colon-free TUI target> \
  script/test-live-hub
```

The captured output/assertions must prove, in one live run:

- compatibility protocol 4 and fixture revision at least 24;
- the packaged fixture installs/enables through the public daemon;
- List and Show return equal canonical surface descriptors with visible typed
  kind/operation values in the TUI;
- resolved package navigation opens the declared admitted surface;
- the delivered surface renders through the production kit frame backend;
- real input hit routing sends the generic action through
  `PluginSurfaceAction`;
- the real typed result changes visible TUI presentation/content; and
- the Hub remains responsive and shuts down cleanly.

Code existence, serde round trips, conformance-report fields without TUI
render/input consumption, or source regexes alone do not satisfy runtime proof.

## Workflow evidence

Run checklist `checklist_1785384751_805594` records:

- loaded vault/repository context;
- no convention conflict;
- pending implementation verification commands/evidence; and
- pending durable-knowledge capture disposition.

Checklist creation returned a plugin-worker timeout after persistence; listing
the run checklists confirmed that the single checklist existed, so it was
adopted without retrying creation.

## Vault gaps worth capturing

No new durable architecture gap is known at Plan time. The ownership and test
rules are already covered by the loaded package-surface, external-client,
renderer-backend, stale-target, and checklist-timeout notes.

Capture through the vault inbox only if implementation reveals a repeatable
new rule, such as a public List-versus-Show projection difference clients must
preserve, a revision-24 fixture behavior not represented by the existing Hub
contract notes, or a source-scan technique needed to detect same-name Git crate
identity splits. Otherwise mark the checklist capture item done with “no new
durable knowledge; existing notes covered the work.”
