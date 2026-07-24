# TUI Hub Compatibility Diagnostics Plan

## Context Loaded

- Pipeline context: `ticket_1781046658_155987`, run `run_1781049154_661813`, step `botster_plan`, gate `botster_plan_gate`.
- Dependency context: dependency ticket "Add hub client connection event diagnostics" is closed.
- Repo context: current branch is `project-pipelines/ticket_1781046658_155987`; worktree is clean before planning; current `botster-hub-client` pin is `e97fc3779488ab8adedb98708898e7106254331e`.
- Current upstream context: `botster-hub` main resolves to `24453ef448fb4c89ed63e784ed518de7ca301cd7`; the current public client crate exposes `DaemonCompatibilityRequirement`, `DaemonCompatibility`, `DaemonDiagnostic`, `DaemonDiagnosticKind`, `DaemonTransportError::Compatibility`, feature constants, and diagnostics on hello/status/response/operator errors.
- Required vault/playbook context: [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], [[plan steps need reviewable plan artifacts]], [[project pipelines checklist worker timeouts require artifact evidence fallback]].
- Current TUI code context: `crates/botster-tui/src/app.rs` already owns the live-runtime app state, connection retry, status panel rendering, terminal attach state, and focused tests for missing socket, protocol branch, status schema, terminal stream availability, and live isolated hub live-runtime.

## Scope

- Update `botster-tui`'s `botster-hub-client` and `botster-hub-test-support` git revisions to a main revision that includes the compatibility/diagnostic client API, expected to be `24453ef448fb4c89ed63e784ed518de7ca301cd7` unless implementation finds a newer compatible main revision at start.
- Adjust `TuiApp` connection setup to use the public compatibility requirement path with `client_name = "botster-tui"` and a deliberately narrowed TUI requirement: sessions, terminal streaming, and resize. Do not require plugin surface render/action features for this live-runtime terminal surface, and do not add private protocol parsing.
- Store and render authoritative compatibility/diagnostic state from hub-client structures:
  - `DaemonStatus.compatibility`
  - top-level `DaemonResponse.diagnostics`
  - `DaemonOperatorError.diagnostics`
  - `DaemonTransportError::Compatibility(error).diagnostics`
  - client-side `DaemonDiagnostic::disconnected` for post-connect disconnect classification where applicable.
- Update the existing status/diagnostics surface so hub unavailable, compatibility mismatch, unsupported feature, terminal stream unavailable, action failure, reconnecting/disconnected, and connected are distinguishable in rendered output.
- Refresh `README.md` Local Hub Production documentation to describe the real compatibility handshake and diagnostics path instead of the old "descriptor pending" limitation.
- Add/adjust tests in `crates/botster-tui/src/app.rs` that prove rendering is driven by structured hub-client diagnostics and compatibility data, not hardcoded demo-only text.

## Non-Scope

- No edits to `botster-hub`, `botster-core`, web, Rails, Project Pipelines plugin, or hub protocol internals from this branch.
- No new local protocol DTOs, socket frame readers, or private daemon/session-worker frame handling in `botster-tui`.
- No terminal data-plane refactor, terminal byte ownership change, PTY input transformation, or scrollback ownership change.
- No broad renderer redesign or generic diagnostics framework beyond what the existing live-runtime status surface needs.
- No mutation of real Botster identity, device state, or private protocol details in tests/docs.

## Assumptions And Unknowns

- Assumption: the implementer should verify the latest acceptable `botster-hub` main revision before editing; planning observed `24453ef448fb4c89ed63e784ed518de7ca301cd7` as current main.
- Decision: `botster-tui` should use a narrowed compatibility requirement containing only the features this surface actually needs: sessions, terminal streaming, and resize. Requiring plugin surface render/action features would overconstrain an otherwise-compatible local daemon for this TUI path.
- Assumption: status rendering may add client-side labels/severity, but message parsing should not become a contract; branch on `DaemonDiagnosticKind`, `operation`, and `feature`.
- Unknown: after updating the pin, `DaemonStatus` construction in existing tests will need new required fields (`compatibility`, `diagnostics`) and may reveal additional compile changes.
- Unknown: live isolated hub test duration may increase because `script/test-live-hub` builds hub binaries from the updated git dependency.

## Affected Surfaces/Files

- `crates/botster-tui/Cargo.toml`: update hub-client and hub-test-support git revision.
- `Cargo.lock`: update locked hub dependency source revisions and any transitive changes from that exact hub revision.
- `crates/botster-tui/src/app.rs`: consume compatibility requirement, compatibility descriptors, diagnostics, transport classifications, status panel text, and app tests.
- `crates/botster-tui/src/app.rs`: update the stale protocol-branch tests and comments for the new reachable `DaemonTransportError::Compatibility` path, while preserving the negative assertion that `NotRunning` does not render as a compatibility mismatch.
- `README.md`: update Local Hub Production diagnostics docs and remove stale pending-descriptor language.
- Possibly `script/test-live-hub` only if the updated hub revision changes binary package names or invocation requirements; otherwise leave it untouched.

## Risks

- Protocol drift risk: updating the git pin may bring unrelated hub-client API changes. Keep compile fixes limited to the public client crate boundary and do not port hub internals.
- Dependency skew risk: updating `botster-hub-client` and `botster-hub-test-support` while `botster-core` remains pinned can produce two `botster-core` revisions or fail to resolve cleanly. Implementation must confirm the lock resolves to one intended `botster-core` revision, or explicitly reconcile the core/test-support pins as part of this ticket.
- Overclassification risk: collapsing all transport errors into compatibility/unavailable would violate acceptance. Preserve distinct branches for not running, protocol/compatibility mismatch, client disconnect, and action/operator failures.
- Terminal fidelity risk: changes in `handle_dispatch`, terminal output, attach state, or UTF-8/input handling could regress PTY behavior. Avoid changing those paths except where diagnostics are already surfaced.
- Docs risk: README could leak local paths or imply private protocol behavior. Keep docs path-neutral and client-API oriented.
- Pipeline tooling risk: `project_pipelines_create_vault_checklist` timed out during planning. Per [[project pipelines checklist worker timeouts require artifact evidence fallback]], this plan and gate evidence preserve the checklist evidence instead.

## Acceptance Checks/Tests

- `script/fmt`
- `script/test`
- `script/clippy`
- Targeted tests in `crates/botster-tui/src/app.rs` should cover:
  - compatibility success renders protocol/version/features from `DaemonStatus.compatibility`;
  - `DaemonTransportError::Compatibility` renders `compatibility_mismatch` distinctly from hub unavailable;
  - missing required feature renders as unsupported capability/feature, not generic unavailable;
  - connected diagnostics from status/response render as connected state;
  - terminal unavailable diagnostics from operator errors render distinctly from selection-only "not attached" state;
  - action failure diagnostics remain visible after unrelated successful refreshes;
  - client disconnect/reconnect remains distinct from not-running/unavailable.
- Existing stale tests must be updated, not deleted:
  - rename/update `defensive_protocol_error_branch_renders_distinct_compatibility_diagnostic` to cover the now-reachable `DaemonTransportError::Compatibility` path;
  - preserve/update `current_pinned_client_not_running_path_is_not_reported_as_compatibility_mismatch` so `NotRunning` remains a distinct unavailable/reconnecting state.
- Existing boundary test must continue proving `botster-tui` uses `botster_hub_client` and does not reintroduce private protocol plumbing.
- `script/test-live-hub` is required for this ticket because the headline acceptance criterion is reachability through the real hub-client handshake, not synthetic branch coverage. It must prove:
  - compatibility success reads non-default descriptor values from a real isolated hub, including protocol, protocol version, and feature list;
  - compatibility mismatch is exercised through `connect_and_hello_with_requirement` against the live daemon using an unsatisfiable requirement, such as a bogus required feature, so `botster-hub-client` constructs `DaemonTransportError::Compatibility`;
  - the rendered TUI diagnostics use those live hub-client results rather than injected errors or hardcoded demo strings.
- If `script/test-live-hub` genuinely cannot run, the implementer must stop and declare the ticket scaffold-only in the implementation report and gate evidence; synthetic tests alone are not sufficient to satisfy this ticket.

## Runtime Path Proof

The production path to prove is `run_loop` -> `TuiApp::new` -> `try_connect` / `force_reconnect` -> `connect_and_hello_with_requirement` or `DaemonConnection::connect` from `botster_hub_client`, then `refresh_status` / `request_and_apply` -> `apply_response` -> `status_panel`. Tests that construct only static strings are insufficient unless they exercise the same `record_transport_error` / `apply_response` paths used by `poll_hub`, reconnect, and request handling.

For compatibility mismatch specifically, proof must go through a live isolated daemon with an unsatisfiable `DaemonCompatibilityRequirement`; direct injection of `DaemonTransportError::Compatibility` may remain as focused unit coverage, but it cannot be the only evidence. For compatibility success, proof must assert descriptor values returned by the real hub are non-default and rendered.

## Pipeline Gates And Artifacts

- Plan artifact: this file.
- Plan gate evidence should attach this plan plus the context, assumptions, scope, affected files, risks, tests, and vault gaps.
- Checklist evidence fallback: checklist creation timed out with `plugin worker invoke timeout`; vault notes read, convention conflicts, verification commands, and capture decision are recorded here and in gate evidence.
- Convention conflicts: none. The plan follows TUI-only scope, public hub-client boundary, path-neutral artifact wording, explicit worktree/target assumptions, and existing repo test scripts.

## Vault Gaps Worth Capturing

- No new durable architecture rule is needed from this planning pass. Existing notes already cover the critical constraints: hub-client as external boundary, diagnostics from distinguishable runtime signals, terminal data-plane ownership, plan artifact durability, and checklist timeout fallback.
- Possible future capture only if implementation finds drift: "TUI compatibility diagnostics require hub-client revision freshness after closed dependency tickets" could be worth recording if stale worktree/API skew recurs.
