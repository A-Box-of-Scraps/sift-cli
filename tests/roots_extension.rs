#![cfg(target_os = "linux")]

use sift::{DiscoveryOptions, IndexRequest, SearchQuery, SnapshotStore};
use std::{fs, path::Path};

fn request(root: &Path, name: &str, files: &[&str]) -> (String, IndexRequest) {
    (
        name.into(),
        IndexRequest {
            root: root.into(),
            files: files.iter().map(Into::into).collect(),
        },
    )
}

#[test]
fn roots_extension_staleness_and_retention() {
    let temp: tempfile::TempDir = tempfile::tempdir().unwrap();
    let a: std::path::PathBuf = temp.path().join("a");
    let b: std::path::PathBuf = temp.path().join("b");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("same"), "original alpha").unwrap();
    fs::write(a.join("retained"), "retained alpha").unwrap();
    fs::write(b.join("same"), "original beta").unwrap();
    let store: SnapshotStore = SnapshotStore::new(temp.path().join("store")).unwrap();
    let options: DiscoveryOptions = DiscoveryOptions::default();
    let base: sift::SnapshotHandle = store
        .index_roots(
            &[request(&a, "a", &["."]), request(&b, "b", &["."])],
            &options,
            None,
        )
        .unwrap();
    let before: Vec<u8> = fs::read(base.as_path().join("db.sqlite")).unwrap();
    let mut query: SearchQuery = SearchQuery::new("original");
    query.root = Some("b".into());
    let hits: Vec<sift::SearchResult> = base.query(&query).unwrap().results;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].root_name, "b");
    assert!(
        base.check_staleness()
            .unwrap()
            .iter()
            .all(|s| s.status == "unchanged")
    );
    fs::write(a.join("same"), "replacement alpha").unwrap();
    fs::write(a.join("new"), "new alpha").unwrap();
    fs::remove_file(a.join("retained")).unwrap();
    let statuses: Vec<sift::SourceStatus> = base.check_staleness().unwrap();
    assert!(statuses.iter().any(|s| s.status == "changed"));
    assert!(statuses.iter().any(|s| s.status == "unavailable"));
    let next: sift::SnapshotHandle = store
        .index_roots(&[request(&a, "a", &["same", "new"])], &options, Some(&base))
        .unwrap();
    assert_eq!(next.info().unwrap().file_count, 4);
    assert_eq!(next.info().unwrap().roots, base.info().unwrap().roots);
    assert_eq!(
        next.query(&SearchQuery::new("retained"))
            .unwrap()
            .results
            .len(),
        1
    );
    assert_eq!(
        next.query(&SearchQuery::new("replacement"))
            .unwrap()
            .results
            .len(),
        1
    );
    assert_eq!(
        base.query(&SearchQuery::new("replacement"))
            .unwrap()
            .results
            .len(),
        0
    );
    assert_eq!(before, fs::read(base.as_path().join("db.sqlite")).unwrap());
    assert!(
        store
            .index_roots(&[request(&b, "a", &["same"])], &options, Some(&base))
            .is_err()
    );
    assert!(
        store
            .index_roots(&[request(&a, "alias", &["same"])], &options, Some(&base))
            .is_err()
    );
    store.delete(&base).unwrap();
    assert!(next.query(&SearchQuery::new("replacement")).is_ok());
}

#[test]
fn failed_and_parallel_extensions_preserve_original() {
    let temp: tempfile::TempDir = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("file"), "unchanged value").unwrap();
    let store: SnapshotStore = SnapshotStore::new(temp.path().join("store")).unwrap();
    let base: sift::SnapshotHandle = store
        .index_roots(
            &[request(temp.path(), "main", &["file"])],
            &DiscoveryOptions::default(),
            None,
        )
        .unwrap();
    let before: Vec<u8> = fs::read(base.as_path().join("db.sqlite")).unwrap();
    fs::write(temp.path().join("bad"), b"bad\0text").unwrap();
    assert!(
        store
            .index_roots(
                &[request(temp.path(), "main", &["file", "bad"])],
                &DiscoveryOptions::default(),
                Some(&base)
            )
            .is_err()
    );
    let workers: Vec<std::thread::JoinHandle<sift::SnapshotHandle>> = (0..3)
        .map(|_| {
            let store: SnapshotStore = store.clone();
            let base: sift::SnapshotHandle = base.clone();
            let root: std::path::PathBuf = temp.path().to_path_buf();
            std::thread::spawn(move || {
                store
                    .index_roots(
                        &[request(&root, "main", &["file"])],
                        &DiscoveryOptions::default(),
                        Some(&base),
                    )
                    .unwrap()
            })
        })
        .collect();
    let mut handles: Vec<std::path::PathBuf> = workers
        .into_iter()
        .map(|w| w.join().unwrap().as_path().to_path_buf())
        .collect();
    handles.sort();
    handles.dedup();
    assert_eq!(handles.len(), 3);
    assert_eq!(before, fs::read(base.as_path().join("db.sqlite")).unwrap());
    assert_eq!(store.cleanup_staging().unwrap(), 0);
}

#[test]
fn delete_validation_readers_and_cleanup() {
    use std::os::unix::fs::symlink;
    let temp: tempfile::TempDir = tempfile::tempdir().unwrap();
    let store: SnapshotStore = SnapshotStore::new(temp.path().join("store")).unwrap();
    let handle: sift::SnapshotHandle = store.create_snapshot().unwrap();
    let other: SnapshotStore = SnapshotStore::new(temp.path().join("other")).unwrap();
    other.create_snapshot().unwrap();
    assert!(other.delete(&handle).is_err());
    fs::write(handle.as_path().join("unexpected"), "keep").unwrap();
    assert!(store.delete(&handle).is_err());
    assert!(handle.info().is_ok());
    fs::remove_file(handle.as_path().join("unexpected")).unwrap();
    let reader: rusqlite::Connection = rusqlite::Connection::open_with_flags(
        handle.as_path().join("db.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    reader
        .execute_batch("BEGIN; SELECT * FROM snapshot_metadata;")
        .unwrap();
    store.delete(&handle).unwrap();
    let count: usize = reader
        .query_row("SELECT count(*) FROM snapshot_metadata", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert!(handle.info().is_err());
    let indexes: std::path::PathBuf = temp.path().join("store/indexes");
    let staging: std::path::PathBuf = indexes.join(".staging-abandoned");
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("db.sqlite"), "partial").unwrap();
    fs::write(staging.join("db.sqlite-journal"), "partial").unwrap();
    assert_eq!(store.cleanup_staging().unwrap(), 1);
    symlink(temp.path(), &staging).unwrap();
    assert!(store.cleanup_staging().is_err());
    assert!(temp.path().exists());
}

#[test]
fn cli_lifecycle_commands() {
    use std::process::Command;
    let temp: tempfile::TempDir = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("file"), "searchable original").unwrap();
    let run = |args: &[&std::ffi::OsStr]| {
        Command::new(env!("CARGO_BIN_EXE_sift"))
            .env("XDG_DATA_HOME", temp.path().join("data"))
            .args(args)
            .output()
            .unwrap()
    };
    let output: std::process::Output = run(&[
        "index".as_ref(),
        "--root".as_ref(),
        temp.path().as_os_str(),
        "--root-name".as_ref(),
        "main".as_ref(),
        "file".as_ref(),
    ]);
    assert!(output.status.success(), "{:?}", output);
    let handle: String = String::from_utf8(output.stdout).unwrap();
    let handle: &std::ffi::OsStr = std::ffi::OsStr::new(handle.trim());
    let output: std::process::Output = run(&[
        "query".as_ref(),
        handle,
        "original".as_ref(),
        "--root".as_ref(),
        "main".as_ref(),
        "--json".as_ref(),
    ]);
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("searchable original")
    );
    let output: std::process::Output = run(&["check".as_ref(), handle]);
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("unchanged")
    );
    let output: std::process::Output = run(&[
        "index".as_ref(),
        "--root".as_ref(),
        temp.path().as_os_str(),
        "--root-name".as_ref(),
        "main".as_ref(),
        "--extend".as_ref(),
        handle,
        "file".as_ref(),
    ]);
    assert!(output.status.success(), "{:?}", output);
    assert!(run(&["delete".as_ref(), handle]).status.success());
    assert!(!run(&["info".as_ref(), handle]).status.success());
    assert!(run(&["cleanup".as_ref()]).status.success());
}

#[test]
fn copied_artifacts_and_synthetic_sources_remain_independent() {
    let temp: tempfile::TempDir = tempfile::tempdir().unwrap();
    let store: SnapshotStore = SnapshotStore::new(temp.path().join("store")).unwrap();
    let base: sift::SnapshotHandle = store
        .index_documents(&[sift::TextDocument {
            name: "note".into(),
            text: "portable content".into(),
        }])
        .unwrap();
    assert_eq!(base.check_staleness().unwrap()[0].status, "unavailable");
    fs::write(temp.path().join("file"), "filesystem content").unwrap();
    let next: sift::SnapshotHandle = store
        .index_roots(
            &[request(temp.path(), "main", &["file"])],
            &DiscoveryOptions::default(),
            Some(&base),
        )
        .unwrap();
    assert_eq!(next.info().unwrap().file_count, 2);
    let copy: std::path::PathBuf = temp.path().join(next.as_path().file_name().unwrap());
    fs::create_dir(&copy).unwrap();
    fs::copy(next.as_path().join("db.sqlite"), copy.join("db.sqlite")).unwrap();
    store.delete(&next).unwrap();
    let copy: sift::SnapshotHandle = sift::SnapshotHandle::from_path(copy).unwrap();
    assert_eq!(
        copy.query(&SearchQuery::new("portable"))
            .unwrap()
            .results
            .len(),
        1
    );
    assert_eq!(copy.info().unwrap().roots.len(), 2);
}
