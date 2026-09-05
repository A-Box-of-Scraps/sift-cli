PRAGMA application_id = 0x53494654;

CREATE TABLE snapshot_metadata (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    snapshot_id TEXT NOT NULL,
    created_at_unix_seconds INTEGER NOT NULL CHECK (created_at_unix_seconds >= 0),
    backend TEXT NOT NULL,
    format_version INTEGER NOT NULL CHECK (format_version > 0),
    preprocessing_config TEXT NOT NULL
) STRICT;
