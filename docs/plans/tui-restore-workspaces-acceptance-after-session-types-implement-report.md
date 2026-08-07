# Implementation report — ticket_1786036326_597046

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`git@github.com:trybotster/botster-tui.git`) |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Run | `run_1786060050_399115` |
| Branch | `project-pipelines/ticket_1786036326_597046` @ `c956457` |
| PR | https://github.com/trybotster/botster-tui/pull/46 |

## Repository playbook and other playbooks/notes applied

- [[implementer-playbook]]
- [[botster-implementer-playbook]]
- [[botster-tui-playbook]] (ownership charter)
- Plan Must Load notes as cited in the approved plan (botster-architecture, cli-patterns, hub qualifies effective session type ids…, cold-cut field rename…, acceptance readiness…, waiver premises…, etc.)
- Deliberately not loaded: [[project-pipelines-playbook]] (no package/plugin path change)

## Files changed

| Path | Change |
| --- | --- |
| `crates/botster-tui/src/app.rs` | `template_id` → `session_type_id` at spawn-form selection; stage copy; split waiver test into contract-matrix env test; add field-key source-scan; clean live-test docs; complete protocol-6 session-types fixture fields |
| `script/test-live-hub` | Removed workspaces hard-block |
| `crates/botster-tui/fixtures/workspaces-spawn-driver-v1.evidence.jsonl` | Example keys → `session_type_id` |
| `README.md` | Drop known-gap block; lifecycle ownership on this ticket; stop sole live-evidence claim for contract-matrix |
| `docs/plans/tui-restore-workspaces-acceptance-after-session-types-plan.md` | Committed reviewed plan |

## Ownership boundaries preserved

- Edits only in `botster-tui` worktree for `tgt_c3d470bab78549df920a41e8fb0e58d8`.
- No edits to `botster-workspaces`, `botster-hub`, `botster-hub-client`, `botster-core`, `botster-web`, or `botster-tui-kit` sources.
- Hub/core binaries built from pin for acceptance environment only (not code changes).
- Workspaces package consumed at post-migration `3ec366a` via `BOTSTER_WORKSPACES_PACKAGE_PATH`.

## Cross-repo dependencies or separately routed work

- Closed deps: `ticket_1785984128_479155` (Workspaces session types), `ticket_1785976581_841608` (protocol-6 TUI pin).
- Open sibling `ticket_1786038825_352271` owns contract-matrix live failure (not this proof path).
- No new cross-repo tickets required for this restore; residual hub strictness around incomplete session-type fixtures is documented below as fixture correction in-TUI.

## Deviations from plan

1. **Installed-driver session-types fixture schema completion (necessary).** Plan assumed the existing minimal `session-types.json` fixture remained valid. On Hub pin `8a60bd58`, `PackageSessionType` requires `label`, `role`, `interaction`, and `lifecycle` in addition to `id`/`command`. Incomplete files caused `CreateSpawnTarget` to disconnect with hub stderr `unexpected daemon response` (and `ListSessionTypes` to report `invalid_repo_session_types` / missing `label`). Implement completed the fixture fields so the runtime path is exercisable. This stays inside TUI ownership (test fixture), not a Hub code change.
2. Source-scan required half is call-site specific (not bare `contains("session_type_id")`), forbidden half uses `concat!("template","_id")`, fixture half uses separate `include_str!` — applies Plan Review info/low findings `finding_1786063320_*`.

No lanes weakened or re-skipped.

## Tests and downstream proof run

### Hermetic / default gate

- `./test.sh` → **153** unit tests + package manifest **ok**
- `app::tests::workspaces_spawn_acceptance_uses_session_type_id_field_key` **ok**
- `app::tests::contract_matrix_mode_requires_its_fixture_env_var` **ok**
- Waiver loop `blocked_workspaces_lanes_report_a_known_gap_for_every_profile` **removed**

### Live binary provenance

| Item | Value |
| --- | --- |
| Hub commit | `8a60bd58841179f8b1fd4040d9362d18ea244230` |
| Hub binary | `/tmp/botster-hub-pin-8a60bd58-target/debug/botster-hub` |
| Session-worker commit | `33ebcd98d19031d23e91b03d8da0ee3f8d1410d4` (botster-core lock of hub pin) |
| Session-worker binary | `/tmp/botster-hub-pin-8a60bd58-target/debug/botster-session-worker` (co-located) |
| Workspaces package | `/Users/jasonconigliari/Projects/botster-workspaces` @ `3ec366abd1fd86dcade81b7a14470dcacfcbd504` |
| Pre-flight | protocol 6, conformance fixture revision 31 |

### Live lanes (all exit 0)

| Profile | Marker |
| --- | --- |
| installed-driver | `installed-workspaces-driver: complete cases=3` |
| plumbing | `workspaces-acceptance: profile=Plumbing ledger=complete` |
| lifecycle | `workspaces-acceptance: profile=Lifecycle ledger=complete` |

Runtime path: installed Workspaces package → `botster_workspaces.open_spawn` → form field `session_type_id` selected via `select_only_acceptance_value` → spawn through `InputRouter` → entity updates (installed-driver cases=3).

## Unverified behavior or residual risk

- Contract-matrix live lane remains owned by open `ticket_1786038825_352271`; not re-proven here.
- Hub still fails closed harshly (client disconnect) on invalid repo-local session-type files rather than always returning a structured CreateSpawnTarget error; fixture now satisfies the pin schema.
- Ambient `Projects/botster-hub` main tip binaries were not used for scoring.

## Missing vault guidance discovered

Candidate captures (align with plan vault-gap section; not written to vault in this step):

1. Package form field renames require first-party acceptance driver tickets before un-skip.
2. Cold-cut consumer field keys need a hermetic source-scan under `script/test`.
3. Waiver hermetic tests must delete only the waiver half.
4. README gap sections can outlive producer fixes.
5. Live Hub binary provenance must match the crate pin under exact protocol equality.
6. **New:** Protocol-6 repo-local `session-types.json` fixtures must include full `PackageSessionType` fields (`label`, namespaced `role`, `interaction`, `lifecycle`, …); incomplete files can crash CreateSpawnTarget paths on Hub `8a60bd58`.

Capture decision for durable vault notes: **nil this step** (candidates already listed in approved plan + #6 recorded here for later capture).

## Botster layers touched

TUI application acceptance driver, hermetic invariants, live-hub wrapper, README — only.
