# Implement report: TUI manage and launch authoritative Hub session types

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`git@github.com:trybotster/botster-tui.git`) |
| target_id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Ticket | `ticket_1785970234_132113` |
| Run | `run_1786074731_672724` |
| Worktree | pipeline worktree for this ticket (not ambient `Projects/`) |

## Repository playbook and other playbooks/notes applied

### Role / charter

1. [[implementer-playbook]]
2. [[botster-implementer-playbook]]
3. [[botster-tui-playbook]] (ownership charter)

### Task-relevant atomic notes

- [[tui and browser are equal clients]]
- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[tui client attach uses hub protocol not session protocol]]
- [[botster hub client state sync is entity frame only]]
- [[botster client subscriptions should not hydrate global state]]
- [[botster entity snapshots are authoritative reconnect baselines]]
- [[hub qualifies effective session type ids as source name slash id]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]
- [[tui error dedup tests must drive real input handlers]]
- [[test script required for rust tests not cargo test]]
- [[closed dependency tickets signal merged source not a consumable release]]
- [[external client hub tests use subprocess spawned hub test support]]

### Not loaded as implementation scope

- [[project-pipelines-playbook]] — no Project Pipelines package/plugin code edited

### Human product decisions

- `question_1786075802_958194` → **A**: keep `MINIMUM_CONFORMANCE_FIXTURE_REVISION = 31`; `FEATURE_SESSION_TYPE_ENTITY_SUBSCRIPTIONS` stays out of `required_features`; live `session-types` profile fail-closes on conformance ≥ 32 + feature presence.

## Files changed

| Path | Change |
| --- | --- |
| `crates/botster-tui/Cargo.toml` + `Cargo.lock` | Kit `902650d`, Hub crates `302190e`, core branch source `33ebcd98…` |
| `crates/botster-tui/src/app.rs` | session_type store/pump; System details Session types UI; ShowSessionTypeDefinition edit; CRUD; target-first SpawnSessionType; freeform product spawn removed; hermetic + live tests |
| `script/test-live-hub` | Independent `session-types` profile |
| `README.md` | Surface, lossless edit, target-first launch, pins, live profile, IA reconciliation |
| `docs/plans/tui-manage-and-launch-authoritative-hub-session-types-plan.md` | Decision ledger for conformance/feature/lock invariants |
| `docs/reports/tui-manage-and-launch-authoritative-hub-session-types-implement-report.md` | This report |

## Ownership boundaries preserved

- **Owned here:** TUI app policy, entity projection, Hub request dispatch, product spawn UX, acceptance harnesses, README, pins to already-merged kit/Hub coordinates.
- **Not owned / not edited:** Hub protocol implementation, kit renderer work, botster-web, Workspaces package, `legacy_test_needs_system_details()`, contract-matrix never-connected assertion (`ticket_1786038825_352271`).

## Cross-repo dependencies / separately routed work

| Item | Status |
| --- | --- |
| Hub session types (`ticket_1785970233_236046`) | closed (merged source; consumable pin `302190e`) |
| Kit repin (`ticket_1786071998_949850`) | closed; pin `902650d` |
| contract-matrix live failure | sibling `ticket_1786038825_352271` (non-owned) |
| Pipeline worktree `:` | `ticket_1786071999_889350` (documented CARGO_TARGET_DIR workaround) |

## Deviations from plan

1. **Conformance minimum stays 31** (human answer A; plan left this open). Recorded in plan decision ledger.
2. **Live launch proof** uses a separate PackageRoot device shell type after authoring round-trip, because Hub resolves device commands as relative paths under `<data-dir>/session-types` and absolute/shell strings are invalid. Authoring path+env still proven via Show/Update/Show on the relative-path agent type.
3. **Headless live runtime** rewritten to CreateSessionType + SpawnSessionType with a device-root script (product path); no freeform product `DaemonRequest::Spawn`.
4. No other intentional scope expansion.

## Tests and downstream proof run

### Lock invariants (verified)

- Exactly **one** `botster-ui-contract` source at Hub `302190ec2acc5ecee744432a6c9ffd1f040ebe01`
- Exactly **two** `botster-core` sources: direct rev `16bf08f29ec723c70c290cf995745ccbf79d4f05` (unchanged) and `branch=main` at `33ebcd98d19031d23e91b03d8da0ee3f8d1410d4`
- Kit pin `902650dfbd56a5bdc99c1e88c04ba2e62442f703`

### Default gates

```sh
export CARGO_TARGET_DIR="/tmp/botster-tui-cargo-tgt-ticket_1785970234_132113"
script/fmt
script/test   # 162 + 1 package_manifest passed (no BOTSTER_HUB_BIN / REQUIRE env)
script/clippy
git diff --check
```

### Live

```sh
export BOTSTER_HUB_BIN=…/botster-hub   # ambient debug hub (conformance 32 observed)
export BOTSTER_SESSION_WORKER_BIN=…/botster-session-worker
export CARGO_TARGET_DIR="/tmp/botster-tui-cargo-tgt-ticket_1785970234_132113"
script/test-live-hub session-types
# output: session-types-live: complete conformance=32 features_has_session_type=true cases=agent,accessory,service,authoring,launch,delete,reconnect
```

### Runtime entry points

- Production: `system_details_visible` → Session types section; toolbar `botster.tui.spawn` → target-first flow → `DaemonRequest::SpawnSessionType`.
- Entity store: held-open `subscribe_entities(..., "session_type", ...)` on connect when feature present.
- Edit: `ShowSessionTypeDefinition` only → form → wholesale `UpdateSessionType`.

## Unverified behavior / residual risk

- Live binaries were ambient Projects hub debug builds (observed conformance 32), not a freshly rebuilt exact pin tree; fail-closed handshake provenance still asserted.
- Repo-source CRUD against admitted spawn targets not exercised live beyond device sources (device create/edit/delete/launch covered).
- Package read-only forced mutation error path only asserted when package rows exist; empty hubs skip that observation.
- Dense form focus order under System details scroll is basic InputRouter coverage (create button), not full field traversal matrix.
- Sibling contract-matrix profile remains red by ownership (`ticket_1786038825_352271`).

## Missing vault guidance discovered

1. TUI Session types currently under System details vs aspirational Hub settings shell (documented in README).
2. Device session type `command` must be a **relative path under `<hub-data-dir>/session-types`**, not an absolute shell string — cross-client gotcha for product SpawnSessionType cold cut.
3. Dual `botster-core` identity (direct rev + hub-test-support branch) must be stated on every consumer repin (recorded in plan + this report).
4. Client-wide conformance minimum vs surface-local feature degradation (answered; capture as vault note candidate after pipeline).


## Review return fixes (`review_1786078722_260151`)

Addressed open findings:

| Finding | Severity | Fix |
| --- | --- | --- |
| Device/package types unlaunchable | blocker | Launch targets = admitted spawn targets ∪ entity `target_id`s (`device:local`, `package:*`) |
| Toolbar Spawn dead end | high | Target-first flow is a Dialog from `surface()`, not System details |
| Blind form editing | high | TextInputs prefer `self.drafts` like package config; keystroke render test |
| Empty token clear ignored | medium | `parse_token_list` / `parse_environment` empty clears rather than restoring seeds |
| ensure_headless writes hub data dir | medium | Helper `#[cfg(test)]` only; production headless smoke uses freeform Spawn harness |
| Plan absolute home path | medium | Path-neutral base_target_path wording |
| Weak authoring test | low | Renamed to pure helper test; added end-to-end launch request + dialog tests |
| delete mutation source | low | Repo delete uses `source_name` |
| Conformance assertion | low | Hermetic pin fixture asserts exactly 32; client MINIMUM stays 31 |
