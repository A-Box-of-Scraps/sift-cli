use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use uuid::Uuid;

use crate::{
    Error, IndexRequest, QueryResponse, Result, RootInfo, SearchQuery, backend::sqlite, chunk,
    data_directory, input,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotHandle(PathBuf);

impl SnapshotHandle {
    pub fn from_path(path: impl Into<PathBuf>) -> Result<Self> {
        let path: PathBuf = path.into();
        let valid_id = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| Uuid::parse_str(name).ok().map(|id| (name, id)))
            .is_some_and(|(name, id)| name == id.to_string());
        if !path.is_absolute() || !valid_id {
            return Err(Error::InvalidHandle(path));
        }
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn info(&self) -> Result<SnapshotInfo> {
        if !fs::symlink_metadata(&self.0)?.is_dir() {
            return Err(Error::InvalidHandle(self.0.clone()));
        }
        let database: PathBuf = self.0.join("db.sqlite");
        if !fs::symlink_metadata(&database)?.is_file() {
            return Err(Error::InvalidHandle(self.0.clone()));
        }
        let info: SnapshotInfo = sqlite::read(&database)?;
        if self.0.file_name().and_then(|name| name.to_str()) != Some(info.id.as_str()) {
            return Err(Error::InvalidMetadata(
                "snapshot ID does not match handle".into(),
            ));
        }
        Ok(info)
    }

    pub fn query(&self, query: &SearchQuery) -> Result<QueryResponse> {
        let prepared: crate::query::PreparedQuery = query.prepare()?;
        if self.info()?.format_version == 1 {
            return Err(Error::NotSearchable);
        }
        Ok(QueryResponse {
            schema_version: 1,
            handle: self.0.clone(),
            results: sqlite::query(&self.0.join("db.sqlite"), &prepared)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotInfo {
    pub id: String,
    pub created_at_unix_seconds: i64,
    pub backend: String,
    pub format_version: u32,
    pub preprocessing_config: String,
    pub roots: Vec<RootInfo>,
    pub file_count: usize,
    pub chunk_count: usize,
}

#[derive(Clone, Debug)]
pub struct SnapshotStore {
    directory: PathBuf,
}

impl SnapshotStore {
    pub fn new(directory: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            directory: std::path::absolute(directory)?,
        })
    }

    pub fn from_environment() -> Result<Self> {
        let xdg: Option<std::ffi::OsString> = std::env::var_os("XDG_DATA_HOME");
        let home: Option<std::ffi::OsString> = std::env::var_os("HOME");
        Self::new(data_directory(
            xdg.as_deref().map(Path::new),
            home.as_deref().map(Path::new),
        )?)
    }

    // Retained for metadata-only format-one artifacts, not file indexing.
    pub fn create_snapshot(&self) -> Result<SnapshotHandle> {
        self.create_with(Uuid::new_v4(), 1, "none", sqlite::create)
    }

    pub fn index(&self, request: &IndexRequest) -> Result<SnapshotHandle> {
        self.index_with_options(request, &crate::input::DiscoveryOptions::default())
    }

    pub fn index_with_options(
        &self,
        request: &IndexRequest,
        options: &crate::input::DiscoveryOptions,
    ) -> Result<SnapshotHandle> {
        let selected: input::SelectedInput = input::select(request, options)?;
        self.index_selected(&selected)
    }

    pub fn index_documents(&self, documents: &[crate::TextDocument]) -> Result<SnapshotHandle> {
        if documents.is_empty() {
            return Err(Error::InvalidOptions(
                "at least one document is required".into(),
            ));
        }
        let root: RootInfo = RootInfo {
            id: "documents".into(),
            name: "documents".into(),
            location: PathBuf::new(),
        };
        let mut names: std::collections::BTreeSet<&String> = std::collections::BTreeSet::new();
        let mut typed: Vec<crate::document::Document> = Vec::new();
        for document in documents {
            if document.name.is_empty() || !names.insert(&document.name) {
                return Err(Error::InvalidOptions(
                    "document names must be nonempty and unique".into(),
                ));
            }
            if document.text.len() as u64 > crate::input::MAX_FILE_BYTES
                || document.text.contains('\0')
            {
                return Err(Error::InvalidOptions(
                    "documents must be NUL-free UTF-8 within 8 MiB".into(),
                ));
            }
            let name: String = document.name.bytes().map(encode_name_byte).collect();
            typed.push(crate::Document {
                root_id: root.id.clone(),
                path: format!("document:{name}"),
                content_hash: blake3::hash(document.text.as_bytes()).to_hex().to_string(),
                text: document.text.clone(),
            });
        }
        self.index_selected(&input::SelectedInput {
            root,
            files: Vec::new(),
            documents: typed,
        })
    }

    fn index_selected(&self, selected: &input::SelectedInput) -> Result<SnapshotHandle> {
        self.create_with(
            Uuid::new_v4(),
            sqlite::FORMAT_VERSION,
            chunk::PREPROCESSING_CONFIG,
            |path, info| sqlite::index(path, info, selected),
        )
    }

    pub fn index_roots(
        &self,
        requests: &[(String, IndexRequest)],
        options: &crate::input::DiscoveryOptions,
        base: Option<&SnapshotHandle>,
    ) -> Result<SnapshotHandle> {
        if requests.is_empty() {
            return Err(Error::InvalidOptions(
                "at least one root is required".into(),
            ));
        }
        let mut roots: Vec<RootInfo> = match base {
            Some(handle) => {
                let info: SnapshotInfo = handle.info()?;
                if info.format_version == 1 {
                    return Err(Error::NotSearchable);
                }
                info.roots
            }
            None => Vec::new(),
        };
        let mut names: std::collections::BTreeSet<&String> = std::collections::BTreeSet::new();
        let mut selections: Vec<input::SelectedInput> = Vec::new();
        for (name, request) in requests {
            if name.is_empty()
                || name == "documents"
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                || !names.insert(name)
            {
                return Err(Error::InvalidOptions(
                    "root names must be unique ASCII identifiers; documents is reserved".into(),
                ));
            }
            let mut selected: input::SelectedInput = input::select(request, options)?;
            bind_root(&mut selected.root, name, &roots)?;
            selected.root.name = name.clone();
            roots.push(selected.root.clone());
            selections.push(selected);
        }
        self.create_with(
            Uuid::new_v4(),
            sqlite::FORMAT_VERSION,
            chunk::PREPROCESSING_CONFIG,
            |path, info| {
                if let Some(base) = base {
                    fs::copy(base.as_path().join("db.sqlite"), path)?;
                    sqlite::extend(path, info, &selections)
                } else {
                    sqlite::index(path, info, &selections[0])?;
                    sqlite::extend(path, info, &selections[1..])
                }
            },
        )
    }

    fn lock(&self) -> Result<fs::File> {
        fs::create_dir_all(&self.directory)?;
        let path: PathBuf = self.directory.join(".lifecycle-lock");
        let mut options: std::fs::OpenOptions = fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        }
        let file: std::fs::File = options.open(path)?;
        file.lock()?;
        Ok(file)
    }

    pub fn delete(&self, handle: &SnapshotHandle) -> Result<()> {
        let _lock: std::fs::File = self.lock()?;
        let indexes: PathBuf = fs::canonicalize(self.directory.join("indexes"))?;
        if handle.as_path().parent() != Some(indexes.as_path())
            || fs::canonicalize(handle.as_path())? != handle.as_path()
        {
            return Err(Error::InvalidHandle(handle.as_path().into()));
        }
        handle.info()?;
        artifact_entries(handle.as_path(), false)?;
        let retired: PathBuf = indexes.join(format!(".staging-delete-{}", Uuid::new_v4()));
        publish(handle.as_path(), &retired)?;
        remove_artifact(&retired, false)
    }

    pub fn cleanup_staging(&self) -> Result<usize> {
        let _lock: std::fs::File = self.lock()?;
        let indexes: PathBuf = self.directory.join("indexes");
        if !indexes.exists() {
            return Ok(0);
        }
        let mut removed = 0;
        for entry in fs::read_dir(indexes)? {
            let entry: std::fs::DirEntry = entry?;
            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(".staging-"))
            {
                remove_artifact(&entry.path(), true)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    // Linux publication is atomic, not power-loss durable. Process termination
    // may leave an unpublished staging directory behind.
    fn create_with(
        &self,
        id: Uuid,
        format_version: u32,
        preprocessing_config: &str,
        initialize: impl FnOnce(&Path, &SnapshotInfo) -> Result<()>,
    ) -> Result<SnapshotHandle> {
        let _lock: std::fs::File = self.lock()?;
        let indexes: PathBuf = self.directory.join("indexes");
        let mut builder: std::fs::DirBuilder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&indexes)?;
        let indexes: PathBuf = fs::canonicalize(indexes)?;
        let mut staging_builder: tempfile::Builder<'_, '_> = tempfile::Builder::new();
        staging_builder.prefix(".staging-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            staging_builder.permissions(fs::Permissions::from_mode(0o700));
        }
        let staging: tempfile::TempDir = staging_builder.tempdir_in(&indexes)?;
        let info: SnapshotInfo = SnapshotInfo {
            id: id.to_string(),
            created_at_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| Error::InvalidMetadata(error.to_string()))?
                .as_secs()
                .try_into()
                .map_err(|_| Error::InvalidMetadata("creation timestamp out of range".into()))?,
            backend: sqlite::BACKEND.into(),
            format_version,
            preprocessing_config: preprocessing_config.into(),
            roots: Vec::new(),
            file_count: 0,
            chunk_count: 0,
        };
        initialize(&staging.path().join("db.sqlite"), &info)?;
        // Reject sidecars: published databases must be self-contained.
        let entries: Vec<std::fs::DirEntry> =
            fs::read_dir(staging.path())?.collect::<std::io::Result<Vec<_>>>()?;
        if entries.len() != 1 || entries[0].file_name() != "db.sqlite" {
            return Err(Error::InvalidMetadata("unexpected staging files".into()));
        }
        let destination: PathBuf = indexes.join(&info.id);
        publish(staging.path(), &destination)?;
        // Disarm cleanup after moving the staging directory.
        let _ = staging.keep();
        Ok(SnapshotHandle(destination))
    }
}

#[cfg(target_os = "linux")]
fn publish(source: &Path, destination: &Path) -> Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};
    renameat_with(CWD, source, CWD, destination, RenameFlags::NOREPLACE).map_err(|error| {
        if error == rustix::io::Errno::EXIST {
            Error::AlreadyExists(destination.to_path_buf())
        } else {
            Error::Io(error.into())
        }
    })
}

#[cfg(not(target_os = "linux"))]
fn publish(_source: &Path, _destination: &Path) -> Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "atomic no-replace snapshot publication currently requires Linux",
    )
    .into())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn interrupted_extension_child() {
        let Some(directory): Option<std::ffi::OsString> =
            std::env::var_os("SIFT_TEST_INTERRUPTION_STORE")
        else {
            return;
        };
        let base: std::ffi::OsString = std::env::var_os("SIFT_TEST_INTERRUPTION_BASE").unwrap();
        let store: SnapshotStore = SnapshotStore::new(directory).unwrap();
        let _ = store.create_with(
            Uuid::new_v4(),
            sqlite::FORMAT_VERSION,
            chunk::PREPROCESSING_CONFIG,
            |path, _| {
                fs::copy(PathBuf::from(base).join("db.sqlite"), path)?;
                let connection: rusqlite::Connection = rusqlite::Connection::open(path)?;
                connection.execute_batch(
                    "BEGIN IMMEDIATE; UPDATE snapshot_metadata SET snapshot_id = 'interrupted';",
                )?;
                std::process::exit(99);
            },
        );
        panic!("child did not exit");
    }

    #[test]
    fn interrupted_extension_is_unpublished_and_recoverable() {
        let temporary: tempfile::TempDir = tempfile::tempdir().unwrap();
        let store: SnapshotStore = SnapshotStore::new(temporary.path()).unwrap();
        let base: SnapshotHandle = store
            .index_documents(&[crate::TextDocument {
                name: "test".into(),
                text: "original searchable content".into(),
            }])
            .unwrap();
        let before: Vec<u8> = fs::read(base.as_path().join("db.sqlite")).unwrap();
        let status: std::process::ExitStatus =
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "snapshot::tests::interrupted_extension_child"])
                .env("SIFT_TEST_INTERRUPTION_STORE", temporary.path())
                .env("SIFT_TEST_INTERRUPTION_BASE", base.as_path())
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap();
        assert_eq!(status.code(), Some(99));
        assert_eq!(store.cleanup_staging().unwrap(), 1);
        assert_eq!(store.cleanup_staging().unwrap(), 0);
        assert_eq!(before, fs::read(base.as_path().join("db.sqlite")).unwrap());
        assert_eq!(
            base.query(&SearchQuery::new("original"))
                .unwrap()
                .results
                .len(),
            1
        );
    }

    #[test]
    fn cleanup_waits_for_active_builder() {
        let temporary: tempfile::TempDir = tempfile::tempdir().unwrap();
        let store: SnapshotStore = SnapshotStore::new(temporary.path()).unwrap();
        let lock: std::fs::File = store.lock().unwrap();
        let staging: PathBuf = temporary.path().join("indexes/.staging-active");
        fs::create_dir_all(&staging).unwrap();
        let (started_tx, started_rx): (std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>) =
            std::sync::mpsc::channel();
        let (done_tx, done_rx): (
            std::sync::mpsc::Sender<usize>,
            std::sync::mpsc::Receiver<usize>,
        ) = std::sync::mpsc::channel();
        let worker: std::thread::JoinHandle<()> = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            done_tx.send(store.cleanup_staging().unwrap()).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(
            done_rx
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err()
        );
        assert!(staging.exists());
        drop(lock);
        assert_eq!(
            done_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            1
        );
        worker.join().unwrap();
    }

    #[test]
    fn failure_cleans_staging_and_does_not_publish() {
        let temporary: tempfile::TempDir = tempfile::tempdir().unwrap();
        let store: SnapshotStore = SnapshotStore::new(temporary.path()).unwrap();
        let result: Result<SnapshotHandle> =
            store.create_with(Uuid::new_v4(), 1, "none", |path, _| {
                fs::write(path, b"partial database")?;
                Err(Error::InvalidMetadata("injected failure".into()))
            });
        assert!(result.is_err());
        assert_eq!(
            fs::read_dir(temporary.path().join("indexes"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn collision_preserves_existing_snapshot_and_cleans_staging() {
        let temporary: tempfile::TempDir = tempfile::tempdir().unwrap();
        let store: SnapshotStore = SnapshotStore::new(temporary.path()).unwrap();
        let id: Uuid = Uuid::new_v4();
        let handle: SnapshotHandle = store.create_with(id, 1, "none", sqlite::create).unwrap();
        let before: Vec<u8> = fs::read(handle.as_path().join("db.sqlite")).unwrap();
        assert!(matches!(
            store.create_with(id, 1, "none", sqlite::create),
            Err(Error::AlreadyExists(_))
        ));
        assert_eq!(
            fs::read(handle.as_path().join("db.sqlite")).unwrap(),
            before
        );
        assert_eq!(
            fs::read_dir(temporary.path().join("indexes"))
                .unwrap()
                .count(),
            1
        );
    }
}

fn encode_name_byte(byte: u8) -> String {
    if byte.is_ascii_alphanumeric() || b"-_.".contains(&byte) {
        (byte as char).to_string()
    } else {
        format!("%{byte:02X}")
    }
}

fn artifact_entries(path: &Path, staging: bool) -> Result<Vec<fs::DirEntry>> {
    let metadata: std::fs::Metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Err(Error::InvalidHandle(path.into()));
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != rustix::process::geteuid().as_raw() {
            return Err(Error::InvalidHandle(path.into()));
        }
    }
    let entries: Vec<std::fs::DirEntry> =
        fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    for entry in &entries {
        let name: std::ffi::OsString = entry.file_name();
        let allowed = name == "db.sqlite" || (staging && name == "db.sqlite-journal");
        if !allowed || !entry.file_type()?.is_file() {
            return Err(Error::InvalidHandle(path.into()));
        }
    }
    Ok(entries)
}

fn remove_artifact(path: &Path, staging: bool) -> Result<()> {
    for entry in artifact_entries(path, staging)? {
        fs::remove_file(entry.path())?;
    }
    fs::remove_dir(path)?;
    Ok(())
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct SourceStatus {
    pub root: String,
    pub path: String,
    pub status: String,
}

impl SnapshotHandle {
    pub fn check_staleness(&self) -> Result<Vec<SourceStatus>> {
        let info: SnapshotInfo = self.info()?;
        if info.format_version == 1 {
            return Err(Error::NotSearchable);
        }
        let mut statuses: Vec<SourceStatus> = Vec::new();
        for (root_id, path, hash) in sqlite::sources(&self.0.join("db.sqlite"))? {
            let root: &RootInfo = info
                .roots
                .iter()
                .find(|root| root.id == root_id)
                .ok_or_else(|| Error::InvalidMetadata("source has no root".into()))?;
            let status: String = if root.location.as_os_str().is_empty() {
                "unavailable".into()
            } else if Path::new(&path).is_absolute()
                || Path::new(&path)
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err(Error::InvalidMetadata("invalid source path".into()));
            } else {
                let source: input::SelectedFile = input::SelectedFile {
                    absolute: root.location.join(&path),
                    logical: path.clone(),
                    explicit: true,
                };
                match input::read(root, &source) {
                    Ok(document) if document.content_hash == hash => "unchanged".into(),
                    Ok(_) => "changed".into(),
                    Err(_) => "unavailable".into(),
                }
            };
            statuses.push(SourceStatus {
                root: root.name.clone(),
                path,
                status,
            });
        }
        Ok(statuses)
    }
}

fn bind_root(selected: &mut RootInfo, name: &str, roots: &[RootInfo]) -> Result<()> {
    if let Some(root) = roots.iter().find(|root| root.name == name) {
        if root.location != selected.location {
            return Err(Error::InvalidOptions(format!(
                "root {name} cannot be rebound"
            )));
        }
        selected.id = root.id.clone();
    } else {
        if roots.iter().any(|root| root.location == selected.location) {
            return Err(Error::InvalidOptions(
                "location already has a different root name".into(),
            ));
        }
        selected.id = Uuid::new_v4().to_string();
    }
    Ok(())
}
