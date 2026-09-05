CREATE TABLE roots (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    location TEXT NOT NULL
) STRICT;

CREATE TABLE files (
    id INTEGER PRIMARY KEY,
    root_id TEXT NOT NULL REFERENCES roots(id),
    path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    byte_count INTEGER NOT NULL CHECK (byte_count >= 0),
    UNIQUE (root_id, path)
) STRICT;

CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id),
    start_line INTEGER NOT NULL CHECK (start_line > 0),
    end_line INTEGER NOT NULL CHECK (end_line >= start_line),
    start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK (end_byte > start_byte),
    text TEXT NOT NULL
) STRICT;

CREATE VIRTUAL TABLE chunk_search USING fts5(
    path,
    body,
    tokenize = "unicode61 remove_diacritics 0 tokenchars '_'"
);
