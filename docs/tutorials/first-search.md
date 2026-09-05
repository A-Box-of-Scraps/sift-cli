[Documentation](../README.md) / Tutorials

# Your first search

This walkthrough creates a small source file, searches its indexed content, and
removes the snapshot. Use Linux with Rust/Cargo, Clang, and LLD installed. Run the
installation command from the Sift repository root.

## 1. Install Sift

```sh
cargo install --path . --locked
sift --help
```

Ensure Cargo's binary directory (normally `$HOME/.cargo/bin`) is on your `PATH`.
Snapshot publication is currently supported only on Linux.

## 2. Create and index a file

In a POSIX-compatible shell:

```sh
workspace=$(mktemp -d)
printf 'fn validate_token(token: &str) -> bool {
    !token.is_empty()
}
' > "$workspace/auth.rs"
handle=$(sift index --root "$workspace" auth.rs)
```

Sift prints a document count to stderr. The variable captures only the absolute
snapshot directory path. Keep that handle for later commands.

## 3. Search the snapshot

```sh
sift query "$handle" 'validate token'
```

The output includes `default:auth.rs:1-3` and the stored function excerpt.
`default` is the source root name. The numbers are indexed line numbers.

```sh
sift query "$handle" 'validate token' --json
sift info "$handle"
```

JSON is intended for scripts. `info` shows snapshot metadata, including the source
root. You can query this handle repeatedly without reindexing.

## 4. Remove the snapshot

```sh
sift delete "$handle"
rm "$workspace/auth.rs"
rmdir "$workspace"
```

Deleting the snapshot does not delete source files. The last two commands remove
the tutorial's source file and temporary directory.

## Next steps

[Select project inputs](../how-to/select-inputs.md),
[extend a snapshot](../how-to/manage-snapshots.md), or read
[how search works](../explanation/search.md). Always read current source before
editing: results describe the indexed revision, not live files.
