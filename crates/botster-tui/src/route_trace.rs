//! Opt-in terminal route counters for diagnosing lost output.
//!
//! Set `BOTSTER_TUI_ROUTE_TRACE=<file>` to enable. The trace is off by
//! default. It keeps counters only, never payload bytes, keyed by terminal
//! route and route generation, plus the Attach requests and responses. The
//! counters are written as JSON to the file on detach and on exit.
//!
//! The reader thread and the application loop both record, so the sink is
//! one process-wide mutex. Every call is a no-op when the trace is off.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::Value;

pub const ROUTE_TRACE_ENV: &str = "BOTSTER_TUI_ROUTE_TRACE";

static TRACE: OnceLock<Option<Mutex<RouteTrace>>> = OnceLock::new();

#[derive(Serialize)]
struct RouteTrace {
    #[serde(skip)]
    path: PathBuf,
    /// Key: `<route> gen=<route generation>`.
    routes: BTreeMap<String, RouteCounters>,
    attach: Vec<Value>,
    dumps: u64,
    last_dump_reason: String,
    last_dump_unix_ms: u128,
}

#[derive(Default, Serialize)]
struct RouteCounters {
    frames: BTreeMap<String, u64>,
    bytes: BTreeMap<String, u64>,
    first_unix_ms: u128,
    last_unix_ms: BTreeMap<String, u128>,
}

/// Enable the trace when the environment names an output file.
pub fn init_from_env() {
    init(std::env::var_os(ROUTE_TRACE_ENV).map(PathBuf::from));
}

fn init(path: Option<PathBuf>) {
    let _ = TRACE.set(path.map(|path| Mutex::new(RouteTrace::new(path))));
}

/// Whether the trace is on. Callers gate trace-only work (formatting,
/// cloning, serialization) behind this so the off path does nothing.
pub fn enabled() -> bool {
    matches!(TRACE.get(), Some(Some(_)))
}

fn with(record: impl FnOnce(&mut RouteTrace)) {
    if let Some(Some(trace)) = TRACE.get()
        && let Ok(mut trace) = trace.lock()
    {
        record(&mut trace);
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default()
}

/// Count one frame of `bytes` at `stage` for one route generation.
pub fn count(route: &str, route_generation: u64, stage: &str, bytes: usize) {
    with(|trace| trace.count(route, route_generation, stage, bytes));
}

/// Record one Attach request, response, or failure in full. The record is
/// built only when the trace is on.
pub fn attach(record: impl FnOnce() -> Value) {
    if enabled() {
        let record = record();
        with(|trace| trace.attach(record));
    }
}

/// Write the current counters to the trace file (write then rename).
pub fn dump(reason: &str) {
    with(|trace| trace.dump(reason));
}

impl RouteTrace {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            routes: BTreeMap::new(),
            attach: Vec::new(),
            dumps: 0,
            last_dump_reason: String::new(),
            last_dump_unix_ms: 0,
        }
    }

    fn count(&mut self, route: &str, route_generation: u64, stage: &str, bytes: usize) {
        let now = now_ms();
        let counters = self
            .routes
            .entry(format!("{route} gen={route_generation}"))
            .or_default();
        if counters.first_unix_ms == 0 {
            counters.first_unix_ms = now;
        }
        *counters.frames.entry(stage.to_string()).or_default() += 1;
        *counters.bytes.entry(stage.to_string()).or_default() += bytes as u64;
        counters.last_unix_ms.insert(stage.to_string(), now);
    }

    fn attach(&mut self, mut record: Value) {
        if let Value::Object(fields) = &mut record {
            fields.insert("unix_ms".to_string(), Value::from(now_ms() as u64));
        }
        self.attach.push(record);
    }

    fn dump(&mut self, reason: &str) {
        self.dumps += 1;
        self.last_dump_reason = reason.to_string();
        self.last_dump_unix_ms = now_ms();
        let Ok(json) = serde_json::to_vec_pretty(&*self) else {
            return;
        };
        let partial = self.path.with_extension("partial");
        if std::fs::write(&partial, json).is_ok() {
            let _ = std::fs::rename(&partial, &self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests never set the process-wide sink, so it is off here.
    #[test]
    fn off_trace_builds_no_attach_record() {
        assert!(!enabled());
        attach(|| panic!("an Attach record was built while the trace is off"));
        count("route", 1, "wire", 1);
        dump("off");
    }

    /// Exercises one local trace; the process-wide sink stays off in tests.
    #[test]
    fn counters_and_attach_records_dump_as_json_without_payloads() {
        let path =
            std::env::temp_dir().join(format!("btui-route-trace-{}.json", std::process::id()));
        let mut trace = RouteTrace::new(path.clone());
        trace.count("btui-sub-1-2", 7, "wire", 40);
        trace.count("btui-sub-1-2", 7, "wire", 2);
        trace.count("btui-sub-1-2", 7, "drop_generation_mismatch", 2);
        trace.attach(serde_json::json!({ "event": "attach_request", "route": "btui-sub-1-2" }));
        trace.dump("test");

        let json: Value =
            serde_json::from_slice(&std::fs::read(&path).expect("trace file")).expect("json");
        let _ = std::fs::remove_file(&path);
        let route = &json["routes"]["btui-sub-1-2 gen=7"];
        assert_eq!(route["frames"]["wire"], 2);
        assert_eq!(route["bytes"]["wire"], 42);
        assert_eq!(route["frames"]["drop_generation_mismatch"], 1);
        assert_eq!(json["attach"][0]["event"], "attach_request");
        assert!(json["attach"][0]["unix_ms"].as_u64().is_some());
        assert_eq!(json["last_dump_reason"], "test");
        assert!(!path.with_extension("partial").exists());
    }
}
