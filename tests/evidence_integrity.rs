use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use proof_lantern::load_project;

struct TestProject {
    root: PathBuf,
}

impl TestProject {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "proof-lantern-evidence-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join(".proof-lantern"))
            .expect("isolated project directory should be created");
        fs::write(
            root.join(".proof-lantern/project.yml"),
            concat!(
                "schema_version: 1\n",
                "project:\n",
                "  name: Evidence Integrity\n",
                "  promise: Keep current evidence inspectable.\n",
                "capabilities:\n",
                "  - id: inspect\n",
                "    label: Inspect evidence\n",
                "    role:\n",
                "      kind: core\n",
                "      order: 1\n",
                "    proof_needed: Open the cited source.\n",
            ),
        )
        .expect("project fixture should be written");
        Self { root }
    }

    fn write_observation(&self, freshness: &str, path: &str, line_end: u32) {
        let observations = format!(
            concat!(
                "{{\n",
                "  \"schema_version\": 1,\n",
                "  \"observations\": [{{\n",
                "    \"capability_id\": \"inspect\",\n",
                "    \"source\": \"imported_test_result\",\n",
                "    \"fact\": {{\n",
                "      \"claim\": \"verification_passed\",\n",
                "      \"freshness\": \"{}\",\n",
                "      \"summary\": \"Recorded proof.\",\n",
                "      \"location\": {{\n",
                "        \"path\": \"{}\",\n",
                "        \"line_start\": 1,\n",
                "        \"line_end\": {}\n",
                "      }}\n",
                "    }}\n",
                "  }}]\n",
                "}}\n",
            ),
            freshness, path, line_end
        );
        fs::write(
            self.root.join(".proof-lantern/observations.json"),
            observations,
        )
        .expect("observation fixture should be written");
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("isolated project directory should be removed");
    }
}

#[test]
fn current_evidence_must_reference_an_existing_file_and_valid_lines() {
    let project = TestProject::new("valid");
    fs::create_dir(project.root().join("src")).expect("source directory should be created");
    fs::write(project.root().join("src/proof.rs"), "first\nsecond\n")
        .expect("evidence source should be written");
    project.write_observation("current", "src/proof.rs", 2);

    load_project(project.root()).expect("valid current evidence should load");
}

#[test]
fn missing_current_evidence_is_rejected_with_the_capability_and_path() {
    let project = TestProject::new("missing");
    project.write_observation("current", "src/missing.rs", 1);

    let message = load_project(project.root()).unwrap_err().to_string();
    assert!(message.contains("evidence for inspect is not inspectable"));
    assert!(message.contains("src/missing.rs"));
}

#[test]
fn a_current_line_citation_cannot_extend_past_the_file() {
    let project = TestProject::new("line-range");
    fs::write(project.root().join("proof.txt"), "only line\n")
        .expect("evidence source should be written");
    project.write_observation("current", "proof.txt", 2);

    let message = load_project(project.root()).unwrap_err().to_string();
    assert!(message.contains("line 2 exceeds the file's 1 lines"));
}

#[test]
fn stale_history_does_not_require_its_old_source_to_remain_on_disk() {
    let project = TestProject::new("stale");
    project.write_observation("stale", "retired/proof.txt", 1);

    load_project(project.root()).expect("stale historical evidence should remain loadable");
}

#[cfg(unix)]
#[test]
fn a_symlink_cannot_make_current_evidence_escape_the_project_root() {
    use std::os::unix::fs::symlink;

    let project = TestProject::new("symlink-project");
    let outside = TestProject::new("symlink-outside");
    fs::write(outside.root().join("outside.txt"), "outside\n")
        .expect("outside source should be written");
    symlink(
        outside.root().join("outside.txt"),
        project.root().join("linked.txt"),
    )
    .expect("test symlink should be created");
    project.write_observation("current", "linked.txt", 1);

    let message = load_project(project.root()).unwrap_err().to_string();
    assert!(message.contains("resolved path escapes the project root"));
}
