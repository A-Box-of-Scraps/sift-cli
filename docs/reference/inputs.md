[Documentation](../README.md) / Reference

# Input discovery and adapters

```sh
sift index --root /work/project src README.md 'tests/**/*.rs'
sift index --root /work/project . --no-gitignore
sift index --root /work/project . --no-ignore --hidden
printf 'src/main.rs\0README.md\0' | sift index --files0-from -
printf 'example text\n' | sift index --stdin --name example
```

## Files and discovery

Relative inputs resolve against `--root`, which defaults to the current directory.
Absolute inputs must be beneath the canonical root. Parent traversal (`..`) is
rejected. Paths must be UTF-8. Existing paths take precedence over glob syntax,
so filenames containing glob characters can be selected literally.

Directories are recursive. Quoted globs match root-relative paths: `*` and `?`
do not cross `/`, while `**` supports recursive matching. A matched directory
selects its visible descendants. Mixed selections are deduplicated by normalized
root-relative path, not by inode; hard links with different names remain distinct.
Files are ingested in path order. An explicit selection takes precedence over a
discovered selection of the same file.

Discovery respects root-local and nested `.ignore`, `.gitignore`, and
`.git/info/exclude` rules. Git rules apply even without a Git repository.
Parent-directory ignore files and global Git excludes are not used, so selection
is scoped to the root rather than the user's machine configuration.

- `--no-gitignore` disables `.gitignore` and `.git/info/exclude`, not `.ignore`.
- `--no-ignore` disables all of these ignore files.
- Hidden files and directories are excluded independently; `--hidden` includes
  them. Neither ignore flag implies `--hidden`.
- Explicit files bypass ignore and hidden filters. Explicit directories and
  globs remain subject to discovery filters.
- Symlinks are never followed during ingestion, including intermediate path
  components. Explicit symlink inputs fail; discovered symlinks are skipped.
  There is no symlink override. The root is canonicalized at selection time.

## Errors and output

Explicit files must be readable regular files containing NUL-free UTF-8, at most
8 MiB each. Violations fail the entire index operation. Discovered files that
violate these rules, disappear, or change detectably during reading are skipped.
Traversal errors and malformed ignore rules produce stderr diagnostics; discovery
continues where possible. No snapshot is published if all selected files fail.

Missing explicit paths, unmatched globs (after filtering), and selections with no
files fail. An empty text file or empty stdin document is valid and counts as a
document, even though it produces no searchable chunks.

Ingestion compares file size and modification time before and after reading.
This detects ordinary concurrent writes, but is not an atomic filesystem snapshot
or a guarantee against undetectable same-size writes. Changes before opening a
file are allowed; hashes and excerpts describe the bytes actually read.
Linux opens use directory descriptors and no-follow flags to reject symlink
substitutions, including changes to root path components.

Skip diagnostics and the CLI completion count go to stderr. Success writes only
one absolute snapshot handle followed by a newline to stdout. Publication remains
atomic; failed ingestion does not publish a partial snapshot.

## NUL-separated lists

`--files0-from -` reads literal file paths from stdin and can be combined with
positional file/directory/glob inputs. Every entry must end in NUL and must name
an existing regular file; empty entries are errors. Whitespace and newlines are
preserved. These entries use the same root-relative rules as positional files,
but are never expanded as globs or traversed as directories. An empty list is
valid only when positional inputs provide a nonempty selection.

## Anonymous and library documents

`--stdin` indexes one bounded UTF-8 document. It cannot be combined with file
inputs, `--files0-from`, `--root`, or discovery flags. `--name` requires `--stdin`
and defaults to `stdin`. Line numbers start at 1 within the stream.

Anonymous documents use the synthetic root `documents`, with an empty root
location. Their stored identifiers start with `document:`. Name bytes other than
ASCII letters, digits, `-`, `_`, and `.` are percent-encoded, including slashes,
percent signs, and control characters. Thus a display name such as `/tmp/a.rs`
becomes `document:%2Ftmp%2Fa.rs`, not a filesystem path. Query `--path` accepts
this stored identifier. No temporary source file is created.

Library callers can use `SnapshotStore::index` with `IndexRequest`, or
`index_with_options` with `DiscoveryOptions`. `SnapshotStore::index_documents`
accepts a slice of `TextDocument { name, text }` without filesystem discovery.
Names must be nonempty and unique within the slice. Text has the same NUL and
size restrictions as files. The library computes hashes and synthetic identifiers;
callers do not provide provenance or hashes. File and anonymous-document sources
are separate index operations. Discovery skip diagnostics currently go to stderr
for library callers as well; the completion count is CLI-only.
