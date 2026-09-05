---
name: sift-usage
description: Manual for using Sift, a CLI that enables easy workspace exploration by indexing and querying files. Use when discovering the workspace.
---

# Sift

Sift indexes (`sift index`) text into immutable local snapshots and returns (`sift query`) ranked excerpts.
Indexation return a handle of the snapshot, wich you can reuse for related queries.
Search is lexical, not semantic, choose likely source words and identifiers.
No results does not prove code is absent.

## Usage

> Sift for discovery, `rg` for enumeration.

A common workflow is to index one or multiple folders and then use the returned handle to query the snapshot. For example:

```sh
# Indexing: sift index <file/folder/glob> [OPTIONS]
handle=$(sift index foo/bar toto/titi src/**/*.ts)

# Query the snapshot: sift query <handle> <query> [OPTIONS]
sift query "$handle" 'token validation' --root 'myproject' --path 'src' 
sift query "$handle" 'validateToken' --limit 10 --json
```

Remember, queries are one quoted argument of ordinary text.
Words and snake_case/camelCase components match; not every query term must occur.

`sift index` also supports indexing from stdin, for example:

```sh
cat **/*.ts | sift index --stdin
# Or
rg -l -0 'TODO' | sift index --files0-from=-
# More complex
cfr app.jar --outputdir /tmp/decompiled; fd -0 -t f . /tmp/decompiled | sift index --files0-from=-
```

- Sift queries default to five results, but you can change this with the `--limit` option (max 100).
- You can filter query results by path with the `--path` option, and by source root with the `--root` option so relative inputs resolve against it.
- Use `--json` for structured output: the object contains `schema_version`, `handle`, and `results`.
  Results include `root_name`, `path`, `start_line`, `end_line`, and `snippet`. Line numbers are one-based and inclusive.
- When indexing, you can enable `--hidden` to include hidden entries, and `--no-ignore` or `--no-gitignore` to include ignored entries.

Use `sift -h` and `sift <command> -h` to get help on the remaining options.

## Snapshot Lifecycle

> Snapshots never update automatically.

```sh
sift check "$handle"
sift info "$handle"
sift index --extend "$handle" ...
sift delete "$handle"
```

- `check`: emits JSON statuses (`unchanged`, `changed`, `unavailable`) on every files; exit code zero does not mean all files are unchanged.
- `info`: displays the snapshot's id, date of creation, files, chunks, and root.
- `index --extend <handle>`: adds new files to the snapshot, returning a new handle.
- `delete`: delete the snapshot, use it when no longer needed.

## Installation

> Fall back to this section if sift is not found when you use it.

Requirements: Linux, Rust/Cargo, Clang, and LLD

For installation you have two options:

1. Build from source:

```sh
tools_dir=$(mktemp -d)
git clone https://github.com/A-Box-of-Scraps/sift-cli.git "$tools_dir/sift-cli"
(cd "$tools_dir/sift-cli" && cargo install --path . --locked)
```
2. Dowload the prebuild binary from the releases page:

```sh
gh release download --repo A-Box-of-Scraps/sift-cli \
  --pattern 'sift-*-x86_64-unknown-linux-gnu.tar.gz'&&
tar xf sift-*.tar.gz &&
mkdir -p ~/.local/bin &&
mv sift ~/.local/bin/sift
```

Before installing, discuss it with the user before doing anything.