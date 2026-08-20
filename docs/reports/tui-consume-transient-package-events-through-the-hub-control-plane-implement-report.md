# Implementation report: TUI consume transient package events through the Hub control plane

- **Ticket:** `ticket_1786663585_944018`
- **Run:** `run_1787197986_912715`
- **Step:** `botster_stack_implement`
- **PR:** none. Pipeline `merge_policy` is `direct`. Ticket delivery policy: merge into `main`, do not create a pull request.
- **Target repository:** `botster-tui` (`trybotster/botster-tui`)
- **Target id:** `tgt_c3d470bab78549df920a41e8fb0e58d8`
- **Base:** `origin/main` `dc7d6002c90dc6c565168df6328a032b640e9b48`
- **Plan revision:** 6 (`artifact_1787245720_425147`, commit `3db08b730d33bac9f7a7be646c5430297a6f13a0`)
- **Implement commit:** recorded after this commit
- **Review revisit:** sequence 13 (`run_step_1787248375_836424`) after Review `changes_required`
- **teardown_class_applies:** no

## Playbooks and notes applied

Repository charter: [[botster-tui-playbook]]

Role / stack:

- [[implementer-playbook]]
- [[botster-implementer-playbook]]
- [[botster-architecture]]
- [[project-pipelines-playbook]]
- [[implement gate must verify committed work and pr link before review]]
- [[implementation artifacts must match actual git state]]
- [[implementation steps must persist report artifacts for review]]
- [[pipeline vault checklists must cite exact resolvable note titles]]
- [[pipeline artifacts should use path neutral worktree references]]
- [[test script required for rust tests not cargo test]]
- [[prefer framework and library components over custom solutions]]

TUI charter must-load notes applied:

- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[tui and browser are equal clients]]
- [[tui client attach uses hub protocol not session protocol]]
- [[first-party Unix attach clients use split Hello and subscription close events]]
- [[first-party clients put terminal mechanism tokens only in terminal compatibility]]
- [[Unix mux polling returns bounded complete-frame batches while input stays readable]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]
- [[TUI live Ghostty has IsolatedHub ghostty plus attach-only ghostty-shared and ghostty-shared-exit]]
- [[TUI contract matrix headless echo can time out after successful Hello]]
- [[first-party Rust consumers pin the UI contract Git tag not a Hub rev]]
- [[Cargo Git URL and selector form are part of crate identity]]
- [[Git-consumed Hub members pin Core protocol by exact revision]]
- [[compatibility fixtures advertise every required optional feature]]
- [[TUI transient notices use run only fail closed matching]]
- [[optional always on entity families back off admission retries independently]]
- [[TUI bin only Core 8fce204 builds require local runtime feature unification]]
- [[client filter tiers require reachable view state]]

Task-surface notes:

- [[Client event subscriptions stay on the multiplexed host-control path]]
- [[Client event holders are connection-scoped]]
- [[Host package-event negotiation survives terminal admission rejection]]
- [[Fair host-control writing selects already-admitted frames]]
- [[exact owner plus name is the only package event subscription key]]
- [[Package-event subject filters are exact strings compiled at admission]]
- [[a transient package event cannot be the sole authority for a durable close]]
- [[additive daemon capabilities do not raise the default client requirement]]
- [[question opened clients subscribe with empty subjects]]
- [[botster plugin entities are canonical for plugin-owned dynamic state]]
- [[live published entity families use one durable sequence source]]
- [[live context fields must belong to live published entity families]]
- [[package event owners use admitted package names not repository names]]

Convention conflicts:

- [[first-party clients put terminal mechanism tokens only in terminal compatibility]] still records host Hello floor **40** and nine host-plane tokens. Plan revision 6 / A1 raises the floor to **44** and adds `package_event_subscriptions`. This implementation follows the approved plan. Terminal mechanism tokens stay on terminal compatibility only.

Not loaded: [[botster runtime teardown lenses]] (plan `teardown_class_applies: no`), [[spa-patterns]] (no browser surface).

## Files changed

- `crates/botster-tui/Cargo.toml` — Hub `7a09292`, Core `8fce204` lockstep pin roll.
- `Cargo.lock` — refreshed git pins.
- `crates/botster-tui/src/app.rs` — Hello feature + floor 44, SubscribeEvents candidate/active state, PackageEvent/EventGap apply, run-only notice filter, always-on workflow-context families with backoff, bounded mux apply, transient notice + attention bands, reconnect hygiene, hermetic tests, IsolatedHub `package-events` live test that binds a PTY session-type run, requires the matching notice, forces EventGap, and measures flood budgets.
- `script/test-live-hub` — `package-events` mode, PP path, sentinel, test-compile workaround for Core `8fce204` bin-only engine build.
- `README.md` — pin table, floor 44, `package_event_subscriptions`, live lane, included/not-included scope.
- `docs/reports/tui-consume-transient-package-events-through-the-hub-control-plane-implement-report.md` — this report.

## Ownership boundaries preserved

All source edits are in `botster-tui`. Hub, Core, Project Pipelines, and TUI Kit are consumed at pinned revisions. Notice and attention bands are app-composed `UiNode`s, not kit primitives.

## Cross-repo routing

| Repository | Stance |
| --- | --- |
| botster-hub `tgt_7e208a0c76a44980a83b63af976b1f22` | Consume `7a09292`. No change. |
| botster-project-pipelines `tgt_a72ca1a83d504385b8648f71409119ab` | Consume `cd7c2f9` (closed `ticket_1787200699_360898`). No change. |
| botster-core `tgt_1f7bce66eb304881980f9b4a2a5ae3fe` | Pin follows Hub lockstep `8fce204`. No change. |
| botster-tui-kit `tgt_3dfae49c02454037bf13554f552baf7f` | Unchanged. |

## Production entry point

`TuiApp::try_connect` is the production connect path. After a successful `connect_and_hello_with_terminal_requirement` it:

1. Sends `SubscribeEvents` for `project-pipelines` / `question.opened` with empty subjects.
2. Promotes candidate → active on `EventSubscribed` before `apply_pending_mux_frames`.
3. Calls `sync_entity_options_subscriptions`, which always demands `project-pipelines.question` and `project-pipelines.session_request` while the Hub connection is up.

`poll_hub` expires the notice on the ≤100 ms tick, then `poll_and_apply_mux_frames` applies at most 32 frames. `apply_mux_event` is the production PackageEvent/EventGap arm. `draw_workspace_shell` renders `workspace-transient-notice` and `workspace-question-attention`.

## Review findings addressed

| Finding | Severity | Resolution |
| --- | --- | --- |
| `finding_1787248364_659392` live lane can pass without the transient notice | high | Live fixture now creates a PTY session-type pipeline, calls `spawn_ticket_session` with `run_id`, requires `session_request.session_id` equality, a nonempty `question_id`, and one rendered matching notice. Fail-closed fallback is gone. |
| `finding_1787248364_569474` flood lane does not measure entity or terminal progress | high | Flood keeps event production active, then measures exact-row entity convergence, live-payload echo, and mux terminal-frame progress. Each published budget is printed. |
| `finding_1787248364_330620` live lane does not prove missed-event durable state | high | IsolatedHub sets `BOTSTER_ENV=test`, `BOTSTER_HUB_TEST_CLIENT_EVENT_QUEUE_MAX=2`, and the Unix event-flush stall. The test applies EventGap before remaining frames, asserts the notice is cleared, and keeps the exact question row plus attention band. |
| `finding_1787248364_303131` Hello test name still claims revision 40 | low | Test renamed to `tui_requires_protocol_7_revision_44_and_split_terminal_hello`. |
| `finding_1787248364_910873` report leaks machine-local paths | medium | Live command fence uses `/path/to/botster-hub` and `/path/to/botster-project-pipelines`. |

## Deviations from plan

- `script/test-live-hub package-events` compiles with `cargo test --no-run` instead of `cargo build -p botster-tui`. A bin-only build of Core `8fce204` compiles `engine/botster.rs` without `local-runtime`, so `IncrementalAttach` is missing. Dev-dependency `botster-hub-test-support` enables that feature. This is R1 pin-roll fallout, not a TUI product change. See [[TUI bin only Core 8fce204 builds require local runtime feature unification]].
- No synthetic event-plane producer fixture (U3). Flood uses repeated PP `ask_human` calls plus the Hub queue-max and flush-stall knobs.

## Tests and downstream proof

Repository gates:

- `./script/fmt` — pass.
- `./script/clippy` (`-D warnings`) — pass.
- `BOTSTER_ENV=test ./script/test` (`cargo test --workspace --all-targets`) — pass, 267 bin tests + package manifest test.

Hermetic unit tests in `app.rs` cover Hello composition, mux demux, bounded drain, run-only filter (including agreeing and disagreeing `session_id` rows in both insertion orders), admission backoff request bound, TTL clock boundary, response race, EventGap, foreign-id drop, always-on demand set, attention count, reconnect teardown.

Live Unix:

```
BOTSTER_HUB_BIN=/path/to/botster-hub/target/debug/botster-hub \
BOTSTER_SESSION_WORKER_BIN=/path/to/botster-hub/target/debug/botster-session-worker \
BOTSTER_PROJECT_PIPELINES_PACKAGE_PATH=/path/to/botster-project-pipelines \
CARGO_TARGET_DIR=$PWD/target \
  ./script/test-live-hub package-events
```

Result: `package-events-live: complete`. Hub binaries from Hub checkout `7a09292`. PP checkout `cd7c2f926fcead78e15e7a9c713ad26dfe883914`.

Live budgets:

- `poll_and_apply_mux_frames` under flood: `tick_ms=0` (limit < 200 ms)
- exact-row entity convergence under flood: `entity_ms=456` (limit ≤ 3,000 ms)
- terminal input echo under flood: `echo_ms=1002` (limit ≤ 3,000 ms)
- terminal output progress under flood: `output_ms=1485` (limit ≤ 3,000 ms)

Downstream proof: none required beyond this repository. TUI is the terminal consumer in this chain.

## Unverified behavior or residual risk

- Ghostty live attach was not re-run on the rolled Core pin. Full workspace tests passed. Shared live lanes need a caller Hub ≥ `7a09292` (A1).
- Botster MCP in this Grok session failed handshake (`BOTSTER_SESSION_UUID` not expanded). Pipeline tools were invoked through `botster mcp-serve` with the real session UUID.

## Missing vault guidance discovered

Ready to capture (and captured to inbox where new):

- A client filter tier is only real if some client view state can produce it.
- Live-published family membership, not field presence, decides whether an entity field is a usable live context source.
- Bin-only Cargo builds of first-party TUI against Core `8fce204` fail unless a consumer enables `local-runtime`, because `engine/botster.rs` is not feature-gated around `IncrementalAttach`.
- The TUI transient-notice pattern (candidate/active SubscribeEvents, run-only fail-closed matching, bounded-backoff always-on entity families, count-only attention band).

[[first-party clients put terminal mechanism tokens only in terminal compatibility]] should be updated to floor 44 and the tenth host-plane token after this lands.
