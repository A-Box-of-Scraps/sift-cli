[Documentation](../README.md) / How-to guides

# Select inputs

These commands assume `sift` is installed and your shell is at your project root.
Each index command creates a separate snapshot; capture its handle if you want to
query or delete it later.

## Index directories or selected files

```sh
handle=$(sift index src README.md 'tests/**/*.rs')
```

Quote globs so Sift, rather than your shell, expands them. To run from elsewhere,
pass `--root /absolute/path/to/project`; relative selections resolve there.

## Include ignored or hidden content

```sh
handle=$(sift index . --no-gitignore)
handle=$(sift index . --no-ignore --hidden)
```

`--no-gitignore` still respects `.ignore`. `--no-ignore` disables all supported
ignore files. Hidden entries require `--hidden` independently. Explicit files
bypass ignore and hidden filters; directories and globs do not. Symlinks are
never followed.

## Preserve file identity in a pipeline

```sh
handle=$(rg --files -0 src | sift index --files0-from -)
```

Run both commands from the same project root. Entries must be NUL-terminated
literal file paths, not directories or globs.

## Index a text stream

```sh
handle=$(printf 'Token validation rejects empty tokens.
' | sift index --stdin --name notes)
sift query "$handle" 'token validation' --json
```

This creates one anonymous document, not a collection of source files. Its line
numbers are stream-relative, and its display name is not a filesystem path.
Do not combine `--stdin` with file inputs or extension.

## Diagnose missing inputs

Inspect stderr for skipped files. Discovered non-text, oversized, or unreadable
files are skipped; explicitly selecting such a file fails the whole operation.
An unmatched glob or empty selection also fails. Empty UTF-8 files are valid but
produce no searchable chunks.

See [input reference](../reference/inputs.md) for exact rules and
[manage snapshots](manage-snapshots.md) to retain or remove the resulting handles.
