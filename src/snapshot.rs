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
        let path = path.into();
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
        let database = self.0.join("db.sqlite");
        if !fs::symlink_metadata(&database)?.is_file() {
            return Err(Error::InvalidHandle(self.0.clone()));
        }
        let info = sqlite::read(&database)?;
        if self.0.file_name().and_then(|name| name.to_str()) != Some(info.id.as_str()) {
            return Err(Error::InvalidMetadata(
                "snapshot ID does not match handle".into(),
            ));
        }
        Ok(info)
    }

    pub fn query(&self, query: &SearchQuery) -> Result<QueryResponse> {
        let prepared = query.prepare()?;
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
        let xdg = std::env::var_os("XDG_DATA_HOME");
        let home = std::env::var_os("HOME");
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
        self.index_with_options(request, &input::DiscoveryOptions::default())
    }

    pub fn index_with_options(
        &self,
        request: &IndexRequest,
        options: &input::DiscoveryOptions,
    ) -> Result<SnapshotHandle> {
        let selected = input::select(request, options)?;
        self.index_selected(&selected)
    }

    pub fn index_documents(&self, documents: &[crate::TextDocument]) -> Result<SnapshotHandle> {
        if documents.is_empty() {
            return Err(Error::InvalidOptions(
                "at least one document is required".into(),
            ));
        }
        let root = RootInfo {
            id: "documents".into(),
            name: "documents".into(),
            location: PathBuf::new(),
        };
        let mut names = std::collections::BTreeSet::new();
        let mut typed = Vec::new();
        for document in documents {
            if document.name.is_empty() || !names.insert(&document.name) {
                return Err(Error::InvalidOptions(
                    "document names must be nonempty and unique".into(),
                ));
            }
            if document.text.len() as u64 > input::MAX_FILE_BYTES || document.text.contains('\0') {
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

    // Linux publication is atomic, not power-loss durable. Process termination
    // may leave an unpublished staging directory behind.
    fn create_with(
        &self,
        id: Uuid,
        format_version: u32,
        preprocessing_config: &str,
        initialize: impl FnOnce(&Path, &SnapshotInfo) -> Result<()>,
    ) -> Result<SnapshotHandle> {
        let indexes = self.directory.join("indexes");
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&indexes)?;
        let indexes = fs::canonicalize(indexes)?;
        let mut staging_builder = tempfile::Builder::new();
        staging_builder.prefix(".staging-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            staging_builder.permissions(fs::Permissions::from_mode(0o700));
        }
        let staging = staging_builder.tempdir_in(&indexes)?;
        let info = SnapshotInfo {
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
        let entries = fs::read_dir(staging.path())?.collect::<std::io::Result<Vec<_>>>()?;
        if entries.len() != 1 || entries[0].file_name() != "db.sqlite" {
            return Err(Error::InvalidMetadata("unexpected staging files".into()));
        }
        let destination = indexes.join(&info.id);
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
    fn failure_cleans_staging_and_does_not_publish() {
        let temporary = tempfile::tempdir().unwrap();
        let store = SnapshotStore::new(temporary.path()).unwrap();
        let result = store.create_with(Uuid::new_v4(), 1, "none", |path, _| {
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
        let temporary = tempfile::tempdir().unwrap();
        let store = SnapshotStore::new(temporary.path()).unwrap();
        let id = Uuid::new_v4();
        let handle = store.create_with(id, 1, "none", sqlite::create).unwrap();
        let before = fs::read(handle.as_path().join("db.sqlite")).unwrap();
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
