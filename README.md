# Sift

> Google Maps for your workspace, in the terminal.

Sift is a CLI for indexing and querying files, designed for LLMs and AI coding agents.
Its goal is to serve a similar discovery role to rg (content search) while returning more relevant results through indexing.

## Installation

Sift currently requires Linux. Install Rust/Cargo, Clang, and LLD, then run these commands from the Sift repository root:

```sh
cargo install --path . --locked
sift --help
```

Ensure Cargo's binary directory (normally `$HOME/.cargo/bin`) is on your `PATH`.

## Usage

From your project directory, index the files you want to search, then query the resulting snapshot:

```sh
handle=$(sift index .)
sift query "$handle" 'token validation'
```

You can also index specific inputs, such as `sift index src README.md`. Sift
respects ignore files and skips hidden entries by default. Indexing prints a
snapshot directory path, called a handle, which you can reuse for multiple queries.

Sift splits files into overlapping chunks and indexes their paths and content
with SQLite FTS5. It matches words and identifier components, including
`snake_case` and `camelCase`, then returns ranked excerpts with file paths and line
numbers. Search is lexical, not embedding-based: use words that appear in the
code or documentation. Unlike `rg`, Sift returns relevant excerpts rather than
every matching line.

Filter results, request JSON for scripts or agents, and inspect the snapshot:

```sh
sift query "$handle" 'token validation' --path src --limit 10
sift query "$handle" 'token validation' --json
sift info "$handle"
```

Snapshots do not update automatically when files change. Run `sift index` again
to create a fresh snapshot, and read the current source before editing. When you
no longer need a snapshot, delete it without affecting your source files:

```sh
sift delete "$handle"
```

## AI agent skill

Check out the [sift-usage skill](skills/sift-usage/SKILL.md)!
Copy `skills/sift-usage` into your agent's skill directory, and your favorite clanker will know how to install and use Sift when exploring!

## Documentation

Start [HERE](docs/README.md)!

## License

Licensed under the [MIT License](LICENSE) by Titouan Réthoré.
