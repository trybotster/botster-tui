# TUI integration cold-cut implementation

Target: `trybotster/botster-tui`, `tgt_c3d470bab78549df920a41e8fb0e58d8`.
Ticket: `ticket_1788460430_647093`. Run: `run_1788570301_694931`.
Status: preparation/unvalidated. Root message `msg_plugin-w_1788657168_1f7475` authorizes this checkpoint. The manifest and documentation select the new tuple. `Cargo.lock` still records the prior tuple until publication approval and normal resolution. No test or build result transfers from `812b200`. Review admission remains blocked until the lock, gates, and report artifact are complete.

Plan basis: approved revision 3 at `e3e73751b1db92ee4099de4e037366b608e3ca54`, updated through revision 5 under historical human answers `question_1788573184_913439` and `question_1788577472_219314`.

The implementation report artifact identifies the final candidate SHA and records command results after commit. No gate result is claimed before execution.

The implementation removes the deleted Drain match, observation variant, one vacuous test, and three Drain assertions. All other migration assertions remain. Each of the two test readers owns one persistent incomplete-frame buffer across its reads. The production mux reader remains the entry point for terminal output.

The manifest, live defaults, and README select Hub `1a0df65230a476cfea362fdc5131e035d303a928` and Core `bf6e7d996bca2786ad4142c870a13c57a490e241`. The live build comment and Core capacity comment use the verified new revisions. The close-event comment removes its historical revision label. Core still defines `INPUT_QUEUE_CAPACITY = 256`.

Implementation files:

- `crates/botster-tui/Cargo.toml`
- `Cargo.lock`
- `crates/botster-tui/src/app.rs`
- `README.md`
- `docs/plans/tui-roll-hub-and-core-pins-to-the-integration-cold-cut-plan.md`
- This report.

The branch also contains the approved plan under `docs/plans/`. This revision updates the active plan to the superseding human decision.

Guidance: `implementer-playbook`, `botster-implementer-playbook`, `botster-tui-playbook`, the runtime Review and Verify overlays, and `project-pipelines-playbook` for the approved barrier. Targeted guidance includes the pin-roll defaults and README note, exact Git identity notes, persistent Unix mux guidance, split Hello and close guidance, live Ghostty profiles, primary-screen history, binary provenance, stable-commit verification, preserved assertion coverage, and repository test wrappers. The pipeline checklist records exact note filenames.

TUI owns client consumption and proof. Hub retains policy, lifecycle, and transport ownership. Core retains terminal mechanisms and worker ownership. No shared Kit or UI contract pin changes. No compatibility code or upstream defect compensation. Runtime-teardown class does not apply under the approved plan.

The Hub matrix belongs to `ticket_1787600679_990088` on `tgt_7e208a0c76a44980a83b63af976b1f22`. The operator forbids formal dependency edges. Review must approve this exact candidate before the steward hands it to Hub. Final Verify must hold for the complete matrix and Hub merge ancestry evidence. Shared attach, shared exit, and browser reconnect remain Hub-matrix proof obligations.

Root reports Core publication and pending Hub publication approval. Fresh lock resolution and provenance checks remain required. The prior candidate used base `b051c6747180fa8375a56f0e4d71aae5bc68f2be`; this preparation did not refresh main.

Required commands at the committed candidate:

- `script/fmt`
- `script/clippy`
- `./test.sh --workspace --all-targets`
- `cargo build -p botster-tui --locked`
- `cargo build --locked`
- The focused pressure test through `./test.sh`, with required debug binaries.
- `script/test-live-hub ghostty`, with explicit revision labels and again with both labels unset.

Each gate records HEAD and clean status before and after execution. The artifact records exits, test counts, marker lines, and log paths. Pin checks require five Core and two Hub lock sources, only required dependency changes, no active old revisions, and no deleted Drain symbols.

The isolated lane and focused proof use debug Hub and worker binaries from the exact clean Hub checkout. The runtime receipt records the actual build command, revisions, paths, and binary hashes. The existing receipt writer hard-codes release commands, so this report uses a receipt that describes the debug build accurately.

Verify finding C identified missing close evidence under debug binaries. The fixture now selects the exact flood session with the existing Hub pressure hook. The producer waits for a test-owned file. The test writes that file after Attached, pre-close Status, and the pressure marker. The focused test and full lane share all close assertions. Both require real Core close delivery through the Unix mux, one recovery, retired subscription and generation, and sibling progress. The exact close reason and the 30-second deadline remain unchanged.

The authorized negative control temporarily removes actual close-event delivery for the selected flood session in a disposable Hub checkout. The artifact records its diff, command, missing-close failure, source restoration, and clean tracked status. No Hub source change belongs to this candidate. Final positive proof uses the restored debug build.

Assumptions and residual risk: shared matrix evidence does not exist for this candidate until independent Review and the Hub matrix finish. No shared resources are owned by this run. This fixture result does not exclude other debug runtime defects. The full Hub matrix must determine that. Final Verify must not advance early. Any candidate change renews the relevant review and proof.

Approved deviation: Verify finding D and Root message `msg_plugin-w_1788657168_1f7475` authorize the new Hub and Core tuple. Plan revision 6 records this decision and the preparation exception. The active plan and every acceptance command now use this revision. The TUI adds no production behavior change. Upstream API and runtime changes require inspection and new proof. The old-pin invariant requires updating the existing Core capacity comment. Finding C and `question_1788577472_219314` also authorize the test fixture repair and disposable negative control. The active plan includes these acceptance checks.

Guidance gaps: the plan identifies stale current-pin prose, per-reader buffer lifetime, caller-owned provenance, and the frozen-candidate barrier. Existing notes already cover these constraints in part. The final checklist records whether this visit adds durable knowledge.

README retains the `9a02e55 or later` minimum-version claim under Root message `msg_plugin-w_1788657222_a67b12`. This is not an active pin. Historical plan evidence keeps its original revisions.
