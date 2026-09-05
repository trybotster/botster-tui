# TUI integration cold-cut implementation

Target: `trybotster/botster-tui`, `tgt_c3d470bab78549df920a41e8fb0e58d8`.
Ticket: `ticket_1788460430_647093`. Run: `run_1788570301_694931`.
Approved plan: revision 3 at `e3e73751b1db92ee4099de4e037366b608e3ca54`.

This report belongs to the candidate commit that adds this file. The implementation report artifact records its exact SHA and command results after commit. No gate result is claimed before execution.

The implementation removes the deleted Drain match, observation variant, one vacuous test, and three Drain assertions. All other assertions remain. Each of the two test readers owns one persistent incomplete-frame buffer across its reads. The production mux reader remains the entry point for terminal output.

The manifest, lockfile, live defaults, and README consume Hub `205cadf6f8dab9dc990537c2c00ef3d27edb31dd` and Core `93acae3f98adbc21dc981d113c4eb2f31ead4ad0`. The Core capacity comment uses the verified new revision. Core still defines `INPUT_QUEUE_CAPACITY = 256`.

Implementation files:

- `crates/botster-tui/Cargo.toml`
- `Cargo.lock`
- `crates/botster-tui/src/app.rs`
- `README.md`
- This report.

The branch also contains the approved plan under `docs/plans/`. The implementation does not change that plan.

Guidance: `implementer-playbook`, `botster-implementer-playbook`, `botster-tui-playbook`, the runtime Review and Verify overlays, and `project-pipelines-playbook` for the approved barrier. Targeted guidance includes the pin-roll defaults and README note, exact Git identity notes, persistent Unix mux guidance, split Hello and close guidance, live Ghostty profiles, primary-screen history, binary provenance, stable-commit verification, preserved assertion coverage, and repository test wrappers. The pipeline checklist records exact note filenames.

TUI owns client consumption and proof. Hub retains policy, lifecycle, and transport ownership. Core retains terminal mechanisms and worker ownership. No shared Kit or UI contract pin changes. No compatibility code or upstream defect compensation. Runtime-teardown class does not apply under the approved plan.

The Hub matrix belongs to `ticket_1787600679_990088` on `tgt_7e208a0c76a44980a83b63af976b1f22`. The operator forbids formal dependency edges. Review must approve this exact candidate before the steward hands it to Hub. Final Verify must hold for the complete matrix and Hub merge ancestry evidence. Shared attach, shared exit, and browser reconnect remain Hub-matrix proof obligations.

Pre-change remote checks returned both required frozen revisions. TUI `origin/main` remains `b051c6747180fa8375a56f0e4d71aae5bc68f2be` after refresh.

Required commands at the committed candidate:

- `script/fmt`
- `script/clippy`
- `./test.sh --workspace --all-targets`
- `cargo build -p botster-tui --locked`
- `cargo build --locked`
- `script/test-live-hub ghostty`, with explicit revision labels and again with both labels unset.

Each gate records HEAD and clean status before and after execution. The artifact records exits, test counts, marker lines, and log paths. Pin checks require five Core and two Hub lock sources, no registry changes, no active old revisions, and no deleted Drain symbols.

The isolated live lane uses a clean Hub checkout at `/private/tmp/tui-cold-cut-hub-647093`. Fresh release builds use `/private/tmp/tui-cold-cut-hub-build-647093`. The repository receipt writer records the checkout revision, locked Core revision, binary paths, and build commands. The artifact preserves the receipt contents.

Assumptions and residual risk: shared matrix evidence does not exist for this candidate until independent Review and the Hub matrix finish. No shared resources are owned by this run. Final Verify must not advance early. Any candidate change renews the relevant review and proof.

Deviations: none. The old-pin invariant requires updating the existing Core capacity comment. The deleted API requires no new adapter or test abstraction.

Guidance gaps: the plan identifies stale current-pin prose, per-reader buffer lifetime, caller-owned provenance, and the frozen-candidate barrier. Existing notes already cover these constraints in part. The final checklist records whether this visit adds durable knowledge.
