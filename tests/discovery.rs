#![cfg(target_os = "linux")]

use sift::{
    DiscoveryOptions, IndexRequest, SearchQuery, SnapshotHandle, SnapshotStore, TextDocument,
};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

struct Fixture {
    temp: tempfile::TempDir,
    root: PathBuf,
    store: SnapshotStore,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        let store = SnapshotStore::new(temp.path().join("data")).unwrap();
        Self { temp, root, store }
    }

    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        let path = self.root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn index(&self, inputs: &[&str], options: DiscoveryOptions) -> sift::Result<SnapshotHandle> {
        self.store.index_with_options(
            &IndexRequest {
                root: self.root.clone(),
                files: inputs.iter().map(Into::into).collect(),
            },
            &options,
        )
    }

    fn cli(&self, args: &[&str], input: &[u8]) -> std::process::Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_sift"))
            .current_dir(&self.root)
            .env("XDG_DATA_HOME", self.temp.path().join("cli"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    }
}

fn count(output: std::process::Output) -> usize {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(text.lines().count(), 1);
    SnapshotHandle::from_path(text.trim())
        .unwrap()
        .info()
        .unwrap()
        .file_count
}

#[test]
fn ignore_flags_hidden_nested_rules_and_explicit_overrides() {
    let f = Fixture::new();
    f.write(".ignore", "ignored/\n");
    f.write(".gitignore", "git/\n*.tmp\n!keep.tmp\n");
    f.write("visible.txt", "visible");
    f.write("ignored/a.txt", "ignored");
    f.write("git/a.txt", "git");
    f.write("drop.tmp", "drop");
    f.write("keep.tmp", "keep");
    f.write(".hidden/a.txt", "hidden");
    f.write("nested/.ignore", "skip.txt\n");
    f.write("nested/skip.txt", "skip");
    f.write("nested/keep.txt", "keep");
    assert_eq!(
        f.index(&["."], DiscoveryOptions::default())
            .unwrap()
            .info()
            .unwrap()
            .file_count,
        3
    );
    assert_eq!(count(f.cli(&["index", ".", "--no-gitignore"], b"")), 5);
    assert_eq!(count(f.cli(&["index", ".", "--no-ignore"], b"")), 7);
    assert_eq!(
        count(f.cli(&["index", ".", "--no-ignore", "--hidden"], b"")),
        11
    );
    assert_eq!(
        f.index(
            &["ignored/a.txt", ".hidden/a.txt"],
            DiscoveryOptions::default()
        )
        .unwrap()
        .info()
        .unwrap()
        .file_count,
        2
    );
}

#[test]
fn mixed_globs_directories_and_unusual_names_deduplicate() {
    let f = Fixture::new();
    f.write("src/a.rs", "alpha");
    f.write("src/nested/b.rs", "beta");
    f.write("src/name with\nnewline.txt", "gamma");
    f.write("literal[1].txt", "delta");
    let h = f
        .index(
            &["src", "src/**/*.rs", "./src/a.rs", "literal[1].txt"],
            DiscoveryOptions::default(),
        )
        .unwrap();
    assert_eq!(h.info().unwrap().file_count, 4);
    assert_eq!(
        f.index(&["src/*.rs"], DiscoveryOptions::default())
            .unwrap()
            .info()
            .unwrap()
            .file_count,
        1
    );
    assert!(
        f.index(&["missing*.rs", "src"], DiscoveryOptions::default())
            .is_err()
    );
    assert!(f.index(&["../*.rs"], DiscoveryOptions::default()).is_err());
    assert_eq!(
        count(f.cli(
            &["index", "src/a.rs", "--files0-from", "-"],
            b"src/name with\nnewline.txt\0src/a.rs\0literal[1].txt\0"
        )),
        3
    );
    for input in [b"\0".as_slice(), b"src/a.rs", b""] {
        assert!(
            !f.cli(&["index", "--files0-from", "-"], input)
                .status
                .success()
        );
    }
}

#[test]
fn discovered_bad_files_skip_but_explicit_files_fail() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    f.write("good.txt", "needle");
    f.write("binary", b"a\0b");
    f.write("invalid", [255]);
    fs::File::create(f.root.join("large"))
        .unwrap()
        .set_len(sift::MAX_FILE_BYTES + 1)
        .unwrap();
    symlink("good.txt", f.root.join("link")).unwrap();
    symlink(".", f.root.join("cycle")).unwrap();
    let output = f.cli(&["index", "."], b"");
    assert!(String::from_utf8_lossy(&output.stderr).contains("skipped"));
    assert_eq!(count(output), 1);
    for path in ["binary", "invalid", "large", "link", "cycle/good.txt"] {
        assert!(f.index(&[".", path], DiscoveryOptions::default()).is_err());
    }
    fs::remove_file(f.root.join("good.txt")).unwrap();
    assert!(f.index(&["."], DiscoveryOptions::default()).is_err());
}

#[test]
fn stdin_and_typed_documents_have_nonfilesystem_names_and_stream_lines() {
    let f = Fixture::new();
    let output = f.cli(
        &["index", "--stdin", "--name", "/tmp/source.rs"],
        b"first\nneedle\n",
    );
    assert!(output.status.success(), "{:?}", output);
    let handle =
        SnapshotHandle::from_path(String::from_utf8(output.stdout).unwrap().trim()).unwrap();
    let result = handle
        .query(&SearchQuery::new("needle"))
        .unwrap()
        .results
        .remove(0);
    assert_eq!(result.root_name, "documents");
    assert_eq!(result.path, "document:%2Ftmp%2Fsource.rs");
    assert_eq!((result.start_line, result.end_line), (1, 2));
    assert!(
        handle.info().unwrap().roots[0]
            .location
            .as_os_str()
            .is_empty()
    );
    let docs = vec![
        TextDocument {
            name: "one".into(),
            text: "alpha".into(),
        },
        TextDocument {
            name: "two".into(),
            text: "beta".into(),
        },
    ];
    assert_eq!(
        f.store
            .index_documents(&docs)
            .unwrap()
            .info()
            .unwrap()
            .file_count,
        2
    );
    assert!(
        f.store
            .index_documents(&[docs[0].clone(), docs[0].clone()])
            .is_err()
    );
    assert!(f.store.index_documents(&[]).is_err());
    assert_eq!(count(f.cli(&["index", "--stdin"], b"")), 1);
    for bytes in [b"\0".as_slice(), &[255]] {
        assert!(!f.cli(&["index", "--stdin"], bytes).status.success());
    }
    for args in [
        vec!["index", "--name", "bad", "."],
        vec!["index", "--stdin", "."],
        vec!["index", "--stdin", "--files0-from", "-"],
    ] {
        assert!(!f.cli(&args, b"").status.success());
    }
}

#[test]
fn unreadable_files_non_utf8_names_and_empty_directories() {
    use std::os::unix::{ffi::OsStringExt, fs::PermissionsExt};
    let f = Fixture::new();
    fs::create_dir(f.root.join("empty")).unwrap();
    assert!(f.index(&["empty"], DiscoveryOptions::default()).is_err());
    f.write("good", "needle");
    let bad_name = std::ffi::OsString::from_vec(vec![b'b', 255]);
    fs::write(f.root.join(&bad_name), "invalid name").unwrap();
    assert_eq!(
        f.index(&["."], DiscoveryOptions::default())
            .unwrap()
            .info()
            .unwrap()
            .file_count,
        1
    );
    assert!(
        f.store
            .index(&IndexRequest {
                root: f.root.clone(),
                files: vec![bad_name.into()]
            })
            .is_err()
    );
    f.write("unreadable", "secret");
    let path = f.root.join("unreadable");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o0)).unwrap();
    if fs::File::open(&path).is_err() {
        assert_eq!(
            f.index(&["."], DiscoveryOptions::default())
                .unwrap()
                .info()
                .unwrap()
                .file_count,
            1
        );
        assert!(
            f.index(&["unreadable"], DiscoveryOptions::default())
                .is_err()
        );
    }
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(
        !f.cli(&["index", "--files0-from", "-"], b"*\0")
            .status
            .success()
    );
    assert!(
        !f.cli(&["index", "--files0-from", "-"], b"empty\0")
            .status
            .success()
    );
}

#[test]
fn git_exclude_and_root_boundary_are_consistent() {
    let f = Fixture::new();
    fs::write(f.temp.path().join(".ignore"), "root/\n*.rs\n").unwrap();
    f.write(".git/info/exclude", "excluded.rs\n");
    f.write("excluded.rs", "excluded");
    f.write("src/a.rs", "alpha");
    assert_eq!(
        f.index(&["."], DiscoveryOptions::default())
            .unwrap()
            .info()
            .unwrap()
            .file_count,
        1
    );
    assert_eq!(count(f.cli(&["index", ".", "--no-gitignore"], b"")), 2);
    assert_eq!(
        f.index(&["././src/./*.rs"], DiscoveryOptions::default())
            .unwrap()
            .info()
            .unwrap()
            .file_count,
        1
    );
    assert_eq!(
        f.store
            .index(&IndexRequest {
                root: f.root.clone(),
                files: vec![f.root.join("src/*.rs")]
            })
            .unwrap()
            .info()
            .unwrap()
            .file_count,
        1
    );
}
