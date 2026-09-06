#![cfg(target_os = "linux")]

use sift::{IndexRequest, SnapshotHandle, SnapshotStore};
use std::{
    fs,
    process::{Command, Output, Stdio},
};

struct Fixture {
    temporary: tempfile::TempDir,
    handle: SnapshotHandle,
}

impl Fixture {
    fn new() -> Self {
        let temporary: tempfile::TempDir = tempfile::tempdir().unwrap();
        let root: std::path::PathBuf = temporary.path().join("project\n\u{1b}[2J");
        fs::create_dir(&root).unwrap();
        let path = "odd\n\u{1b}[31m\u{202e}\\café.txt";
        fs::write(
            root.join(path),
            "needle café\ttext\r\n\u{1b}[2J\u{7}\u{8}\u{9b}31m\u{202e}hidden\u{2066}end\n",
        )
        .unwrap();
        let handle: SnapshotHandle = SnapshotStore::new(temporary.path().join("data"))
            .unwrap()
            .index(&IndexRequest {
                root,
                files: vec![path.into()],
            })
            .unwrap();
        Self { temporary, handle }
    }

    fn cli(&self) -> Command {
        let mut command: Command = Command::new(env!("CARGO_BIN_EXE_sift"));
        command.env("XDG_DATA_HOME", self.temporary.path().join("cli-data"));
        command
    }

    fn query(&self) -> Command {
        let mut command: Command = self.cli();
        command.arg("query").arg(self.handle.as_path());
        command
    }
}

fn failure(output: Output, code: i32) {
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(!output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
}

#[test]
fn invalid_queries_have_usage_status_and_no_payload() {
    let fixture: Fixture = Fixture::new();
    let before: Vec<u8> = fs::read(fixture.handle.as_path().join("db.sqlite")).unwrap();
    let many_terms: String = (0..65)
        .map(|i| format!("word{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    for query in [
        "",
        " \n ",
        "*** : ()",
        "___",
        &"a".repeat(4097),
        &many_terms,
    ] {
        for json in [false, true] {
            let mut command: Command = fixture.query();
            command.arg(query);
            if json {
                command.arg("--json");
            }
            failure(command.output().unwrap(), 2);
        }
    }
    for options in [
        ["--limit", "0"],
        ["--limit", "101"],
        ["--limit", "no"],
        ["--path", "/absolute"],
        ["--path", "src/../other"],
    ] {
        failure(
            fixture
                .query()
                .arg("needle")
                .args(options)
                .output()
                .unwrap(),
            2,
        );
    }
    assert_eq!(
        before,
        fs::read(fixture.handle.as_path().join("db.sqlite")).unwrap()
    );
}

#[test]
fn damaged_and_unsupported_artifacts_fail_without_modification() {
    for mutation in [
        "corrupt",
        "DELETE FROM snapshot_metadata",
        "UPDATE snapshot_metadata SET format_version = 999",
        "UPDATE snapshot_metadata SET backend = 'other'",
    ] {
        let fixture: Fixture = Fixture::new();
        let database: std::path::PathBuf = fixture.handle.as_path().join("db.sqlite");
        if mutation == "corrupt" {
            fs::write(&database, b"not sqlite").unwrap();
        } else {
            let connection: rusqlite::Connection = rusqlite::Connection::open(&database).unwrap();
            connection.execute_batch(mutation).unwrap();
            connection.close().unwrap();
        }
        let before: Vec<u8> = fs::read(&database).unwrap();
        for subcommand in ["info", "query", "check"] {
            let mut command: Command = fixture.cli();
            command.arg(subcommand).arg(fixture.handle.as_path());
            if subcommand == "query" {
                command.args(["needle", "--json"]);
            }
            failure(command.output().unwrap(), 1);
        }
        assert_eq!(before, fs::read(database).unwrap());
    }
}

#[test]
fn closed_output_pipes_fail_without_panicking() {
    let fixture: Fixture = Fixture::new();
    for subcommand in ["info", "query", "check", "index", "cleanup"] {
        let mut command: Command = fixture.cli();
        command.arg(subcommand);
        match subcommand {
            "index" => {
                command
                    .arg("--root")
                    .arg(&fixture.handle.info().unwrap().roots[0].location)
                    .arg(".");
            }
            "cleanup" => {}
            _ => {
                command.arg(fixture.handle.as_path());
                if subcommand == "query" {
                    command.args(["needle", "--json"]);
                }
            }
        }
        // Close the reader before spawning so even short writes fail deterministically.
        let (reader, writer): (std::io::PipeReader, std::io::PipeWriter) = std::io::pipe().unwrap();
        drop(reader);
        command.stdout(Stdio::from(writer));
        failure(command.output().unwrap(), 1);
    }
}

#[test]
fn human_output_escapes_controls_but_json_preserves_source() {
    let fixture: Fixture = Fixture::new();
    let expected: sift::QueryResponse = fixture
        .handle
        .query(&sift::SearchQuery::new("needle"))
        .unwrap();
    let json: Output = fixture.query().args(["needle", "--json"]).output().unwrap();
    assert!(json.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&json.stdout).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    for output in [
        fixture.query().arg("needle").output().unwrap(),
        fixture
            .cli()
            .arg("info")
            .arg(fixture.handle.as_path())
            .output()
            .unwrap(),
    ] {
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let text: String = String::from_utf8(output.stdout).unwrap();
        assert!(
            !text
                .chars()
                .any(|c| (c.is_control() && c != '\n' && c != '\t')
                    || matches!(c, '\u{202e}' | '\u{2066}'))
        );
        assert!(text.contains("\\u{1b}"));
    }
    let text: Output = fixture.query().arg("needle").output().unwrap();
    assert!(
        String::from_utf8(text.stdout)
            .unwrap()
            .contains("café\ttext\\r\n")
    );
    let error: Output = fixture
        .cli()
        .arg("info")
        .arg("bad\n\u{1b}[2J\u{202e}")
        .output()
        .unwrap();
    assert!(!error.stderr.contains(&0x1b));
    assert_eq!(String::from_utf8_lossy(&error.stderr).lines().count(), 1);
    failure(error, 1);
}

#[test]
fn filesystem_permission_failures_have_no_success_payload() {
    use std::os::unix::{fs::PermissionsExt, process::CommandExt};
    let fixture: Fixture = Fixture::new();
    fs::set_permissions(fixture.temporary.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let blocked: std::path::PathBuf = fixture.temporary.path().join("blocked");
    fs::create_dir(&blocked).unwrap();
    fs::write(blocked.join("source.txt"), "needle").unwrap();
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();
    for operation in ["read", "write", "snapshot"] {
        let mut command: Command = fixture.cli();
        match operation {
            "read" => {
                command
                    .args(["index", "--root"])
                    .arg(&blocked)
                    .arg("source.txt");
            }
            "write" => {
                command
                    .env("XDG_DATA_HOME", &blocked)
                    .args(["index", "--stdin"]);
            }
            _ => {
                command.arg("info").arg(fixture.handle.as_path());
            }
        }
        if operation == "snapshot" {
            fs::set_permissions(
                fixture.handle.as_path().join("db.sqlite"),
                fs::Permissions::from_mode(0o000),
            )
            .unwrap();
        }
        if rustix::process::geteuid().is_root() {
            command.gid(65534).uid(65534);
        }
        failure(command.stdin(Stdio::null()).output().unwrap(), 1);
    }
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        !fixture
            .temporary
            .path()
            .join("cli-data/sift-cli/indexes")
            .exists()
    );
}
