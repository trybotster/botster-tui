---
ticket: ticket_1785192707_900922
title: TUI route generic plugin UI actions through the Hub contract
step: botster_stack_plan
run: run_1785260754_539439
---

# TUI Generic Plugin UI Action Routing Plan

## Target and context loaded

- Target repository: `trybotster/botster-tui`.
- Target ID: `tgt_c3d470bab78549df920a41e8fb0e58d8`.
- Target resolution: `project_pipelines_current_context` supplied the opaque target ID; `list_spawn_targets` mapped it to `botster-tui`; this ticket worktree's `origin` is `git@github.com:trybotster/botster-tui.git`.
- Repository charter: [[botster-tui-playbook]].
- Role and architecture context: [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], and [[spa-patterns]].
- Required TUI surface context: [[tui and browser are equal clients]], [[botster tui consumes tui kit through a thin app policy adapter]], [[botster-tui-kit-playbook]], [[tui client attach uses hub protocol not session protocol]], [[tui and socket terminal streams use clientworker transport adapters]], [[botster tui uinode event routing captures hit regions during draw]], [[tui error dedup tests must drive real input handlers]], [[botster-runtime-reviewer-playbook]], and [[botster-runtime-verifier-playbook]].
- Targeted action/presentation context: [[plugin authored tui surfaces dispatch via action props not node id literals]], [[plugin surface actions route by explicit metadata]], [[botster plugin modal state belongs in client-local presentation state]], [[ui presentation operations are authored by accepted action results]], [[conformance helpers must dispatch the action id read from the rendered node]], [[botster core contract surface needs consumer proof]], [[botster web drops core uiaction payload and ignores interaction props]], and [[core uiaction has no label so clients must not synthesize one]].
- Planning/workflow context: [[prefer framework and library components over custom solutions]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], [[botster pipeline needs continuous product owner between agent steps]], [[plan agents must author vault context as wikilinks not home paths]], [[vault example paths are not repository placement conventions]], and [[project-pipelines-playbook]]. Project Pipelines is workflow context only; its package/plugin code is not an implementation surface.
- Run context: plan step `botster_stack_plan`, base `main`, one required attestation gate, a closed dependency on `ticket_1785192700_939910`, and an open blocking dependency on `ticket_1785261259_330503`. Plan Review independently validated the routing and contract seam, then returned the run for the product decisions recorded below.
- Repository context: `README.md` defines the TUI as a hub client over shared contract and kit APIs; repository gates are `script/fmt`, `script/test`, `script/clippy`, the smoke binary, and `script/test-live-hub`; current plan artifacts live under `docs/plans`.
- Production path: `run_loop` draws `TuiApp::surface()` through the local renderer adapter and routes real Crossterm events through `InputRouter` into `TuiApp::handle_dispatch`. Today plugin surfaces are reduced to pre-rendered text inside System details, so their hit regions never join the production hit map. `handle_dispatch` drops request identity and sends every action into the built-in `botster.tui.*` switch. Hub plugin action results remain untyped JSON display text and do not affect presentation or the visible root.

## Dependency evidence and ordering

- Hub UI contract PR `trybotster/botster-hub#168` merged as `d79403c74520fa054fb8b5996958dcf739d2fee3`. It owns `botster-ui-contract`, protocol version 4, typed `DaemonPluginSurface.body: UiNode`, canonical `PluginSurfaceAction { package_name, request: UiActionRequest }`, and typed `UiActionResult`.
- TUI-kit PR `trybotster/botster-tui-kit#20` merged as `500840dbdef97f52fe420dc8c73ddad0372fed4e`. It owns `PresentationState`, `render_node_with_presentation_state`, and accepted-only `apply_action_result` transitions with replacement roots.
- Live presentation producer PR `trybotster/botster-hub#170` merged as `3d3623f2907c78c7e4f3d4f3e3bf1dfdc09cf729`. Its real plugin-worker fixture adds action-result `set` operations for the shared Dialog and selected-workspace equality detail, then exercises reject and accept/clear/replacement.
- The merged kit still pins `botster-ui-contract` at `d79403c…`. Pinning this TUI's hub client/test support to `3d3623f…` at the same time would create two Git-source identities for the contract crate and incompatible Rust `UiNode`/action types.
- A narrow cross-repository prerequisite was therefore created against `botster-tui-kit`: `ticket_1785261259_330503` ("TUI kit: align UI contract pin with live presentation conformance Hub revision"). It is registered as dependency `dependency_1785261263_673329` of this ticket and itself depends on the closed live-producer ticket through `dependency_1785261309_849647`.
- Implementation must not begin until `ticket_1785261259_330503` closes with a merged kit revision. This plan does not waive the live producer, use sibling/path patches, or duplicate contract types to bypass the dependency.

## Scope

1. Cold-switch this crate from the Core-owned UI model to the Hub-owned contract:
   - consume the aligned merged `botster-tui-kit`;
   - consume `botster-hub-client` and `botster-hub-test-support` at `3d3623f…` or a later single verified merged revision containing the same contract/live producer;
   - replace `botster-core-ui` UI imports with the one `botster-ui-contract` source used by both dependencies;
   - update `Cargo.lock` without compatibility aliases or parallel action envelopes;
   - raise the TUI's minimum Hub protocol from 2 to 4 and `MINIMUM_CONFORMANCE_FIXTURE_REVISION` from 16 to 19 with the client revision;
   - update the existing guard assertion for the fixture floor and fail through the structured compatibility-mismatch path on an older protocol or fixture revision; do not add a protocol or fixture fallback.
2. Promote one delivered plugin surface to a real active, owning TUI surface:
   - retain package name, surface ID, typed root, presentation state, and latest typed result together;
   - keep the stable TUI-owned application shell and render the typed root through `render_node_with_presentation_state` in its content region, so dialog/form/button regions enter the real hit map;
   - an accepted plugin replacement replaces only the plugin-owned content root, never the surrounding application shell;
   - reset this client-local scope when the Hub connection or package/surface owner changes, while keeping presentation and selected-detail state stable across redraws and accepted replacements within the same owner.
3. Keep the app/kit boundary thin:
   - kit continues to own Dialog/Form/Button rendering, modal focus, drafts, keyboard/mouse routing, predicate evaluation, and accepted-only presentation mechanics;
   - `botster-tui` owns active package/surface identity, request correlation, Hub requests, result correlation, diagnostics, and which replacement root becomes shell-visible.
4. Route actions by rendered ownership before the built-in switch:
   - when a plugin surface is active, route every action from that rendered surface through `DaemonRequest::PluginSurfaceAction { package_name, request }`;
   - preserve the canonical request's request ID, surface ID, action ID, node ID, kind, values, and payload;
   - reject a surface/request ownership mismatch locally;
   - do not let an arbitrary or colliding plugin action ID fall through to a `botster.tui.*` branch;
   - do not emit a plugin action request when no active owning plugin surface exists.
   - exclusivity applies to actions emitted by the plugin content tree, not to TUI-owned global safety/navigation input;
   - intercept a pressed, unmodified `Esc` in the application event loop before plugin action dispatch: preserve the existing confirmation cancel behavior first; otherwise, when a plugin surface is active (including a plugin modal or selected detail), clear the complete active plugin scope, reset its router/presentation state, and return to the shell's System surface without sending a plugin action; only at the base shell may the existing `Esc` quit behavior apply. `q` and `Ctrl-C` remain global quit inputs.
5. Apply typed results only to the matching in-flight owner/request:
   - validate/correlate request, surface, action, and node identity before changing client state;
   - use kit `apply_action_result` so rejected/deferred/error results cannot mutate presentation or replace the root;
   - on acceptance, apply `set`/`toggle`/`clear`, make any owner-authored replacement the active visible root, and preserve router focus reconciliation;
   - on rejection, retain the original form/dialog, drafts, modal focus, field/form errors, and actionable feedback.
6. Extend unit, real-frame, and isolated-Hub tests through production entry points. Update README ownership, dependency pins, live contract behavior, and the now-delivered owner-routed action scope.

## Non-scope

- No changes to Hub runtime, `botster-ui-contract`, TUI-kit renderer/input mechanics, plugin Lua policy, Project Pipelines policy, browser behavior, Rails, Git/worktree policy, MCP tools, or session-worker protocol.
- No workspace-specific branches, Project Pipelines action IDs, node-ID dispatch literals, local presentation action namespace, client-invented dialog triggers, or product-specific replacement rules.
- No duplicate `UiNode`, `UiActionRequest`, `UiActionResult`, presentation, or compatibility structs.
- No entity-store/bound-list implementation beyond the ticketed selected-detail presentation fixture.
- No terminal data-plane, attach, resize, scrollback, mouse passthrough, package lifecycle, or session lifecycle refactor.
- No plugin-authored override of the TUI-owned `Esc`, `q`, or `Ctrl-C` safety/navigation contract.
- No adjacent cleanup or broad split of the existing large `app.rs`; new local structure should be only what is necessary to make ownership and result application explicit.

## Ownership boundaries and cross-repository seams

- `botster-tui` owns the stable application shell, global safety/navigation input, active plugin content scope, application-mode selection, request IDs, Hub dispatch, result correlation, visible content replacement selection, and user-facing diagnostics.
- `botster-tui-kit` owns reusable render/input/presentation mechanics. Any missing generic behavior discovered during implementation must be routed to its target, not copied locally.
- `botster-hub` owns the daemon request/response DTOs, contract crate, plugin execution, result identity validation, shared fixtures, and isolated-Hub producer proof.
- `botster-core` remains only for non-UI runnable-entrypoint connection types still used by this crate; it is not an alternative UI source.
- Browser parity is contract evidence supplied by the Hub conformance package and existing browser-shaped producer path. This ticket changes no browser files and must not redefine semantics for the TUI.

## Assumptions and unknowns

- Decision ledger: human answer to Project Pipelines question `question_1785262009_418874` chose the stable shell wrapper, TUI-owned global `Esc` return path, cold compatibility migration with no fallback, and Plan revision while the kit repin runs. The target Hub revision makes that cold floor protocol 4 plus conformance fixture revision 19.
- Assumption: the alignment prerequisite changes only the kit's contract pin and lockfile, leaving the merged public kit API intact.
- Assumption: `3d3623f…` remains the minimum Hub revision for live `set`/selected-workspace producer proof; a later merged revision is acceptable only if it preserves protocol 4, fixture revision 19+, and the same typed APIs.
- Assumption: a single active plugin surface is the current application policy. Per-surface state therefore means state coupled to the active Hub/package/surface identity and reset on owner change, not a speculative multi-surface cache.
- Assumption and product decision: plugin action ownership is exclusive inside the content region, while a stable, non-action-bearing TUI shell remains around it and retains the global `Esc`/quit input contract. Plugin replacements replace the content root only.
- Assumption and product decision: from any active plugin modal/detail/root, unmodified `Esc` returns directly to the shell's System surface by clearing the active plugin scope; it does not synthesize a plugin action or locally rewrite plugin presentation operations.
- Assumption and product decision: protocol 4 and conformance fixture revision 19 are the new cold minimums. Protocol 2 or 3 Hubs, and protocol-4 Hubs at fixture revision 16–18, fail clearly through compatibility diagnostics, with no dual protocol or fixture path.
- Ask-human threshold: stop and ask if the stable shell/content-region design cannot preserve kit modal focus, drafts, or hit-map ownership without changing `botster-tui-kit`; do not silently replace the whole application root, mix host action nodes into the plugin router, or invent a second input/presentation implementation.
- Unknown: the exact test-support helper/report additions at the final aligned Hub/kit pins. Implementation should use the public report fields and rendered action metadata rather than parallel constants.
- Convention conflict: none. The plan uses the authoritative Hub contract and kit mechanics, keeps product policy in the client/plugin owners, performs a cold contract switch, and registers the cross-repository prerequisite instead of broadening this run.

## Affected surfaces and files

- `crates/botster-tui/Cargo.toml`
  - bump Hub client/test support and aligned TUI-kit revisions;
  - add/use the authoritative `botster-ui-contract` package source as needed for direct imports;
  - remove the `botster-core-ui` alias and keep Core only for non-UI connection/test support.
- `Cargo.lock`
  - resolve one UI contract source and the required merged dependency revisions.
- `crates/botster-tui/src/renderer.rs`
  - migrate imports to `botster-ui-contract`;
  - accept the active surface ID in `ActionRequestContext`;
  - expose the kit presentation-aware production/test render entry points without reimplementing them.
- `crates/botster-tui/src/app.rs`
  - active plugin surface owner/state;
  - stable shell plus plugin content-region render/router-context selection;
  - TUI-owned `Esc` exit handling and active-scope reset before plugin dispatch;
  - minimum conformance fixture revision 19 and its guarded compatibility requirement;
  - generic plugin action dispatch and exact request preservation;
  - typed response/result application, identity checks, accepted replacement, rejected error retention;
  - request observation and focused/live tests.
- `README.md`
  - authoritative contract/kit/Hub pins, the cold protocol 4/fixture revision 19 minimums and older-Hub failure behavior, stable shell/`Esc` navigation, production plugin interaction path, live conformance coverage, and removal of the stale "owner-routed plugin action execution not included" statement.
- `docs/plans/tui-generic-plugin-ui-action-routing-plan.md`
  - this reviewable plan artifact.

## Implementation sequence

1. Confirm the alignment dependency is closed and record its merged kit commit. Update the three related pins, minimum protocol, and minimum conformance fixture revision together and compile before behavior edits. Change the guarded fixture-floor assertion from 16 to 19. Use `cargo tree -d` plus source inspection to prove there is exactly one `botster-ui-contract`.
2. Migrate renderer/app/test imports from `botster_core_ui::{RequestId, ui::*}` to the Hub-owned names, including `UiActionRequestId`, and update typed `DaemonPluginSurface`/`UiActionResult` usage. Delete old JSON-deserialize/display compatibility paths rather than maintaining both.
3. Introduce the smallest active-surface state that couples package/surface identity, current typed root, caller-scoped `PresentationState`, and latest matching result. Clear it on transport-generation invalidation and reset it on owner change.
4. Retain the stable application shell and render the active plugin root in its content region through the presentation-aware kit path. Keep the same `InputRouter` across result redraws for draft and focus retention; recreate/rebind it only when the owning surface changes, using the owner's surface ID in canonical requests. An accepted replacement becomes the content root, not the application root.
5. Split `handle_dispatch` by current content owner before matching action IDs. Plugin-owned action requests go intact to the Hub; workspace-owned requests retain the existing built-in switch. In `run_loop`, handle unmodified `Esc` before router/plugin action dispatch so confirmation cancellation remains first, an active plugin scope returns to the System shell, and base-shell `Esc` retains its current quit behavior. Add a dedicated synchronous plugin request/result path so the expected request identity is available when applying the response.
6. Apply matching typed results atomically through the kit transition. Surface rejected field/form errors without replacing or closing the form; accepted transitions update presentation and the visible root.
7. Replace pre-rendered plugin text tests with real-frame/action tests and extend the isolated-Hub conformance test through the actual app/router path.
8. Update README and run every repository gate plus the live downstream proof.

## Risks

- Duplicate contract source: different Hub revisions produce incompatible Rust types even when source text matches. Mitigation: the registered kit-alignment dependency and `cargo tree -d` proof.
- False-green renderer proof: `render_to_lines` over a detached fixture can pass while the production app still flattens the surface to text. Mitigation: assert `TuiApp` production root, hit map, router dispatch, Hub request, result application, and redraw.
- Ownership confusion: mixing built-in and plugin actions in one router context could execute host commands from plugin-authored IDs. Mitigation: exclusive active-owner routing and an action-ID collision regression.
- Operator trap: exclusive plugin action routing could strand the operator if the former built-in System toggle is no longer reachable. Mitigation: a stable TUI shell and a global, non-plugin-addressable `Esc` path that clears plugin scope before dispatch.
- Stale or mismatched results: a result for a prior surface/request could mutate current presentation. Mitigation: correlate full request identity and active owner before `apply_action_result`.
- Draft/focus loss: recreating the router after every response would make rejection appear to retain the tree while losing operator input. Mitigation: preserve the router within one owner scope and test real keyboard focus/drafts before and after rejection.
- Hidden replacement: storing a replacement without making it the production root would satisfy state assertions but not users. Mitigation: render and interact with the replacement after acceptance.
- Presentation leakage: retaining state across package/surface or reconnect boundaries could reveal stale details/dialogs. Mitigation: scope/reset by Hub generation, package, and surface.
- Regression surface: `app.rs` also owns terminal, packages, apps, and session behavior. Mitigation: surgical helpers plus full workspace tests, strict clippy, smoke, and live Hub verification.
- Compatibility floors: the Hub client repin raises the minimum daemon protocol from 2 to 4, while the live presentation producer requires conformance fixture revision 19 rather than the current TUI floor of 16. Mitigation: raise and document both cold minimums, update the constant's guard assertion, retain the existing structured compatibility-mismatch path, assert diagnostics name the expected minima for protocol 2/3 and fixture revision 16–18 Hubs, and add no fallback.
- Live harness flake: Hub test support has documented pre-existing timing-sensitive lifecycle tests, but this repository's live wrapper is still required. Any failure needs exact branch/base attribution; it is not a blanket waiver.

## Acceptance checks and downstream proof

Repository gates:

```sh
script/fmt
script/test
script/clippy
cargo run -p botster-tui -- --smoke
BOTSTER_HUB_BIN=/path/to/botster-hub \
BOTSTER_SESSION_WORKER_BIN=/path/to/botster-session-worker \
CARGO_TARGET_DIR=/tmp/botster-tui-generic-plugin-actions-target \
  script/test-live-hub
cargo tree -d
git diff --check
```

Required behavioral assertions:

- The dependency graph contains one authoritative `botster-ui-contract`; no `botster-core-ui` import/dependency or local UI/action compatibility type remains.
- A real delivered plugin root is rendered in the production frame with action-bearing hit regions; it is not flattened to display-only text.
- The application shell remains present around the plugin-owned content region, contains no action node routed through the plugin context, and an accepted replacement becomes visibly interactive inside that region rather than replacing the shell.
- Keyboard and mouse activation of the shared Dialog/Form/Button fixture pass through `InputRouter` and `TuiApp::handle_dispatch`.
- An arbitrary plugin button action and a form submit emit `PluginSurfaceAction` with the active package plus the exact canonical request surface/action/node/kind/values/payload read from the rendered control.
- A plugin action ID that is unknown or collides with `botster.tui.*` routes to the owning plugin and does not execute built-in application behavior.
- With no active owner, no plugin action request is emitted. A request whose surface differs from the active owner is rejected locally.
- Drive the real `run_loop` key branch with unmodified `Esc` from a plugin modal, selected detail, and replacement root. Each case emits no `PluginSurfaceAction`, clears the active package/surface/root/presentation/router scope, returns to the System shell, and leaves built-in workspace controls interactive. At the base shell, existing `Esc` quit behavior remains; `q` and `Ctrl-C` continue to quit globally.
- The live fixture's open action goes through Hub → plugin worker → accepted `set`; the next real-frame render shows both the Dialog and the selected-workspace equality detail.
- The rejected form response retains the dialog, current typed root, router drafts, modal focus, field error keyed by `contract-app-message`, and form error; it applies no presentation or replacement effects.
- The later accepted submit applies `clear`, closes the dialog, installs `contract-action-replacement` as the visible plugin content root inside the stable shell, and restores/reconciles focus according to kit mechanics.
- Result identity mismatch and invalid replacement fixture paths remain Hub-owned structured failures and do not mutate TUI presentation/root state.
- The compatibility requirement reports minimum protocol 4 and minimum conformance fixture revision 19. Synthetic protocol 2 and 3 descriptors, plus protocol-4 descriptors at fixture revision 16, 17, or 18, fail with a structured, rendered compatibility mismatch naming the unsatisfied minimum; protocol 4 at fixture revision 19 connects. The guard assertion pins `MINIMUM_CONFORMANCE_FIXTURE_REVISION` to 19, and no compatibility fallback exists.
- Existing built-in workspace/session/package actions, terminal forwarding/resize/mouse behavior, remaining compatibility diagnostics, and live session lifecycle tests remain green.
- The live test uses the merged Hub/test-support artifact and real plugin-worker fixture. Local synthetic fixtures remain focused unit aids, not the downstream oracle.

## Pipeline and vault checklist evidence

- Run vault checklist: `checklist_1785260945_622115`.
- The checklist creation call timed out, but listing showed it persisted; it was adopted rather than duplicated.
- The vault-context item records the playbooks and atomic notes above.
- Convention review result: none.
- Verification remains pending for Implement/Verify and must attach the exact commands/results above.
- Durable capture decision: planning identified no uncaptured TUI rule beyond the already-existing action-routing, presentation ownership, thin-adapter, and runtime-input notes. If implementation reveals a repeatable active-surface correlation/reset rule not covered by them, capture it through the vault inbox pipeline after behavior is proven.

## Vault gaps worth capturing

- No new atomic note is warranted from planning alone.
- Capture after implementation only if the final code establishes a reusable rule not already covered: for example, that a first-party client must bind one router context and presentation store to one active Hub/package/surface owner and reject stale result identities before applying kit transitions.
- The stale [[botster-tui-kit-playbook]] wording that assigns UI schema authority to Core was already identified during the dependency run; its correction belongs in the vault/plugin-maintenance flow, not in this TUI implementation.
