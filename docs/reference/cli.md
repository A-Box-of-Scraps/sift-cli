[Documentation](../README.md) / Reference

# CLI

Use `sift --help` or `sift <command> --help` for command help. Handles are explicit
snapshot directory paths; there is no active or most-recent snapshot.

## Commands

| Command | Purpose and successful stdout |
| --- | --- |
| `sift index <INPUT>... [OPTIONS]` | Build a snapshot; print its absolute handle and newline. |
| `sift index --files0-from - [INPUT]... [OPTIONS]` | Read literal NUL-terminated file paths from stdin. |
| `sift index --stdin [--name NAME]` | Index one text document; print its handle. |
| `sift query <HANDLE> <QUERY> [OPTIONS]` | Print ranked excerpts or JSON. |
| `sift info <HANDLE>` | Print labeled metadata and root records as text. |
| `sift check <HANDLE>` | Print a JSON array of source statuses. |
| `sift delete <HANDLE>` | Delete a managed snapshot; no stdout. |
| `sift cleanup` | Remove abandoned staging directories; print their count. |

Quote the query as one shell argument. `--help` and `--version` print information
and exit successfully.

## Index options

| Option | Behavior |
| --- | --- |
| `--root PATH` | Filesystem source root; defaults to the current directory. |
| `--root-name NAME` | Stored root name; defaults to `default`. |
| `--extend HANDLE` | Build a new snapshot from an existing snapshot plus selected inputs. |
| `--no-gitignore` | Disable `.gitignore` and `.git/info/exclude` rules only. |
| `--no-ignore` | Disable all supported ignore files; takes precedence over `--no-gitignore`. |
| `--hidden` | Include hidden entries during discovery. |
| `--files0-from -` | Read file paths from stdin; only `-` is supported. |
| `--stdin` | Read one UTF-8 text document from stdin. |
| `--name NAME` | Name the stdin document; requires `--stdin`, defaults to `stdin`. |

`--stdin` conflicts with positional inputs, `--files0-from`, `--root`, explicit
`--root-name`, `--extend`, and discovery flags. See [inputs](inputs.md) for path
resolution, ignore behavior, and validation.

## Query options and limits

| Option | Behavior |
| --- | --- |
| `--limit N` | Maximum results, 1 through 100; default 5. Fewer results are allowed. |
| `--path PATH` | Exact stored file or subtree, using path-component boundaries. |
| `--root NAME` | Exact stored root name; an unknown name returns no results. |
| `--json` | Emit the [versioned JSON response](output.md). |

Queries use ordinary text, not regex or raw FTS syntax. Maximum query length is
4096 UTF-8 bytes. Tokenization must produce 1 through 64 distinct searchable
terms, including identifier variants. Empty or punctuation-only queries fail.

Path filters apply to the stored namespace, not the querying working directory.
Absolute paths, NUL, and `..` components are rejected. Empty components and `.`
are removed; an empty normalized filter means no path restriction. Without a root
filter, the path filter applies across all roots. Anonymous document identifiers
can also be used as path filters.

Documents are limited to 8 MiB of NUL-free UTF-8. Chunks contain at most 32 lines
and 2048 bytes, with up to four lines of overlap. See [search](../explanation/search.md)
for selection behavior.

## Storage

The store is `$XDG_DATA_HOME/sift-cli` when `XDG_DATA_HOME` is absolute. Otherwise
it is `$HOME/.local/share/sift-cli` when `HOME` is absolute. If neither is usable,
store creation fails. Published handles are UUID directories under `indexes/`.

`index`, `delete`, and `cleanup` select the store from the environment. Reading
commands use the supplied handle. Deletion requires a handle in the selected
store. Handles are references, not access credentials.

Publication and managed deletion require Linux. See [snapshot support
limits](snapshots.md#format-portability-and-durability) before transferring artifacts.

## Exit statuses and diagnostics

| Status | Meaning |
| --- | --- |
| `0` | Success, including no query matches and checks reporting changed sources. |
| `1` | Operational failure, such as input read errors, invalid artifacts, or output write failure. |
| `2` | Argument parsing failure or an `InvalidOptions` validation error. |

Not every invalid input is an invocation error: explicit file ingestion failures
are operational errors. Diagnostics and index completion counts go to stderr.
A failed output write can leave partial stdout. In particular, a snapshot may
already be published when writing its handle fails. Broken output pipes are
reported as failures, not silently treated as success.
