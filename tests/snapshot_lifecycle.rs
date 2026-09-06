use std::{fs, path::Path, process::Command};

use sift::{Error, SnapshotHandle, SnapshotStore, data_directory};

#[test]
fn data_paths_follow_xdg_without_changing_environment() {
    let home: Option<&Path> = Some(Path::new("/home/test"));
    for xdg in [None, Some(Path::new("")), Some(Path::new("relative"))] {
        assert_eq!(
            data_directory(xdg, home).unwrap(),
            Path::new("/home/test/.local/share/sift-cli")
        );
    }
    assert_eq!(
        data_directory(Some(Path::new("/data")), None).unwrap(),
        Path::new("/data/sift-cli")
    );
    assert!(matches!(
        data_directory(None, None),
        Err(Error::MissingDataDirectory)
    ));
    assert!(data_directory(None, Some(Path::new("relative"))).is_err());
}

#[test]
fn missing_handle_does_not_create_files() {
    let temporary: tempfile::TempDir = tempfile::tempdir().unwrap();
    let directory: std::path::PathBuf = temporary.path().join(uuid::Uuid::new_v4().to_string());
    let handle: SnapshotHandle = SnapshotHandle::from_path(&directory).unwrap();
    assert!(handle.info().is_err());
    assert!(!directory.exists());
    fs::create_dir(&directory).unwrap();
    assert!(handle.info().is_err());
    assert!(!directory.join("db.sqlite").exists());
}

#[test]
fn rejects_relative_and_staging_handles() {
    assert!(SnapshotHandle::from_path(uuid::Uuid::new_v4().to_string()).is_err());
    assert!(SnapshotHandle::from_path("/tmp/.staging-abc").is_err());
    assert!(SnapshotHandle::from_path("/tmp/not-a-snapshot").is_err());
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    fn snapshot() -> (tempfile::TempDir, SnapshotHandle) {
        let temporary: tempfile::TempDir = tempfile::tempdir().unwrap();
        let handle: SnapshotHandle = SnapshotStore::new(temporary.path())
            .unwrap()
            .create_snapshot()
            .unwrap();
        (temporary, handle)
    }

    #[test]
    fn persists_metadata_and_reopens_without_changing_artifact() {
        let (_temporary, handle): (tempfile::TempDir, SnapshotHandle) = snapshot();
        let database: std::path::PathBuf = handle.as_path().join("db.sqlite");
        let before: Vec<u8> = fs::read(&database).unwrap();
        let info: sift::SnapshotInfo = handle.info().unwrap();
        assert_eq!(info.backend, "sqlite");
        assert_eq!(info.format_version, 1);
        assert_eq!(info.preprocessing_config, "none");
        assert!(info.created_at_unix_seconds > 0);
        let reopened: SnapshotHandle = SnapshotHandle::from_path(handle.as_path()).unwrap();
        assert_eq!(info, reopened.info().unwrap());
        assert_eq!(before, fs::read(database).unwrap());
        assert_eq!(fs::read_dir(handle.as_path()).unwrap().count(), 1);
    }

    #[test]
    fn independent_builds_preserve_old_snapshot() {
        let (temporary, first): (tempfile::TempDir, SnapshotHandle) = snapshot();
        let before: Vec<u8> = fs::read(first.as_path().join("db.sqlite")).unwrap();
        let second: SnapshotHandle = SnapshotStore::new(temporary.path())
            .unwrap()
            .create_snapshot()
            .unwrap();
        assert_ne!(first, second);
        assert_ne!(first.info().unwrap().id, second.info().unwrap().id);
        assert_eq!(before, fs::read(first.as_path().join("db.sqlite")).unwrap());
    }

    #[test]
    fn concurrent_builds_publish_independent_artifacts() {
        let temporary: tempfile::TempDir = tempfile::tempdir().unwrap();
        let store: SnapshotStore = SnapshotStore::new(temporary.path()).unwrap();
        let workers: Vec<std::thread::JoinHandle<sift::SnapshotHandle>> = (0..4)
            .map(|_| {
                let store: SnapshotStore = store.clone();
                std::thread::spawn(move || store.create_snapshot().unwrap())
            })
            .collect();
        let mut ids: Vec<String> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap().info().unwrap().id)
            .collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 4);
        assert_eq!(
            fs::read_dir(temporary.path().join("indexes"))
                .unwrap()
                .count(),
            4
        );
    }

    #[test]
    fn rejects_unsupported_or_invalid_metadata() {
        for (sql, expected) in [
            (
                "UPDATE snapshot_metadata SET format_version = 999",
                "format",
            ),
            ("UPDATE snapshot_metadata SET backend = 'other'", "backend"),
            (
                "UPDATE snapshot_metadata SET snapshot_id = 'wrong'",
                "metadata",
            ),
            (
                "UPDATE snapshot_metadata SET preprocessing_config = 'wrong'",
                "metadata",
            ),
            ("PRAGMA application_id = 0", "metadata"),
        ] {
            let (_temporary, handle): (tempfile::TempDir, SnapshotHandle) = snapshot();
            let connection: rusqlite::Connection =
                rusqlite::Connection::open(handle.as_path().join("db.sqlite")).unwrap();
            connection.execute_batch(sql).unwrap();
            connection.close().unwrap();
            let error: Error = handle.info().unwrap_err();
            match expected {
                "format" => assert!(matches!(error, Error::UnsupportedFormat(999))),
                "backend" => assert!(matches!(error, Error::UnsupportedBackend(_))),
                _ => assert!(matches!(error, Error::InvalidMetadata(_))),
            }
        }
    }

    #[test]
    fn rejects_corrupt_database_and_missing_metadata() {
        let (_temporary, handle): (tempfile::TempDir, SnapshotHandle) = snapshot();
        let database: std::path::PathBuf = handle.as_path().join("db.sqlite");
        let connection: rusqlite::Connection = rusqlite::Connection::open(&database).unwrap();
        connection
            .execute("DELETE FROM snapshot_metadata", [])
            .unwrap();
        connection.close().unwrap();
        assert!(handle.info().is_err());
        fs::write(&database, b"not sqlite").unwrap();
        assert!(handle.info().is_err());
        assert_eq!(fs::read(database).unwrap(), b"not sqlite");
    }

    #[test]
    fn snapshot_directory_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let (_temporary, handle): (tempfile::TempDir, SnapshotHandle) = snapshot();
        assert_eq!(
            fs::metadata(handle.as_path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn cli_info_reports_metadata() {
        let (_temporary, handle): (tempfile::TempDir, SnapshotHandle) = snapshot();
        let output: std::process::Output = Command::new(env!("CARGO_BIN_EXE_sift"))
            .arg("info")
            .arg(handle.as_path())
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains(&handle.info().unwrap().id)
        );
    }
}

#[test]
fn cli_failure_has_no_success_payload() {
    let temporary: tempfile::TempDir = tempfile::tempdir().unwrap();
    let missing: std::path::PathBuf = temporary.path().join(uuid::Uuid::new_v4().to_string());
    let output: std::process::Output = Command::new(env!("CARGO_BIN_EXE_sift"))
        .arg("info")
        .arg(&missing)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    assert!(!missing.exists());
    let output: std::process::Output = Command::new(env!("CARGO_BIN_EXE_sift"))
        .arg("info")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
