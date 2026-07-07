# TUI Admitted Plugin Navigation Registry Plan

## Context Loaded

- Pipeline context: ticket `ticket_1783371411_717269`, run `run_1783381571_151557`, step `botster_plan`; dependency `ticket_1783371372_931094` is closed; no prior artifacts, findings, questions, or answers were present.
- Vault/playbooks: [[planner-playbook]], [[botster-planner-playbook]], [[botster-architecture]], [[cli-patterns]], [[spa-patterns]], [[project pipeline orchestration belongs in a device-level botster plugin]], [[project pipelines needs an operator workbench not more primitives]], [[project pipelines ui contract belongs in the plugin readme]], [[botster orchestration should spawn agents with explicit target ids]], [[botster orchestration prompts must bind agents to explicit worktrees]], [[plan steps need reviewable plan artifacts]], [[project pipelines checklist worker timeouts require artifact evidence fallback]].
- Repo context: `botster-tui` is a Rust TUI client over hub contracts and `botster-tui-kit`; production path is `DogfoodApp::refresh_read_models()` -> hub `DaemonRequest`s -> `DogfoodApp::surface()` -> shared UiNode renderer. Existing tests already cover package/app route DTO display, plugin surface render/action DTOs, invalid UiNode diagnostics, and live hub plugin contract matrix smoke.
- Dependency context: current pinned `botster-hub-client` rev is `27118ab75f4ff511ccdfcfa754f74b878c0b9b45`; current `botster-hub` `main` is `3807e1388fa560940c77192f7648bf9638108ab8`. The pinned client does not expose a navigation registry request/DTO, so implementation should refresh the hub-client/test-support pins to the dependency ticket's admitted navigation contract.

## Scope

- Refresh `botster-hub-client` and `botster-hub-test-support` git pins, plus `Cargo.lock`, to a hub revision that exposes the admitted plugin navigation registry and related test-support fixtures.
- Replace TUI plugin navigation discovery with hub-admitted navigation DTO consumption. The TUI should request the registry from the hub, store the returned navigation entries in `DogfoodApp`, render them as client presentation rows, and open plugin routes from those entries.
- Preserve plugin content rendering through the existing `PluginSurfaceRender` -> `DaemonPluginSurface` -> `UiNode` validation/render path. Plugin surfaces should occupy the app content area owned by the root UiNode layout.
- Add precise TUI diagnostics for iframe/custom HTML UiNodes or route targets that cannot render natively. Diagnostics should include the entry/surface title and source URL or asset URL plus sandbox/details when present, or use an existing external-open affordance if the refreshed client contract already exposes one.
- Extend existing unit tests and live hub conformance to prove registry consumption, plugin route open, disabled/blocked navigation diagnostics, and iframe/custom HTML unsupported diagnostics.
- Update README wording only where needed to describe the new navigation source of truth and unsupported iframe behavior.

## Non-Scope

- Do not parse raw package manifests for app navigation.
- Do not add sidebar replacement, route-layout, or plugin-owned layout concepts.
- Do not implement native iframe/webview/custom HTML rendering in TUI v1.
- Do not redesign the TUI shell, package lifecycle UI, terminal streaming path, or renderer kit.
- Do not add new workflow/plugin primitives or broad package registry refactors.

## Assumptions And Unknowns

- Assumption: the closed hub dependency exposes a stable `botster-hub-client` request/response DTO for admitted navigation entries, with enough fields to open plugin surfaces and display disabled/blocked state.
- Assumption: hub test support includes or can expose fixtures for admitted navigation registry entries, blocked entries, and iframe/custom HTML entries from the same plugin contract matrix.
- Assumption: opening a plugin route from navigation should map to the existing hub plugin surface render request for UiNode routes. If the refreshed DTO exposes a different open/resolve request, use that public hub request rather than inventing TUI routing.
- Unknown: exact type and variant names in the refreshed hub-client API. Implementer should inspect the refreshed crate and preserve its names rather than adding adapter DTOs in this repo.
- Unknown: whether custom HTML is represented as a UiNode kind, a route target, or an asset-backed navigation target. The implementation should handle the public refreshed representation precisely and fail visible rather than blank.

## Affected Surfaces And Files

- `crates/botster-tui/Cargo.toml` and `Cargo.lock`: hub-client/test-support pin refresh.
- `crates/botster-tui/src/app.rs`: add navigation registry state, request/response handling, render rows, route-open action handling, iframe/custom HTML diagnostics, and tests.
- `crates/botster-tui/src/renderer.rs`: likely unchanged unless refreshed core adds a first-class unsupported UiNode kind that should be classified before generic capability validation.
- `crates/botster-tui/tests/package_manifest_test.rs`: likely unchanged; keep manifest test focused on local runnable app packaging.
- `script/test-live-hub`: likely unchanged unless the refreshed hub test-support fixture path or harness API changes.
- `README.md`: update local hub dogfood section if behavior or command evidence changes.
- `docs/plans/tui-admitted-plugin-navigation-registry-plan.md`: this plan artifact.

## Implementation Plan

1. Refresh hub dependencies to the dependency ticket's merged hub revision, then inspect `botster-hub-client` for navigation registry request, response kind, DTO fields, route target/open request, and iframe/custom HTML representation.
2. Add `DogfoodApp` state for admitted navigation entries and refresh it from the new hub request during `refresh_read_models()`. Keep `ListApps`/`ListPackages` for status, lifecycle, and package management, but stop using package routes as the plugin navigation source of truth.
3. Render a navigation section from the admitted registry entries. Include title/label, package, route/surface identity, enabled/blocked state, blocked reasons/diagnostics, and an Open action for supported plugin surface entries.
4. Handle the Open action by using the refreshed hub-client route/surface contract to request the selected plugin surface. Keep the selected/opened plugin surface in the existing `plugin_surface` field or a narrow replacement if the refreshed DTO requires route identity.
5. Add unsupported iframe/custom HTML diagnostics before generic UiNode failure messaging. The diagnostic must name the navigation entry or surface title, source URL or asset URL, and sandbox/custom HTML details available from the DTO. If a public external-open action already exists in the DTO, render that affordance instead of a dead Open action.
6. Update tests using refreshed hub test-support fixtures and local response builders: registry entries render, Open sends the right hub request and renders delivered UiNode, disabled/blocked entries stay visible with diagnostics and do not silently open, iframe/custom HTML yields the precise unsupported/external-open diagnostic.
7. Run formatting, unit tests, strict clippy, and the live hub smoke with the refreshed fixture.

## Risks

- Contract drift risk: the refreshed hub dependency may also update `botster-core`/`botster-tui-kit` expectations. Keep changes pinned to public DTO/compiler errors and avoid local compatibility shims unless required by a real deployment boundary.
- False-positive test risk: fixture-only tests could prove DTO rendering without proving production refresh/open paths. Require at least one test that drives `refresh_read_models()`/observed request handling and one live hub smoke path.
- Blank-content risk: iframe/custom HTML may deserialize as a valid hub payload but remain non-renderable in Ratatui. The plan requires a specific diagnostic to prevent silent blank plugin pages.
- Scope creep risk: adding a richer navigation shell could turn into layout policy. Keep navigation as TUI presentation over the hub registry; root UiNode owns page layout and plugin content remains full app content.

## Acceptance Checks And Tests

- `script/fmt`
- `script/test`
- `script/clippy`
- `CARGO_TARGET_DIR=/tmp/botster-tui-impl-target script/test-live-hub`
- Targeted unit assertions:
  - `refresh_read_models()` requests the hub navigation registry.
  - TUI renders admitted plugin navigation entries from registry DTOs, not package manifest/routes.
  - Open action for a plugin surface entry sends the public hub request and renders the delivered UiNode through the existing renderer path.
  - Disabled/blocked navigation entries show hub-provided reasons/diagnostics and do not silently produce blank content.
  - Iframe/custom HTML entries show a precise unsupported/external-open diagnostic containing title/source/sandbox details.
- Manual evidence for production path: identify in implementation report which `DogfoodApp` refresh/open action calls the new hub request and which rendered node shows the selected plugin surface.

## Vault Gaps Worth Capturing

- Capture only if implementation reveals a durable gotcha, such as the exact refreshed navigation DTO naming, iframe/custom HTML representation, or a hub test-support fixture boundary not already covered by [[botster first party client support matrices belong in hub test support]] and related notes.
- No convention conflict found during planning: the plan keeps TUI as a generic hub client, avoids manifest parsing, preserves UiNode rendering, and does not add new layout primitives.
