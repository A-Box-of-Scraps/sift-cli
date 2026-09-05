use std::path::Path;

use rusqlite::{Connection, OpenFlags, params};

use crate::{Error, Result, SnapshotInfo};

pub(crate) const BACKEND: &str = "sqlite";
pub(crate) const FORMAT_VERSION: u32 = 1;
const APPLICATION_ID: i64 = 0x5349_4654;

pub(crate) fn create(path: &Path, info: &SnapshotInfo) -> Result<()> {
    let mut connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    // A published artifact must not depend on a WAL sidecar.
    connection.execute_batch("PRAGMA journal_mode = DELETE; PRAGMA synchronous = FULL;")?;
    let transaction = connection.transaction()?;
    transaction.execute_batch(include_str!("schema.sql"))?;
    transaction.execute(
        "INSERT INTO snapshot_metadata VALUES (1, ?1, ?2, ?3, ?4, ?5)",
        params![
            info.id,
            info.created_at_unix_seconds,
            info.backend,
            info.format_version,
            info.preprocessing_config
        ],
    )?;
    transaction.commit()?;
    connection.close().map_err(|(_, error)| error)?;
    Ok(())
}

pub(crate) fn read(path: &Path) -> Result<SnapshotInfo> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let application_id: i64 =
        connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    if application_id != APPLICATION_ID {
        return Err(Error::InvalidMetadata("not a Sift database".into()));
    }
    let info = connection.query_row(
        "SELECT snapshot_id, created_at_unix_seconds, backend, format_version,
                preprocessing_config FROM snapshot_metadata WHERE singleton = 1",
        [],
        |row| {
            Ok(SnapshotInfo {
                id: row.get(0)?,
                created_at_unix_seconds: row.get(1)?,
                backend: row.get(2)?,
                format_version: row.get(3)?,
                preprocessing_config: row.get(4)?,
            })
        },
    )?;
    if info.format_version != FORMAT_VERSION {
        return Err(Error::UnsupportedFormat(info.format_version));
    }
    if info.backend != BACKEND {
        return Err(Error::UnsupportedBackend(info.backend));
    }
    if info.created_at_unix_seconds < 0 {
        return Err(Error::InvalidMetadata("negative creation timestamp".into()));
    }
    if info.preprocessing_config != "none" {
        return Err(Error::InvalidMetadata(
            "metadata-only format requires preprocessing_config=none".into(),
        ));
    }
    Ok(info)
}
