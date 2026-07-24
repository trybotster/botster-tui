# TUI Production Client and Live-Hub Verification Plan

## Pipeline and repository routing

- Ticket: `ticket_1784854094_682643`
- Run: `run_1784868767_848241`
- Step: `botster_stack_plan`
- Gate: `botster_stack_plan_gate`
- Target repository: `trybotster/botster-tui`
- Target ID: `tgt_c3d470bab78549df920a41e8fb0e58d8`
- Routed repository charter: [[botster-tui-playbook]]
- Worktree: the Project Pipelines worktree for this ticket, based on
  `botster-tui` main at `72ee491`.

The target was resolved from Project Pipelines context and the Hub spawn-target
registry before the ambient worktree was inspected.

## Context loaded

Role and workflow guidance:

- [[planner-playbook]]
- [[botster-planner-playbook]]
- [[project-pipelines-playbook]]
- [[botster-architecture]]
- [[cli-patterns]]
- [[spa-patterns]]
- [[project pipeline orchestration belongs in a device-level botster plugin]]
- [[project pipelines needs an operator workbench not more primitives]]
- [[project pipelines ui contract belongs in the plugin readme]]
- [[botster orchestration should spawn agents with explicit target ids]]
- [[botster orchestration prompts must bind agents to explicit worktrees]]
- [[botster pipeline needs continuous product owner between agent steps]]
- [[plan agents must author vault context as wikilinks not home paths]]
- [[vault example paths are not repository placement conventions]]

Repository and runtime guidance:

- [[botster-tui-playbook]]
- [[botster-tui-kit-playbook]]
- [[botster-runtime-reviewer-playbook]]
- [[botster-runtime-verifier-playbook]]
- [[tui and browser are equal clients]]
- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[botster hub client crate is the external client boundary]]
- [[tui client attach uses hub protocol not session protocol]]
- [[botster local client api lives over hubruntime not raw core routers]]
- [[tui and socket terminal streams use clientworker transport adapters]]
- [[botster tui uinode event routing captures hit regions during draw]]
- [[tui error dedup tests must drive real input handlers]]
- [[botster clients restore visible terminal state from readscreen before buffered live output]]
- [[live hub target dirs can cache stale same version client schema]]

Project Pipelines gate-discipline notes:

- [[implement gate must verify committed work and pr link before review]]
- [[verify must recheck resolved findings against the live worktree]]
- [[project pipelines sqlite write locks require preserved verdicts and operator restart]]

Repository evidence:

- `crates/botster-tui/src/app.rs` currently owns argument/environment parsing,
  client connection setup, the root application state, semantic input dispatch,
  session entity reconciliation, terminal attachment/readback, reconnect, local
  diagnostics, and the isolated live-runtime test.
- `botster-package.json` currently declares redundant connection injections and
  maps the structured injection kind to a raw socket value.
- `crates/botster-tui/tests/package_manifest_test.rs` currently requires the
  redundant raw-socket injection.
- `script/test-live-hub` discovers and builds a Hub source tree through Cargo
  dependency layout, then invokes a legacy-named Rust test.
- `README.md` documents raw-socket startup and test-only product vocabulary.
- Current planning documents and application identifiers retain the obsolete
  project-only identity.
- Repository gates are `script/fmt`, `script/test`, `script/clippy`, and
  `script/test-live-hub`; no separate checked-in CI workflow exists.

Dependency evidence:

- Core ticket `ticket_1784861040_676123` is closed and merged on Core main at
  `16bf08f`. It exports `RunnableEntrypointHubConnection`, its exhaustive
  Unix-socket transport, validation, exact JSON fixtures, and negative fixtures.
  This run now records that closed ticket as a dependency.
- The Hub productionization ticket `ticket_1784854076_565213` depends on this
  TUI ticket, so Hub launcher changes are downstream, not part of this run.
- The Hub implementation branch has producer commit `30a4233`, which serializes
  the Core descriptor into each manifest-declared target. It is suitable for
  explicit downstream proof during implementation, but this repository must
  not pin private Hub implementation code or absorb Hub ownership.
- Hub's foreground conformance still has a separately routed Core test-support
  migration, `ticket_1784868736_701877`. This TUI run must not preserve the raw
  socket contract to satisfy stale conformance.

## Product decision ledger

- Default: the production package entrypoint receives one typed Hub connection
  descriptor. Core-required package storage context is consumed only as
  user-visible runtime context and never as connection identity.
- Non-goal: retain raw socket arguments, raw socket environment fallback,
  duplicate injection kinds, or compatibility aliases.
- Non-goal: add a TUI-owned transport, endpoint discovery protocol, lifecycle
  authority, polling fallback, or Hub launcher policy.
- Follow-up acceptable: Hub reruns `up` followed by `open tui` after the TUI
  revision merges, then the final integration ticket repeats that proof from
  merged mains.
- Ask-human threshold: stop if implementation requires accepting both the typed
  descriptor and a raw legacy value, changing the canonical Core JSON shape, or
  moving Hub/package supervision into this repository.

## Scope

### 1. Cold-cut connection configuration to the Core contract

- Advance the exact `botster-core` and `botster-core-test-support` pins to a
  revision containing the merged typed descriptor and refresh `Cargo.lock`.
- Replace the app's raw socket option with a parsed
  `RunnableEntrypointHubConnection`. Deserialize and validate the canonical JSON
  before constructing `botster_hub_client::DaemonEndpoint`.
- Support the manifest-declared `BOTSTER_HUB_CONNECTION` input only. Missing,
  malformed, unknown-field, unsupported-transport, blank-path, and relative-path
  values must produce actionable configuration diagnostics without falling back
  to a raw socket.
- Consume `BOTSTER_HUB_DATA_DIR` as visible package storage context in System
  details without using it for endpoint discovery.
- Change `botster-package.json` to require `hub_connection` and Core-required
  `data_dir` context. Remove the raw-socket injection and obsolete environment
  declarations cold turkey.
- Update the manifest test to prove the exact required injection contract and
  target names against Core's current manifest model.

Production wiring proof must follow:

`main -> AppArgs::parse -> typed descriptor validation -> DaemonEndpoint -> root TUI app -> botster-hub-client requests`

The isolated live-runtime test and downstream Hub package-open run must invoke
that entrypoint. A DTO-only unit test is necessary but not sufficient.

### 2. Rename the production client surface consistently

- Rename the root app type, headless mode field/function/flag/environment
  variable, live test, fixture/runtime labels, diagnostics, and output prefixes
  to TUI, production, or live-runtime vocabulary.
- Rename application-owned UiNode and hit-region IDs from the obsolete prefix
  to a stable `tui-` prefix. Update every renderer, focus, scroll, mouse,
  terminal-forwarding, and assertion consumer in the same change.
- Preserve product behavior while renaming: selection remains distinct from
  attachment; reconnect establishes a fresh entity subscription without
  auto-attaching; terminal forwarding requires the active attachment identity;
  `ReadScreen` restoration precedes buffered live output.
- Rewrite current README and plan-document wording. Rename the one
  legacy-named live-Hub plan file with `git mv`. Do not rewrite Git history.
- Do not broaden the rename into reusable TUI-kit identifiers or shared Hub
  contracts; no such change is required for the repository audit.

### 3. Make live-Hub verification runtime-selected and timing-safe

- Keep `script/test-live-hub` as the repository's production/live-runtime
  acceptance entrypoint, but stop deriving a Hub source checkout through Cargo
  package layout.
- Accept explicit `BOTSTER_HUB_BIN` and `BOTSTER_SESSION_WORKER_BIN` paths for
  an isolated runtime. A PATH lookup may be the documented convenience default;
  every selected binary path and temporary target/root directory remains
  overridable.
- Build/stage only this repository's TUI binary. Let the Rust harness create an
  isolated temporary Hub root, generate unique session identifiers, and clean
  up only the sessions/runtime it owns.
- Pass the selected endpoint to the TUI as canonical connection JSON. Do not
  pass or parse a raw socket value.
- Remove fixed sleeps and sleep-bearing child commands from the acceptance
  path. Wait on Hub readiness, authoritative entity frames, attach/readback
  events, request responses, mode flags, process-exit frames, and bounded
  timeout-bearing protocol reads.
- Rename the focused Rust test and all headless live-runtime output. Keep its
  existing proof of authoritative session lifecycle, explicit attach,
  terminal input/readback, ordered history, reconnect with a fresh
  subscription, natural exit/removal, plugin surface rendering, compatibility
  mismatch, and cleanup.
- Add a production-path unavailable-Hub case using a valid descriptor whose
  isolated endpoint is absent. Assert the user-visible actionable diagnostic
  through the real app error/render path.
- Exercise package install/enable/open with Hub producer commit `30a4233` when
  its explicit binaries are available. After this TUI change merges, the Hub
  ticket owns the authoritative rerun through `botster-hub up` and
  `botster-hub open tui`; the cross-repository integration ticket owns the
  final merged-main proof.

### 4. Documentation and full tracked-file audit

- Update README launch examples to use the structured descriptor and the
  package-open production path.
- Document live harness inputs, PATH/default behavior, isolation/cleanup
  ownership, and how to target an explicitly built Hub revision.
- Mechanically update every current plan document that contains the retired
  vocabulary while preserving its technical decisions and historical ticket
  references.
- Audit all tracked source, tests, executable scripts, README, manifest, ADRs,
  and current plans. The audit itself must not require exclusions for current
  tracked documentation.

## Non-scope

- Hub `up`, package refresh, entrypoint serialization, foreground supervision,
  CLI selector policy, or readiness policy.
- Core DTO/schema changes or compatibility aliases.
- `botster-hub-client` protocol changes, private socket framing, direct
  session-worker frames, or raw core routing.
- Reusable renderer/input changes in `botster-tui-kit`.
- Web client work, plugin workflow policy, marketplace behavior, broad TUI
  redesign, or unrelated cleanup.
- Rewriting Git history.

## Repository ownership and cross-repository dependencies

- `botster-tui` owns descriptor consumption, app-level naming, local
  diagnostics, package declaration, production client behavior, and its live
  acceptance wrapper.
- `botster-core` owns the descriptor JSON/validation contract and canonical
  fixtures. This run consumes the merged contract and must not redefine it.
- `botster-hub-client` owns public daemon DTOs and attach/request helpers. The
  TUI continues to use that boundary only.
- `botster-hub` owns descriptor production, installed-package launch, `up`,
  `open`, and foreground supervision. Its ticket is downstream of this run.
- `botster-hub-test-support` owns shared foreground/package conformance. Its
  stale raw-socket expectation is being migrated by the routed Core
  test-support ticket, not worked around here.
- `botster-tui-kit` continues to own generic rendering, hit maps, and input
  mechanics. Only app-owned IDs and tests change here.

No new open prerequisite is needed for implementation: the Core contract is
merged. Full package-launch acceptance remains a downstream proof because the
Hub ticket intentionally depends on this TUI ticket.

## Assumptions and unknowns

- Assumption: `BOTSTER_HUB_CONNECTION` is the manifest-owned environment target
  for this package; target names remain package-specific while the value shape
  remains Core-owned.
- Assumption: a direct live-runtime run and a Hub-launched package run execute
  the same binary/parser and differ only in who supplies the canonical JSON.
- Assumption: package storage context is Hub-owned and not a TUI identity input;
  it is exposed as runtime context while only the typed descriptor selects Hub.
- Assumption: existing session lifecycle, attach, readback, reconnect, mouse,
  plugin-surface, and diagnostic behavior is correct and should be preserved
  while the harness is renamed and made timing-safe.
- Unknown: the final merged Hub revision after its productionization ticket.
  Use explicit branch binaries for provisional proof and record the exact
  revision; do not bake that revision into TUI runtime policy.
- Unknown: whether PATH lookup is available in every developer environment.
  Explicit binary variables are authoritative and the script must fail with a
  concise setup message when neither explicit paths nor PATH commands resolve.
- Unknown: whether removing the stale shared foreground conformance call
  temporarily reduces a report field. Preserve equivalent TUI-owned
  package-open proof against the new Hub branch, and let the routed test-support
  ticket restore shared conformance without a local compatibility shim.
- Convention conflict: none. The repository-wide rename is required by the
  explicit cold-turkey migration rule, so the normal port-on-touch preference
  does not apply.

## Affected surfaces and files

- `Cargo.toml`, `Cargo.lock` — merged Core contract/test fixtures.
- `botster-package.json` — required structured connection and Core-required
  storage-context injections.
- `crates/botster-tui/src/app.rs` — configuration parser, root app/name/IDs,
  diagnostics, headless live-runtime flow, tests, wait behavior, and production
  runtime proof.
- `crates/botster-tui/tests/package_manifest_test.rs` — exact manifest contract.
- `script/test-live-hub` — explicit runtime selection and renamed focused test.
- `README.md` — production launch, diagnostics, package-open, and harness docs.
- `docs/plans/*.md` — current-document vocabulary rewrite, including one file
  rename.

No TUI-kit, Hub, Core, Web, or Project Pipelines source file belongs in this
run's diff.

## Implementation sequence

1. Update Core pins and lockfile; import the canonical descriptor/fixtures.
2. Cold-cut `AppArgs`, endpoint construction, manifest, and manifest/parser
   tests to structured JSON.
3. Rename the root app, live-runtime mode, fixtures, UiNode IDs, test names, and
   all consumers in one compile-fixing pass.
4. Refactor the live acceptance wrapper/test to explicit binaries, structured
   endpoint configuration, owned temporary runtime state, and event-driven
   waits.
5. Preserve and extend real-path lifecycle, terminal, reconnect, package-open,
   and unavailable-Hub assertions.
6. Rewrite README and current plans; rename the legacy-named plan.
7. Run focused audits, repository gates, live proof against Hub `30a4233`, and
   record exact revisions/commands.

## Risks and mitigations

- Bulk ID rename can silently break focus, scrolling, mouse routing, or terminal
  forwarding. Update each producer/consumer pair together and retain real
  handler/hit-map tests.
- A permissive parser could accidentally preserve the old raw path. Deserialize
  the Core type directly, call its validator, and run canonical negative
  fixtures plus an explicit no-fallback test.
- Missing configuration could collapse into a generic transport failure.
  Separate configuration diagnostics from unavailable-endpoint diagnostics and
  render both through the production surface.
- Current Hub main does not yet produce the new value. Use explicit binaries
  from producer commit `30a4233` for provisional proof; do not weaken the TUI
  contract or pin unmerged Hub policy to make old main pass.
- Shared foreground conformance is temporarily stale. Replace no contract with
  a local fake; use canonical Core fixtures and the real Hub producer branch,
  then require downstream shared-conformance proof.
- Fixed delays can make live proof flaky or conceal readiness bugs. Replace each
  delay with an observed Hub/entity/terminal condition and a bounded timeout.
- Reused Cargo targets can hide same-version protocol drift. Use fresh,
  caller-overridable TUI and Hub target directories for cross-repository proof.
- Mechanical documentation rewriting can alter historical meaning. Change
  identity vocabulary and identifiers while preserving decisions, ticket IDs,
  and technical chronology; review the docs-only diff separately.
- A broad text replacement could touch generated or unrelated files. Restrict
  edits to tracked current repository files and inspect `git diff --stat` plus
  the complete diff before commit.

## Acceptance checks and evidence

Focused contract checks:

- Valid canonical descriptor JSON parses, validates, and produces the expected
  Unix-socket `DaemonEndpoint`.
- Core canonical invalid fixtures are rejected, including missing/unknown
  fields, malformed/unknown transport, blank/whitespace path, and relative
  path.
- Supplying only the retired raw-socket input does not connect or trigger a
  fallback.
- The package manifest validates with required `hub_connection` and `data_dir`
  injections and their exact declared environment targets.
- Root app rendering/input tests pass with the new `tui-` IDs; terminal mouse
  forwarding still reaches `DaemonRequest::SendInput` only for the active
  attachment.

Repository audit:

```sh
legacy_term='dog''food'
if git grep -ni "$legacy_term" -- \
  README.md botster-package.json crates script docs; then
  exit 1
fi
rg -n 'thread::sleep|(^|[;&[:space:]])sleep[[:space:]]' \
  crates/botster-tui/src/app.rs script/test-live-hub
```

The first command must produce no matches. The second must produce no
live-acceptance timing dependency; any unrelated production timing use requires
an explicit disposition.

Repository gates:

```sh
script/fmt
script/test
script/clippy
CARGO_TARGET_DIR=<fresh-tui-target> \
BOTSTER_HUB_BIN=<hub-30a4233-binary> \
BOTSTER_SESSION_WORKER_BIN=<matching-worker-binary> \
script/test-live-hub
```

Live-runtime evidence must prove:

- isolated Hub readiness without a fixed delay;
- authoritative session snapshot, spawn/lifecycle deltas, natural exit, and
  removal;
- explicit selection and attach;
- prior-output restoration through attach plus `ReadScreen`, followed by live
  input/readback in order;
- a fresh reconnect generation, no automatic attach, explicit reattach, and no
  duplicated terminal history;
- production terminal input and mouse-mode forwarding remain attached-session
  scoped;
- compatibility mismatch and unavailable-Hub diagnostics render through the
  production app;
- package install/enable/open launches this binary with the canonical
  descriptor; storage context is visible but not treated as client identity;
- all owned sessions and the isolated runtime are cleaned up.

Downstream proof required after merge:

1. Hub ticket `ticket_1784854076_565213` pins/installs the merged TUI revision.
2. From the Hub-supported runtime, run `botster-hub up` and then
   `botster-hub open tui`.
3. Reprove package launch, session lifecycle, terminal
   attach/input/readback, reconnect, and unavailable-Hub diagnostics through
   that production entrypoint.
4. The final integration ticket `ticket_1784854143_789468` repeats the workflow
   against exact merged-main revisions and records those revisions.

Plan/gate artifacts must record the exact dependency revisions, command exits,
any environment-specific skips, and the downstream proof owner. Review must not
accept code existence or manifest validation alone as production wiring proof.

## Vault gaps worth capturing

- After merge, update [[botster tui consumes tui kit through a thin app policy adapter]]
  so its examples use the production root app and stable TUI IDs.
- Capture a TUI-specific note that runnable package clients decode and validate
  Core's typed connection descriptor at their manifest-declared target, while
  Hub remains the producer and `botster-hub-client` remains the daemon protocol
  boundary.
- Capture the reusable harness rule: first-party client live tests accept
  explicit runtime binaries/endpoints, own isolated temporary state, and wait on
  protocol evidence rather than dependency checkout layout or fixed delays.
- No vault write belongs in the implementation diff. The Verify step should
  record whether these gaps were captured after the behavior and downstream
  proof are stable.
