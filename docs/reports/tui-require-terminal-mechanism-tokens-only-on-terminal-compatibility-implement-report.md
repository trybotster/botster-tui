# Implementation report: TUI require terminal mechanism tokens only on terminal_compatibility

- **Ticket:** `ticket_1786756492_156718`
- **Run:** `run_1786756665_970591`
- **Step:** `botster_stack_implement`
- **PR:** none. Pipeline `merge_policy` is `direct`. Do not create a pull request.
- **Target repository:** `botster-tui` (`trybotster/botster-tui`)
- **Target id:** `tgt_c3d470bab78549df920a41e8fb0e58d8`
- **Resolved from:** independent `list_spawn_targets` (`name=botster-tui`, `repo_name=trybotster/botster-tui`). Matches the approved plan routing.
- **Base:** `origin/main` `96d7c42b4e0c0359a2ba601e1bc95515ffaca323`
- **Branch:** `project-pipelines/ticket_1786756492_156718`
- **teardown_class_applies:** false
- **session_type_eligibility_consumer:** false

## Playbooks and notes applied

Repository charter: [[botster-tui-playbook]]

Role / stack:

- [[implementer-playbook]]
- [[botster-implementer-playbook]]
- [[botster-architecture]]
- [[cli-patterns]]
- [[implement gate must verify committed work and pr link before review]]
- [[implementation artifacts must match actual git state]]
- [[implementation steps must persist report artifacts for review]]
- [[pipeline vault checklists must cite exact resolvable note titles]]
- [[pipeline artifacts should use path neutral worktree references]]
- [[test script required for rust tests not cargo test]]
- [[project pipelines checklist worker timeouts require artifact evidence fallback]]
- [[pre existing failure waivers must isolate the first non cascade failure on base]]
- [[live hub proof records distinct hub and locked core binary provenance]]

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
- [[external client hub tests use subprocess spawned hub test support]]

Not loaded: [[project-pipelines-playbook]] — no Project Pipelines package or plugin paths. Not loaded: [[botster runtime teardown lenses]] — teardown class does not apply.

Convention conflicts: none.

## Files changed

- `crates/botster-tui/src/app.rs` — drop `terminal_streaming`, `resize`, and `snapshot_delivery=ready_then_history` from `tui_compatibility_requirement()`; invert host assertions; add host-omission and terminal-requirement tests using `botster_terminal_protocol_client::FEATURE_*`; stop the headless live Status scan from requiring the three host tokens.
- `README.md` — one sentence: host Hello lists host-plane features only; the three mechanism tokens live on `terminal_compatibility`.
- `docs/plans/tui-require-terminal-mechanism-tokens-only-on-terminal-compatibility-plan.md` — approved plan (uncommitted in this Implement worktree after the spawn retry; committed here as the reviewable plan artifact).
- `docs/reports/tui-require-terminal-mechanism-tokens-only-on-terminal-compatibility-implement-report.md` — this report.

## Ownership boundaries preserved

Edited only TUI host Hello composition, hermetic/live assertions, README, and this ticket's plan/report. Did not edit Hub, Web, hub-client source, Core protocol crates, mux decode, attach hydration, Ghostty apply, or close-event handling. Cargo pins, host `PROTOCOL_VERSION` 7, and host conformance floor 40 are unchanged.

Production entry point: `HubConnection::connect` → `tui_compatibility_requirement()` + `tui_terminal_compatibility_requirement()` → `connect_and_hello_with_terminal_requirement` → `admit_terminal_hello`. The connect helper and terminal requirement function were not rewritten.

## Cross-repo routing

| Ticket | Target | Role |
| --- | --- | --- |
| `ticket_1786661010_198387` | Hub `tgt_7e208a0c76a44980a83b63af976b1f22` | Downstream cold-cut. Already depends on this ticket. Not edited here. |
| First-party Web | `botster-web` | Already ships the same host/terminal split. Reference only. |

No new Project Pipelines dependency.

## Deviations from plan

- Did not merge to `main` from Implement. Review is the next pipeline step. Merge policy remains direct (no PR).
- The Implement worktree was created from `origin/main` after the first Implement spawn failure, so the Plan file was untracked here. It is committed with the implementation so Review has the approved plan on the branch.
- Live contract-matrix (`script/test-live-hub` default) still times out waiting for `botster-tui-ready` after attach. That failure is pre-existing on `origin/main` with the same command and binaries (see Tests). Not fixed here; the plan forbids mux/attach/hydration changes.

## Tests and downstream proof

Hermetic, ticket worktree, `BOTSTER_ENV=test`:

| Command | Result |
| --- | --- |
| `script/fmt` | pass |
| `script/clippy` | pass (`-D warnings`) |
| `script/test` | pass: 241 unit tests + 1 `package_manifest` test |
| targeted Hello tests | `tui_requires_protocol_7_revision_40_and_split_terminal_hello`, `host_hello_accepts_fixture_that_omits_terminal_mechanism_tokens`, `terminal_hello_still_requires_core_mechanism_tokens`, `missing_terminal_snapshot_delivery_on_hello_ack_fails_before_attach`, `missing_terminal_compatibility_ack_field_fails_before_attach` all pass |

Live binaries, built locked from the Hub checkout at `959c58f55726d098299cced8af151d8f496f41e3` into a fresh target directory:

- `botster-hub` from that Hub SHA
- `botster-session-worker` from package `botster-core-daemon`, locked Core `f4f6bf5babe92dfb9241a760c414187f711c2c42`
- Both realpaths live under the same fresh target `debug/` directory
- Contract-matrix fixture: Hub checkout `packages/hub-test-support/fixtures/plugin-contract-matrix`

| Command | Branch (`this change`) | Base (`origin/main` `96d7c42`) |
| --- | --- | --- |
| `script/test-live-hub` (contract-matrix) | exit 101 | exit 101 |
| `script/test-live-hub ghostty` | pass (`ghostty-live-complete`) | not rerun; attach path proved on branch |

First root live failure, isolated:

- Test: `app::tests::headless_live_runtime_runs_against_isolated_hub_when_binaries_are_available`
- Symptom on both branch and base: session-lifecycle conformance passes; `package-storage-context: configured` prints (Hello/connect succeeded); then `timed out waiting for terminal output "botster-tui-ready"; terminal-output-prefix: ""`
- Identical command, identical Hub `959c58f` / worker `f4f6bf5` binaries
- Therefore not caused by dropping the three tokens from host `required_features`

`script/test-live-hub ghostty` on this branch against those same binaries completed live attach, history/scrollback, sibling isolation, and `core_adapter_closed`. The test's own `hub_rev=` line prints the Cargo hub-client pin `4f30d695`, not the live binary SHA. Live binary provenance is Hub `959c58f` / Core `f4f6bf5` as built above.

## Unverified behavior or residual risk

- Contract-matrix headless `run_headless_live_runtime` still does not observe `botster-tui-ready` / `echo:botster-tui-headless` against current Hub main. Pre-existing on `origin/main`. Out of this ticket's Hello-composition scope.
- Current Hub main may still advertise the three tokens on host Status. Tests no longer require that advertisement. After this merge, Hub `ticket_1786661010_198387` may remove them.
- Ghostty live proof is the live attach gate that passed. Contract-matrix package-open echo remains unverified on current Hub main for both this branch and base.

## Missing vault guidance discovered

The plane-ownership notes exist. The missing client composition rule is now captured in the vault inbox as `first-party-clients-require-terminal-mechanism-tokens-only-on-terminal-compatibility.md` (prose claim: first-party clients require terminal mechanism tokens only on terminal_compatibility). Do not treat that inbox file as a promoted vault note until the vault pipeline processes it.
