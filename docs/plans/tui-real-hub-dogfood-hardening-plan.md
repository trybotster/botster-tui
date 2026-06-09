# TUI real hub dogfood hardening plan

## Context loaded

- Pipeline context: `ticket_1781026822_594812`, run `run_1781026832_661142`, current step `botster_plan`, gate `botster_plan_gate`; no prior artifacts, findings, reviews, questions, or answers were present.
- Vault/playbook context: [[identity]], [[goals]], [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], [[plan agents must author vault context as wikilinks not home paths]].
- Repo context inspected: `README.md`, `Cargo.toml`, `crates/botster-tui/Cargo.toml`, `crates/botster-tui/src/app.rs`, `crates/botster-tui/src/renderer.rs`, `script/fmt`, `script/test`, `script/clippy`, and `script/test-live-hub`.
- Checklist evidence: run checklist `Plan vault discipline` records loaded notes and no convention conflict. Checklist creation initially timed out in the plugin worker, but the created checklist was later visible and updated.
- Plan Review context loaded: review `review_1781027400_807834` returned changes required with open findings `finding_1781027400_888910`, `finding_1781027400_517702`, `finding_1781027400_600477`, and `finding_1781027400_685423`. This revision resolves those findings by constraining terminal rendering to the existing core `terminal_view` contract, strengthening primitive-rendering acceptance checks, committing to focus-to-selection sync, and adding identity/data-dir isolation as explicit verification.

## Scope

- Keep the work inside `botster-tui`: the Rust TUI app, renderer adapter, crate manifest dependency pins only if needed, README/manual commands, and test scripts.
- Harden the real hub dogfood user path already present in `DogfoodApp`: connect, list or spawn sessions, attach the selected session, drain terminal output, send terminal input, resize, reconnect or explicitly document a narrower supported reconnect behavior, surface success/error feedback, and teardown in the headless path.
- Preserve `botster-hub-client` as the only daemon protocol boundary. Any needed protocol behavior must go through its public request/response/event types and helpers.
- Ensure terminal bytes render through the `terminal_view` primitive path rather than a sibling debug-only presentation path. `botster-core` validates `terminal_view` with only `session_id` and `title` props, so this must be a `botster-tui` renderer-only change in `render_terminal_view`: source terminal bytes from the existing app state or existing child text as needed, but paint them as the terminal primitive's own content inside its block without adding new core props, slots, or core edits.
- Ensure form draft state remains visible while editing, before submit/validate applies it to the app command.
- Ensure attach targets the selected/focused session, not an implicit first-session fallback except when no selection exists. The plan commits to bridging keyboard/list focus from `InputRouter` or the list hit-map into `DogfoodApp.selected_session` before attach.
- Keep or improve automated isolated hub coverage through `botster-hub-test-support` and the `script/test-live-hub` wrapper.
- Ensure live hub tests use an isolated, unique temporary root/data dir and never mutate the user's real Botster identity or durable home state.
- Update README/manual commands only to match the final supported path.

## Non-scope

- Do not edit `botster-hub`, `botster-core`, `botster-hub-client`, or `botster-hub-test-support` from this branch.
- Do not introduce a private socket protocol, duplicated frame constants, or local request/response enums that mirror hub-client protocol.
- Do not add pairing, remote auth, hub provisioning, plugin policy, Project Pipelines policy, browser SPA behavior, or entity-store hydration beyond what this ticket needs for the first-party TUI dogfood path.
- Do not add broad renderer rewrites, optional configurability, or speculative abstractions around the app loop.

## Assumptions and unknowns

- Assumption: the pinned `botster-hub-client` and `botster-hub-test-support` revisions are the merged dependency surface referenced by the ticket unless implementation discovers a missing API.
- Assumption: reconnect acceptance can be satisfied by deterministic disconnect/reconnect behavior plus re-pull/list/reattach evidence, without requiring transparent preservation across every daemon crash mode.
- Assumption: the headless dogfood test may remain skipped in ordinary `script/test` when explicit hub binaries are absent, but `script/test-live-hub` must force the live test with matching built binaries.
- Unknown: whether current `DaemonConnection` exposes a clean way to simulate transport loss in a unit test. If not, test reconnect by dropping the client/app connection state or by using the live hub harness path.
- Resolved constraint from Plan Review: `terminal_view` cannot accept a direct `output` prop because the core schema only allows `session_id` and `title`, and core is out of scope. Implementation must not add an output prop or rely on a core schema change.
- Resolved selection direction from Plan Review: keyboard/list focus must sync to `DogfoodApp.selected_session`; click activation alone is not enough for this ticket.
- Unknown: the smallest exact bridge for focused list state may be `InputRouter.selected_row("dogfood-session-list")`, an explicit dispatch payload, or a small app-loop handoff. Implement must choose the smallest option already supported by the renderer/router shape.

## Affected surfaces/files

- `crates/botster-tui/src/app.rs`: `DogfoodApp` connection lifecycle, session selection/attach, form draft application, terminal output state passed to the surface, headless dogfood flow, live hub test, and unique isolated test root handling.
- `crates/botster-tui/src/renderer.rs`: `TerminalView` rendering as primitive-owned content, hit-map terminal input/resize forwarding, list/form selection and draft behavior, and any focused-row exposure needed by the app.
- `crates/botster-tui/Cargo.toml` and `Cargo.lock`: only if the pinned hub-client/test-support dependency needs to move to the merged revision.
- `README.md`: manual dogfood commands and supported reconnect/selection behavior.
- `script/test-live-hub`: only if the live harness command needs adjustment to build or pass matching hub/session-worker binaries.
- `docs/plans/tui-real-hub-dogfood-hardening-plan.md`: this reviewable Plan artifact.

## Risks

- Protocol drift risk: using local socket assumptions would regress the external-client boundary. Mitigation: all hub calls stay typed through `botster-hub-client`; reviewers should search for hand-rolled frame constants.
- Core contract risk: adding terminal output as a new `terminal_view` prop or slot would violate `botster-core` validation and panic at runtime. Mitigation: keep the fix renderer-only inside `botster-tui` and preserve the existing core `terminal_view` prop contract: `session_id` and `title` only.
- Runtime proof risk: unit tests can prove code shape without proving a real user path. Mitigation: require `script/test-live-hub` or equivalent live isolated hub evidence.
- Reconnect risk: reconnect may appear connected but leave sessions/entity state stale. Mitigation: re-list/re-pull sessions after connect and reattach selected session when supported, or document the narrower tested behavior.
- Terminal fidelity risk: ordinary text input may pass while nested TUI mouse/control bytes fail. Mitigation: keep terminal input forwarding byte-oriented and avoid treating terminal_view input as ordinary text-field edits.
- Selection risk: keyboard/list focus can diverge from app-level selected session. Mitigation: add focused-row to selected-session mapping or explicit activation tests.
- Identity/data-dir risk: a real hub dogfood test could accidentally touch the user's durable Botster identity or share state across runs. Mitigation: require an isolated unique temporary hub root/data dir, explicit socket path, and non-use of the user's real `~/.botster` state.
- Test environment risk: live hub binaries are external to this crate. Mitigation: keep `script/test-live-hub` building matching pinned binaries and use `BOTSTER_TUI_REQUIRE_HUB_TEST=1` for non-skippable evidence.

## Acceptance checks/tests

- `script/fmt`
- `script/test`
- `script/clippy`
- `CARGO_TARGET_DIR=/tmp/botster-tui-impl-target script/test-live-hub`
- Add or retain focused tests proving:
  - no private protocol path exists in `botster-tui`;
  - terminal output is primitive-rendered through `terminal_view`, with a test that fails against today's sibling debug `Text` node path, such as asserting `dogfood-terminal-output` no longer exists as a sibling child or proving output renders from a `TerminalView` without delegating to generic child rendering;
  - command draft edits are visible before submit;
  - keyboard/list focused session syncs to `DogfoodApp.selected_session` and is the attach target;
  - reconnect runs connect plus session re-pull/reattach behavior, or the documented narrower behavior is asserted;
  - headless dogfood against an isolated local hub connects, renders/builds the surface, spawns or selects a session, attaches/drains output, sends input where supported, observes success/error state, and tears down without touching real user identity or a shared durable data dir.
- Manual README commands let a reviewer run the same isolated dogfood path locally with a temporary hub data dir and explicit socket path.

## Vault gaps worth capturing

- Capture the proven `botster-tui` real-hub dogfood pattern after implementation if it establishes a reusable external-client test shape for connect/list/spawn/attach/drain/input/reconnect.
- Capture the final reconnect contract if implementation intentionally narrows behavior to re-pull plus reattach rather than full transparent session continuity.
- Capture the renderer-only `terminal_view` content pattern if it becomes the reusable way to satisfy the core schema while avoiding sibling debug text nodes.
- Capture any checklist worker timeout recurrence only if it affects gate/review persistence again; current fallback was to preserve evidence in checklist updates and this plan artifact.
