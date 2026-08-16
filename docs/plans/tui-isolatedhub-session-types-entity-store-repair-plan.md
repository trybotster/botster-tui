# TUI: IsolatedHub session-types live profile misses created agent type

## Plan revision

| Field | Value |
| --- | --- |
| Pass | **3** — Plan after Plan Review `changes_required` (`review_1786916321_171566`) |
| Prior artifacts | `artifact_1786914938_528848` (pass 1), `artifact_1786916216_980142` (pass 2) |
| This run | `run_1786914336_386503` / `run_step_1786916333_910403` |
| Open finding | `finding_1786916321_849581` (high) |
| Runtime-teardown class | **Does not apply** |

### Finding disposition

| Finding | Severity | Disposition |
| --- | --- | --- |
| `finding_1786915719_225027` | high | **Resolved in pass 2.** Classification stands: held subscription delivered the exact row at 4528 ms. Not a production hydration defect. |
| `finding_1786915719_572510` | medium | **Resolved in pass 2.** Production resubscribe remains off the path. |
| `finding_1786916321_849581` — eight-second wait still depends on an owner-loop timing window | high | **Adopt.** Pass 2's 8 s poll still waits through `ENTITY_RECONCILIATION_INTERVAL` × nine slices. That conflicts with [[live acceptance tests must not depend on a loop tick window]]. **Selected branch: harness-only `SubscribeEntities` snapshot after each mutation.** Do not lengthen the 4 s poll. Do not add production resubscribe. Do not copy the Create catalog into the store. |

### Classification (unchanged, still binding)

Live measure on Hub `c72712e` / worker `fc541a59`:

| Observation | Value |
| --- | --- |
| Exact created id | `device/live-agent-666000` |
| First appearance on the **held** subscription | 4528 ms, iter 81, `seq=1`, no error |
| Producer | IsolatedHub `SubscriberDelivery` on a 500 ms nine-slice owner loop |

That measure explains the 4 s miss. It does **not** authorize waiting through that cadence.

### Product decision (pass 3)

The live profile must prove the created type is in the TUI entity store. It does **not** need to prove that the original held subscription received the owner-loop upsert. Reconnect later in the same profile still proves snapshot restore on a held subscription.

Public in-scope trigger: Hub `SubscribeEntities { entity_type: "session_type", ... }` sends the family **Snapshot on the subscribe request path** (`register_entity_subscription` in Hub `c72712e`). TUI already uses that path through `subscribe_entities` / `start_session_type_subscription`. That is a real public owner boundary, not a list-refresh and not a Create-catalog write.

Human question: **not asked.** Review named this trigger. It exists on the pinned Hub client. Held-subscription-across-Create is not the ticket proof.

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`git@github.com:trybotster/botster-tui.git`) |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Base | TUI HEAD `8b4df69e27b65071aa94b7e5d6b31d0990c041fc` |

Authoritative routing comes from the ticket `target_id`.

## Repository playbook loaded

- [[botster-tui-playbook]]

## Other role / surface playbooks and atomic notes loaded

1. [[planner-playbook]]
2. [[botster-planner-playbook]]
3. [[botster-tui-playbook]]
4. [[botster pipeline needs continuous product owner between agent steps]]

Targeted:

- [[live acceptance tests must not depend on a loop tick window]]
- [[acceptance readiness requires the exact expected entity not any authoritative snapshot]]
- [[botster hub client state sync is entity frame only]]
- [[botster client subscriptions should not hydrate global state]]
- [[botster entity snapshots are authoritative reconnect baselines]]
- [[hub qualifies effective session type ids as source name slash id]]
- [[host-plane session_type deltas use per-subscriber contiguous snapshot_seq]]
- [[external client hub tests use subprocess spawned hub test support]]
- [[live hub proof records distinct hub and locked core binary provenance]]
- [[tui client attach uses hub protocol not session protocol]]
- [[PTY integration tests poll for readiness not fixed sleeps]]

Not loaded: [[botster runtime teardown lenses]], [[botster-hub-playbook]] as edit charter, [[project-pipelines-playbook]].

## Context loaded

Ticket: IsolatedHub `script/test-live-hub session-types` misses `device/{id}` in the entity store after Create. Parent `ticket_1786661010_115885` still needs Option A from this profile. Consumer-of-eligibility overlay still applies (list-for-target + spawn Option A, hub `c72712e` / test-support ≥ 0.1.26 / conf ≥ 33, live proof, no `target_id` equality filter).

`start_session_type_subscription` does **not** stop an existing pump. A harness restart must call `invalidate_session_type_generation` first (`SESSION_ENTITY_STOP_TIMEOUT` is 750 ms).

## Scope

### In scope

1. Add a `#[cfg(test)]` helper, for example `refresh_session_type_subscription_for_test`, used only by the IsolatedHub session-types live profile.
2. After each successful session-type **mutation** in that profile (Create agent, accessory, service, launch-type ensure, Delete), the helper must:
   - stop the current pump through `invalidate_session_type_generation` (fail if stop times out)
   - start a new subscribe through `start_session_type_subscription` (the production `subscribe_entities` path)
   - `poll_hub` until the exact expected id is present (or, for Delete, absent **and** `has_snapshot`) or a **short transport deadline** (2 s is enough for a request-path snapshot)
   - on timeout, panic with keys, `has_snapshot`, `snapshot_seq`, `subscription_id`, `app.error`, `session_type_subscription_error`
3. Keep exact-id readiness. An empty new snapshot is not success for Create.
4. Keep Option A keyboard list-for-target + spawn `target_id = T`.
5. Hermetic: source-scan that `submit_session_type_form` does not call the refresh helper or `invalidate_session_type_generation` on the success path. Helper unit test: stop-before-start; a second start without stop is not the helper contract.
6. Live `script/test-live-hub session-types` against Hub `c72712e` / worker `fc541a59`. Require `session-types-live: complete` and binary provenance.
7. README: pin paragraph matches Foundation `c72712e` / `fc541a59`. Describe the IsolatedHub refresh as a test subscribe snapshot, not a longer poll.

### Non-scope

- Production Create / Update / Delete resubscribe
- Lengthening `0..80` / 8 s owner-loop polls as the repair
- Writing `DaemonResponse.session_types` into `session_type_entities`
- Hub owner-loop or interval changes
- Web, Ghostty shared, Workspaces, contract-matrix
- Changing handshake floors

## Repository ownership boundaries and cross-repo dependencies

| Layer | Owns | Does not own |
| --- | --- | --- |
| **botster-tui** | Test-only subscribe refresh, exact-id wait, Option A profile | Eligibility, catalog, owner-loop upserts |
| **botster-hub** | Subscribe-path snapshot, 500 ms delivery | This TUI harness |

No new Hub dependency. This ticket remains a blocker of `ticket_1786661010_115885`.

## Assumptions and unknowns

1. **Decided:** The 4 s miss is owner-loop cadence, not a missing Create.
2. **Decided:** The live profile may replace the held subscription after mutation. Ticket proof is store presence via entity frames, not upsert-on-the-original-socket.
3. **Assumption:** Subscribe-path snapshot after Create contains `device/{authored_id}` because Create already committed. Pass-2 measure showed Hub lists that id by 4.53 s; subscribe snapshot reads the same catalog immediately.
4. **Assumption:** `invalidate` then `start` leaves exactly one live `session_type` pump. Implement must assert the old pump stopped (`session_type_subscription` is the new pump).
5. **Residual:** If subscribe snapshot after Create is empty of the new id, that is Hub catalog visibility on the request path. Stop and register a Hub ticket against `tgt_7e208a0c76a44980a83b63af976b1f22`. Do not fall back to an 8 s poll.

## Affected surfaces / files

| Path | Change |
| --- | --- |
| `crates/botster-tui/src/app.rs` | `#[cfg(test)]` refresh helper; IsolatedHub session-types waits call it after mutations. `submit_session_type_form` unchanged. |
| `README.md` | Pin text + harness refresh note. |
| `docs/plans/tui-isolatedhub-session-types-entity-store-repair-plan.md` | This plan. |
| `docs/reports/tui-isolatedhub-session-types-entity-store-repair-implement.md` | Implement report. |

Production entry point is unchanged. The IsolatedHub profile is the required proof that the store holds the created type.

## Risks

| Risk | Mitigation |
| --- | --- |
| Restart leaks the old reader | Helper must stop-before-start and fail on stop timeout. |
| Stale frames from the old `subscription_id` | `begin_generation` on the new id; `apply` rejects other ids. |
| Treating this as production resubscribe later | Source-scan + plan ledger: production form submit does not refresh. |
| Empty subscribe snapshot | Fail closed. Do not wait 8 s. Hub ticket if catalog is empty. |
| Option A dropped | Keep the launch half. |

## Acceptance checks / tests

### Hermetic

```sh
script/fmt
script/test
script/clippy
git diff --check
```

- Existing session-type reducer tests stay.
- Source-scan: `submit_session_type_form` success path does not refresh subscribe.
- Keep no-`target_id`-equality spawn-picker scan.

### Live (required)

```sh
export BOTSTER_HUB_BIN=/path/to/hub-c72712e/botster-hub
export BOTSTER_SESSION_WORKER_BIN=/path/to/lock-core-fc541a59/botster-session-worker
script/test-live-hub session-types
```

Must observe exact test `ok`, `session-types-live: complete`, created type in the store before later cases, Option A list-for-target + spawn `target_id=T`, handshake ≥ 33 + `session_type_entity_subscriptions`, recorded Hub `c72712e` and worker Core `fc541a59`.

A longer idle poll without the subscribe trigger is not acceptance.

## Vault gaps

1. IsolatedHub session-type upserts are owner-loop; subscribe snapshots are request-path. Live first-party tests should refresh subscribe after mutation rather than wait a slice cycle. Capture after Implement if Review wants it durable.
2. No capture this visit.

## Product decision ledger

| Item | Decision |
| --- | --- |
| Repository | `botster-tui` / `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Defect class | Live wait used owner-loop cadence |
| Repair | Harness-only `SubscribeEntities` snapshot after mutation |
| Production resubscribe | **No** |
| 8 s / longer poll | **No** |
| Store channel | Entity snapshot/upsert/remove only |
| Held-subscription-across-Create | Not required |
| Option A | Keep |
| Hub edits | Out of scope |
| Ask-human | Only if subscribe snapshot after Create lacks the exact id |

## Convention conflicts

None. The selected trigger follows [[live acceptance tests must not depend on a loop tick window]] (test-only request through a real owner boundary), [[botster hub client state sync is entity frame only]], and [[acceptance readiness requires the exact expected entity not any authoritative snapshot]].

## Worktree hygiene

| Item | Status |
| --- | --- |
| Tracked `.gitignore` | Present, matches HEAD |
| Colon in worktree path | Absent |
| Vault checklist | Reuse `checklist_1786914491_625075`. Do not create another. |
