use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use uuid::Uuid;

use crate::{Error, Result, backend::sqlite, data_directory};

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotInfo {
    pub id: String,
    pub created_at_unix_seconds: i64,
    pub backend: String,
    pub format_version: u32,
    pub preprocessing_config: String,
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

    // Metadata only, not a searchable index. Publication requires Linux.
    // Process termination may leave staging behind; atomic publication does not
    // guarantee power-loss durability.
    pub fn create_snapshot(&self) -> Result<SnapshotHandle> {
        self.create_with(Uuid::new_v4(), sqlite::create)
    }

    fn create_with(
        &self,
        id: Uuid,
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
            format_version: sqlite::FORMAT_VERSION,
            preprocessing_config: "none".into(),
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
        let result = store.create_with(Uuid::new_v4(), |path, _| {
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
        let handle = store.create_with(id, sqlite::create).unwrap();
        let before = fs::read(handle.as_path().join("db.sqlite")).unwrap();
        assert!(matches!(
            store.create_with(id, sqlite::create),
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
