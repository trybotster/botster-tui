# Implementation report: TUI remove Project Pipelines coupling from the generic package-event client

- **Ticket:** `ticket_1787278327_199618`
- **Run:** `run_1787278336_152073`
- **Step:** `botster_stack_implement`
- **PR:** none. Plan assumption 1 and ticket delivery policy: merge into `main`, do not create a pull request.
- **Target repository:** `botster-tui` (`trybotster/botster-tui`)
- **Target id:** `tgt_c3d470bab78549df920a41e8fb0e58d8`
- **Base:** `origin/main` `0032fe97c76bcaccb09e540247106a9a998c23c6`
- **Plan revision:** 3 (`artifact_1787369927_453206`, commit `34c5a0af05`)
- **Plan Review:** `review_1787370123_136710` approved
- **Implement commit:** `27e515c82041e337dffdce1b414cb435dd9bee36`. Report SHA recorded in the follow-up commit.
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
- [[pin rolls update live lane provenance defaults and README pin prose]]
- [[first-party Rust consumers pin the UI contract Git tag not a Hub rev]]
- [[test script required for rust tests not cargo test]]
- [[prefer framework and library components over custom solutions]]

Targeted notes:

- [[generic botster clients must not hardcode package event reactions]]
- [[client notice reactions belong to package declarations not client constants]]
- [[event plane client proof uses library contract fixtures]]
- [[published package owned notice reaction cutover is ui contract 0 3 3 and hub test support 0 1 41]]
- [[question opened notices target the agent session subject]]
- [[exact owner plus name is the only package event subscription key]]
- [[Package-event subject filters are exact strings compiled at admission]]
- [[current shared session client lanes do not prove package events]]
- [[optional always on entity families back off admission retries independently]]
- [[TUI transient notices use run only fail closed matching]]
- [[each acceptance condition names its authoritative production oracle]]
- [[a ui contract import line change costs one test line in each generic client]]
- [[Cargo Git URL and selector form are part of crate identity]]
- [[Git-consumed Hub members pin Core protocol by exact revision]]

Not loaded: [[botster runtime teardown lenses]] (plan `teardown_class_applies: false`).

Convention conflicts: none on product behavior. Kit still pins tag `botster-ui-contract-v0.3.2` while this consumer and `botster-hub-client` pin `botster-ui-contract-v0.3.3`. Cargo rejects a same-URL git `[patch]`, so the workspace uses a path vendor at `third_party/botster-ui-contract` as the one crate identity. That is pin-identity repair, not kit policy.

## Files changed

- `crates/botster-tui/Cargo.toml` — Hub `baeb04dcb4a11de4c3932d16bf09a8e5ff6ba4b5`, UI contract tag `botster-ui-contract-v0.3.3`.
- `Cargo.toml` — path `[patch]` so kit's v0.3.2 tag and this consumer share one `botster-ui-contract` identity.
- `Cargo.lock` — one `botster-ui-contract` 0.3.3 path source; Hub client/test-support at `baeb04d`.
- `third_party/botster-ui-contract/**` — vendored tag `botster-ui-contract-v0.3.3` (`12e0cc6994be18024e4bdfffb22947526a652204`) plus `PIN.md`.
- `crates/botster-tui/src/app.rs` — descriptor-driven notice subscriptions, session-subject scoping, `resolve_notice_text`, deletion of Project Pipelines production constants and durable attention, generic decode-boundary tests, IsolatedHub matrix live lane. Ghostty live default Hub rev rolled to `baeb04d`.
- `crates/botster-tui/tests/production_source_has_no_package_product_tokens.rs` — whole-`src` ownership scan.
- `script/test-live-hub` — `package-events` mode no longer requires a Project Pipelines package path.
- `README.md` — pin table, live-lane defaults, and notice-behavior prose.
- `docs/reports/tui-remove-project-pipelines-coupling-from-generic-package-event-client-implement-report.md` — this report.

## Ownership boundaries preserved

- `botster-tui` owns client subscription, filtering, gap reaction, reconnect, and notice rendering. It reads `DaemonPackage.notice_reactions` and does not own event contracts.
- `botster-hub` owns the descriptor projection. This run consumes pin `baeb04d` and does not change Hub.
- `botster-project-pipelines` owns `question.opened` emission and durable question UI. This run deletes TUI durable attention rather than generalizing it.
- `botster-tui-kit` stays policy-free. The path vendor only unifies Cargo identity.

Production entry point: `TuiApp::try_connect` refreshes packages, then `sync_notice_subscriptions` on connect, package-list refresh, focus change including focus to none, and reconnect. `apply_mux_frames` is the host-control decode path that applies `PackageEvent` and `EventGap`. `draw_workspace_shell` / `TuiApp::surface` render `transient_notice_band`.

## Cross-repo routing

Closed dependencies consumed, not re-implemented:

- `ticket_1787278643_145174` (botster-hub) — notice reaction descriptor
- `ticket_1787278658_151737` (botster-project-pipelines) — session subject emission
- `ticket_1787349524_364728` (botster-hub) — UI contract v0.3.3 tag

No new cross-repo tickets. Kit remaining on v0.3.2 is a consumer-side identity patch, not a kit policy change.

## Deviations from plan

1. **UI-contract path vendor.** Plan one-source proof assumed rolling the TUI direct tag plus Hub client would yield one git tag source. Kit `c83ba6c` still pins `botster-ui-contract-v0.3.2`. Same-URL git `[patch]` is rejected. Path vendor of v0.3.3 is the one identity. `cargo tree -p botster-tui -i botster-ui-contract --locked` reports that single path crate.
2. **Live emit overlay.** The published Hub `plugin-contract-matrix` fixture declares `contract.ready` but `plugin.lua` does not emit. The live lane copies the Hub-owned fixture and injects a `contract.emit_ready` tool, keeping package name `botster.plugin-contract-matrix`. Unit tests do not need that overlay.
3. **Conformance floor stays 44.** Hub pin reports fixture revision 46. Hello still admits 44. The pin-publication assertion now expects 46. Relied-on Ghostty/session-plugin fixtures still satisfy `>= 44`.
4. **No pull request.** Matches plan assumption 1.

Removing `question_attention_band` removes the open-question count from the TUI until Project Pipelines republishes it through a package surface. That is the directed outcome.

## Tests and downstream proof

- `cargo fmt --all -- --check` — pass
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — pass
- `cargo build -p botster-tui --locked` — pass (production build gate)
- `./test.sh` — 267 binary tests + 1 package-manifest + 1 ownership scan, zero failures
- `cargo tree -p botster-tui -i botster-ui-contract --locked` — one `botster-ui-contract` 0.3.3 path source
- `./script/test-live-hub package-events` — `package-events-live: complete` against Hub binaries from checkout `baeb04d` and Core worker `7eafa47`

Live lane proved: descriptor-driven subscribe with session subject, matching notice render, Hub rejection of a foreign subject (no extra mux event frame), EventGap clear, reconnect without replay.

## Unverified behavior or residual risk

- Shared Ghostty lanes were not re-run. They stay terminal-only per [[current shared session client lanes do not prove package events]].
- The live emit overlay is test-local. Until Hub's published matrix fixture emits `contract.ready`, a stock fixture install will subscribe and wait.
- Durable open-question count is gone from the TUI by design.
- Kit still names tag v0.3.2 in its own manifest. The path patch hides that split only in this consumer graph.

## Missing vault guidance discovered

1. A production-ownership scan must live outside `src` if it names the forbidden tokens, and must walk the whole `src` tree. Captured as the new `tests/` scan; still worth a vault note with the measured `app.rs` `#[cfg(test)]` counts from the plan.
2. A first-party consumer that pins a UI-contract tag and also consumes Kit must prove one Cargo identity after a tag bump, including the Kit tag skew case. Existing notes cover TUI+Hub-client split, not Kit remaining one tag behind.
3. Hub-owned ABI fixtures that declare notices may still lack an emit handler. Generic client live proof then needs a documented overlay or a Hub fixture emit tool.
