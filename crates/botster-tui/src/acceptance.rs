use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const SCHEMA: &str = "botster.tui.workspaces-spawn-driver/v1";
pub const SCENARIO_ENV: &str = "BOTSTER_TUI_ACCEPTANCE_SCENARIO";
pub const EVIDENCE_ENV: &str = "BOTSTER_TUI_ACCEPTANCE_EVIDENCE";

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

#[derive(Debug)]
pub struct Config {
    pub scenario: Scenario,
    pub evidence_path: PathBuf,
}

impl Config {
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
        let scenario: Scenario = serde_json::from_slice(&bytes).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("decode acceptance scenario: {error}"),
            )
        })?;
        scenario.validate()?;
        Ok(Some(Self {
            scenario,
            evidence_path,
        }))
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

fn require_identifier(name: &str, value: &str) -> io::Result<()> {
    if value.trim().is_empty() || value != value.trim() {
        return invalid(format!("{name} must be a non-empty trimmed string"));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> io::Result<()> {
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
    pub fn create(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        Ok(Self {
            writer: BufWriter::new(file),
            terminal_written: false,
        })
    }

    pub fn event(&mut self, kind: &str, case_id: Option<&str>, payload: Value) -> io::Result<()> {
        if self.terminal_written {
            return invalid("acceptance evidence already contains a terminal event");
        }
        let terminal = matches!(kind, "complete" | "failure");
        let record = EvidenceRecord {
            schema: SCHEMA,
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

        assert!(Config::from_paths(Some(scenario_path.clone()), None).is_err());
        assert!(Config::from_paths(None, Some(evidence_path.clone())).is_err());
        assert!(
            Config::from_paths(Some(scenario_path.clone()), Some(scenario_path.clone())).is_err()
        );

        let config = Config::from_paths(Some(scenario_path), Some(evidence_path.clone()))
            .unwrap()
            .expect("paired paths enable acceptance mode");
        fs::write(&evidence_path, b"already exists\n").unwrap();
        assert!(EvidenceWriter::create(&config.evidence_path).is_err());
    }

    #[test]
    fn evidence_uses_create_new_and_one_terminal_event() {
        let root = std::env::temp_dir().join(format!(
            "botster-tui-acceptance-{}",
            crate::app::short_suffix()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("evidence.jsonl");
        let mut writer = EvidenceWriter::create(&path).unwrap();
        writer.event("ready", None, json!({})).unwrap();
        writer.event("complete", None, json!({})).unwrap();
        assert!(writer.event("complete", None, json!({})).is_err());
        assert!(EvidenceWriter::create(&path).is_err());
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
    fn failure_evidence_is_bounded_and_terminal() {
        let root = std::env::temp_dir().join(format!(
            "botster-tui-acceptance-failure-{}",
            crate::app::short_suffix()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("evidence.jsonl");
        let mut writer = EvidenceWriter::create(&path).unwrap();
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
}
