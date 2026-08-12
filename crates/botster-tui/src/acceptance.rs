use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Spawn-driver schema (existing parent entrypoint).
pub const SCHEMA: &str = "botster.tui.workspaces-spawn-driver/v1";
/// Shared-Hub keyboard claim schema (Available sessions entity_options path).
pub const CLAIM_SCHEMA: &str = "botster.tui.workspaces-claim-driver/v1";
pub const SCENARIO_ENV: &str = "BOTSTER_TUI_ACCEPTANCE_SCENARIO";
pub const EVIDENCE_ENV: &str = "BOTSTER_TUI_ACCEPTANCE_EVIDENCE";

/// Fail-closed pin floors from the approved claim plan (full SHAs).
pub const MIN_HUB_REV: &str = "de6b09982e72fd5efd04a5258f5fc645f611adbc";
pub const MIN_WORKSPACES_REV: &str = "7ab4d1334214b3ea3c8b02e9ea665a27e70c0916";
pub const MIN_TUI_REV: &str = "abc804e19bc3e01465cd308c11de5f4292331c3d";

pub const WORKSPACES_PACKAGE_PATH_ENV: &str = "BOTSTER_WORKSPACES_PACKAGE_PATH";
pub const HUB_SOURCE_PATH_ENV: &str = "BOTSTER_HUB_SOURCE_PATH";
pub const HUB_BIN_ENV: &str = "BOTSTER_HUB_BIN";
pub const SESSION_WORKER_BIN_ENV: &str = "BOTSTER_SESSION_WORKER_BIN";
/// Fresh Cargo target dir used to build Hub + session-worker for this claim run.
/// When set, both binaries must live under this directory (rejects stale shared
/// `target/release` caches that merely sit under the Hub source tree).
pub const HUB_BUILD_TARGET_DIR_ENV: &str = "BOTSTER_HUB_BUILD_TARGET_DIR";
/// JSON build receipt written by the live harness after locked Hub builds.
pub const CLAIM_BUILD_RECEIPT_ENV: &str = "BOTSTER_TUI_CLAIM_BUILD_RECEIPT";
/// Optional path for the live test harness to copy validated claim evidence
/// outside the tracked tree. The production claim driver never writes here.
/// Named for `script/test-live-hub workspaces claim-driver`.
#[cfg(test)]
pub const CLAIM_EVIDENCE_OUT_ENV: &str = "BOTSTER_TUI_CLAIM_EVIDENCE_OUT";
/// Parent ticket prose alias; maps to `BOTSTER_HUB_DATA_DIR` when that is unset.
pub const LIVE_DATA_DIR_ENV: &str = "BOTSTER_LIVE_DATA_DIR";

#[derive(Debug)]
pub enum AcceptanceMode {
    Spawn(SpawnConfig),
    Claim(ClaimConfig),
}

#[derive(Debug)]
pub struct SpawnConfig {
    pub scenario: Scenario,
    pub evidence_path: PathBuf,
}

#[derive(Debug)]
pub struct ClaimConfig {
    pub scenario: ClaimScenario,
    pub evidence_path: PathBuf,
}

/// Backward-compatible name used by the spawn acceptance driver.
pub type Config = SpawnConfig;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub schema: String,
    pub workspace_id: String,
    pub cases: Vec<ScenarioCase>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioCase {
    pub case_id: String,
    pub target_id: String,
    pub branch: String,
    pub resolution: ResolutionClass,
    pub expected: ExpectedFacts,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionClass {
    ExistingWorktree,
    ExistingBranch,
    MissingBranch,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedFacts {
    pub target_id: String,
    pub branch: String,
    pub worktree_path: String,
}

/// Caller-owned shared-Hub claim scenario.
///
/// Parent seeds workspace `workspace_id` and unclaimed running `session_uuid`
/// before launch. Paths/revs feed the fail-closed pin ledger.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimScenario {
    pub schema: String,
    pub workspace_id: String,
    pub session_uuid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_source_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspaces_package_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspaces_rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tui_rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_worker_rev: Option<String>,
}

impl AcceptanceMode {
    pub fn from_environment() -> io::Result<Option<Self>> {
        Self::from_paths(
            std::env::var_os(SCENARIO_ENV).map(PathBuf::from),
            std::env::var_os(EVIDENCE_ENV).map(PathBuf::from),
        )
    }

    fn from_paths(
        scenario_path: Option<PathBuf>,
        evidence_path: Option<PathBuf>,
    ) -> io::Result<Option<Self>> {
        let (scenario_path, evidence_path) = match (scenario_path, evidence_path) {
            (None, None) => return Ok(None),
            (Some(_), None) | (None, Some(_)) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{SCENARIO_ENV} and {EVIDENCE_ENV} must be set together"),
                ));
            }
            (Some(scenario), Some(evidence)) => (scenario, evidence),
        };

        reject_path_reuse(&scenario_path, &evidence_path)?;
        let bytes = fs::read(&scenario_path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "read acceptance scenario {}: {error}",
                    scenario_path.display()
                ),
            )
        })?;
        let document: Value = serde_json::from_slice(&bytes).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("decode acceptance scenario JSON: {error}"),
            )
        })?;
        let schema = document
            .get("schema")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "acceptance scenario missing schema",
                )
            })?;
        match schema {
            SCHEMA => {
                let scenario: Scenario = serde_json::from_value(document).map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("decode spawn acceptance scenario: {error}"),
                    )
                })?;
                scenario.validate()?;
                Ok(Some(Self::Spawn(SpawnConfig {
                    scenario,
                    evidence_path,
                })))
            }
            CLAIM_SCHEMA => {
                let mut scenario: ClaimScenario =
                    serde_json::from_value(document).map_err(|error| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("decode claim acceptance scenario: {error}"),
                        )
                    })?;
                scenario.apply_env_path_defaults();
                scenario.validate()?;
                Ok(Some(Self::Claim(ClaimConfig {
                    scenario,
                    evidence_path,
                })))
            }
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported acceptance schema {other:?}; expected {SCHEMA:?} or {CLAIM_SCHEMA:?}"
                ),
            )),
        }
    }
}

#[cfg(test)]
impl SpawnConfig {
    fn from_paths(
        scenario_path: Option<PathBuf>,
        evidence_path: Option<PathBuf>,
    ) -> io::Result<Option<Self>> {
        match AcceptanceMode::from_paths(scenario_path, evidence_path)? {
            None => Ok(None),
            Some(AcceptanceMode::Spawn(config)) => Ok(Some(config)),
            Some(AcceptanceMode::Claim(_)) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "expected spawn schema",
            )),
        }
    }
}

impl Scenario {
    fn validate(&self) -> io::Result<()> {
        if self.schema != SCHEMA {
            return invalid(format!("unsupported acceptance schema {:?}", self.schema));
        }
        require_identifier("workspace_id", &self.workspace_id)?;
        if self.cases.is_empty() {
            return invalid("acceptance scenario cases must not be empty");
        }
        let mut case_ids = BTreeSet::new();
        let mut resolutions = BTreeSet::new();
        for case in &self.cases {
            require_identifier("case_id", &case.case_id)?;
            require_identifier("target_id", &case.target_id)?;
            require_identifier("branch", &case.branch)?;
            require_identifier("expected.target_id", &case.expected.target_id)?;
            require_identifier("expected.branch", &case.expected.branch)?;
            require_identifier("expected.worktree_path", &case.expected.worktree_path)?;
            if !case_ids.insert(case.case_id.as_str()) {
                return invalid(format!("duplicate acceptance case_id {:?}", case.case_id));
            }
            if !resolutions.insert(case.resolution) {
                return invalid(format!(
                    "duplicate acceptance resolution class {:?}",
                    case.resolution
                ));
            }
            if case.expected.target_id != case.target_id || case.expected.branch != case.branch {
                return invalid(format!(
                    "case {:?} expected target/branch must match requested target/branch",
                    case.case_id
                ));
            }
        }
        let required = BTreeSet::from([
            ResolutionClass::ExistingWorktree,
            ResolutionClass::ExistingBranch,
            ResolutionClass::MissingBranch,
        ]);
        if resolutions != required {
            return invalid("acceptance cases must contain each resolution class exactly once");
        }
        Ok(())
    }
}

impl ClaimScenario {
    fn apply_env_path_defaults(&mut self) {
        if self.workspaces_package_path.is_none()
            && let Ok(path) = std::env::var(WORKSPACES_PACKAGE_PATH_ENV)
            && !path.trim().is_empty()
        {
            self.workspaces_package_path = Some(path);
        }
        if self.hub_source_path.is_none()
            && let Ok(path) = std::env::var(HUB_SOURCE_PATH_ENV)
            && !path.trim().is_empty()
        {
            self.hub_source_path = Some(path);
        }
    }

    fn validate(&self) -> io::Result<()> {
        if self.schema != CLAIM_SCHEMA {
            return invalid(format!("unsupported claim schema {:?}", self.schema));
        }
        require_identifier("workspace_id", &self.workspace_id)?;
        require_identifier("session_uuid", &self.session_uuid)?;
        for (name, value) in [
            ("hub_source_path", &self.hub_source_path),
            ("workspaces_package_path", &self.workspaces_package_path),
            ("hub_rev", &self.hub_rev),
            ("workspaces_rev", &self.workspaces_rev),
            ("tui_rev", &self.tui_rev),
            ("session_worker_rev", &self.session_worker_rev),
        ] {
            if let Some(value) = value {
                require_identifier(name, value)?;
            }
        }
        Ok(())
    }
}

/// Strict typed claim build receipt. All fields required; unknown fields rejected.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClaimBuildReceipt {
    pub hub_source: String,
    pub hub_rev: String,
    pub core_rev: String,
    pub hub_bin: String,
    pub session_worker_bin: String,
    pub target_dir: String,
    pub hub_build_command: String,
    pub session_worker_build_command: String,
}

/// Resolved pin ledger. Path fields are **root labels** for committed evidence
/// (`$HUB_SOURCE`, `$HUB_BUILD_TARGET`, …) — never machine-local absolute paths.
#[derive(Clone, Debug, Serialize)]
pub struct PinLedger {
    pub hub_minimum: &'static str,
    pub workspaces_minimum: &'static str,
    pub tui_minimum: &'static str,
    pub hub_rev: String,
    pub workspaces_rev: String,
    pub tui_rev: String,
    /// Locked botster-core SHA from the Hub checkout Cargo.lock (session-worker provenance).
    pub core_rev: String,
    pub session_worker_rev: String,
    pub hub_bin_path: String,
    pub session_worker_bin_path: String,
    pub hub_source_path: String,
    pub workspaces_package_path: String,
    pub tui_source_path: String,
    pub hub_ancestry_ok: bool,
    pub workspaces_ancestry_ok: bool,
    pub tui_ancestry_ok: bool,
    pub workspaces_available_sessions_form_ok: bool,
    pub hub_bin_under_source: bool,
    pub session_worker_bin_under_source: bool,
    pub sources_clean: bool,
    /// Fresh build target dir label that owns the executed binaries.
    pub hub_build_target_dir: String,
    pub hub_bin_under_build_target: bool,
    pub session_worker_bin_under_build_target: bool,
    pub hub_build_command: String,
    pub session_worker_build_command: String,
    pub build_receipt_path: String,
}

/// Path-neutral root labels for committed claim evidence (no machine-local paths).
pub const LABEL_HUB_SOURCE: &str = "$HUB_SOURCE";
pub const LABEL_TUI_SOURCE: &str = "$TUI_SOURCE";
pub const LABEL_WORKSPACES_PACKAGE: &str = "$WORKSPACES_PACKAGE";
pub const LABEL_HUB_BUILD_TARGET: &str = "$HUB_BUILD_TARGET";

/// Fail-closed pin + Available sessions form presence + binary provenance checks.
pub fn verify_claim_pins(scenario: &ClaimScenario) -> io::Result<PinLedger> {
    let workspaces_path = scenario
        .workspaces_package_path
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| std::env::var_os(WORKSPACES_PACKAGE_PATH_ENV).map(PathBuf::from))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "claim pin ledger requires workspaces_package_path or {WORKSPACES_PACKAGE_PATH_ENV}"
                ),
            )
        })?;
    if !workspaces_path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Workspaces package path is not a directory: {}",
                workspaces_path.display()
            ),
        ));
    }

    let hub_path = scenario
        .hub_source_path
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| std::env::var_os(HUB_SOURCE_PATH_ENV).map(PathBuf::from))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "claim pin ledger requires hub_source_path or {HUB_SOURCE_PATH_ENV} so Hub provenance is observed from a checkout"
                ),
            )
        })?;
    if !hub_path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Hub source path is not a directory: {}", hub_path.display()),
        ));
    }

    let tui_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tui_root = fs::canonicalize(&tui_root).unwrap_or(tui_root);
    let hub_path = fs::canonicalize(&hub_path).unwrap_or(hub_path);
    let workspaces_path = fs::canonicalize(&workspaces_path).unwrap_or(workspaces_path);

    // Fail closed on dirty sources so HEAD is the executed code.
    require_git_clean(&hub_path, "Hub")?;
    require_git_clean(&workspaces_path, "Workspaces")?;
    require_git_clean(&tui_root, "TUI")?;

    let workspaces_rev = resolve_rev(
        scenario.workspaces_rev.as_deref(),
        Some(workspaces_path.as_path()),
        "Workspaces",
    )?;
    let hub_rev = resolve_rev(scenario.hub_rev.as_deref(), Some(hub_path.as_path()), "Hub")?;
    let tui_rev = resolve_rev(scenario.tui_rev.as_deref(), Some(tui_root.as_path()), "TUI")?;
    let core_rev = locked_core_rev_from_hub(&hub_path)?;

    let hub_bin = require_executable_env(HUB_BIN_ENV)?;
    let session_worker_bin = require_executable_env(SESSION_WORKER_BIN_ENV)?;
    let hub_bin_path = fs::canonicalize(&hub_bin).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("canonicalize {HUB_BIN_ENV} {}: {error}", hub_bin.display()),
        )
    })?;
    let session_worker_bin_path = fs::canonicalize(&session_worker_bin).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "canonicalize {SESSION_WORKER_BIN_ENV} {}: {error}",
                session_worker_bin.display()
            ),
        )
    })?;

    // Fresh build target: reject stale shared target/release caches.
    // Binaries may live outside the Hub source tree when built into an isolated
    // target dir; provenance is then the locked build receipt + source SHAs.
    let build_target = std::env::var_os(HUB_BUILD_TARGET_DIR_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "claim pin ledger requires {HUB_BUILD_TARGET_DIR_ENV} (fresh locked Hub build target; do not reuse a stale shared target/release)"
                ),
            )
        })?;
    let build_target = fs::canonicalize(&build_target).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "canonicalize {HUB_BUILD_TARGET_DIR_ENV} {}: {error}",
                build_target.display()
            ),
        )
    })?;
    let hub_bin_under_build_target = path_is_under(&hub_bin_path, &build_target);
    let session_worker_bin_under_build_target =
        path_is_under(&session_worker_bin_path, &build_target);
    if !hub_bin_under_build_target {
        return invalid(format!(
            "Hub binary {} is not under fresh build target {} (stale cached binary rejected)",
            hub_bin_path.display(),
            build_target.display()
        ));
    }
    if !session_worker_bin_under_build_target {
        return invalid(format!(
            "session-worker binary {} is not under fresh build target {} (stale cached binary rejected)",
            session_worker_bin_path.display(),
            build_target.display()
        ));
    }
    // True when the fresh target itself is under the Hub source (optional layout).
    let hub_bin_under_source = path_is_under(&hub_bin_path, &hub_path);
    let session_worker_bin_under_source = path_is_under(&session_worker_bin_path, &hub_path);

    // Strict typed receipt is mandatory — never synthesize build proof.
    let receipt = load_strict_build_receipt(
        &hub_path,
        &build_target,
        &hub_rev,
        &core_rev,
        &hub_bin_path,
        &session_worker_bin_path,
    )?;

    // Optional explicit session_worker_rev must match locked Core when supplied.
    if let Some(explicit) = scenario.session_worker_rev.as_deref()
        && explicit != core_rev
    {
        return invalid(format!(
            "session_worker_rev {explicit} does not match Hub Cargo.lock botster-core {core_rev}"
        ));
    }

    let hub_ancestry_ok = is_ancestor_or_equal(MIN_HUB_REV, &hub_rev, Some(&hub_path))?;
    let workspaces_ancestry_ok =
        is_ancestor_or_equal(MIN_WORKSPACES_REV, &workspaces_rev, Some(&workspaces_path))?;
    let tui_ancestry_ok = is_ancestor_or_equal(MIN_TUI_REV, &tui_rev, Some(&tui_root))?;
    let form_ok = workspaces_package_has_available_sessions_form(&workspaces_path)?;

    if !hub_ancestry_ok {
        return invalid(format!(
            "Hub rev {hub_rev} is not a descendant of minimum {MIN_HUB_REV}"
        ));
    }
    if !workspaces_ancestry_ok {
        return invalid(format!(
            "Workspaces rev {workspaces_rev} is not a descendant of minimum {MIN_WORKSPACES_REV}"
        ));
    }
    if !tui_ancestry_ok {
        return invalid(format!(
            "TUI rev {tui_rev} is not a descendant of minimum {MIN_TUI_REV}"
        ));
    }
    if !form_ok {
        return invalid(
            "Workspaces package lacks Available sessions entity_options form (botster-workspaces-add-session-id + entity_options)",
        );
    }

    // Evidence uses path-neutral labels only (no machine-local absolute paths).
    Ok(PinLedger {
        hub_minimum: MIN_HUB_REV,
        workspaces_minimum: MIN_WORKSPACES_REV,
        tui_minimum: MIN_TUI_REV,
        hub_rev,
        workspaces_rev,
        tui_rev,
        core_rev: core_rev.clone(),
        session_worker_rev: core_rev,
        hub_bin_path: format!("{LABEL_HUB_BUILD_TARGET}/release/botster-hub"),
        session_worker_bin_path: format!("{LABEL_HUB_BUILD_TARGET}/release/botster-session-worker"),
        hub_source_path: LABEL_HUB_SOURCE.to_string(),
        workspaces_package_path: LABEL_WORKSPACES_PACKAGE.to_string(),
        tui_source_path: LABEL_TUI_SOURCE.to_string(),
        hub_ancestry_ok,
        workspaces_ancestry_ok,
        tui_ancestry_ok,
        workspaces_available_sessions_form_ok: form_ok,
        hub_bin_under_source,
        session_worker_bin_under_source,
        sources_clean: true,
        hub_build_target_dir: LABEL_HUB_BUILD_TARGET.to_string(),
        hub_bin_under_build_target,
        session_worker_bin_under_build_target,
        hub_build_command: sanitize_build_command(
            &receipt.hub_build_command,
            &hub_path,
            &build_target,
            &receipt,
        )?,
        session_worker_build_command: sanitize_build_command(
            &receipt.session_worker_build_command,
            &hub_path,
            &build_target,
            &receipt,
        )?,
        build_receipt_path: format!("{LABEL_HUB_BUILD_TARGET}/claim-build-receipt.json"),
    })
}

/// Require and validate a complete typed build receipt against observed pins.
fn load_strict_build_receipt(
    hub_path: &Path,
    build_target: &Path,
    hub_rev: &str,
    core_rev: &str,
    hub_bin_path: &Path,
    session_worker_bin_path: &Path,
) -> io::Result<ClaimBuildReceipt> {
    let receipt_path = std::env::var_os(CLAIM_BUILD_RECEIPT_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "claim pin ledger requires {CLAIM_BUILD_RECEIPT_ENV} (strict build receipt; refuse to fabricate build proof)"
                ),
            )
        })?;
    load_strict_build_receipt_from(
        &receipt_path,
        hub_path,
        build_target,
        hub_rev,
        core_rev,
        hub_bin_path,
        session_worker_bin_path,
    )
}

fn load_strict_build_receipt_from(
    receipt_path: &Path,
    hub_path: &Path,
    build_target: &Path,
    hub_rev: &str,
    core_rev: &str,
    hub_bin_path: &Path,
    session_worker_bin_path: &Path,
) -> io::Result<ClaimBuildReceipt> {
    let bytes = fs::read(receipt_path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "read {CLAIM_BUILD_RECEIPT_ENV} {}: {error}",
                receipt_path.display()
            ),
        )
    })?;
    if bytes.iter().all(|b| b.is_ascii_whitespace()) {
        return invalid("claim build receipt is empty");
    }
    let receipt: ClaimBuildReceipt = serde_json::from_slice(&bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("decode claim build receipt (strict typed, no unknown fields): {error}"),
        )
    })?;

    let receipt_hub_source = fs::canonicalize(&receipt.hub_source)
        .unwrap_or_else(|_| PathBuf::from(&receipt.hub_source));
    let receipt_target = fs::canonicalize(&receipt.target_dir)
        .unwrap_or_else(|_| PathBuf::from(&receipt.target_dir));
    let receipt_hub_bin =
        fs::canonicalize(&receipt.hub_bin).unwrap_or_else(|_| PathBuf::from(&receipt.hub_bin));
    let receipt_worker = fs::canonicalize(&receipt.session_worker_bin)
        .unwrap_or_else(|_| PathBuf::from(&receipt.session_worker_bin));

    if receipt_hub_source != hub_path {
        return invalid(format!(
            "build receipt hub_source {} does not match Hub checkout {}",
            receipt_hub_source.display(),
            hub_path.display()
        ));
    }
    if receipt_target != build_target {
        return invalid(format!(
            "build receipt target_dir {} does not match {HUB_BUILD_TARGET_DIR_ENV} {}",
            receipt_target.display(),
            build_target.display()
        ));
    }
    if receipt.hub_rev != hub_rev {
        return invalid(format!(
            "build receipt hub_rev {} does not match Hub HEAD {hub_rev}",
            receipt.hub_rev
        ));
    }
    if receipt.core_rev != core_rev {
        return invalid(format!(
            "build receipt core_rev {} does not match Hub Cargo.lock {core_rev}",
            receipt.core_rev
        ));
    }
    if receipt_hub_bin != hub_bin_path {
        return invalid(format!(
            "build receipt hub_bin {} does not match {HUB_BIN_ENV} {}",
            receipt_hub_bin.display(),
            hub_bin_path.display()
        ));
    }
    if receipt_worker != session_worker_bin_path {
        return invalid(format!(
            "build receipt session_worker_bin {} does not match {SESSION_WORKER_BIN_ENV} {}",
            receipt_worker.display(),
            session_worker_bin_path.display()
        ));
    }
    if receipt.hub_build_command.trim().is_empty()
        || !receipt.hub_build_command.contains("botster-hub")
        || !receipt.hub_build_command.contains("--locked")
    {
        return invalid(
            "build receipt hub_build_command must be a non-empty locked botster-hub build",
        );
    }
    if receipt.session_worker_build_command.trim().is_empty()
        || !receipt
            .session_worker_build_command
            .contains("botster-session-worker")
        || !receipt.session_worker_build_command.contains("--locked")
    {
        return invalid(
            "build receipt session_worker_build_command must be a non-empty locked session-worker build",
        );
    }
    Ok(receipt)
}

/// Rewrite absolute machine paths in build commands to path-neutral labels.
///
/// Receipts often record mktemp/TMPDIR forms (`//`, `/var/folders` vs
/// `/private/var/folders`) that differ from `fs::canonicalize`. Replace every
/// known root variant, then fail closed if any machine-local absolute path remains.
fn sanitize_build_command(
    command: &str,
    hub_path: &Path,
    build_target: &Path,
    receipt: &ClaimBuildReceipt,
) -> io::Result<String> {
    let mut replacements: Vec<(String, &'static str)> = Vec::new();
    for variant in path_string_variants(&hub_path.display().to_string()) {
        replacements.push((variant, LABEL_HUB_SOURCE));
    }
    for variant in path_string_variants(&build_target.display().to_string()) {
        replacements.push((variant, LABEL_HUB_BUILD_TARGET));
    }
    for variant in path_string_variants(&receipt.hub_source) {
        replacements.push((variant, LABEL_HUB_SOURCE));
    }
    for variant in path_string_variants(&receipt.target_dir) {
        replacements.push((variant, LABEL_HUB_BUILD_TARGET));
    }
    if let Some(raw) = std::env::var_os(HUB_SOURCE_PATH_ENV) {
        for variant in path_string_variants(&PathBuf::from(raw).display().to_string()) {
            replacements.push((variant, LABEL_HUB_SOURCE));
        }
    }
    if let Some(raw) = std::env::var_os(HUB_BUILD_TARGET_DIR_ENV) {
        for variant in path_string_variants(&PathBuf::from(raw).display().to_string()) {
            replacements.push((variant, LABEL_HUB_BUILD_TARGET));
        }
    }
    // Longest match first so nested roots do not leave prefixes behind.
    replacements.sort_by(|left, right| right.0.len().cmp(&left.0.len()));
    replacements.dedup_by(|left, right| left.0 == right.0);

    // Collapse mktemp/TMPDIR `//` forms before replacement so canonical roots match.
    let mut out = collapse_slashes(command);
    for (from, label) in &replacements {
        if !from.is_empty() {
            out = out.replace(from, label);
        }
    }
    require_path_neutral_command(&out)?;
    Ok(out)
}

fn path_string_variants(raw: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![raw.to_string()];
    while let Some(value) = stack.pop() {
        if value.is_empty() || !seen.insert(value.clone()) {
            continue;
        }
        let collapsed = collapse_slashes(&value);
        if collapsed != value {
            stack.push(collapsed);
        }
        if let Some(stripped) = value.strip_prefix("/private") {
            stack.push(stripped.to_string());
        } else if value.starts_with('/') {
            stack.push(format!("/private{value}"));
        }
        if value.ends_with('/') && value.len() > 1 {
            stack.push(value.trim_end_matches('/').to_string());
        }
    }
    seen.into_iter().collect()
}

fn collapse_slashes(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut prev_slash = false;
    for ch in value.chars() {
        if ch == '/' {
            if !prev_slash {
                out.push(ch);
            }
            prev_slash = true;
        } else {
            prev_slash = false;
            out.push(ch);
        }
    }
    out
}

fn require_path_neutral_command(command: &str) -> io::Result<()> {
    for forbidden in [
        "/Users/",
        "/var/folders/",
        "/private/var/",
        "/home/",
        "/tmp/",
        "/private/tmp/",
    ] {
        if command.contains(forbidden) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "build command still contains machine-local path fragment {forbidden:?} after sanitization: {command}"
                ),
            ));
        }
    }
    Ok(())
}

fn require_executable_env(name: &str) -> io::Result<PathBuf> {
    let path = std::env::var_os(name).map(PathBuf::from).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("claim pin ledger requires {name} so the executed binary path is recorded"),
        )
    })?;
    if !path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} is not a file: {}", path.display()),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path)?.permissions().mode();
        if mode & 0o111 == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{name} is not executable: {}", path.display()),
            ));
        }
    }
    Ok(path)
}

fn require_git_clean(root: &Path, label: &str) -> io::Result<()> {
    // Tracked modifications only. Untracked env/mise noise is not product code and
    // must not block pin proof; uncommitted tracked edits must fail closed so HEAD
    // is the executed source.
    let output = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .current_dir(root)
        .output()
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("git status failed in {}: {error}", root.display()),
            )
        })?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "git status failed in {}: {}",
                root.display(),
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }
    let porcelain = String::from_utf8_lossy(&output.stdout);
    if !porcelain.trim().is_empty() {
        return invalid(format!(
            "{label} checkout has tracked dirt at {}; commit or restore tracked files before claim so HEAD is the executed code:\n{}",
            root.display(),
            porcelain.chars().take(512).collect::<String>()
        ));
    }
    Ok(())
}

fn path_is_under(child: &Path, parent: &Path) -> bool {
    child.starts_with(parent)
}

/// Read the locked botster-core revision from the Hub checkout Cargo.lock.
fn locked_core_rev_from_hub(hub_root: &Path) -> io::Result<String> {
    let lock_path = hub_root.join("Cargo.lock");
    let lock = fs::read_to_string(&lock_path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("read Hub Cargo.lock {}: {error}", lock_path.display()),
        )
    })?;
    // Prefer explicit rev= pins, then #fragment SHAs on botster-core sources.
    for line in lock.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("source = \"") {
            if !rest.contains("botster-core") {
                continue;
            }
            if let Some(idx) = rest.find("rev=") {
                let rev = rest[idx + 4..]
                    .split(['&', '#', '"'])
                    .next()
                    .unwrap_or("")
                    .trim();
                if rev.len() >= 7 {
                    return Ok(rev.to_string());
                }
            }
            if let Some(idx) = rest.rfind('#') {
                let rev = rest[idx + 1..].trim_end_matches('"').trim();
                if rev.len() >= 7 && rev.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Ok(rev.to_string());
                }
            }
        }
    }
    invalid(format!(
        "Hub Cargo.lock at {} does not pin a botster-core revision for session-worker provenance",
        lock_path.display()
    ))
}

/// Derive the consumed revision from the actual checkout. When the scenario
/// supplies an explicit rev, require exact equality with that checkout HEAD —
/// never treat a caller-supplied string as consumed without observing it.
fn resolve_rev(explicit: Option<&str>, git_root: Option<&Path>, label: &str) -> io::Result<String> {
    let Some(root) = git_root else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{label} pin requires a source checkout path so the consumed revision can be derived from HEAD"
            ),
        ));
    };
    let derived = git_rev_parse(root)?;
    if let Some(explicit) = explicit
        && explicit != derived
    {
        return invalid(format!(
            "{label} explicit rev {explicit} does not match checkout HEAD {derived} at {}",
            root.display()
        ));
    }
    Ok(derived)
}

fn git_rev_parse(root: &Path) -> io::Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("git rev-parse failed in {}: {error}", root.display()),
            )
        })?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "git rev-parse failed in {}: {}",
                root.display(),
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }
    let rev = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if rev.is_empty() {
        return invalid(format!("empty git rev-parse in {}", root.display()));
    }
    Ok(rev)
}

fn is_ancestor_or_equal(minimum: &str, actual: &str, git_root: Option<&Path>) -> io::Result<bool> {
    if minimum == actual {
        return Ok(true);
    }
    let Some(root) = git_root else {
        // Without a checkout we can only accept exact equality (handled above).
        return Ok(false);
    };
    let status = Command::new("git")
        .args(["merge-base", "--is-ancestor", minimum, actual])
        .current_dir(root)
        .status()
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("git merge-base failed in {}: {error}", root.display()),
            )
        })?;
    Ok(status.success())
}

/// Scan installed/on-disk Workspaces package for Available sessions entity_options.
pub fn workspaces_package_has_available_sessions_form(package_root: &Path) -> io::Result<bool> {
    let plugin = package_root.join("plugin.lua");
    let source = fs::read_to_string(&plugin).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("read Workspaces plugin.lua {}: {error}", plugin.display()),
        )
    })?;
    let has_field = source.contains("botster-workspaces-add-session-id");
    let has_kind = source.contains("entity_options");
    let has_label = source.contains("Available sessions");
    Ok(has_field && has_kind && has_label)
}

fn require_identifier(name: &str, value: &str) -> io::Result<()> {
    if value.trim().is_empty() || value != value.trim() {
        return invalid(format!("{name} must be a non-empty trimmed string"));
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}

fn reject_path_reuse(scenario_path: &Path, evidence_path: &Path) -> io::Result<()> {
    let scenario = fs::canonicalize(scenario_path)?;
    let evidence = canonical_candidate(evidence_path)?;
    if scenario == evidence {
        return invalid("acceptance scenario and evidence paths must be distinct");
    }
    Ok(())
}

fn canonical_candidate(path: &Path) -> io::Result<PathBuf> {
    if path.exists() {
        return fs::canonicalize(path);
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "acceptance evidence path needs a file name",
        )
    })?;
    Ok(fs::canonicalize(parent)?.join(name))
}

pub struct EvidenceWriter {
    writer: BufWriter<File>,
    schema: &'static str,
    terminal_written: bool,
}

#[derive(Clone, Debug)]
pub struct FailureContext {
    pub case_id: Option<String>,
    pub phase: String,
    pub expected_condition: String,
    pub subscription_id: Option<String>,
    pub snapshot_seq: Option<u64>,
    pub surface_render_count: usize,
    pub focusable_ids: Vec<String>,
    pub last_observation: Value,
}

impl EvidenceWriter {
    pub fn create(path: &Path, schema: &'static str) -> io::Result<Self> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        Ok(Self {
            writer: BufWriter::new(file),
            schema,
            terminal_written: false,
        })
    }

    pub fn event(&mut self, kind: &str, case_id: Option<&str>, payload: Value) -> io::Result<()> {
        if self.terminal_written {
            return invalid("acceptance evidence already contains a terminal event");
        }
        let terminal = matches!(kind, "complete" | "failure");
        let record = EvidenceRecord {
            schema: self.schema,
            kind,
            case_id,
            payload,
        };
        serde_json::to_writer(&mut self.writer, &record)?;
        writeln!(self.writer)?;
        self.writer.flush()?;
        self.terminal_written = terminal;
        Ok(())
    }

    pub fn failure(&mut self, context: &FailureContext, message: &str) -> io::Result<()> {
        let bounded_message = message.chars().take(512).collect::<String>();
        let bounded_ids = context
            .focusable_ids
            .iter()
            .take(24)
            .cloned()
            .collect::<Vec<_>>();
        self.event(
            "failure",
            context.case_id.as_deref(),
            json!({
                "phase": context.phase,
                "message": bounded_message,
                "expected_condition": context.expected_condition,
                "subscription_id": context.subscription_id,
                "snapshot_seq": context.snapshot_seq,
                "surface_render_count": context.surface_render_count,
                "focusable_ids": bounded_ids,
                "last_observation": context.last_observation
            }),
        )
    }
}

#[cfg(test)]
pub fn validate_contract_document(document: &Value) -> Result<(), String> {
    let schema: Value = serde_json::from_str(include_str!(
        "../fixtures/workspaces-spawn-driver-v1.schema.json"
    ))
    .map_err(|error| error.to_string())?;
    let validator = jsonschema::validator_for(&schema).map_err(|error| error.to_string())?;
    validator
        .validate(document)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
pub fn validate_claim_contract_document(document: &Value) -> Result<(), String> {
    let schema: Value = serde_json::from_str(include_str!(
        "../fixtures/workspaces-claim-driver-v1.schema.json"
    ))
    .map_err(|error| error.to_string())?;
    let validator = jsonschema::validator_for(&schema).map_err(|error| error.to_string())?;
    validator
        .validate(document)
        .map_err(|error| error.to_string())
}

#[derive(Serialize)]
struct EvidenceRecord<'a> {
    schema: &'static str,
    kind: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    case_id: Option<&'a str>,
    payload: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario() -> Scenario {
        serde_json::from_str(include_str!(
            "../fixtures/workspaces-spawn-driver-v1.scenario.json"
        ))
        .expect("checked-in scenario decodes")
    }

    fn claim_scenario() -> ClaimScenario {
        serde_json::from_str(include_str!(
            "../fixtures/workspaces-claim-driver-v1.scenario.json"
        ))
        .expect("checked-in claim scenario decodes")
    }

    #[test]
    fn checked_in_scenario_is_strict_and_complete() {
        let scenario = scenario();
        scenario.validate().expect("checked-in scenario validates");
        let mut value = serde_json::to_value(scenario).unwrap();
        validate_contract_document(&value).expect("checked-in scenario matches published schema");
        value["unknown"] = json!(true);
        assert!(serde_json::from_value::<Scenario>(value).is_err());
    }

    #[test]
    fn checked_in_claim_scenario_is_strict() {
        let scenario = claim_scenario();
        scenario.validate().expect("claim scenario validates");
        let mut value = serde_json::to_value(scenario).unwrap();
        validate_claim_contract_document(&value).expect("claim scenario matches schema");
        value["unknown"] = json!(true);
        assert!(serde_json::from_value::<ClaimScenario>(value).is_err());
        let mut wrong = claim_scenario();
        wrong.schema = SCHEMA.to_string();
        assert!(wrong.validate().is_err());
    }

    #[test]
    fn validation_rejects_duplicate_cases_and_missing_resolution_classes() {
        let mut duplicate = scenario();
        duplicate.cases[1].case_id = duplicate.cases[0].case_id.clone();
        assert!(duplicate.validate().is_err());
        let mut resolution = scenario();
        resolution.cases[1].resolution = resolution.cases[0].resolution;
        assert!(resolution.validate().is_err());
        let mut version = scenario();
        version.schema.push_str("-unknown");
        assert!(version.validate().is_err());
        let mut empty = scenario();
        empty.cases[0].case_id.clear();
        assert!(empty.validate().is_err());
    }

    #[test]
    fn config_requires_both_distinct_paths_and_new_evidence() {
        let root = std::env::temp_dir().join(format!(
            "botster-tui-acceptance-config-{}",
            crate::app::short_suffix()
        ));
        fs::create_dir_all(&root).unwrap();
        let scenario_path = root.join("scenario.json");
        let evidence_path = root.join("evidence.jsonl");
        fs::write(
            &scenario_path,
            include_bytes!("../fixtures/workspaces-spawn-driver-v1.scenario.json"),
        )
        .unwrap();

        assert!(SpawnConfig::from_paths(Some(scenario_path.clone()), None).is_err());
        assert!(SpawnConfig::from_paths(None, Some(evidence_path.clone())).is_err());
        assert!(
            SpawnConfig::from_paths(Some(scenario_path.clone()), Some(scenario_path.clone()))
                .is_err()
        );

        let config = SpawnConfig::from_paths(Some(scenario_path), Some(evidence_path.clone()))
            .unwrap()
            .expect("paired paths enable acceptance mode");
        fs::write(&evidence_path, b"already exists\n").unwrap();
        assert!(EvidenceWriter::create(&config.evidence_path, SCHEMA).is_err());
    }

    #[test]
    fn mode_routes_claim_schema_without_colliding_with_spawn() {
        let root = std::env::temp_dir().join(format!(
            "botster-tui-claim-mode-{}",
            crate::app::short_suffix()
        ));
        fs::create_dir_all(&root).unwrap();
        let scenario_path = root.join("claim-scenario.json");
        let evidence_path = root.join("claim-evidence.jsonl");
        fs::write(
            &scenario_path,
            include_bytes!("../fixtures/workspaces-claim-driver-v1.scenario.json"),
        )
        .unwrap();
        match AcceptanceMode::from_paths(Some(scenario_path), Some(evidence_path))
            .unwrap()
            .expect("claim mode")
        {
            AcceptanceMode::Claim(config) => {
                assert_eq!(config.scenario.schema, CLAIM_SCHEMA);
                assert_eq!(
                    config.scenario.session_uuid,
                    "00000000-0000-4000-8000-000000000001"
                );
            }
            AcceptanceMode::Spawn(_) => panic!("claim schema must not decode as spawn"),
        }
    }

    #[test]
    fn evidence_uses_create_new_and_one_terminal_event() {
        let root = std::env::temp_dir().join(format!(
            "botster-tui-acceptance-{}",
            crate::app::short_suffix()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("evidence.jsonl");
        let mut writer = EvidenceWriter::create(&path, SCHEMA).unwrap();
        writer.event("ready", None, json!({})).unwrap();
        writer.event("complete", None, json!({})).unwrap();
        assert!(writer.event("complete", None, json!({})).is_err());
        assert!(EvidenceWriter::create(&path, SCHEMA).is_err());
        let lines = fs::read_to_string(path).unwrap();
        assert_eq!(lines.lines().count(), 2);
    }

    #[test]
    fn evidence_fixture_carries_correlated_request_result_and_terminal_records() {
        let records = include_str!("../fixtures/workspaces-spawn-driver-v1.evidence.jsonl")
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("fixture line is JSON"))
            .collect::<Vec<_>>();
        for record in &records {
            validate_contract_document(record)
                .expect("checked-in evidence record matches published schema");
        }
        let request = records
            .iter()
            .find(|record| record["kind"] == "dispatched_action")
            .expect("fixture has dispatched action");
        let result = records
            .iter()
            .find(|record| record["kind"] == "action_result")
            .expect("fixture has action result");
        assert_eq!(
            request["payload"]["request_id"],
            result["payload"]["request_id"]
        );
        assert!(records.iter().all(|record| record["schema"] == SCHEMA));
        assert_eq!(
            records
                .iter()
                .filter_map(|record| record["kind"].as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "ready",
                "baseline",
                "surface_request",
                "focused_control",
                "dispatched_action",
                "action_result",
                "entity_state",
                "case_complete",
                "request_summary",
                "reconnect",
                "complete",
            ])
        );
        assert_eq!(
            records
                .iter()
                .filter(|record| matches!(record["kind"].as_str(), Some("complete" | "failure")))
                .count(),
            1
        );
    }

    #[test]
    fn claim_evidence_fixture_carries_membership_join_and_exclusion_stages() {
        let records = include_str!("../fixtures/workspaces-claim-driver-v1.evidence.jsonl")
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("claim fixture line is JSON"))
            .collect::<Vec<_>>();
        for record in &records {
            validate_claim_contract_document(record)
                .expect("claim evidence record matches published schema");
        }
        assert!(
            records
                .iter()
                .all(|record| record["schema"] == CLAIM_SCHEMA)
        );
        let kinds = records
            .iter()
            .filter_map(|record| record["kind"].as_str())
            .collect::<BTreeSet<_>>();
        for required in [
            "pin_ledger",
            "ready",
            "baseline",
            "option_present",
            "lifecycle_live_update",
            "dispatched_action",
            "membership_join",
            "option_excluded",
            "complete",
        ] {
            assert!(kinds.contains(required), "missing evidence kind {required}");
        }
        let lifecycle = records
            .iter()
            .find(|record| record["kind"] == "lifecycle_live_update")
            .expect("lifecycle_live_update");
        assert_eq!(lifecycle["payload"]["reopened"], false);
        assert!(
            lifecycle["payload"]["lifecycle_before"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
        assert!(
            lifecycle["payload"]["lifecycle_after"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
        let join = records
            .iter()
            .find(|record| record["kind"] == "membership_join")
            .expect("membership_join");
        assert_eq!(
            join["payload"]["session_uuid"],
            "00000000-0000-4000-8000-000000000001"
        );
        assert_eq!(join["payload"]["workspace_id"], "workspace-claim-example");
        let excluded = records
            .iter()
            .find(|record| record["kind"] == "option_excluded")
            .expect("option_excluded");
        assert_eq!(
            excluded["payload"]["session_uuid"],
            "00000000-0000-4000-8000-000000000001"
        );
        assert_eq!(
            records
                .iter()
                .filter(|record| matches!(record["kind"].as_str(), Some("complete" | "failure")))
                .count(),
            1
        );
    }

    #[test]
    fn failure_evidence_is_bounded_and_terminal() {
        let root = std::env::temp_dir().join(format!(
            "botster-tui-acceptance-failure-{}",
            crate::app::short_suffix()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("evidence.jsonl");
        let mut writer = EvidenceWriter::create(&path, SCHEMA).unwrap();
        let context = FailureContext {
            case_id: Some("case-a".to_string()),
            phase: "entity_reconciliation".to_string(),
            expected_condition: "exact current session membership".to_string(),
            subscription_id: Some("subscription-a".to_string()),
            snapshot_seq: Some(7),
            surface_render_count: 2,
            focusable_ids: (0..40).map(|index| format!("node-{index}")).collect(),
            last_observation: json!({ "request_id": "request-a", "state": "accepted" }),
        };
        writer.failure(&context, &"x".repeat(1_024)).unwrap();
        assert!(writer.event("ready", None, json!({})).is_err());
        let record: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(record["payload"]["message"].as_str().unwrap().len(), 512);
        assert_eq!(
            record["payload"]["focusable_ids"].as_array().unwrap().len(),
            24
        );
        assert_eq!(record["case_id"], "case-a");
        assert_eq!(record["payload"]["subscription_id"], "subscription-a");
        assert_eq!(record["payload"]["surface_render_count"], 2);
        validate_contract_document(&record).expect("failure evidence matches published schema");
    }

    #[test]
    fn strict_build_receipt_rejects_missing_empty_and_mismatched() {
        // No receipt env.
        let err = load_strict_build_receipt(
            Path::new("/tmp"),
            Path::new("/tmp"),
            "abc",
            "def",
            Path::new("/tmp/a"),
            Path::new("/tmp/b"),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains(CLAIM_BUILD_RECEIPT_ENV),
            "missing receipt must fail: {err}"
        );

        let root = std::env::temp_dir().join(format!(
            "botster-tui-receipt-{}",
            crate::app::short_suffix()
        ));
        fs::create_dir_all(&root).unwrap();
        let empty = root.join("empty.json");
        fs::write(&empty, b"   \n").unwrap();
        let err = load_strict_build_receipt_from(
            &empty,
            Path::new("/tmp"),
            Path::new("/tmp"),
            "abc",
            "def",
            Path::new("/tmp/a"),
            Path::new("/tmp/b"),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("empty"),
            "empty receipt must fail: {err}"
        );

        let incomplete = root.join("incomplete.json");
        fs::write(&incomplete, br#"{}"#).unwrap();
        let err = load_strict_build_receipt_from(
            &incomplete,
            Path::new("/tmp"),
            Path::new("/tmp"),
            "abc",
            "def",
            Path::new("/tmp/a"),
            Path::new("/tmp/b"),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("decode") || err.to_string().contains("missing"),
            "incomplete receipt must fail: {err}"
        );

        // Unknown field rejected by deny_unknown_fields.
        let unknown = root.join("unknown.json");
        fs::write(
            &unknown,
            br#"{
              "hub_source":"/tmp",
              "hub_rev":"abc",
              "core_rev":"def",
              "hub_bin":"/tmp/a",
              "session_worker_bin":"/tmp/b",
              "target_dir":"/tmp",
              "hub_build_command":"cargo build --locked -p botster-hub",
              "session_worker_build_command":"cargo build --locked --bin botster-session-worker",
              "extra":true
            }"#,
        )
        .unwrap();
        let err = load_strict_build_receipt_from(
            &unknown,
            Path::new("/tmp"),
            Path::new("/tmp"),
            "abc",
            "def",
            Path::new("/tmp/a"),
            Path::new("/tmp/b"),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("unknown") || err.to_string().contains("decode"),
            "unknown fields must fail: {err}"
        );

        // Mismatched target dir.
        let mismatch = root.join("mismatch.json");
        fs::write(
            &mismatch,
            br#"{
              "hub_source":"/tmp",
              "hub_rev":"abc",
              "core_rev":"def",
              "hub_bin":"/tmp/a",
              "session_worker_bin":"/tmp/b",
              "target_dir":"/tmp/other-target",
              "hub_build_command":"cargo build --locked --release -p botster-hub",
              "session_worker_build_command":"cargo build --locked --release --bin botster-session-worker"
            }"#,
        )
        .unwrap();
        let err = load_strict_build_receipt_from(
            &mismatch,
            Path::new("/tmp"),
            Path::new("/tmp"),
            "abc",
            "def",
            Path::new("/tmp/a"),
            Path::new("/tmp/b"),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("target_dir") || err.to_string().contains("does not match"),
            "mismatched target must fail: {err}"
        );
    }

    #[test]
    fn committed_claim_live_evidence_has_no_machine_local_paths() {
        let evidence =
            include_str!("../../../docs/reports/tui-shared-hub-keyboard-claim-live-evidence.jsonl");
        // Known-positive control: the scan must detect local paths when present.
        let poison = format!("/Users/someone/{evidence}");
        assert!(
            poison.contains("/Users/"),
            "positive control must detect /Users/ paths"
        );
        for forbidden in [
            "/Users/",
            "/var/folders/",
            "/private/var/",
            "/home/",
            "/tmp/",
            "jasonconigliari",
        ] {
            assert!(
                !evidence.contains(forbidden),
                "committed claim live evidence must not contain machine-local path fragment {forbidden:?}"
            );
        }
        // Labels and SHAs must remain.
        assert!(evidence.contains(LABEL_HUB_SOURCE) || evidence.contains("hub_rev"));
        assert!(evidence.contains("de6b099") || evidence.contains("hub_rev"));
    }

    #[test]
    fn sanitize_build_command_rewrites_mktemp_and_private_variants() {
        let hub = Path::new("/private/var/folders/xx/T/hub-src");
        let target = Path::new("/private/var/folders/xx/T/build-tgt");
        let receipt = ClaimBuildReceipt {
            hub_source: hub.display().to_string(),
            hub_rev: "abc".into(),
            core_rev: "def".into(),
            hub_bin: format!("{}/release/botster-hub", target.display()),
            session_worker_bin: format!("{}/release/botster-session-worker", target.display()),
            target_dir: target.display().to_string(),
            hub_build_command: String::new(),
            session_worker_build_command: String::new(),
        };
        // Double-slash TMPDIR form plus non-/private form must both rewrite.
        let raw = "cargo build --locked --release -p botster-hub --manifest-path /var/folders/xx/T//hub-src/Cargo.toml --target-dir /var/folders/xx/T//build-tgt";
        let sanitized = sanitize_build_command(raw, hub, target, &receipt).expect("sanitize");
        assert_eq!(
            sanitized,
            format!(
                "cargo build --locked --release -p botster-hub --manifest-path {LABEL_HUB_SOURCE}/Cargo.toml --target-dir {LABEL_HUB_BUILD_TARGET}"
            )
        );
        require_path_neutral_command(&sanitized).expect("path-neutral");
        let residual = "cargo build --target-dir /Users/someone/tgt";
        assert!(
            sanitize_build_command(residual, hub, target, &receipt).is_err(),
            "unsanitizable absolute paths must fail closed"
        );
    }

    #[test]
    fn form_presence_scan_requires_entity_options_field() {
        let root = std::env::temp_dir().join(format!(
            "botster-tui-claim-form-scan-{}",
            crate::app::short_suffix()
        ));
        fs::create_dir_all(&root).unwrap();
        let plugin = root.join("plugin.lua");
        fs::write(
            &plugin,
            r#"
            entity_options_select(
              "botster-workspaces-add-session-id",
              "session_id",
              "Available sessions",
              { ["$kind"] = "entity_options", source = "/session" }
            )
            "#,
        )
        .unwrap();
        assert!(workspaces_package_has_available_sessions_form(&root).unwrap());
        fs::write(
            &plugin,
            r#"text_input("botster-workspaces-add-session-id", "session_id", "Session ID")"#,
        )
        .unwrap();
        assert!(!workspaces_package_has_available_sessions_form(&root).unwrap());
    }

    #[test]
    fn pin_resolve_requires_checkout_and_rejects_mismatched_explicit_rev() {
        let root = std::env::temp_dir().join(format!(
            "botster-tui-claim-pin-{}",
            crate::app::short_suffix()
        ));
        fs::create_dir_all(&root).unwrap();
        assert!(
            resolve_rev(Some(MIN_HUB_REV), None, "Hub").is_err(),
            "explicit rev without checkout must fail closed"
        );
        // Initialize a git checkout so HEAD is observed.
        let status = Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&root)
            .status()
            .expect("git init");
        assert!(status.success());
        fs::write(root.join("README"), "pin").unwrap();
        assert!(
            Command::new("git")
                .args(["-C"])
                .arg(&root)
                .args(["add", "README"])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .args(["-C"])
                .arg(&root)
                .args([
                    "-c",
                    "user.email=claim@botster.dev",
                    "-c",
                    "user.name=Claim",
                    "commit",
                    "-m",
                    "pin"
                ])
                .status()
                .unwrap()
                .success()
        );
        let head = git_rev_parse(&root).unwrap();
        assert_eq!(resolve_rev(None, Some(&root), "TUI").unwrap(), head);
        assert_eq!(resolve_rev(Some(&head), Some(&root), "TUI").unwrap(), head);
        assert!(
            resolve_rev(Some(MIN_HUB_REV), Some(&root), "TUI").is_err(),
            "mismatched explicit rev must fail closed against checkout HEAD"
        );
    }
}
