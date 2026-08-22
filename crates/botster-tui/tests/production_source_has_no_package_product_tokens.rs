use std::path::{Path, PathBuf};

#[test]
fn production_tui_source_contains_no_project_pipelines_tokens() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hits = Vec::new();
    walk(&src, &mut hits);
    assert!(
        hits.is_empty(),
        "production TUI source must not name Project Pipelines: {hits:?}"
    );
}

fn walk(dir: &Path, hits: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("read src") {
        let entry = entry.expect("dirent");
        let path = entry.path();
        if path.is_dir() {
            walk(&path, hits);
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read source");
        for (index, line) in text.lines().enumerate() {
            if line.contains("project-pipelines")
                || line.contains("project_pipelines")
                || line.contains("question.opened")
            {
                hits.push(format!("{}:{}:{line}", path.display(), index + 1));
            }
        }
    }
}
