[Documentation](../README.md) / Reference

# Rust API

The CLI uses the reusable `sift` library. Generate signature-level documentation
from this checkout with:

```sh
cargo doc --no-deps --open
```

## Entry points

| Type or method | Purpose |
| --- | --- |
| `SnapshotStore::new` | Select an explicit store directory. |
| `SnapshotStore::from_environment` | Select the CLI's environment-based store. |
| `SnapshotStore::index` | Index an `IndexRequest` with default discovery options. |
| `SnapshotStore::index_with_options` | Index with explicit `DiscoveryOptions`. |
| `SnapshotStore::index_roots` | Index named `(String, IndexRequest)` pairs with options and an optional base handle. |
| `SnapshotStore::index_documents` | Index a slice of `TextDocument { name, text }`. |
| `SnapshotStore::delete` | Delete a handle owned by this store. |
| `SnapshotStore::cleanup_staging` | Remove abandoned staging directories and return their count. |
| `SnapshotHandle::from_path` | Open and validate a supplied snapshot path. |
| `SnapshotHandle::as_path` | Borrow the handle path. |
| `SnapshotHandle::info` | Read `SnapshotInfo`. |
| `SnapshotHandle::query` | Execute a `SearchQuery`, returning `QueryResponse`. |
| `SnapshotHandle::check_staleness` | Return a vector of `SourceStatus`. |

`SnapshotStore::create_snapshot` is retained for metadata-only format-1 artifacts;
use indexing methods for searchable snapshots.

## Request and result types

`IndexRequest` contains `root` and `files`. `DiscoveryOptions` contains an
`IgnoreMode` and `hidden`. Typed document names must be nonempty and unique within
the input slice; the library computes source identifiers and hashes.

`SearchQuery::new(text)` defaults to a limit of 5 with no path or root filter.
Its public fields are `text`, `limit`, `path`, and `root`. `QueryResponse` and
`SearchResult` expose the fields in the [output reference](output.md).

`SnapshotInfo` exposes the ID, creation timestamp, backend, format version,
preprocessing configuration, roots, file count, and chunk count. Public support
types include `RootInfo`, `Document`, `Chunk`, `Error`, and `Result`.
`chunk_text`, `MAX_CHUNK_BYTES`, `MAX_FILE_BYTES`, and `data_directory` are also
exported. See [library exports](../../src/lib.rs) for the authoritative list.

Library operations share CLI [input](inputs.md) and [snapshot](snapshots.md)
semantics. Discovery skip diagnostics currently go to stderr for library callers;
the completion count and terminal rendering are CLI-only.
