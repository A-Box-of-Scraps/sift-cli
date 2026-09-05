---
name: sift-usage
description: Guide to exploring a workspace with Sift, a CLI that indexes files and returns ranked excerpts. Use when discovering relevant code or documentation.
---

# Sift

Sift indexes text into immutable local snapshots with `sift index` and returns ranked excerpts with `sift query`.
Indexing returns a snapshot handle that you can reuse for related queries.
Search is lexical, not semantic. Choose words and identifiers likely to appear in the source.
An empty result set does not prove that the code is absent.

## Quick start

> Use Sift for discovery and `rg` for exhaustive matching.

From your project directory, index the workspace and query the returned snapshot:

```sh
handle=$(sift index .)
sift query "$handle" 'token validation' --path src
sift query "$handle" 'validateToken' --limit 10 --json
```

Read the current source before editing. Excerpts contain indexed text, which may no longer match the files on disk.

## Input selection

Index one or more files, directories, or globs. Quote globs so Sift, rather than your shell, expands them:

```sh
handle=$(sift index src README.md 'tests/**/*.rs')
```

Relative inputs resolve against `sift index --root PATH`, which defaults to the current directory.
Absolute inputs must be within that root. Use `--root-name NAME` to assign a name for query filtering:

```sh
handle=$(sift index --root /work/myproject --root-name myproject src)
sift query "$handle" 'token validation' --root myproject --path src
```

Discovery skips hidden and ignored entries by default:

- `--hidden` includes hidden entries.
- `--no-gitignore` disables `.gitignore` and `.git/info/exclude` rules, but still respects `.ignore`.
- `--no-ignore` disables all supported ignore files.

Neither ignore option includes hidden entries automatically.
Explicit files bypass hidden and ignore filters; directories and globs do not.

### File lists and text streams

To preserve file paths and line numbers, pass NUL-terminated file paths to `--files0-from -`.
Run both commands from the project root:

```sh
handle=$(rg -l -0 'TODO' | sift index --files0-from -)
```

Use `--stdin` for a single text document:

```sh
handle=$(printf 'Token validation rejects empty tokens.\n' |
  sift index --stdin --name notes)
```

This creates one anonymous document. Concatenating files into stdin loses their individual paths, and line numbers refer to the combined stream.
Do not combine `--stdin` with file inputs or extension.

For generated sources, index the output directory after generation succeeds:

```sh
cfr app.jar --outputdir /tmp/decompiled &&
handle=$(sift index --root /tmp/decompiled .)
```

Each indexing example creates a separate snapshot. Keep its handle if you want to query or delete it later.

## Query options

Pass each query as a single quoted argument containing ordinary text, not regex or raw FTS syntax.
Words and `snake_case`/`camelCase` components match; not every query term must occur.

- `--limit N` sets the maximum number of results, from 1 to 100. The default is 5.
- `--path PATH` filters by an exact stored file path or subtree, relative to the indexed root.
- `--root NAME` filters by an exact stored root name, not a filesystem directory. 
  The default filesystem root name is `default`; an unknown name returns no results.
- `--json` returns an object containing `schema_version`, `handle`, and
  `results`. Results include `root_name`, `path`, `start_line`, `end_line`, and
  `snippet`. Line numbers are one-based and inclusive.

Use `sift -h` and `sift <command> -h` for the remaining options.

## Snapshot lifecycle

> Snapshots never update automatically.

```sh
sift check "$handle"
sift info "$handle"
```

- `check` emits a JSON array with a status for each indexed source: `unchanged`,
  `changed`, or `unavailable`. Exit code zero does not mean all sources are
  unchanged. It does not discover new files or refresh the snapshot.
- `info` displays the snapshot ID, creation time, file and chunk counts, and root records.

To update selected files or add new ones, extend the snapshot.
Run this example from the same project root as the quick start:

```sh
next_handle=$(sift index --extend "$handle" .)
```

Extension creates an independent snapshot and returns a new handle; it does not modify the original.
Selected changed sources are updated, and new sources are added. Unselected sources, including deleted files, are retained.
To drop removed sources, create a fresh snapshot without `--extend`.

Delete snapshots when they are no longer needed.
This does not delete source files or affect snapshots created by extension:

```sh
sift delete "$handle"
```

## Installation

Use this section only if `sift` is not available. Ask the user for permission before installing it.

### Build from source

Requirements: Linux, Git, Rust/Cargo, Clang, and LLD.

```sh
tools_dir=$(mktemp -d) &&
git clone https://github.com/A-Box-of-Scraps/sift-cli.git "$tools_dir/sift-cli" &&
(cd "$tools_dir/sift-cli" && cargo install --path . --locked)
```

Ensure Cargo's binary directory, normally `$HOME/.cargo/bin`, is on `PATH`.

### Download a prebuilt binary

This option targets Linux x86-64 and requires GitHub CLI (`gh`) and `tar`.
Download and extract in a temporary directory to avoid overwriting workspace files:

```sh
tools_dir=$(mktemp -d) &&
gh release download --repo A-Box-of-Scraps/sift-cli \
  --pattern 'sift-*-x86_64-unknown-linux-gnu.tar.gz' \
  --dir "$tools_dir" &&
tar xf "$tools_dir"/sift-*-x86_64-unknown-linux-gnu.tar.gz -C "$tools_dir" &&
mkdir -p "$HOME/.local/bin" &&
install -m 755 "$tools_dir/sift" "$HOME/.local/bin/sift"
```

Ensure `$HOME/.local/bin` is on `PATH`. After either installation method, verify with `sift --help`.
