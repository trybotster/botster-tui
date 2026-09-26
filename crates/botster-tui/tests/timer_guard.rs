//! Source guard: every sleep, timeout, or timer in this repository carries a
//! `timer:` marker naming why it is allowed.
//!
//! Waiting for something to become true must use an event (a channel, a
//! wake, a PTY byte, a process exit). The only timers left are marked on the
//! same or the previous line with one of:
//!
//! - `// timer: deadline — <what expires and the exceptional outcome>`
//! - `// timer: backoff — <which error>`
//! - `// timer: rate-limit — <what is capped and why>`
//! - `// timer: os-no-event — <which OS fact has no event source>`
//! - `// timer: ui-lifetime — <element, duration source>`
//! - `// timer: measurement-window — <what rate is measured>` (resource probes
//!   and benchmarks only; this repository has none today)
//!
//! The scan covers Rust sources under `crates/` and every file in `script/`,
//! including shell `sleep` inside Rust string literals. Comment-only lines are
//! skipped; this file is excluded because it names the patterns it bans.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Known DEFECT sites that wait for an event source owned by another
/// repository: (file, exact trimmed line, reason). Every entry must still
/// match, so fixing a site forces its entry out of this list.
const PENDING_DEFECTS: &[(&str, &str, &str)] = &[
    (
        "crates/botster-tui/tests/live_tui.rs",
        "let until = Instant::now() + SESSION_RUNNING_DEADLINE;",
        "PersistentHub readiness polls Status until `botster-hub start --ready-fd` lands (Hub pair)",
    ),
    (
        "crates/botster-tui/tests/live_tui.rs",
        "thread::sleep(Duration::from_millis(100));",
        "PersistentHub readiness polls Status until `botster-hub start --ready-fd` lands (Hub pair)",
    ),
];

const CATEGORIES: [&str; 6] = [
    "deadline",
    "backoff",
    "rate-limit",
    "os-no-event",
    "ui-lifetime",
    "measurement-window",
];

/// Whether `line` starts, waits on, or creates a timer.
fn is_timer_line(line: &str) -> bool {
    let code = line.trim_start();
    if code.starts_with("//") || code.starts_with('#') && !code.starts_with("#[") {
        return false;
    }
    let words = |needle: &str| {
        code.match_indices(needle).any(|(at, _)| {
            let before = code[..at].chars().next_back();
            let after = code[at + needle.len()..].chars().next();
            !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
                && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
    };
    words("sleep")
        || code.contains("timeout(")
        || code.contains("event::poll(")
        || code.contains("Instant::now() +")
}

/// Whether `line` carries a valid `timer:` marker with a reason.
fn has_marker(line: &str) -> bool {
    let Some((_, marker)) = line.split_once("// timer: ") else {
        return false;
    };
    CATEGORIES.iter().any(|category| {
        marker.strip_prefix(category).is_some_and(|rest| {
            let reason = rest.trim_start().trim_start_matches(['—', '-']).trim();
            (rest.starts_with(" —") || rest.starts_with(" -")) && !reason.is_empty()
        })
    })
}

/// Unmarked timer lines in `source`, as 1-based line numbers.
fn violations(source: &str) -> Vec<usize> {
    let lines = source.lines().collect::<Vec<_>>();
    lines
        .iter()
        .enumerate()
        .filter(|(index, line)| {
            is_timer_line(line)
                && !has_marker(line)
                && !index
                    .checked_sub(1)
                    .is_some_and(|previous| has_marker(lines[previous]))
        })
        .map(|(index, _)| index + 1)
        .collect()
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate lives at <root>/crates/<name>")
        .to_path_buf()
}

fn scanned_files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, rust_only: bool, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "target") {
                    continue;
                }
                walk(&path, rust_only, out);
            } else if !rust_only || path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(&root.join("crates"), true, &mut files);
    walk(&root.join("script"), false, &mut files);
    files.retain(|path| !path.ends_with("tests/timer_guard.rs"));
    files.sort();
    files
}

#[test]
fn every_timer_in_the_repository_is_marked() {
    let root = repository_root();
    let files = scanned_files(&root);
    assert!(
        !files.is_empty(),
        "the guard found no sources under {}",
        root.display()
    );
    let mut unmarked = Vec::new();
    let mut pending_seen = vec![false; PENDING_DEFECTS.len()];
    for path in files {
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        let relative = path.strip_prefix(&root).unwrap_or(&path).to_path_buf();
        let lines = source.lines().collect::<Vec<_>>();
        for line in violations(&source) {
            let text = lines[line - 1].trim();
            let pending = PENDING_DEFECTS
                .iter()
                .position(|(file, content, _)| relative == Path::new(file) && text == *content);
            match pending {
                Some(index) => pending_seen[index] = true,
                None => unmarked.push(format!("{}:{line}", relative.display())),
            }
        }
    }
    assert!(
        unmarked.is_empty(),
        "timers without a `// timer: <category> — <reason>` marker:\n{}",
        unmarked.join("\n")
    );
    let stale = PENDING_DEFECTS
        .iter()
        .zip(&pending_seen)
        .filter(|(_, seen)| !**seen)
        .map(|((file, content, _), _)| format!("{file}: {content}"))
        .collect::<Vec<_>>();
    assert!(
        stale.is_empty(),
        "pending DEFECT entries no longer match; remove them:\n{}",
        stale.join("\n")
    );
}

#[test]
fn guard_flags_unmarked_timers_and_accepts_marked_ones() {
    let unmarked = "fn f() {\n    std::thread::sleep(d);\n}\n";
    assert_eq!(violations(unmarked), vec![2]);
    let shell = "let cmd = \"while true; do sleep 0.05; done\";\n";
    assert_eq!(violations(shell), vec![1], "shell sleep inside a string");
    let created = "let until = Instant::now() + LIMIT;\n";
    assert_eq!(violations(created), vec![1], "deadline creation");
    let received = "rx.recv_timeout(left);\n";
    assert_eq!(violations(received), vec![1]);

    let previous = "// timer: deadline — the reply arrives within the bound; expiry fails\nrx.recv_timeout(left);\n";
    assert!(violations(previous).is_empty());
    let same = "let until = Instant::now() + TTL; // timer: ui-lifetime — notice, server ttl_ms\n";
    assert!(violations(same).is_empty());
    let window = "// timer: measurement-window — idle wake rate over one second\nstd::thread::sleep(window);\n";
    assert!(violations(window).is_empty());

    let no_reason = "// timer: deadline —\nrx.recv_timeout(left);\n";
    assert_eq!(violations(no_reason), vec![2], "a marker needs a reason");
    let unknown = "// timer: settle — let it catch up\nstd::thread::sleep(d);\n";
    assert_eq!(violations(unknown), vec![2], "unknown category");
    let comment = "// we never sleep here\n";
    assert!(
        violations(comment).is_empty(),
        "comment-only lines are prose"
    );
    let identifier = "let sleeper = asleep_count;\n";
    assert!(
        violations(identifier).is_empty(),
        "sleep must be a whole word"
    );
}
