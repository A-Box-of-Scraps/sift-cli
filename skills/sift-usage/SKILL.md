---
name: sift-usage
description: Manual for using Sift, a CLI that allows easy exploriation by indexing and querying files. Use when discovering the workspace.
---

# Sift usage

## When to use

- Use Sift to find likely files and excerpts before reading large amounts of code.
- Index once and reuse the handle for related questions. Index a newly discovered
  dependency or text corpus separately when useful.
- Use `rg` for exact strings, regex, or exhaustive matches, and direct file reads
  when you already know the location. Sift complements these tools.
- Search is lexical, not semantic: choose likely source words and identifiers,
  not long conversational questions. No results does not prove code is absent.

## Install

Check for an existing binary first:

```sh
command -v sift
sift --help
```

If missing, Sift currently requires Linux, Rust/Cargo, Clang, and LLD. Use the
environment's package-management process for missing prerequisites; ask before
making privileged system changes. Do not assume the crates.io package named
`sift` is this project.

From an existing Sift repository checkout:

```sh
cargo install --path . --locked
```

Otherwise clone into a separate tools directory, not the project being explored:

```sh
tools_dir=$(mktemp -d)
git clone https://github.com/A-Box-of-Scraps/sift-cli.git "$tools_dir/sift-cli"
(cd "$tools_dir/sift-cli" && cargo install --path . --locked)
```

Ensure Cargo's binary directory is on `PATH`, then verify the CLI:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
sift --help
```

## Usage

Run from the project root. Select relevant existing directories/files, or use
`.` for the whole project:

```sh
handle=$(sift index .)
sift query "$handle" 'token validation'
sift query "$handle" 'validateToken' --path src --limit 10 --json
```

Indexing prints an absolute snapshot directory path to stdout; diagnostics go to
stderr. Keep this handle and the source root in your working notes if tool calls
do not share shell variables. There is no implicit active snapshot.

- Queries are one quoted argument of ordinary text, not regex or FTS syntax.
  Words and snake_case/camelCase components match; not every query term must occur.
- Start with the default five results. Refine words or `--path` before increasing
  `--limit` (maximum 100). Path filters are exact stored files or subtrees, not globs.
- Use `--json` for structured output: the object contains `schema_version`,
  `handle`, and `results`. Results include `root_name`, `path`, `start_line`,
  `end_line`, and `snippet`. Line numbers are one-based and inclusive.
- Read the current file around a hit before editing. Excerpts are stored chunks,
  not necessarily complete functions or current source. Verify exact identifiers
  with `rg` when needed.
- If results are poor, try alternative source vocabulary, remove filters, and
  check which inputs were indexed. Ignore files and hidden-entry filtering apply
  by default. Enable `--hidden` or ignore overrides only for intended inputs.

Use `sift --help` and `sift <command> --help` for the remaining options.

## Examples

### Select files with rg

Preserve source paths and line numbers using NUL-separated file paths. Run both
commands from the same project root:

```sh
handle=$(rg --files -0 src | sift index --files0-from -)
sift query "$handle" 'request retry'
```

### Search a Java dependency decompiled with CFR

With Java and a CFR JAR already available, replace the example JAR paths below.
Decompile into a separate directory, then index the generated text, not the binary
JAR:

```sh
decompiled=$(mktemp -d)
java -jar /path/to/cfr.jar /path/to/dependency.jar --outputdir "$decompiled"
dependency=$(sift index --root "$decompiled" --root-name dependency .)
sift query "$dependency" 'connection timeout' --root dependency --json
```

Resolve result paths under `$decompiled`. Treat the files as reconstructed source,
not the original source or an editable dependency checkout. Keep the generated
directory while you need follow-up reads or freshness checks.

### Search captured text

```sh
notes=$(printf 'Token validation rejects empty tokens.\n' | sift index --stdin --name notes)
sift query "$notes" 'token validation' --json
```

`--stdin` creates one anonymous document. Its line numbers refer to the stream;
its display name is not a filesystem path. Prefer `--files0-from -` for file lists.

## Snapshot lifecycle

```sh
sift info "$handle"
sift check "$handle"
```

Snapshots never update automatically. `check` emits JSON statuses (`unchanged`,
`changed`, `unavailable`); exit code zero does not mean all files are unchanged.
It does not discover new files. Reindex when changes make the snapshot less useful,
not before every query:

```sh
fresh=$(sift index .)
```

`sift index --extend "$handle" ...` creates a new snapshot with added or refreshed
selected inputs. Omitted files remain, even if deleted on disk. Build a fresh
snapshot without `--extend` when those files must disappear.

Delete only snapshots you created and no longer need:

```sh
sift delete "$handle"
```

Deletion does not change source files. Snapshots have no automatic expiration;
`sift cleanup` removes abandoned staging directories, not published snapshots.
