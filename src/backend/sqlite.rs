use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};

use crate::{
    Error, Result, RootInfo, SearchResult, SnapshotInfo, chunk,
    input::{self, SelectedInput},
    query::{self, PreparedQuery},
    tokenize,
};

pub(crate) const BACKEND: &str = "sqlite";
pub(crate) const FORMAT_VERSION: u32 = 2;
const APPLICATION_ID: i64 = 0x5349_4654;

pub(crate) fn create(path: &Path, info: &SnapshotInfo) -> Result<()> {
    build(path, info, |_| Ok(()))
}

fn build(
    path: &Path,
    info: &SnapshotInfo,
    populate: impl FnOnce(&Transaction<'_>) -> Result<()>,
) -> Result<()> {
    let mut connection: Connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    // A published artifact must not depend on a WAL sidecar.
    connection.execute_batch(
        "PRAGMA journal_mode = DELETE; PRAGMA synchronous = FULL; PRAGMA foreign_keys = ON;",
    )?;
    let transaction: Transaction<'_> = connection.transaction()?;
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
    populate(&transaction)?;
    transaction.commit()?;
    connection.close().map_err(|(_, error)| error)?;
    Ok(())
}

pub(crate) fn index(path: &Path, info: &SnapshotInfo, selected: &SelectedInput) -> Result<()> {
    build(path, info, |transaction| {
        transaction.execute_batch(include_str!("search.sql"))?;
        populate(transaction, selected)
    })
}

fn populate(transaction: &Transaction<'_>, selected: &SelectedInput) -> Result<()> {
    transaction.execute(
        "INSERT OR IGNORE INTO roots VALUES (?1, ?2, ?3)",
        params![
            selected.root.id,
            selected.root.name,
            selected.root.location.to_str()
        ],
    )?;
    let mut insert_file: rusqlite::Statement<'_> = transaction.prepare(
        "INSERT INTO files (root_id, path, content_hash, byte_count) VALUES (?1, ?2, ?3, ?4)",
    )?;
    let mut insert_chunk: rusqlite::Statement<'_> = transaction.prepare(
        "INSERT INTO chunks (file_id, start_line, end_line, start_byte, end_byte, text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    let mut insert_search: rusqlite::Statement<'_> =
        transaction.prepare("INSERT INTO chunk_search (rowid, path, body) VALUES (?1, ?2, ?3)")?;
    let mut count = 0;
    let documents = selected
        .files
        .iter()
        .map(|source| match input::read(&selected.root, source) {
            Ok(document) => Ok(Some(document)),
            Err(error) if !source.explicit => {
                input::report_skip(&error.to_string());
                Ok(None)
            }
            Err(error) => Err(error),
        })
        .chain(
            selected
                .documents
                .iter()
                .cloned()
                .map(|document| Ok(Some(document))),
        );
    for document in documents {
        let Some(document): Option<crate::document::Document> = document? else {
            continue;
        };
        count += 1;
        if unchanged(transaction, &document)? {
            continue;
        }
        insert_file.execute(params![
            document.root_id,
            document.path,
            document.content_hash,
            document.text.len()
        ])?;
        let file_id = transaction.last_insert_rowid();
        let searchable_path: String = tokenize::searchable(&document.path);
        for chunk in chunk::chunk_text(&document.text) {
            insert_chunk.execute(params![
                file_id,
                chunk.start_line,
                chunk.end_line,
                chunk.start_byte,
                chunk.end_byte,
                chunk.text
            ])?;
            insert_search.execute(params![
                transaction.last_insert_rowid(),
                searchable_path,
                tokenize::searchable(chunk.text)
            ])?;
        }
    }
    if count == 0 {
        return Err(Error::InvalidOptions(
            "selection contains no indexable documents".into(),
        ));
    }
    transaction.execute(
        "INSERT INTO chunk_search(chunk_search) VALUES ('optimize')",
        [],
    )?;
    Ok(())
}

pub(crate) fn read(path: &Path) -> Result<SnapshotInfo> {
    let connection: Connection =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let application_id: i64 =
        connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    if application_id != APPLICATION_ID {
        return Err(Error::InvalidMetadata("not a Sift database".into()));
    }
    let mut info: SnapshotInfo = connection.query_row(
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
                roots: Vec::new(),
                file_count: 0,
                chunk_count: 0,
            })
        },
    )?;
    if info.format_version != 1 && info.format_version != FORMAT_VERSION {
        return Err(Error::UnsupportedFormat(info.format_version));
    }
    if info.backend != BACKEND {
        return Err(Error::UnsupportedBackend(info.backend));
    }
    if info.created_at_unix_seconds < 0 {
        return Err(Error::InvalidMetadata("negative creation timestamp".into()));
    }
    let expected_config = if info.format_version == 1 {
        "none"
    } else {
        chunk::PREPROCESSING_CONFIG
    };
    if info.preprocessing_config != expected_config {
        return Err(Error::InvalidMetadata(
            "unrecognized preprocessing configuration".into(),
        ));
    }
    if info.format_version == FORMAT_VERSION {
        let mut statement: rusqlite::Statement<'_> =
            connection.prepare("SELECT id, name, location FROM roots ORDER BY name")?;
        info.roots = statement
            .query_map([], |row| {
                Ok(RootInfo {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    location: std::path::PathBuf::from(row.get::<_, String>(2)?),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        info.file_count =
            connection.query_row("SELECT count(*) FROM files", [], |row| row.get(0))?;
        info.chunk_count =
            connection.query_row("SELECT count(*) FROM chunks", [], |row| row.get(0))?;
    }
    Ok(info)
}

pub(crate) fn query(path: &Path, query: &PreparedQuery) -> Result<Vec<SearchResult>> {
    let connection: Connection =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    // FTS syntax is generated here, never accepted from the caller.
    let expression: String = query
        .terms
        .iter()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ");
    let mut statement: rusqlite::Statement<'_> = connection.prepare(
        "SELECT c.id, r.id, r.name, f.path, c.start_line, c.end_line,
                c.start_byte, c.end_byte, c.text
         FROM chunk_search
         JOIN chunks c ON c.id = chunk_search.rowid
         JOIN files f ON f.id = c.file_id
         JOIN roots r ON r.id = f.root_id
         WHERE chunk_search MATCH ?1
           AND (?4 IS NULL OR r.name = ?4)
           AND (?2 IS NULL OR f.path = ?2 OR substr(f.path, 1, length(?2) + 1) = ?2 || '/')
         ORDER BY bm25(chunk_search, 3.0, 1.0), c.id LIMIT ?3",
    )?;
    let candidates = statement.query_map(
        params![expression, query.path, query.limit * 10, query.root],
        |row| {
            Ok(SearchResult {
                id: format!("chunk-{}", row.get::<_, i64>(0)?),
                root_id: row.get(1)?,
                root_name: row.get(2)?,
                path: row.get(3)?,
                start_line: row.get(4)?,
                end_line: row.get(5)?,
                start_byte: row.get(6)?,
                end_byte: row.get(7)?,
                snippet: row.get(8)?,
                truncated: false,
            })
        },
    )?;
    let candidates: Vec<SearchResult> = candidates.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(query::select(candidates, query.limit))
}

pub(crate) fn extend(path: &Path, info: &SnapshotInfo, selections: &[SelectedInput]) -> Result<()> {
    if selections.is_empty() {
        return Ok(());
    }
    let mut connection: Connection = Connection::open(path)?;
    connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = DELETE;")?;
    let transaction: Transaction<'_> = connection.transaction()?;
    transaction.execute(
        "UPDATE snapshot_metadata SET snapshot_id = ?1, created_at_unix_seconds = ?2",
        params![info.id, info.created_at_unix_seconds],
    )?;
    for selected in selections {
        populate(&transaction, selected)?;
    }
    transaction.commit()?;
    connection.execute_batch("VACUUM")?;
    connection.close().map_err(|(_, error)| error)?;
    Ok(())
}

pub(crate) fn sources(path: &Path) -> Result<Vec<(String, String, String)>> {
    let connection: Connection =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    Ok(connection
        .prepare("SELECT root_id, path, content_hash FROM files ORDER BY root_id, path")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

fn unchanged(transaction: &Transaction<'_>, document: &crate::Document) -> Result<bool> {
    let existing: Option<(i64, String)> = transaction
        .query_row(
            "SELECT id, content_hash FROM files WHERE root_id = ?1 AND path = ?2",
            params![document.root_id, document.path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((id, hash)) = existing {
        if hash == document.content_hash {
            return Ok(true);
        }
        transaction.execute(
            "DELETE FROM chunk_search WHERE rowid IN (SELECT id FROM chunks WHERE file_id = ?1)",
            [id],
        )?;
        transaction.execute("DELETE FROM chunks WHERE file_id = ?1", [id])?;
        transaction.execute("DELETE FROM files WHERE id = ?1", [id])?;
    }
    Ok(false)
}
