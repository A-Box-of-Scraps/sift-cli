# Snapshot lifecycle and roots

## Commands and API

```sh
first=$(sift index --root /work/api --root-name api src)
second=$(sift index --extend "$first" --root /work/ui --root-name ui src)
third=$(sift index --extend "$second" --root /work/api --root-name api src)
sift query "$third" 'request handler' --root api --path src
sift check "$third"
sift delete "$first"
sift cleanup
```

`SnapshotStore::index_roots` accepts multiple `(name, IndexRequest)` pairs,
discovery options, and an optional base handle. The CLI selects one root per
invocation; extension can add more roots. Existing single-root APIs remain
available. `SearchQuery.root` filters by exact root name before candidate
selection; an unknown name returns no results. Path filters apply independently
within each selected root.

Root names are nonempty ASCII letters, digits, underscores, or hyphens. Names
must be unique in an invocation. `default` is the CLI default; `documents` is
reserved for typed/anonymous documents. Filesystem locations are canonical
absolute UTF-8 directory paths. A root name in an extension must retain its
location, and a location cannot acquire another name. Nested roots are allowed
and remain distinct sources. Each source is keyed by root ID plus relative path,
not by relative path alone. Root IDs remain stable along an extension lineage;
independently created snapshots do not promise matching IDs.

Relocation/rebinding is not supported. Rebuild after moving a source tree, or
add it under a different root name (which retains the old root and its sources).
Moving a snapshot does not relocate its sources. Anonymous documents have no
filesystem location and are not stale-checkable. CLI stdin cannot be combined
with extension; filesystem extension of a typed-document snapshot retains its
documents.

## Extension

Extension copies the base database into private staging, updates the copy in a
transaction, compacts it, closes it, and publishes a new UUID directory. It never
modifies the base. Matching hashes skip chunking and search-index updates, but
inputs are still read and hashed. Changed sources replace their old chunks; new
sources are added. Sources absent from the inputs, including deleted files, are
retained. Discovery skips retain any old source with that identity. An explicit
input failure aborts the whole extension. Each selected root must supply at least
one indexable document.

Published artifacts are independent copies, not hard links. Deleting a base does
not affect its descendants. Result IDs are snapshot-local; do not carry them
across extension. Rebuilding from scratch is the way to drop selected sources.

## Retention, deletion, and recovery

There is no automatic expiration, quota, or garbage collection of published
snapshots. Retain handles as long as needed and explicitly delete them. Disk use
can temporarily include the base, its full staged copy, SQLite journal, and
compaction workspace. Extension compacts the new copy; deleting snapshots
reclaims their storage after open readers release it.

Deletion is restricted to a direct, canonical UUID child of the selected store's
`indexes` directory. It validates Sift metadata, matching snapshot ID, directory
ownership, and the exact regular-file layout. Symlink handles, foreign stores,
and unexpected files are rejected. It renames the directory out of the handle
namespace before unlinking the database. It never recursively removes arbitrary
trees. A repeated delete fails rather than reporting success.

Readers that already opened the database can finish after deletion on Linux.
Readers racing to open it can receive a filesystem/SQLite error, including
between metadata validation and query opening. Queries never silently switch to
another snapshot. There is no guarantee that every concurrent query succeeds.

Store mutation uses a process-released exclusive file lock. Builds (including
independent parallel extension requests), deletion, and cleanup are serialized
within one store; queries do not take that lock. Parallel extensions receive
distinct handles and use the same unmodified base, not each other's results.
Do not remove `.lifecycle-lock` or modify store artifacts externally while Sift
is running. The store is trusted local storage, not a security boundary against
another process running as the same user.

Normal build failures remove staging automatically. Abrupt process termination
can leave `.staging-*` directories, including retired deletion directories.
`sift cleanup` waits for the store lock, then removes these abandoned directories
and prints their count. Only owned directories containing regular `db.sqlite`
and optional `db.sqlite-journal` files (or empty directories) are eligible;
unexpected entries cause an error, not recursive deletion. Cleanup never expires
published handles. Recovery does not depend on an age threshold or PID reuse.

## Staleness

`check <handle>` emits a JSON array with `root`, `path`, and `status` fields.
Statuses are `unchanged`, `changed`, and `unavailable`. It securely reads each
stored filesystem source with the normal input limits and symlink restrictions,
then compares BLAKE3 hashes. Missing, unreadable, invalid, or oversized files and
anonymous documents are `unavailable`. It does not discover new files. Successful
checks exit zero even when sources have changed; callers inspect the statuses.
Checks are observations, not filesystem-wide atomic snapshots.

Queries always return indexed text. Neither checks nor queries refresh the
snapshot or substitute current filesystem contents for stored excerpts.

## Format, portability, and durability

Formats 1 (metadata-only) and 2 (searchable) are supported. Extension and staleness
checks require format 2. Unknown formats, backends, and preprocessing settings
are rejected. Named roots require no schema change: format 2 already stores root
identity and names separately from file paths.

A closed published directory contains only `db.sqlite`. Copy the complete UUID
directory without renaming its UUID to transfer an artifact. Queries and info
use stored content and do not require source trees; staleness checks and extension
still use the original absolute source locations. Import into another store is a
manual offline copy into its `indexes` directory, with ownership and permissions
set for that store's user. Do not copy a staging database or overwrite a handle.

Publication and managed deletion are supported only on Linux, using no-replace
rename. Other platforms explicitly reject publication rather than using an
unsafe replacement fallback. Cross-platform artifact reading is not a tested
support guarantee. Store filesystems must support SQLite, file locks, and atomic
no-replace directory rename; network filesystems are not supported.

Atomic visibility and process-crash recovery are supported. Power-loss durability
is **not** guaranteed: directory entries are not fsynced. A machine crash may
lose a recently returned handle. Tests cover publication collision, initialization
failure, process exit with an open extension transaction, abandoned cleanup,
independent extensions, and preservation of the original artifact.
