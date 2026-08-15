# Plan: TUI require terminal mechanism tokens only on terminal_compatibility

Ticket: `ticket_1786756492_156718`
Run: `run_1786756665_970591`
Step: `botster_stack_plan`

Required by Hub cold-cut `ticket_1786661010_198387` (question `question_1786756438_318832` option 1).

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`trybotster/botster-tui`) |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Resolved from | `list_spawn_targets` (`name=botster-tui`, `repo_name=trybotster/botster-tui`, path is the botster-tui checkout) |
| Ambient worktree | pipeline worktree for this run; routing is the ticket target, not the process cwd |
| Base | `origin/main` `96d7c42b4e0c0359a2ba601e1bc95515ffaca323` |
| Branch | `project-pipelines/ticket_1786756492_156718` |
| `teardown_class_applies` | **false** — Hello required-feature placement only. No WebRTC/peer lifecycle, SessionIo/ClientWorker teardown, multi-peer ownership, CPU/battery/FD spin, or terminal-state vs live-runtime divergence. Do not load [[botster runtime teardown lenses]]. |
| Session-type eligibility consumer | **false** |

## Repository playbook loaded

- [[botster-tui-playbook]]

## Other playbooks and notes loaded

Role / stack: [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[plan agents must author vault context as wikilinks not home paths]], [[pipeline vault checklists must cite exact resolvable note titles]], [[vault example paths are not repository placement conventions]], [[plan steps need reviewable plan artifacts]], [[plan review must verify a plan artifact exists before trusting gate summaries]], [[cross repo dependency registration must use dependency repo target]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]].

Not loaded: [[project-pipelines-playbook]] — this run does not touch Project Pipelines package or plugin paths. Not loaded: [[botster runtime teardown lenses]] — teardown class does not apply.

Targeted atomic notes:

- [[first-party Unix attach clients use split Hello and subscription close events]]
- [[Unix Hello can reject terminal admission while host operations remain available]]
- [[public protocol versions host control and Core terminal planes independently]]
- [[Core reports terminal mechanism capabilities and Hub admits their use]]
- [[proposed each protocol plane owns its compatibility descriptors]]
- [[compatibility fixtures advertise every required optional feature]]
- [[ready then history is advertised as optional daemon support]]
- [[ready then history is a compatibility feature not an Attach field]]
- [[additive daemon capabilities do not raise the default client requirement]]
- [[tui client attach uses hub protocol not session protocol]]
- [[tui and browser are equal clients]]
- [[Core terminal protocol separates Hub-safe envelopes from client semantic bodies]]
- [[test script required for rust tests not cargo test]]
- [[external client hub tests use subprocess spawned hub test support]]

Reference, not change surface: first-party Web already splits these tokens in `src/botster/protocolPlanes.ts`. Do not edit `botster-web` or `botster-hub`.

## Context loaded

Parent TUI ticket `ticket_1786661009_551067` is closed. It already consumes independent planes:

- Production connect: `HubConnection::connect` → `connect_and_hello_with_terminal_requirement`.
- Host Hello: `tui_compatibility_requirement()` as `DaemonCompatibilityRequirement`.
- Terminal Hello: `tui_terminal_compatibility_requirement()` = `TerminalCompatibilityRequirement::for_ready_then_history_attach()` with `client_name = "botster-tui"`.
- `admit_terminal_hello` requires `ack.terminal_compatibility` and `ensure_terminal_compatible`.
- Parent plan said: do not use host `FEATURE_TERMINAL_STREAMING` / `FEATURE_RESIZE` / `FEATURE_SNAPSHOT_DELIVERY_READY_THEN_HISTORY` as terminal-plane authority.

Residual defect this ticket fixes: host `tui_compatibility_requirement().required_features` still lists those three Core terminal-plane tokens. Current host list:

```
sessions
terminal_streaming
resize
package_navigation
plugin_surface_render
plugin_surface_action
terminal_readback
session_entity_subscriptions
mode_gated_input
snapshot_delivery=ready_then_history
unix_terminal_adapter
terminal_subscription_closed
```

Core `TerminalCompatibilityRequirement::for_ready_then_history_attach()` already requires `terminal_streaming`, `resize`, and `snapshot_delivery=ready_then_history`. TUI already sends that on `Hello.terminal_compatibility`.

Web already requires the three tokens only on `terminalCompatibilityRequirement`. Web host required features are host-plane only. Current Hub main already accepts that Web Hello. This TUI change is the Unix-client match.

Hub `ticket_1786661010_198387` already depends on this ticket. Hub must not remove those host descriptor tokens until this ticket merges and verifies.

Current pins stay:

| Crate | Pin |
| --- | --- |
| `botster-hub-client` / `botster-hub-test-support` | Hub `4f30d6952f9a29541ab3a670a54bf5e136b8eb8e` |
| Core / Ghostty / terminal-protocol-client | `f4f6bf5babe92dfb9241a760c414187f711c2c42` |
| Host protocol | 7 |
| Host conformance floor | 40 |

Do not bump host `PROTOCOL_VERSION`. Do not bump the host conformance floor. Do not change Cargo pins.

## Scope

Surgical TUI host Hello cleanup. Keep one production connect path.

1. Remove from `tui_compatibility_requirement().required_features`:
   - `FEATURE_TERMINAL_STREAMING`
   - `FEATURE_RESIZE`
   - `FEATURE_SNAPSHOT_DELIVERY_READY_THEN_HISTORY`
2. Keep host Hello requirements that are host-plane:
   - `FEATURE_SESSIONS`
   - `FEATURE_PACKAGE_NAVIGATION`
   - `FEATURE_PLUGIN_SURFACE_RENDER`
   - `FEATURE_PLUGIN_SURFACE_ACTION`
   - `FEATURE_TERMINAL_READBACK`
   - `FEATURE_SESSION_ENTITY_SUBSCRIPTIONS`
   - `FEATURE_MODE_GATED_INPUT`
   - `FEATURE_UNIX_TERMINAL_ADAPTER`
   - `FEATURE_TERMINAL_SUBSCRIPTION_CLOSED`
3. Keep `tui_terminal_compatibility_requirement()` as `for_ready_then_history_attach()` with `client_name = "botster-tui"`. Do not re-list the three tokens on the host plane.
4. Update hermetic and live assertions that treat those three tokens as host `compatibility.features` or host `required_features`.
5. Add hermetic proof that host `ensure_compatible` succeeds when a host fixture omits the three tokens.
6. Add hermetic proof that terminal `required_features` still contains the three tokens, using `botster_terminal_protocol_client` constants as the terminal-plane oracle.
7. Optional README sentence: host Hello lists host-plane features only; mechanism tokens live on `terminal_compatibility`.

Production entry point that must use the new host list: `HubConnection::connect` → `tui_compatibility_requirement()` + `tui_terminal_compatibility_requirement()` → `connect_and_hello_with_terminal_requirement` → `admit_terminal_hello`.

## Non-scope

- Do not edit `botster-hub`, `botster-web`, `botster-hub-client` source, or Core protocol crates.
- Do not bump host `PROTOCOL_VERSION` or host conformance floor 40.
- Do not change Cargo Git pins.
- Do not change mux decode, attach hydration, Ghostty apply, close-event handling, or entity pumps.
- Do not add a second connect helper or dual Hello path.
- Do not create a pull request. Merge directly into `main`.
- Do not register a new Hub ticket. Downstream Hub cold-cut already depends on this ticket.
- Do not treat advertised host extras on current Hub main as a requirement.

## Repository ownership boundaries and cross-repo dependencies

| Layer | Owner | This ticket |
| --- | --- | --- |
| TUI host Hello composition | `botster-tui` | Change `tui_compatibility_requirement` and assertions |
| Terminal Hello composition | `botster-tui` consuming Core | Keep current `for_ready_then_history_attach()` |
| Host control descriptors | Hub / hub-client | Consume only. Do not edit. |
| Terminal mechanism tokens | Core terminal protocol | Consume only. Do not edit. |
| Hub host advertisement drain | Hub cold-cut `ticket_1786661010_198387` | Downstream. Blocked on this merge. |
| Web host/terminal split | `botster-web` | Already shipped. Reference only. |

No new Project Pipelines dependency. Hub `ticket_1786661010_198387` already registers `dependency_1786756497_701609` on this ticket with Hub target `tgt_7e208a0c76a44980a83b63af976b1f22`. Do not register that reverse edge against the TUI target.

## Assumptions and unknowns

Assumptions:

- Current Hub main accepts a host Hello that omits the three tokens when `terminal_compatibility` still requires them. First-party Web already proves this against current Hub main.
- Core `TerminalCompatibilityRequirement::for_ready_then_history_attach()` remains the complete terminal-plane requirement for those three tokens.
- Host `ensure_compatible` checks that required features are advertised. Extra advertised host features do not fail Hello. Current Hub may still advertise the three tokens on host Status.
- After this merge, Hub may remove those tokens from the host descriptor. TUI tests must remain valid in both states.
- Live proof uses the operator `BOTSTER_HUB_BIN` / `script/test-live-hub` current Hub main binary. It does not change the Cargo hub-client pin.
- `status_response` fixture features that list `terminal_streaming` / `resize` are advertised-support decoration for render tests, not host Hello requirements. Leave them unless an import becomes unused.

Unknowns:

- Exact live Hub SHA in the Implement environment. Require recorded live binary provenance. Do not accept a soft residual such as "current Hub still advertises these tokens, so host required_features can stay."
- Whether clippy will treat unused `FEATURE_TERMINAL_STREAMING` / `FEATURE_RESIZE` / `FEATURE_SNAPSHOT_DELIVERY_READY_THEN_HISTORY` hub-client imports as errors after production removal. Keep imports only where tests still need them. Prefer Core client constants for terminal-plane assertions.

## Affected surfaces / files

| File | Change |
| --- | --- |
| `crates/botster-tui/src/app.rs` | Remove the three tokens from `tui_compatibility_requirement()`. Update `tui_requires_protocol_7_revision_40_and_split_terminal_hello`. Update `run_headless_live_runtime` host feature scan. Add host-omission and terminal-requirement assertions. Clean unused hub-client imports if needed. |
| `README.md` | Optional one-sentence host vs terminal Hello clarification. |
| `docs/plans/tui-require-terminal-mechanism-tokens-only-on-terminal-compatibility-plan.md` | This plan. |

Do not change `HubConnection::connect`, `admit_terminal_hello`, or `tui_terminal_compatibility_requirement()` unless a compile fix is required after import cleanup.

## Risks

- Live/headless tests that scan host `compatibility.features` for `terminal_streaming` or `resize` will fail after Hub cold-cut if they stay. Update them now.
- `tui_requires_protocol_7_revision_40_and_split_terminal_hello` currently asserts host `required_features` contains `snapshot_delivery=ready_then_history`. That assertion must invert.
- `compatible_hub()` in that test still pushes `FEATURE_SNAPSHOT_DELIVERY_READY_THEN_HISTORY` onto advertised host features. After this ticket, host `ensure_compatible` must succeed without that push and without the three tokens in the advertised host list.
- Fixture helpers that still advertise the three tokens on host Status are not wrong, but they cannot be the only host-compatibility proof.
- Do not weaken live attach proof. Hello success alone is not enough. Live attach must still work.

## Acceptance checks / tests

Hermetic, through repo wrappers (`script/fmt`, `script/clippy`, `script/test`; `test.sh` if used for `BOTSTER_ENV=test`):

1. `tui_compatibility_requirement().required_features` does not contain `terminal_streaming`, `resize`, or `snapshot_delivery=ready_then_history`.
2. `tui_compatibility_requirement().required_features` still contains the nine host-plane tokens listed in Scope.
3. Host protocol remains 7. Host conformance floor remains 40.
4. `tui_terminal_compatibility_requirement().required_features` contains the three terminal tokens. Assert with `botster_terminal_protocol_client::FEATURE_*`, not as host authority.
5. `botster_hub_client::ensure_compatible` accepts `tui_compatibility_requirement()` against a host fixture that omits the three tokens and still advertises the nine host-plane tokens.
6. Existing `missing_terminal_snapshot_delivery_on_hello_ack_fails_before_attach` still fails closed before Attach.
7. Existing `missing_terminal_compatibility_ack_field_fails_before_attach` still fails closed.
8. `script/fmt`, `script/clippy`, and `script/test` pass.

Live, against current Hub main (`script/test-live-hub`):

9. `script/test-live-hub` contract-matrix: Hello succeeds. Host Status `compatibility.features` scan no longer requires the three terminal tokens. Attach/input still prints `terminal-output: echo:botster-tui-headless`.
10. `script/test-live-hub ghostty`: live attach still works against current Hub main. Record Hub binary SHA and worker provenance. Do not accept residual host-token presence as proof.

Downstream:

11. After merge to `main`, Hub cold-cut `ticket_1786661010_198387` may remove the three tokens from the host descriptor. This ticket does not perform that Hub edit.

Merge policy: merge directly into `main`. Do not create a PR.

## Vault gaps worth capturing

Existing notes assign plane ownership but do not state the client Hello composition rule that Web already shipped:

- First-party clients must require `terminal_streaming`, `resize`, and `snapshot_delivery=ready_then_history` only on `Hello.terminal_compatibility`.
- Those tokens must not appear on host `DaemonCompatibilityRequirement.required_features`.

Capture that after Implement proves the TUI split. Do not capture a planned change as if it already shipped.

No convention conflict. This ticket completes the parent TUI residual and matches [[tui and browser are equal clients]] plus [[public protocol versions host control and Core terminal planes independently]].

## Worktree hygiene

- Tracked `.gitignore` has content (7 lines). Restored from HEAD. Do not truncate.
- Pipeline worktree path has no `:`. No `CARGO_TARGET_DIR` override is required for this Plan visit.

## Implement sequence

1. Change `tui_compatibility_requirement()` host list.
2. Invert host-token assertions. Add host-omission and terminal-requirement tests.
3. Update `run_headless_live_runtime` advertised-host feature scan.
4. Optional README sentence.
5. Run `script/fmt`, `script/clippy`, `script/test`.
6. Run live `script/test-live-hub` contract-matrix and `ghostty` against current Hub main.
7. Merge directly to `main`.
