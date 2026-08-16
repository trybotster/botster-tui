# Implement report: IsolatedHub session-types entity-store repair

## Target repository and target_id

| Field | Value |
| --- | --- |
| Target repository | `botster-tui` (`git@github.com:trybotster/botster-tui.git`) |
| Target id | `tgt_c3d470bab78549df920a41e8fb0e58d8` |
| Ticket | `ticket_1786912267_788084` |
| Run | `run_1786914336_386503` |
| Run step | `run_step_1786916556_546084` (`botster_stack_implement`) |
| Base | TUI `8b4df69e27b65071aa94b7e5d6b31d0990c041fc` |
| Runtime-teardown class | **Does not apply** |
| Merge policy | `direct` (no PR link required) |

Authoritative routing comes from the ticket `target_id`. It matches the approved plan.

## Repository playbook and other playbooks/notes applied

### Role / charter

1. [[implementer-playbook]]
2. [[botster-implementer-playbook]]
3. [[botster-tui-playbook]] (edit charter)
4. [[project-pipelines-playbook]] (workflow: one-writer, artifacts, committed-work gate, reuse existing vault checklist)

### Targeted atomic notes

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
- [[implementation artifacts must match actual git state]]
- [[implement gate must verify committed work and pr link before review]]
- [[implementation steps must persist report artifacts for review]]
- [[pipeline vault checklists must cite exact resolvable note titles]]
- [[pipeline artifacts should use path neutral worktree references]]

Not used as an edit charter: [[botster-hub-playbook]], [[botster runtime teardown lenses]].

### Botster layers changed

- **botster-tui** IsolatedHub session-types live harness and README pin text
- **Not changed:** Hub owner-loop, production Create/Update/Delete, Web, Ghostty, Workspaces, kit

## Files changed

| Path | Change |
| --- | --- |
| `crates/botster-tui/src/app.rs` | `#[cfg(test)]` `refresh_session_type_subscription_for_test` plus exact-id wait; IsolatedHub session-types waits call it after Create/accessory/service/launch-type ensure/Delete. `submit_session_type_form` unchanged. Hermetic source-scan and stop-before-start tests. |
| `README.md` | Session-types pin text matches Foundation Hub `c72712e` / Core `fc541a59`. IsolatedHub refresh is a test subscribe snapshot, not a longer poll. |
| `docs/plans/tui-isolatedhub-session-types-entity-store-repair-plan.md` | Approved plan (pass 3). |
| `docs/reports/tui-isolatedhub-session-types-entity-store-repair-implement.md` | This report. |

## Ownership boundaries preserved

- Eligibility, catalog, and owner-loop upserts remain Hub-owned.
- TUI production form submit does not refresh or invalidate the `session_type` subscription.
- The repair uses the public `SubscribeEntities` path already owned by `start_session_type_subscription`.
- No Hub, Web, kit, or Workspaces source edits.

## Cross-repo dependencies or separately routed work

| Item | Status |
| --- | --- |
| Parent `ticket_1786661010_115885` Option A consumer | Still blocked on this TUI profile; launch half kept |
| Hub `tgt_7e208a0c76a44980a83b63af976b1f22` | No new Hub ticket. Subscribe snapshot after Create contained the exact id. |
| Web / Ghostty shared / Workspaces / contract-matrix | Out of scope |

## Deviations from plan

None. README conformance copy in the session-types live paragraph now says fail-closed below 33, matching the live profile and plan. That is documentation alignment required by the pin paragraph, not a behavior change.

## Tests and downstream proof

Hermetic, from the ticket worktree:

```sh
script/fmt
script/test
script/clippy
git diff --check
```

| Gate | Result |
| --- | --- |
| `script/fmt` | pass |
| `script/test` | **252** unit + 1 package-manifest = green, including `refresh_session_type_subscription_for_test_stops_before_start` and `submit_session_type_form_success_path_does_not_refresh_subscribe` |
| `script/clippy` | pass (strict `-D warnings`) |
| `git diff --check` | pass |
| `script/test-live-hub session-types` | pass; `session-types-live: complete conformance=43 features_has_session_type=true cases=agent,accessory,service,authoring,launch,delete,reconnect` |

Kept no-`target_id`-equality spawn-picker scan.

### Live binary provenance

| Binary | Provenance |
| --- | --- |
| `botster-hub` | Clean Hub checkout `c72712e2606b8abe77e1b91c2a736791036fadd8`, `target/debug/botster-hub` |
| `botster-session-worker` | Same Hub checkout lockfile Core `fc541a59338d0591ba4fb3fa522a030d212d26d0`, package `botster-core-daemon`, `target/debug/botster-session-worker` |

Handshake: conformance **43** (>= 33) and `session_type_entity_subscriptions` present.

### Production path proven

Production Create / Update / Delete is unchanged. The IsolatedHub profile is the required proof that the store holds the created type:

1. After each in-scope mutation, the harness stops the old pump (`invalidate_session_type_generation`, fail on 750 ms timeout).
2. It starts a new production `subscribe_entities` / `start_session_type_subscription`.
3. It `poll_hub`s for at most 2 s until the exact `device/{id}` is present, or for Delete until it is absent and `has_snapshot` is true.
4. Option A remains: keyboard list-for-target + spawn `target_id = T`.
5. Reconnect later still restores an exact remaining id on a held subscription.

A longer idle poll without that subscribe trigger is not the acceptance path.

## Unverified behavior or residual risk

- Authoring Update in the live profile still does not refresh subscribe. The plan listed Create, accessory, service, launch-type ensure, and Delete only. Update is proven through `ShowSessionTypeDefinition`, not store-label convergence.
- If a future Hub revision sends an empty subscribe snapshot after a committed Create, this harness fails closed inside 2 s. That would need a Hub ticket against `tgt_7e208a0c76a44980a83b63af976b1f22`.
- Shared Ghostty, Workspaces, and contract-matrix lanes were not rerun.

## Missing vault guidance discovered

IsolatedHub `session_type` upserts still arrive on the owner-loop, while subscribe snapshots are request-path. Live first-party tests should refresh subscribe after mutation rather than wait a slice cycle. That gap was already named in the plan. No vault capture this visit.

## Convention conflicts

None.

## Assumptions

1. Subscribe-path snapshot after Create contains `device/{authored_id}` because Create already committed. Live proof on this visit confirmed that.
2. `invalidate` then `start` leaves exactly one live `session_type` pump. The helper asserts a new `subscription_id` and a live pump.
3. Runtime-teardown lenses do not apply.
