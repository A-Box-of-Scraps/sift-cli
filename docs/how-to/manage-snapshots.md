[Documentation](../README.md) / How-to guides

# Manage snapshots

Use explicit handles for every operation. The examples assume existing `src`
and `docs` directories in your project and an installed `sift` binary.

## Add or refresh selected sources

```sh
code=$(sift index --root-name app src)
project=$(sift index --extend "$code" --root-name app docs)
updated=$(sift index --extend "$project" --root-name app src)
```

Keep the same filesystem root and root name when refreshing a source. Each
command returns a new independent snapshot; earlier handles remain usable.
Changed selected files are replaced, unchanged files are retained, and new files
are added. Files absent from the selection are retained, even if deleted on disk.
To remove old sources, build a fresh snapshot without `--extend`.

## Add another source tree and filter searches

Replace `/work/dependency` with an existing source directory:

```sh
combined=$(sift index --extend "$updated" --root /work/dependency --root-name dependency .)
sift query "$combined" 'request handler' --root dependency --path src
```

Query `--root` is a stored name, not a filesystem location. `--path` selects a
root-relative file or subtree. Omit `--root` to search across all indexed roots.
Root names and locations cannot be rebound during extension.

## Check whether indexed sources changed

```sh
sift check "$updated"
```

Inspect the JSON statuses: `unchanged`, `changed`, or `unavailable`. A successful
check exits zero even if files changed. It neither refreshes the snapshot nor
discovers new files. Read current files before making edits.

## Reclaim storage

```sh
sift delete "$code"
sift delete "$project"
sift delete "$updated"
sift delete "$combined"
sift cleanup
```

Delete only handles you no longer need. Deleting a base does not affect its
extensions. Cleanup removes abandoned staging directories after interrupted
operations, not published snapshots. There is no automatic expiration.

See [snapshot reference](../reference/snapshots.md) for concurrency, recovery,
portability, and durability limits; see [CLI storage](../reference/cli.md#storage)
if a handle belongs to another store.
