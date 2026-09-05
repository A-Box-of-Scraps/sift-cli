#![cfg(target_os = "linux")]

use std::{fs, path::Path, process::Command};

use sift::{
    Error, IndexRequest, MAX_CHUNK_BYTES, MAX_FILE_BYTES, SearchQuery, SnapshotHandle,
    SnapshotStore,
};

struct Fixture {
    temporary: tempfile::TempDir,
    store: SnapshotStore,
    root: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("project");
        fs::create_dir(&root).unwrap();
        let store = SnapshotStore::new(temporary.path().join("data")).unwrap();
        Self {
            temporary,
            store,
            root,
        }
    }

    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let destination = self.root.join(path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, bytes).unwrap();
    }

    fn index(&self, files: &[&str]) -> sift::Result<SnapshotHandle> {
        self.store.index(&IndexRequest {
            root: self.root.clone(),
            files: files.iter().map(Into::into).collect(),
        })
    }

    fn cli(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sift"));
        command
            .current_dir(&self.root)
            .env("XDG_DATA_HOME", self.temporary.path().join("cli-data"));
        command
    }

    fn assert_unpublished(&self) {
        let indexes = self.temporary.path().join("data/indexes");
        assert!(!indexes.exists() || fs::read_dir(indexes).unwrap().next().is_none());
    }
}

#[test]
fn indexes_provenance_and_deduplicates_paths() {
    let fixture = Fixture::new();
    let text = "fn validateToken() { /* check authentication */ }\n";
    fixture.write("src/auth.rs", text);
    fixture.write("empty.txt", "");
    let handle = fixture
        .index(&["src/auth.rs", "./src/auth.rs", "empty.txt"])
        .unwrap();
    let info = handle.info().unwrap();
    assert_eq!(info.format_version, 2);
    assert_eq!(info.file_count, 2);
    assert_eq!(info.chunk_count, 1);
    assert_eq!(info.roots.len(), 1);
    assert_eq!(info.roots[0].location, fixture.root);
    let result = handle
        .query(&SearchQuery::new("validateToken"))
        .unwrap()
        .results
        .remove(0);
    assert_eq!(result.path, "src/auth.rs");
    assert_eq!(result.root_id, info.roots[0].id);
    assert_eq!(result.snippet, text);
    assert_eq!(result.start_line, 1);
    assert_eq!(result.end_line, 1);
    assert_eq!(result.start_byte, 0);
    assert_eq!(result.end_byte, text.len());
    assert!(!result.truncated);
    let connection = rusqlite::Connection::open_with_flags(
        handle.as_path().join("db.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let hash: String = connection
        .query_row(
            "SELECT content_hash FROM files WHERE path = 'src/auth.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(hash, blake3::hash(text.as_bytes()).to_hex().as_str());
}

#[test]
fn safe_queries_match_identifiers_paths_and_unicode() {
    let fixture = Fixture::new();
    fixture.write(
        "src/auth.rs",
        "validateToken HTTPServer snake_case caf\u{e9} OR NEAR\n",
    );
    let handle = fixture.index(&["src/auth.rs"]).unwrap();
    for text in [
        "validateToken",
        "validate token",
        "HTTP server",
        "snake_case",
        "snake case",
        "caf\u{e9}",
        "src/auth.rs",
        "\"OR\" : NEAR ( * )",
        "\"; DROP TABLE files; -- validateToken",
    ] {
        let response = handle.query(&SearchQuery::new(text)).unwrap();
        assert_eq!(response.results.len(), 1, "{text}");
    }
    assert_eq!(handle.info().unwrap().file_count, 1);
}

#[test]
fn bm25_prefers_a_focused_match_and_limits_results() {
    let fixture = Fixture::new();
    fixture.write(
        "a.txt",
        format!("authentication {}", "unrelated ".repeat(100)),
    );
    fixture.write("b.txt", "authentication");
    let handle = fixture.index(&["a.txt", "b.txt"]).unwrap();
    let mut query = SearchQuery::new("authentication");
    query.limit = 1;
    let response = handle.query(&query).unwrap();
    assert_eq!(response.results.len(), 1);
    assert_eq!(response.results[0].path, "b.txt");
}

#[test]
fn path_filters_use_components_before_limiting_and_escape_no_wildcards() {
    let fixture = Fixture::new();
    for path in [
        "src/auth/file.rs",
        "src/authz/file.rs",
        "src/100%/file.rs",
        "src/100x/file.rs",
        "src/under_score/file.rs",
    ] {
        fixture.write(path, "authentication");
    }
    let handle = fixture
        .index(&[
            "src/auth/file.rs",
            "src/authz/file.rs",
            "src/100%/file.rs",
            "src/100x/file.rs",
            "src/under_score/file.rs",
        ])
        .unwrap();
    for (filter, expected) in [
        ("src/auth", "src/auth/file.rs"),
        ("./src/auth/", "src/auth/file.rs"),
        ("src/auth/file.rs", "src/auth/file.rs"),
        ("src/100%", "src/100%/file.rs"),
        ("src/under_score", "src/under_score/file.rs"),
    ] {
        let mut query = SearchQuery::new("authentication");
        query.limit = 100;
        query.path = Some(filter.into());
        let response = handle.query(&query).unwrap();
        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].path, expected);
        query.limit = 1;
        assert_eq!(handle.query(&query).unwrap().results[0].path, expected);
    }
    let mut query = SearchQuery::new("authentication");
    query.path = Some("src/au".into());
    assert!(handle.query(&query).unwrap().results.is_empty());
}

#[test]
fn default_limit_caps_results_and_roots_disambiguate_identical_paths() {
    let fixture = Fixture::new();
    let paths: Vec<_> = (0..8).map(|i| format!("file{i}.txt")).collect();
    for path in &paths {
        fixture.write(path, "authentication");
    }
    let handle = fixture
        .index(&paths.iter().map(String::as_str).collect::<Vec<_>>())
        .unwrap();
    assert_eq!(
        handle
            .query(&SearchQuery::new("authentication"))
            .unwrap()
            .results
            .len(),
        5
    );
    let other = Fixture::new();
    other.write("file0.txt", "authentication");
    let second = other.index(&["file0.txt"]).unwrap();
    assert_ne!(
        handle.info().unwrap().roots[0].id,
        second.info().unwrap().roots[0].id
    );
}

#[test]
fn old_results_survive_source_edits_and_removal() {
    let fixture = Fixture::new();
    fixture.write("a.txt", "originalToken\n");
    let first = fixture.index(&["a.txt"]).unwrap();
    let bytes = fs::read(first.as_path().join("db.sqlite")).unwrap();
    fixture.write("a.txt", "replacementToken\n");
    let second = fixture.index(&["a.txt"]).unwrap();
    fs::remove_file(fixture.root.join("a.txt")).unwrap();
    assert_eq!(
        first
            .query(&SearchQuery::new("originalToken"))
            .unwrap()
            .results[0]
            .snippet,
        "originalToken\n"
    );
    assert!(
        first
            .query(&SearchQuery::new("replacement"))
            .unwrap()
            .results
            .is_empty()
    );
    assert_eq!(
        second
            .query(&SearchQuery::new("replacementToken"))
            .unwrap()
            .results[0]
            .snippet,
        "replacementToken\n"
    );
    assert_eq!(bytes, fs::read(first.as_path().join("db.sqlite")).unwrap());
    assert_eq!(fs::read_dir(first.as_path()).unwrap().count(), 1);
    assert_eq!(first.info().unwrap().roots, second.info().unwrap().roots);
}

#[test]
fn chunks_are_bounded_faithful_and_non_overlapping_in_results() {
    let fixture = Fixture::new();
    let text = (1..=100)
        .map(|i| format!("authentication line {i}\r\n"))
        .collect::<String>();
    fixture.write("a.txt", &text);
    let handle = fixture.index(&["a.txt"]).unwrap();
    let mut query = SearchQuery::new("authentication");
    query.limit = 100;
    let results = handle.query(&query).unwrap().results;
    assert!(results.len() > 1);
    for (i, result) in results.iter().enumerate() {
        assert!(result.snippet.len() <= MAX_CHUNK_BYTES);
        assert_eq!(result.snippet, text[result.start_byte..result.end_byte]);
        for other in &results[..i] {
            assert!(result.end_byte <= other.start_byte || other.end_byte <= result.start_byte);
        }
    }
}

#[test]
fn long_unicode_lines_split_without_losing_source_offsets() {
    let fixture = Fixture::new();
    let text = "caf\u{e9} ".repeat(1000);
    fixture.write("a.txt", &text);
    let handle = fixture.index(&["a.txt"]).unwrap();
    let response = handle.query(&SearchQuery::new("caf\u{e9}")).unwrap();
    assert!(response.results.len() > 1);
    for result in response.results {
        assert_eq!((result.start_line, result.end_line), (1, 1));
        assert_eq!(result.snippet, text[result.start_byte..result.end_byte]);
        assert!(result.snippet.len() <= MAX_CHUNK_BYTES);
    }
}

#[test]
fn invalid_files_fail_without_publication() {
    for bytes in [b"not\0text".to_vec(), vec![0xff, 0xfe]] {
        let fixture = Fixture::new();
        fixture.write("a-good.txt", "valid text");
        fixture.write("z-bad.txt", bytes);
        assert!(matches!(
            fixture.index(&["a-good.txt", "z-bad.txt"]),
            Err(Error::Input { .. })
        ));
        fixture.assert_unpublished();
    }
    let fixture = Fixture::new();
    fixture.write("large.txt", "");
    fs::File::options()
        .write(true)
        .open(fixture.root.join("large.txt"))
        .unwrap()
        .set_len(MAX_FILE_BYTES + 1)
        .unwrap();
    assert!(
        fixture
            .index(&["large.txt"])
            .unwrap_err()
            .to_string()
            .contains("8 MiB")
    );
    fixture.assert_unpublished();
}

#[test]
fn rejects_unsupported_selection_and_symlink_components() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    fixture.write("src/a.txt", "hello");
    symlink("src/a.txt", fixture.root.join("link.txt")).unwrap();
    symlink("src", fixture.root.join("linked-dir")).unwrap();
    for files in [
        vec![],
        vec!["missing"],
        vec!["../outside"],
        vec!["link.txt"],
        vec!["linked-dir/a.txt"],
        vec!["src/*.missing"],
    ] {
        assert!(fixture.index(&files).is_err(), "{files:?}");
        fixture.assert_unpublished();
    }
}

#[test]
fn query_validates_options_and_rejects_metadata_only_snapshots() {
    let fixture = Fixture::new();
    fixture.write("empty.txt", "");
    let handle = fixture.index(&["empty.txt"]).unwrap();
    assert!(
        handle
            .query(&SearchQuery::new("anything"))
            .unwrap()
            .results
            .is_empty()
    );
    for text in ["", " \n ", "*** : ()", "___"] {
        assert!(matches!(
            handle.query(&SearchQuery::new(text)),
            Err(Error::InvalidOptions(_))
        ));
    }
    for limit in [0, 101, usize::MAX] {
        let mut query = SearchQuery::new("text");
        query.limit = limit;
        assert!(matches!(
            handle.query(&query),
            Err(Error::InvalidOptions(_))
        ));
    }
    for path in ["/absolute", "src/../other"] {
        let mut query = SearchQuery::new("text");
        query.path = Some(path.into());
        assert!(matches!(
            handle.query(&query),
            Err(Error::InvalidOptions(_))
        ));
    }
    let metadata = fixture.store.create_snapshot().unwrap();
    assert!(matches!(
        metadata.query(&SearchQuery::new("text")),
        Err(Error::NotSearchable)
    ));
}

#[test]
fn cli_round_trip_json_and_filter_ignore_query_working_directory() {
    let fixture = Fixture::new();
    fixture.write("src/auth.rs", "validateToken\n");
    let indexed = fixture
        .cli()
        .args(["index", "src/auth.rs"])
        .output()
        .unwrap();
    assert!(
        indexed.status.success(),
        "{}",
        String::from_utf8_lossy(&indexed.stderr)
    );
    assert!(String::from_utf8_lossy(&indexed.stderr).contains("indexed 1 documents"));
    let stdout = String::from_utf8(indexed.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    let handle = stdout.trim();
    assert!(
        Path::new(handle).starts_with(fixture.temporary.path().join("cli-data/sift-cli/indexes"))
    );
    let queried = fixture
        .cli()
        .current_dir(fixture.temporary.path())
        .args(["query", handle, "validate token", "--path", "src", "--json"])
        .output()
        .unwrap();
    assert!(
        queried.status.success(),
        "{}",
        String::from_utf8_lossy(&queried.stderr)
    );
    assert!(queried.stderr.is_empty());
    let payload: serde_json::Value = serde_json::from_slice(&queried.stdout).unwrap();
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["handle"], handle);
    assert_eq!(payload["results"][0]["path"], "src/auth.rs");
    assert_eq!(payload["results"][0]["snippet"], "validateToken\n");
    assert_eq!(payload["results"][0]["truncated"], false);
    assert!(payload["results"][0].get("score").is_none());
    assert_empty_and_invalid_queries(&fixture, handle);
}

fn assert_empty_and_invalid_queries(fixture: &Fixture, handle: &str) {
    let no_results = fixture
        .cli()
        .args(["query", handle, "absentterm", "--json"])
        .output()
        .unwrap();
    assert!(no_results.status.success());
    let payload: serde_json::Value = serde_json::from_slice(&no_results.stdout).unwrap();
    assert_eq!(payload["results"], serde_json::json!([]));
    let text = fixture
        .cli()
        .args(["query", handle, "absentterm"])
        .output()
        .unwrap();
    assert!(text.status.success());
    assert_eq!(text.stdout, b"No results.\n");
    let invalid = fixture
        .cli()
        .args(["query", handle, "hello", "--limit", "0"])
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
}

#[test]
fn cli_root_override_and_failed_index_output() {
    let fixture = Fixture::new();
    fixture.write("src/a.txt", "text");
    let indexed = fixture
        .cli()
        .current_dir(fixture.temporary.path())
        .args(["index", "--root"])
        .arg(&fixture.root)
        .arg("src/a.txt")
        .output()
        .unwrap();
    assert!(
        indexed.status.success(),
        "{}",
        String::from_utf8_lossy(&indexed.stderr)
    );
    let failed = fixture
        .cli()
        .args(["index", "missing.txt"])
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(1));
    assert!(failed.stdout.is_empty());
    assert!(!failed.stderr.is_empty());
    let missing_args = fixture.cli().arg("index").output().unwrap();
    assert_eq!(missing_args.status.code(), Some(2));
}

#[test]
fn repeated_excerpts_leave_room_for_other_implementations() {
    let fixture = Fixture::new();
    fixture.write(
        "primary.rs",
        include_str!("../benchmarks/corpus/diversity/primary.rs"),
    );
    fixture.write(
        "replica.rs",
        include_str!("../benchmarks/corpus/diversity/replica_a.rs"),
    );
    fixture.write(
        "scheduler.py",
        include_str!("../benchmarks/corpus/diversity/scheduler.py"),
    );
    fixture.write(
        "monitor.go",
        include_str!("../benchmarks/corpus/diversity/monitor.go"),
    );
    let handle = fixture
        .index(&["primary.rs", "replica.rs", "scheduler.py", "monitor.go"])
        .unwrap();
    let mut query = SearchQuery::new("cache_refresh_task");
    query.limit = 3;
    let results = handle.query(&query).unwrap().results;
    assert!(results.iter().any(|result| result.path == "scheduler.py"));
    assert!(results.iter().any(|result| result.path == "monitor.go"));
    query.path = Some("primary.rs".into());
    let results = handle.query(&query).unwrap().results;
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|result| result.path == "primary.rs"));
}
