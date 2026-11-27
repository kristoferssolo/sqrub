//! Runs the sqrub binary against temporary projects

use claims::assert_ok;
use std::fs;
use std::process::{Command, Output};
use tempfile::TempDir;

const UPPERCASE: &str = "tr '[:lower:]' '[:upper:]'";

const UNFORMATTED: &str = r#"fn main() {
    sqlx::query!("select 1");
}
"#;

const FORMATTED: &str = r#"fn main() {
    sqlx::query!("SELECT 1");
}
"#;

struct Project {
    root: TempDir,
}

impl Project {
    fn new(main_rs: &str) -> Self {
        let root = assert_ok!(TempDir::new());
        assert_ok!(fs::create_dir(root.path().join("src")));
        assert_ok!(fs::write(root.path().join("src/main.rs"), main_rs));
        Self { root }
    }

    fn sqrub(&self, args: &[&str]) -> Output {
        assert_ok!(
            Command::new(env!("CARGO_BIN_EXE_sqrub"))
                .args(args)
                .current_dir(self.root.path())
                .output()
        )
    }

    fn main_rs(&self) -> String {
        assert_ok!(fs::read_to_string(self.root.path().join("src/main.rs")))
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn formats_files_in_place() {
    let project = Project::new(UNFORMATTED);
    let output = project.sqrub(&["-f", UPPERCASE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "./src/main.rs: changed\n");
    assert_eq!(project.main_rs(), FORMATTED);
}

#[test]
fn check_reports_changes_without_writing() {
    let project = Project::new(UNFORMATTED);
    let output = project.sqrub(&["--check", "-f", UPPERCASE]);

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(stdout(&output), "./src/main.rs: would change\n");
    assert_eq!(project.main_rs(), UNFORMATTED);
}

#[test]
fn formatted_projects_pass_check_silently() {
    let project = Project::new(FORMATTED);
    let output = project.sqrub(&["--check", "-f", UPPERCASE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
}

#[test]
fn formatter_failure_leaves_the_file_and_exits_2() {
    let project = Project::new(UNFORMATTED);
    let output = project.sqrub(&["-f", "sh -c 'cat >/dev/null; echo bad sql >&2; exit 1'"]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = stderr(&output);
    assert!(stderr.contains("failed to format query"), "{stderr}");
    assert!(stderr.contains("bad sql"), "{stderr}");
    assert_eq!(project.main_rs(), UNFORMATTED);
}

#[test]
fn missing_input_exits_2_after_formatting_the_rest() {
    let project = Project::new(UNFORMATTED);
    let output = project.sqrub(&["-f", UPPERCASE, "missing", "src"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("failed to find Rust files"));
    assert_eq!(project.main_rs(), FORMATTED);
}
