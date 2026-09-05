[Documentation](../README.md) / Reference

# Query output

## Compact text

Each result has a header followed by its stored excerpt:

```text
default:src/auth.rs:42-58
<source excerpt>
```

Results are separated by a blank line. No matches prints `No results.` and a
newline. Root names and paths use Rust default character escaping. Snippets
preserve newlines and tabs but escape other characters as needed for terminal
safety. Text output is for reading; use JSON rather than parsing its headers.

Index handles are emitted as raw path bytes when piped, and escaped when stdout
is a terminal. Diagnostics and metadata text also escape untrusted characters.

## JSON envelope

`query --json` emits one JSON object followed by a newline:

```json
{
  "schema_version": 1,
  "handle": "/example/store/indexes/00000000-0000-4000-8000-000000000000",
  "results": []
}
```

The handle above is illustrative. No matches is an empty `results` array, not an
error. Results are ordered by retrieval selection; there is no confidence score.

| Result field | Type | Meaning |
| --- | --- | --- |
| `id` | string | Snapshot-local result identifier; do not reuse across extension. |
| `root_id` | string | Stored source root identity. |
| `root_name` | string | Human-readable stored root name. |
| `path` | string | Root-relative source path or synthetic `document:` identifier. |
| `start_line`, `end_line` | integer | One-based inclusive line range in indexed text. |
| `start_byte`, `end_byte` | integer | Zero-based UTF-8 byte range, start inclusive and end exclusive. |
| `snippet` | string | Exact stored text for that byte range after JSON decoding. |
| `truncated` | boolean | Currently `false`: the complete selected chunk is returned. |

A chunk is not necessarily a complete file or line. Long lines can span multiple
chunks with the same line number. `truncated: false` does not mean the entire
source file is present. JSON escaping preserves the source on decoding; it does
not make decoded text safe to print directly to a terminal.

The response schema version is separate from the snapshot format version.
Consumers should check `schema_version`; no cross-version compatibility guarantee
is documented. Root IDs persist through an extension lineage, but result IDs
remain local to each snapshot.

## Staleness output

`check` emits a JSON array, without the query envelope:

```json
[{"root":"default","path":"src/auth.rs","status":"changed"}]
```

Statuses are `unchanged`, `changed`, and `unavailable`. See
[staleness semantics](snapshots.md#staleness). `info` has text output only.
