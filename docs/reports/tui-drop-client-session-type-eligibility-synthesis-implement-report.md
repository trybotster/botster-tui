# Implement report: TUI drop client session-type eligibility synthesis

## Target

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Ticket | `ticket_1786387865_677482` |
| Run | `run_1786394077_353311` |
| Runtime-teardown class | **Does not apply** |

## Repository playbook and other guidance applied

### Role / charter

1. [[implementer-playbook]]
2. [[botster-implementer-playbook]]
3. [[botster-tui-playbook]] (edit charter)
4. [[project-pipelines-playbook]] (workflow: one-writer, artifacts, gates, commit/PR handoff)

### Targeted atomic notes

- [[tui and browser are equal clients]]
- [[tui client attach uses hub protocol not session protocol]]
- [[device hub owns admitted spawn targets not ambient repo cwd]]
- [[hub qualifies effective session type ids as source name slash id]]
- [[incomplete repo local session types drop the hub client connection]]
- [[adding a hub client feature constant is a three site change]] (no new feature constant; list-for-target is additive at conformance 33)
- [[web-session-creation-must-be-target-first]]
- [[botster-hub-client-playbook]] (consume published DTOs only)
- [[external client hub tests use subprocess spawned hub test support]]
- [[tui error dedup tests must drive real input handlers]]
- [[test script required for rust tests not cargo test]]
- [[pipeline run worktrees allow only one active writer]]

### Botster layers changed

- **botster-tui** product presentation, target-first spawn dialog, pin consume, hermetic + live tests, README
- **Not changed:** Hub eligibility, Web, core taxonomy, kit code

## Files changed

| Path | Change |
| --- | --- |
| `crates/botster-tui/Cargo.toml` | Hub pin → `cb93df53…` (hub-client, ui-contract, hub-test-support) |
| `Cargo.toml` | Workspace `[patch]` to unify `botster-ui-contract` |
| `Cargo.lock` | Pin + dual core + single path-patched ui-contract |
| `third_party/botster-ui-contract/**` | Vendored byte-identical contract for path patch (kit pin skew) |
| `crates/botster-tui/src/app.rs` | Admitted-only launch targets; sync `ListSessionTypesForTarget`; flow-local rows; spawn `target_id=T`; conformance 33; hermetic + live proofs |
| `README.md` | Session types product launch cold-cut + pins + live gate env |
| `docs/plans/tui-drop-client-session-type-eligibility-synthesis-plan.md` | Approved plan (artifact) |
| `docs/reports/tui-drop-client-session-type-eligibility-synthesis-implement-report.md` | This report |

## Ownership boundaries preserved

- Eligibility / list / spawn acceptance remain Hub-owned (parent ticket closed).
- TUI only consumes `DaemonRequest::ListSessionTypesForTarget` and renders Hub rows.
- No freeform product `DaemonRequest::Spawn`.
- Management/catalog entity subscription path unchanged.
- Kit code not edited; contract unify is a consumer-side path patch only.

## Cross-repo dependencies / separately routed work

| Item | Status |
| --- | --- |
| Hub eligibility `ticket_1786387816_590636` | Closed; consumed pin `cb93df53…` |
| Web sibling | Out of scope |
| `ticket_1786038825_352271` (app.rs contending) | Open; separable; one-writer preflight only |
| `ticket_1786071999_889350` (colon path cargo) | Open non-blocking; workaround `CARGO_TARGET_DIR` |
| Kit pin still at `902650df…` (ui-contract older rev) | Path-patched in this repo; optional future kit mechanical repin would remove vendor |

## Deviations from plan

1. **ui-contract path patch / third_party vendor.** Plan assumed a single Hub-rev ui-contract after pin. Kit still pins ui-contract at `302190ec…`; Cargo keys git deps by rev, so byte-identical crates at two revs are two types. Same-source `[patch]` is rejected by Cargo. Implemented a path patch via `third_party/botster-ui-contract` (byte-identical with Hub pin). Documented in README. Not a product-behavior deviation.
2. **No other scope expansion.** No async loading UX; no Web/Hub edits.

## Review pass disposition (Implement visit 2)

| Finding | Severity | Disposition |
| --- | --- | --- |
| `finding_1786396642_261695` — Required keyboard launch path has no production-input proof | high | **Fixed.** Hermetic Tab+Enter InputRouter test + live profile keyboard path through production router; report claims updated. |

## Tests and downstream proof

Environment for all cargo-backed gates:

```sh
export CARGO_TARGET_DIR="/tmp/botster-tui-cargo-tgt-session-types"
```

| Gate | Result |
| --- | --- |
| `script/fmt` | pass |
| `script/test` | **174** unit + 1 package-manifest = green (includes keyboard InputRouter spawn proof) |
| `script/clippy` | pass (strict) |
| `script/test-live-hub session-types` | pass; log line `session-types-live: complete conformance=33 … launch …`; launch step uses production **Tab+Enter** InputRouter path |

### Live binary provenance

| Binary | Path | Build |
| --- | --- | --- |
| botster-hub | `/tmp/botster-hub-build-cb93df5/debug/botster-hub` | Hub source `cb93df53d66fead323973b5233d4589562cf57b1` |
| botster-session-worker | `/tmp/botster-hub-build-cb93df5/debug/botster-session-worker` | Same pin; `cargo build --locked -p botster-core --bin botster-session-worker` |

### Production path proven

Toolbar `botster.tui.spawn` → `begin_target_first_spawn` → pick admitted `T` → sync `ListSessionTypesForTarget` → pick listed Global → `SpawnSessionType` with `target_id = T`.

Keyboard proof (Review finding `finding_1786396642_261695`):

1. Hermetic: `product_spawn_list_and_pick_are_reachable_through_keyboard_input_router` — real-frame `InputRouter` **Tab** focus + **Enter** activate for both picker steps; asserts list + spawn requests.
2. Live pin-matched profile: same Tab/Enter helpers through production router for admitted `T` and Hub-listed Global (no direct `spawn_pick_*` for the launch step).

### Dual core lock

- Direct: `16bf08f29ec723c70c290cf995745ccbf79d4f05`
- Via hub-test-support `branch=main`: `ff115694caf61e435bfb3d7ffcc5a6459689c8d9`

### Commit hygiene

- `.gitignore` restored/matches mainline (ignores `/target/`, `/.env`, `/.env.*`, `/mise.local.toml`)
- `.env` and `mise.local.toml` remain untracked and uncommitted

## Unverified behavior / residual risk

- Workspaces live lanes (`plumbing` / `lifecycle` / `installed-driver`) not re-run this visit; pin-matched package path may still be required for those env-gated profiles.
- Kit remains on older ui-contract pin; vendor path patch is a durable workaround until a mechanical kit repin lands.
- Concurrent open ticket `ticket_1786038825_352271` may rebase-contend on `app.rs`.

## Missing vault guidance discovered

1. First-party clients must not invent launch targets from session-type entity `target_id`; spawn pickers call Hub `ListSessionTypesForTarget` for admitted `T`. (Plan vault gap #1.)
2. Spawn-point list returns available winners only — unavailable diagnostics stay on management catalog. (Plan vault gap #2.)
3. Pipeline worktrees with `:` in the path require colon-free `CARGO_TARGET_DIR` for **all** cargo-backed script gates (until `ticket_1786071999_889350`). (Plan vault gap #3.)
4. (Implement discovery) Byte-identical `botster-ui-contract` at two Hub revs is still two Cargo types; consumer path-patch or lockstep kit pin is required. Prior plan vault gap already noted this class of defect.

If already captured from Hub parent, treat #1–#2 as reinforcement; #4 is the actionable consumer gotcha from this implement.
