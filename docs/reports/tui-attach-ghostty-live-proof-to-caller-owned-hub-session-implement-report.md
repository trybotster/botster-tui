# Implementation report: TUI attach Ghostty live proof to a caller-owned Hub session

- **Ticket:** `ticket_1786868597_171437`
- **Run:** `run_1786868609_472512`
- **Step:** `botster_stack_implement`
- **PR:** none. Pipeline `merge_policy` is `direct`. Plan: merge into main, do not create a PR.
- **Target repository:** `botster-tui` (`trybotster/botster-tui`)
- **Target id:** `tgt_c3d470bab78549df920a41e8fb0e58d8`
- **Base:** `origin/main` `fc1ff6238ae707c355febbc03eeab5130cccf91c`
- **First Implement commit:** `9cff7dda067f03120d494be660a051fb6c9ad279`
- **Review-revisit commit:** `4eaa9a7e16a572a99739e4b7086d286a85606982`
- **teardown_class_applies:** yes
- **Plan revision:** 4 (`artifact_1786900493_408949`)
- **Hub occupancy dependency:** `ticket_1786870433_515008` closed at `c72712e2606b8abe77e1b91c2a736791036fadd8`

## Playbooks and notes applied

Repository charter: [[botster-tui-playbook]]

Role / stack:

- [[implementer-playbook]]
- [[botster-implementer-playbook]]
- [[botster-architecture]]
- [[cli-patterns]]
- [[botster runtime teardown lenses]]
- [[implement gate must verify committed work and pr link before review]]
- [[implementation artifacts must match actual git state]]
- [[implementation steps must persist report artifacts for review]]
- [[pipeline vault checklists must cite exact resolvable note titles]]
- [[test script required for rust tests not cargo test]]

Consumed, not edited:

- [[botster-tui-kit-playbook]]
- [[botster-hub-client-playbook]]
- [[botster-terminal-ghostty-playbook]]

Targeted notes:

- [[tui client attach uses hub protocol not session protocol]]
- [[botster hub client crate is the external client boundary]]
- [[first-party Unix attach clients use split Hello and subscription close events]]
- [[first-party clients put terminal mechanism tokens only in terminal compatibility]]
- [[canceling incremental attach aborts the decoder and sends Detach]]
- [[daemon socket attach must detach subscriptions on disconnect and exit]]
- [[mux envelope delivery does not prove Hub route ownership]]
- [[live attach counters and omitted occupancy fields are not identity oracles]]
- [[Core terminal subscription ownership is session, subscription, and generation]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]
- [[pre READY attach failed ends client hydration]]

Not loaded: [[project-pipelines-playbook]] (no Project Pipelines package or plugin paths).

Convention conflicts: none. TUI Hello requires `attach_occupancy` as an occupancy-specific client. The hub-client default required-feature list is unchanged.

## Files changed

- `crates/botster-tui/Cargo.toml` — pin `botster-hub-client` and `botster-hub-test-support` to Hub `c72712e2606b8abe77e1b91c2a736791036fadd8`.
- `Cargo.lock` — lock Hub `c72712e` and one Core identity at `fc541a`. UI contract stays on tag `botster-ui-contract-v0.3.2`.
- `crates/botster-tui/src/app.rs` — default host Hello stays floor 40 without occupancy; `ghostty-shared` uses `tui_attach_occupancy_requirement()`; bounded `request_with_deadline` (write then read); Detach-if-writable on `force_reconnect`, cancel, recovery, and `record_transport_error`; shared-profile fail-closed parsers; stub bound tests; `ghostty-shared` and `ghostty-shared-exit` live tests; IsolatedHub flood Status taken before the poll loop.
- `script/test-live-hub` — `ghostty-shared` and `ghostty-shared-exit`; skip Hub/worker binary resolution; wrapper fail-closed on connection + session id; stream cargo output for shared profiles.
- `README.md` — occupancy pin, floor 43, caller injectors, two wrapper modes, caller end-session step.
- `docs/plans/tui-attach-ghostty-live-proof-to-caller-owned-hub-session-plan.md` — approved plan plus Implement adaptations.
- `docs/reports/tui-attach-ghostty-live-proof-to-caller-owned-hub-session-implement-report.md` — this report.

`.gitignore` remains 7 lines.

## Ownership boundaries preserved

Work stayed in `botster-tui`. No Hub, Web, kit, or Ghostty crate edits. Occupancy is consumed from closed Hub `ticket_1786870433_515008`. TUI does not spawn, create, or ShutdownSession the shared session.

## Cross-repo routing

- Hub occupancy oracle: closed `ticket_1786870433_515008` / `tgt_7e208a0c76a44980a83b63af976b1f22`. Consumed, not edited.
- Parent Hub integration `ticket_1786661010_115885` can attach Web and TUI to `north-star-shared` after merge. This ticket does not run that joint proof.
- Web sibling `ticket_1786868596_331812` is parallel. Not edited.

## Deviations from plan

- IsolatedHub `ghostty` flood Status is issued immediately after sibling attach. Hub `c72712e` can emit `core_adapter_closed` on the first `poll_hub`, which skipped the host-Status oracle. Flood/write-budget assertions are otherwise unchanged. Recorded in the committed plan.
- Shared wrapper streams cargo test output. Buffering until process exit hid `ghostty-shared-exit-attached` and deadlocked run 2. Recorded in the committed plan.
- Hub client `c72712e` no longer re-exports terminal mechanism tokens. Tests import them from `botster-terminal-protocol-client`.
- Review required restoring default Hello to floor 40 without `attach_occupancy`. Occupancy is `tui_attach_occupancy_requirement()` on `ghostty-shared` only. This supersedes plan rev 4's default Hello bump.
- Review required aligning Core-family pins to Hub `c72712e`'s Core `fc541a`. This supersedes plan rev 4's "keep f4f6bf5 unless compile break".

## Tests and downstream proof

Repo gates:

- `script/fmt`
- `script/clippy`
- `script/test` — 249 unit tests + package manifest, all ok

Bounded Detach stubs (no live Hub):

- `bounded_detach_returns_when_hub_withholds_the_response` — production `force_reconnect` returns in under 3s, no ShutdownSession
- `bounded_detach_returns_when_peer_stops_reading` — saturated send buffer, same bound

Fail-closed:

- missing/malformed `BOTSTER_HUB_CONNECTION`
- missing/empty `BOTSTER_SHARED_SESSION_ID`
- wrapper `ghostty-shared` / `ghostty-shared-exit` fail without Hub binaries

Live caller-owned Hub (caller IsolatedHub outside the profile, session `north-star-shared`):

- `script/test-live-hub ghostty-shared` printed `ghostty-shared-complete`
- `script/test-live-hub ghostty-shared-exit` printed `ghostty-shared-exit-attached`, caller sent `ShutdownSession` from Hub control, then `ghostty-shared-exit-complete`

IsolatedHub profile:

- `script/test-live-hub ghostty` printed `ghostty-live-complete` against Hub `c72712e` / worker `fc541a`

Production entry points:

- `HubConnection::connect` → `tui_compatibility_requirement()` now requires `attach_occupancy` and floor 43
- `detach_attached`, `recover_current_subscription`, `force_reconnect`, and `record_transport_error` use `request_with_deadline` / Detach-if-writable
- `poll_hub` still maps readable EOF to `ClientDisconnected` → `record_transport_error`

## Runtime-teardown lenses

Every lens from [[botster runtime teardown lenses]] is implemented:

| Lens | Evidence |
| --- | --- |
| Isolation | Shared live test cuts one TUI pair; sibling occupancy and host session remain |
| Bounds | `DETACH_ON_DISCONNECT_BOUND` = 2s write+read; both stubs return in < 3s |
| Late-message matrix | Detach bounded; ShutdownSession forbidden on shared paths; ProcessExited observed in run 2 |
| Production-path proof | Socket cut through `poll_hub` → `ClientDisconnected`; occupancy exact-absence; caller-ended exit |
| Ownership identity | Release uses `(session_id, subscription_id)` plus generation as identity evidence; replacement mints a new subscription id |
| Sibling fail-closed | Sibling echo after cut; no TUI ShutdownSession on timeout or EOF |

## Unverified behavior or residual risk

- Joint Web+TUI attach to `north-star-shared` is the parent ticket's proof, not this one.
- IsolatedHub `ghostty` flood Status timing depends on Hub `c72712e`. The Status oracle now runs before the first flood poll.
- Zig / Ghostty native build environment remains an unknown for fresh clones.

## Missing vault guidance

- [[first-party clients put terminal mechanism tokens only in terminal compatibility]] still says floor 40 and nine host-plane tokens. This ticket raises the TUI floor to 43 and adds `attach_occupancy` as a tenth host-plane token.
- Captured after proof: inbox `tui-live-ghostty-has-isolatedhub-and-caller-owned-shared-profiles.md`.
