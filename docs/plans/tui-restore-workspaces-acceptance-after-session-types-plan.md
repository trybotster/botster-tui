# TUI: restore Workspaces acceptance coverage after the session-types migration

## Plan revision

| Field | Value |
| --- | --- |
| Pass | 2 — Plan Review `changes_required` (`review_1786060986_362275`) |
| Open findings closed by this revision | `finding_1786060987_108239`, `finding_1786060987_489488`, `finding_1786060986_814267`, `finding_1786060987_795131`, `finding_1786060987_804049` |

### Finding disposition

| Finding | Severity | Disposition |
| --- | --- | --- |
| `finding_1786060987_108239` — cold cut has zero default-gate coverage | high | **Adopt.** Add mandatory hermetic source-scan test under `script/test` (repo idiom). |
| `finding_1786060987_489488` — live binary provenance unspecified | high | **Adopt.** Build hub + session-worker from pin `8a60bd58841179f8b1fd4040d9362d18ea244230`; pre-flight protocol 6 / conformance ≥ 31; record absolute paths + commits in Implement evidence. |
| `finding_1786060986_814267` — deleting waiver test drops contract-matrix coverage | high | **Adopt.** Split test: preserve contract-matrix fixture-env assertion; delete only waiver loop + vacuous guard-leak assertion. |
| `finding_1786060987_795131` — README defers lifecycle to closed ticket | medium | **Adopt.** State lifecycle proof is **this ticket’s obligation** with no remaining downstream owner; rewrite README accordingly. |
| `finding_1786060987_804049` — plan file untracked | low | **Adopt.** Implement first commit on the run branch must include this plan file. |

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`git@github.com:trybotster/botster-tui.git`) |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Ticket | `ticket_1786036326_597046` — "TUI: restore Workspaces acceptance coverage after the session-types migration" |
| Project | `project_1785970196_204877` |
| Pipeline / run | `botster_stack_delivery` / `run_1786060050_399115` |
| Current step | `botster_stack_plan` (return visit after Plan Review) |
| Base | `main` at `176384f` ("Merge pull request #45 … ticket_1785976581_841608") |
| Worktree | Pipeline worktree for this ticket (explicit target path `Projects/botster-tui`); do not treat ambient cwd as ownership authority |

Authoritative target comes from the ticket/run `target_id`, not from the process working directory. Remote resolves to `trybotster/botster-tui`.

## Repository playbook loaded

- [[botster-tui-playbook]] — ownership charter for this target

## Other role / surface playbooks and atomic notes loaded

### Role overlays (required order)

1. [[planner-playbook]]
2. [[botster-planner-playbook]]
3. [[botster-tui-playbook]]

### [[botster-planner-playbook]] Must Load set

- [[botster-architecture]] — TUI is a client over Hub contracts; Workspaces package policy is not owned here.
- [[cli-patterns]] — Rust CLI/TUI acceptance, live-Hub harness, and Cargo gate expectations.
- [[spa-patterns]] — loaded because the overlay requires it; **adds no task-specific constraint** (no browser surface changes in this ticket).
- [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]] — pipeline product notes; no package/plugin path change in this run.
- [[botster orchestration should spawn agents with explicit target ids]] — this plan binds to `tgt_c3d470bab78549df920a41e8fb0e58d8` only.
- [[botster orchestration prompts must bind agents to explicit worktrees]] — implement against the pipeline worktree / explicit target path, never a sibling checkout.
- [[plan agents must author vault context as wikilinks not home paths]]
- [[pipeline vault checklists must cite exact resolvable note titles]]
- [[vault example paths are not repository placement conventions]] — plan lives under existing `docs/plans/` prior art in this repo.

### [[botster-tui-playbook]] Must Load set (task-relevant)

- [[tui and browser are equal clients]]
- [[botster tui consumes tui kit through a thin app policy adapter]]
- [[botster-tui-kit-playbook]] — renderer/input mechanics consumed, not modified
- [[tui client attach uses hub protocol not session protocol]]
- [[tui and socket terminal streams use clientworker transport adapters]]
- [[botster tui uinode event routing captures hit regions during draw]] — acceptance dispatches through realized `HitMap` / `InputRouter`
- [[tui error dedup tests must drive real input handlers]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]] — lifecycle lane waits on exact UUID + `lifecycle_class`

### Targeted atomic notes for this ticket

- [[hub qualifies effective session type ids as source name slash id]] — producer options are qualified `<source name>/<id>`; driver must select a rendered option, not invent a bare id
- [[a cold cut field rename can be a value shape change not only a key change]] — rename is a cold cut; no `template_id` alias
- [[fixture driven acceptance smoke tests can prove first party package plumbing]]
- [[live acceptance tests must not depend on a loop tick window]]
- [[shared hub workspaces acceptance omits package path without skipping its lane]] — contrast: this repo's live-hub **installs** Workspaces via explicit `BOTSTER_WORKSPACES_PACKAGE_PATH`
- [[botster-workspaces-playbook]] — **consumed package charter** (field rename + capability scope ownership); not an edit target
- [[waiver premises must be revalidated when blocking dependencies land]] — README gap prose and closed `ticket_1785296184_677408` must not re-defer lifecycle proof

### Deliberately not loaded

- [[project-pipelines-playbook]] — no Project Pipelines package/plugin path or workflow-policy change is in scope; this run only uses the delivery pipeline as the execution vehicle.
- Other repository charters (`botster-core`, `botster-hub`, `botster-web`, …) — not edit targets; Hub/Workspaces prerequisites are already closed dependency tickets.

## Context loaded

### Ticket intent (two pieces, both `botster-tui`-owned)

1. **Acceptance driver field rename.** The installed Workspaces spawn driver calls `select_only_acceptance_value(..., "template_id", ...)`. Workspaces migration renamed the form field / tool argument to `session_type_id`. Keeping the old name is a forbidden compatibility alias.
2. **Lane un-skip and proof.** `script/test-live-hub workspaces {installed-driver,plumbing,lifecycle}` currently hard-exits blocked pending `ticket_1785984128_479155`. Remove the guard and prove all three green against a protocol-6 Hub. **Do not weaken or delete a lane to make it pass.**

### Registered dependencies (both closed)

| Depends on | Title | Status |
| --- | --- | --- |
| `ticket_1785984128_479155` | Workspaces: migrate package to authoritative session types | **closed** |
| `ticket_1785976581_841608` | TUI: remove package compatibility hub_version fixture on Hub client bump | **closed** (merged as PR #45; base of this branch) |

Verified outside the pipeline graph:

- Local `botster-workspaces` `main` at `3ec366a` (PR #15 merge) declares capability `session_type_managed_git_spawn`, spawn schema requires `session_type_id`, rejects `template_id`, and form select props use `name = "session_type_id"`. Lifecycle bindings exist (`lifecycle_binding`, current/ended/unavailable groups).
- This branch already pins Hub client / UI contract / hub-test-support to `8a60bd58841179f8b1fd4040d9362d18ea244230` (protocol version 6, conformance fixture revision 31 at that pin).
- `ticket_1785296184_677408` ("Workspaces: project canonical current and ended session lifecycle") is **closed**. There is **no remaining Workspaces-owned ticket** that owns the TUI lifecycle lane. **Lifecycle proof is this ticket’s obligation.** Implement must not re-defer it by reading stale README prose.
- Open sibling `ticket_1786038825_352271` owns a **contract-matrix** live failure; it is **not** this ticket’s Workspaces proof path and must not be cited as the live-evidence source after lanes are restored.

### Code reality on this base (`176384f`)

| Surface | Current state |
| --- | --- |
| `crates/botster-tui/src/app.rs` (~3963–3970) | `select_only_acceptance_value(..., "template_id", ...)` after spawn-form open; stage copy still says "single eligible template" |
| `select_only_acceptance_value` | Keys on **form field `name`**, not node id; requires **exactly one** rendered option and selects it via keyboard |
| `script/test-live-hub` workspaces case | Hard block (lines ~75–94) exits 1 before `resolve_workspaces_package`; retained body is intentionally unreachable until guard removal |
| `app::tests::blocked_workspaces_lanes_report_a_known_gap_for_every_profile` | **Two halves:** (1) lines ~13274–13302 waiver loop over three profiles — must die with the guard; (2) lines ~13304–13320 contract-matrix missing-fixture assertion — **non-waiver coverage that must be preserved** as its own test; the "guard must not leak into contract-matrix" assertion becomes vacuous after guard removal and should go with the waiver half |
| Live tests `installed_workspaces_spawn_driver_runs_through_apps_open` and `workspaces_live_acceptance_runs_against_real_package` | Fully implemented; doc comments still say blocked pending Workspaces migration; both use `skip_or_panic` so default `script/test` does **not** exercise them |
| Hermetic source-scan idiom | `tui_hub_boundary_uses_public_client_without_private_protocol_plumbing` (`app.rs` ~13050) uses `source_without_line_comments()` + required/forbidden tokens (`concat!` so the test does not match itself) |
| `README.md` known-gap + lifecycle | Documents triple-lane block; still claims Workspaces main lacks lifecycle tree and names closed `ticket_1785296184_677408`; cites contract-matrix as sole live-Hub evidence |
| Evidence example fixture | `workspaces-spawn-driver-v1.evidence.jsonl` still embeds `template_id` inside free-form `values` / `normalized_values` (schema does not fix those keys; default tests do not assert them either) |

### Runtime entry path (what "green" means)

Production / acceptance path that must prove the rename:

1. Isolated Hub installs and enables real `botster-workspaces` (capability scope now matches protocol-6 grants).
2. Installed TUI opens Workspaces, activates producer-authored `botster_workspaces.open_spawn`, fills target/branch, selects the **only** rendered `session_type_id` option, submits `botster_workspaces.spawn` through `InputRouter`.
3. Pushed session entities update membership without session-list polls.
4. `script/test-live-hub` wrapper greps for mode-specific completion markers (`installed-workspaces-driver: complete cases=3` or `workspaces-acceptance: profile=… ledger=complete`).

Evidence that code merely exists is insufficient; the three live lanes are the runtime proof. The hermetic source-scan is an additional **default-gate invariant** so a silent field-key regression cannot hide behind env-gated live tests.

## Scope

Surgical TUI-only restore of Workspaces acceptance after closed upstream migrations:

1. Rename the acceptance driver’s form field key from `template_id` to `session_type_id` at the production acceptance path in `app.rs`.
2. Align adjacent operator-facing diagnostic copy that still says "template" for that stage (same function call site).
3. Update the checked-in evidence **example** fixture so example `values` / `normalized_values` use `session_type_id` (cold-cut documentation; schema remains free-form objects).
4. Remove the `script/test-live-hub` workspaces hard-block so the three profiles execute the retained harness body.
5. **Split then delete** `blocked_workspaces_lanes_report_a_known_gap_for_every_profile`:
   - **Preserve** the contract-matrix half as a standalone hermetic test (e.g. `contract_matrix_mode_requires_its_fixture_env_var`) that runs `script/test-live-hub contract-matrix` without `BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE` and asserts stderr contains `BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE is required` and exit non-zero.
   - **Delete** only the three-profile waiver loop (ticket-id / known-gap assertions) and the now-vacuous "guard must not leak into contract-matrix" assertion.
   - **Do not** replace the waiver loop with a new ticket-id skip test.
6. Add a **hermetic source-scan unit test** (runs under default `script/test`) following the `source_without_line_comments()` idiom:
   - Required: acceptance spawn-form selector uses the current field token `session_type_id` (construct the token so the test body does not self-match wrongly if needed; prefer scanning for the exact call-site argument pattern the production path uses).
   - Forbidden: no `template_id` field-key survival in the acceptance spawn path **or** the checked-in evidence fixture (`include_str!` or read the fixture from `CARGO_MANIFEST_DIR`).
   - This converts the previous "manual grep" into an enforced invariant; live lanes remain the runtime proof that the field works end-to-end.
7. Strip "blocked pending …" doc comments on the live tests.
8. Rewrite README:
   - Drop "Known gap — all three Workspaces live-acceptance lanes are blocked" once lanes are restored (or document only residual **proven** failures, never re-applied skips).
   - **Drop** "intentionally lacks that lifecycle tree" (disproven at Workspaces `plugin.lua` lifecycle bindings).
   - **Drop** any claim that closed `ticket_1785296184_677408` still owns lifecycle proof; state that this ticket owns the three-lane restore including lifecycle.
   - **Stop** citing contract-matrix as the sole live-Hub evidence for the pinned revision; note that open `ticket_1786038825_352271` owns that lane’s current failure if mentioning it at all.
9. Prove all three live profiles green against:
   - Hub + session-worker binaries **built from commit** `8a60bd58841179f8b1fd4040d9362d18ea244230` (matches crate pins including hub-test-support; do **not** reuse ambient `target/` builds or `origin/main` tip),
   - a **clean** `botster-workspaces` checkout at post-migration `main` (or equivalent commit containing the capability + field rename), supplied only via `BOTSTER_WORKSPACES_PACKAGE_PATH`.
10. Commit this plan file on the run branch (Implement first commit) so the reviewed artifact is durable in git history.

## Non-scope

- No edits to `botster-workspaces`, `botster-hub`, `botster-hub-client`, `botster-core`, `botster-web`, or `botster-tui-kit` source in this run.
- No Hub pin bumps, protocol changes, or compatibility dual paths.
- No reintroduction of `template_id` as an alias, dual-field form, or soft fallback.
- No deletion, merge, or weakening of the three Workspaces live profiles.
- No broader acceptance refactors (scenario schema version bump, new driver modes, shared-hub browser smoke ownership).
- No product UX changes to interactive System details or non-Workspaces surfaces.
- No inventing fixture Workspaces packages when the real package is available; the harness already forbids sibling discovery.
- No fixing open `ticket_1786038825_352271` (contract-matrix failure) in this run.

## Repository ownership boundaries and cross-repo dependencies

| Concern | Owner | This run |
| --- | --- | --- |
| Form field name `session_type_id`, spawn tool schema, capability `session_type_managed_git_spawn`, lifecycle surface bindings | `botster-workspaces` | **Consume** closed `ticket_1785984128_479155`; pass clean package path |
| Protocol 6 / client pin / exact compatibility equality | `botster-tui` + Hub client pin | **Already landed** via closed `ticket_1785976581_841608` |
| Acceptance field key, live-hub guard, TUI live tests, hermetic field-key scan, README gap text | `botster-tui` | **Implement here** |
| Package enable grants, managed git spawn mechanics | Hub | Unchanged; enable must succeed once package scope matches; binaries built at pin for this run’s live proof |
| Shared-hub browser Workspaces smoke | `botster-web` | Out of scope |
| Lifecycle consumer proof via TUI live lane | **This ticket** (`botster-tui`) | **Not** deferred to closed `ticket_1785296184_677408` |

Cross-repo prerequisites are **already registered and closed**. Do not broaden this ticket into Workspaces or Hub code if a lane fails; report an exact finding with command evidence. Ticket policy: residual failure after both deps land is a finding, **not** a re-applied skip.

**Lifecycle re-deferral hazard (explicit):** README:499–503 currently tells operators that lifecycle fails closed pending Workspaces ticket `ticket_1785296184_677408`. That ticket is closed and Workspaces main already ships lifecycle bindings. An Implement agent that trusts README before this plan could re-defer lifecycle — **forbidden**. Lifecycle green (or an exact residual finding) is in scope for **this** ticket.

## Assumptions and unknowns

### Assumptions (explicit)

1. Closed dependency tickets mean the **merged** Workspaces and TUI protocol-6 work is what Implement will consume; local Projects checkouts used for planning probes are not authority for commits in this worktree’s `Cargo.lock` pins.
2. `select_only_acceptance_value` remains correct for the post-migration form: Workspaces still renders **exactly one** eligible session-type option in the installed-driver fixture (single repo session type `acceptance`, effective id qualified as `<target_id>/<id>` per [[hub qualifies effective session type ids as source name slash id]]). The helper selects the rendered option value; it does not hard-code bare `acceptance`.
3. No cold-cut alias is acceptable even if it would green a test faster.
4. **Live binary provenance is part of this ticket’s proof, not ambient operator luck.** Implement must build `botster-hub` and `botster-session-worker` from **`8a60bd58841179f8b1fd4040d9362d18ea244230`** (the same Hub revision this crate pins, including hub-test-support). Reusing undated trees under `Projects/*/target/{debug,release}` or building `origin/main` tip is **out of policy** because exact protocol equality makes a wrong binary look like a red lane, and Hub main has moved past the pin (including a conformance 31→32 bump). Absolute binary paths + source commits must appear in Implement gate evidence.
5. Artifact destination remains `docs/plans/` per this repository’s README/prior art, not a vault example path. The plan file must be **git-tracked** on the run branch before Implement claims completion.

### Unknowns (Implement must resolve with evidence, not guesses)

1. Whether `lifecycle` is green end-to-end on current Workspaces main against this TUI harness, or whether a producer/consumer mismatch remains (e.g. binding counts, keyboard remove path). If red: file a precise finding; do not re-block all three lanes for one profile without ticket authority; **do not** re-attribute failure to closed `ticket_1785296184_677408`.
2. Whether any live evidence assertions compare nested `values.template_id` (today they do not for the fixture shape check; re-verify after first green installed-driver run).
3. Whether Hub enable still fails for any non-scope reason (trust, path package, capability lock) once the known scope mismatch is gone — diagnose with pin-matched binaries first.

## Affected surfaces / files

Expected edit set (smallest surgical change):

| Path | Change |
| --- | --- |
| `crates/botster-tui/src/app.rs` | `"template_id"` → `"session_type_id"` at spawn-form selection; stage string "template" → "session type"; split/preserve contract-matrix hermetic assertion; delete waiver loop only; **add** field-key source-scan unit test; clean live-test doc comments |
| `script/test-live-hub` | Delete the workspaces hard-block (comments + `exit 1`) so `resolve_workspaces_package` and the retained profile body run |
| `crates/botster-tui/fixtures/workspaces-spawn-driver-v1.evidence.jsonl` | Example payload keys `template_id` → `session_type_id` |
| `README.md` | Replace known-gap block; drop closed-ticket lifecycle deferral; drop false "lacks lifecycle tree"; stop pointing operators solely at broken contract-matrix as live evidence |
| `docs/plans/tui-restore-workspaces-acceptance-after-session-types-plan.md` | This plan (must be committed on the run branch) |

No Cargo pin changes expected.

## Implementation sequence (for Implement)

1. **First commit includes this plan file** so the reviewed artifact is durable (`git add docs/plans/tui-restore-workspaces-acceptance-after-session-types-plan.md` with the code, or as the opening commit).
2. Confirm base is `main` containing the protocol-6 pin; confirm clean Workspaces package path at post-migration commit (capability `session_type_managed_git_spawn`, form field `session_type_id`).
3. **Build live binaries from the pin** (example shape; exact build commands may match Hub repo docs):
   ```sh
   HUB_PIN=8a60bd58841179f8b1fd4040d9362d18ea244230
   # checkout/build botster-hub + botster-session-worker at $HUB_PIN into a fresh target dir
   # export absolute BOTSTER_HUB_BIN and BOTSTER_SESSION_WORKER_BIN
   ```
   Record: Hub commit, session-worker commit (same pin if co-located), absolute paths, build command.
4. Apply field rename + diagnostic string update in `run_acceptance_case` spawn-form stage.
5. Update evidence example fixture nested keys.
6. Remove script workspaces guard.
7. Split `blocked_workspaces_lanes_report_a_known_gap_for_every_profile`: keep contract-matrix fixture-env test; delete waiver loop + vacuous guard-leak check.
8. Add hermetic field-key source-scan test (required `session_type_id`, forbidden `template_id` in acceptance path + evidence fixture).
9. Strip blocked doc comments on live tests.
10. Run unit suite (`script/test`) — must exercise the new source-scan and preserved contract-matrix env test.
11. **Pre-flight before interpreting any Workspaces lane result:** connect the pin-built Hub and assert protocol **6** and conformance fixture revision **≥ 31** (or the exact descriptor this pin reports). A pre-flight failure is an **environment/provenance** defect, not a Workspaces lane finding.
12. Run the three live lanes with pin-built binaries:
   ```sh
   BOTSTER_HUB_BIN=/abs/path/to/hub-at-8a60bd58 \
   BOTSTER_SESSION_WORKER_BIN=/abs/path/to/session-worker-at-8a60bd58 \
   BOTSTER_WORKSPACES_PACKAGE_PATH=/clean/botster-workspaces-at-post-migration \
   CARGO_TARGET_DIR=/tmp/botster-tui-ws-<profile> \
     script/test-live-hub workspaces <installed-driver|plumbing|lifecycle>
   ```
13. Rewrite README from **fresh** lane results (and the closed-ticket / lifecycle ownership rules above), not from memory.
14. Attach Implement gate evidence: unit commands, source-scan test name, binary commits + paths, pre-flight identity, three lane transcripts with completion markers.

## Risks

| Risk | Mitigation |
| --- | --- |
| Re-applying a skip when lifecycle alone fails | Ticket forbids; report finding; keep plumbing + installed-driver green |
| Re-deferring lifecycle to closed Workspaces ticket | Plan + README rewrite make this ticket the sole owner |
| Selecting bare session type id after rename | Helper uses rendered options only; fixture has one option; fail closed on 0/N options |
| Cold-cut only proven by env-gated live tests | Hermetic source-scan under `script/test` pins field key |
| Fixture example still teaching `template_id` | Update evidence JSONL; source-scan forbids it |
| Deleting waiver test also drops contract-matrix coverage | Split: preserve fixture-env assertion as its own test |
| Stale / main-tip Hub binary misread as lane failure | Build from pin `8a60bd58…`; pre-flight protocol/conformance; record provenance |
| Wrong Workspaces checkout (pre-migration) | Fail at EnablePackage; validate package path is post-`3ec366a` / equivalent |
| Live binaries missing | Fail closed via `BOTSTER_TUI_REQUIRE_HUB_TEST=1`; do not soft-skip |
| Over-broad cleanup while editing `app.rs` | Touch only rename, split test, source-scan, comments |
| Plan file lost from worktree | Commit on run branch in Implement |

## Acceptance checks / tests

### Mandatory unit / hermetic (default `script/test`)

- `script/test` (or `cargo test -p botster-tui --locked` as documented) green.
- **New** source-scan test green: acceptance spawn-form selector uses `session_type_id`; no `template_id` field key remains in the acceptance path or `workspaces-spawn-driver-v1.evidence.jsonl`.
- Waiver loop of `blocked_workspaces_lanes_report_a_known_gap_for_every_profile` **gone**.
- **Preserved** contract-matrix fixture-env assertion green as its own hermetic test (`contract_matrix_mode_requires_its_fixture_env_var` or equivalent name).
- Acceptance module still validates evidence fixture against schema.

### Mandatory live (runtime path proof)

**Binary provenance (required evidence, not optional):**

| Item | Required value |
| --- | --- |
| Hub source commit | `8a60bd58841179f8b1fd4040d9362d18ea244230` |
| Session-worker source commit | same pin (co-built from that Hub revision) |
| `BOTSTER_HUB_BIN` / `BOTSTER_SESSION_WORKER_BIN` | absolute paths to those builds |
| Pre-flight | connected Hub reports protocol 6 and conformance ≥ 31 before any lane is scored |

All three must exit 0 with their completion markers:

| Profile | Command | Success signal |
| --- | --- | --- |
| installed-driver | `script/test-live-hub workspaces installed-driver` | `test app::tests::installed_workspaces_spawn_driver_runs_through_apps_open ... ok` and `installed-workspaces-driver: complete cases=3` |
| plumbing | `script/test-live-hub workspaces plumbing` | `workspaces-acceptance: profile=Plumbing ledger=complete` |
| lifecycle | `script/test-live-hub workspaces lifecycle` | `workspaces-acceptance: profile=Lifecycle ledger=complete` |

Shared requirements:

- `BOTSTER_WORKSPACES_PACKAGE_PATH` points at a clean post-migration Workspaces package (manifest name `botster-workspaces`, capability scope `session_type_managed_git_spawn`).
- EnablePackage succeeds (proves the former hard-deny root cause is gone).
- Spawn path proves `session_type_id` field selection (driver fails closed if the field name is wrong; source-scan makes silent revert fail under unit test).

### Downstream proof charter

Per [[botster-tui-playbook]] and Workspaces consumer docs: lifecycle lane is the TUI-side proof that generic entity binding + keyboard actions consume producer lifecycle trees.

**Ownership statement (Plan Review finding `finding_1786060987_795131`):**

- Producer lifecycle work was owned by **closed** `ticket_1785296184_677408` and has landed on Workspaces main.
- **This ticket owns completing the TUI lifecycle consumer gate.** There is no remaining downstream Workspaces ticket to re-defer to.
- Completing (or filing an exact residual finding for) `script/test-live-hub workspaces lifecycle` is in scope for Implement of `ticket_1786036326_597046`.

### Plan gate itself

- This document is the reviewable artifact under `docs/plans/`.
- Implement must **commit** it on the run branch (finding `finding_1786060987_804049`); Plan stage registers it as an artifact URI even when still untracked — that is insufficient for history durability.
- Vault checklist items record exact note titles, convention conflicts (`none` expected), and capture decision.

## Vault gaps worth capturing

1. **Consumer field-key cold cuts lag package renames.** When a package renames a form `name` / tool argument, every first-party acceptance driver that keys on field name is a hard break; the TUI and Web both needed follow-up tickets. Candidate capture: "package form field renames require first-party acceptance driver tickets before un-skip."
2. **Env-gated live proof is not a substitute for a default-gate invariant on cold-cut field keys.** Sibling tickets `ticket_1786038825_352271` and `ticket_1786042828_142991` already show harness/UI regressions hiding behind `skip_or_panic`. Candidate capture: "cold-cut consumer field keys need a hermetic source-scan under script/test."
3. **Waiver hermetic tests must die with the skip — but only the waiver half.** Tests that co-locate unrelated env validation with a skip assertion silently lose coverage if deleted wholesale.
4. **README gap sections can outlive the producer fix and re-defer closed work.** Treat "known gap" prose as evidence that needs revalidation when dependency tickets close ([[waiver premises must be revalidated when blocking dependencies land]]).
5. **Live Hub binary provenance must match the crate pin under exact protocol equality.** Ambient `target/` binaries and `origin/main` tip are misattribution hazards after pin-based protocol equality.

If Implement discovers no new durable knowledge beyond the above, record capture as nil with reason.

## Botster layers touched

- **TUI application acceptance driver, hermetic acceptance invariants, and live-hub wrapper only.**
- Not: Lua plugins, Hub daemon source edits, session worker source edits, SPA, Rails relay, MCP product surface (except consuming Workspaces MCP/UI actions already shipped). Building pin-matched Hub binaries for acceptance is **environment setup**, not a Hub code change in this run.

## Worktree / target assumptions

- Implement and verify only in the pipeline worktree for `tgt_c3d470bab78549df920a41e8fb0e58d8`.
- Workspaces package path is an **external** input path, not a second edit worktree for this ticket.
- Hub source checkout used only to **build** pin `8a60bd58…` binaries; do not patch Hub in this ticket.

## Pipeline gates and artifacts

| Stage | Gate / artifact |
| --- | --- |
| Plan | `botster_stack_plan_gate` + this plan URI + vault checklist |
| Plan review | Architecture fit, cold-cut correctness, no re-skip, ownership boundaries, finding disposition |
| Implement | Code + hermetic scan + preserved contract-matrix env test + pin-built binary provenance + three live-lane transcripts + **committed plan file** |
| Review / Verify | Runtime evidence + default-gate scan, not mere presence of un-skipped script lines |

## Required docs updates

- `README.md` known-gap / lifecycle / live-evidence sections (required; explicit ownership rewrite).
- This plan under `docs/plans/` (required; must be git-committed on the run branch).
- No plugin README changes (TUI is not the Workspaces package).
