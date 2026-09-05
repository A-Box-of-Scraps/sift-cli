[Documentation](../README.md) / Explanation

# How Sift searches

Sift is a discovery tool: find useful locations, then verify them with direct file
reads or precise searches such as `rg`. It returns a small set of ranked excerpts,
not every matching line.

## Lexical retrieval, not semantic understanding

The current backend is SQLite FTS5. Sift lowercases searchable tokens, retains
whole identifiers, and also splits snake_case and camelCase components. Paths
and bodies are searchable. Query terms are OR-connected, so a result can match
only part of a query. Natural-language questions are accepted, but Sift does not
use embeddings or guarantee understanding when vocabulary differs from the text.

Queries are not interpreted as regex or backend syntax. An absent whole identifier
can still retrieve code through its components. Use an exact text search to
verify that the whole identifier or phrase occurs.

## Bounded chunks and diverse excerpts

The generic chunker preserves source bytes and offsets. Windows are bounded by
32 lines and 2048 bytes, with up to four overlapping lines. This works without a
language parser, but can cut across functions or split unusually long lines.

FTS5 ranks candidates with BM25 and path/body weights of 3:1. Sift materializes
at most ten times the requested result limit. It prefers distinct excerpts and
no more than two hits per root/file, then fills remaining slots from deferred
candidates. These are soft preferences, not hard quotas. Overlapping byte ranges
from the same source are always excluded.

A bounded candidate pool can miss useful alternatives beyond its cutoff or return
fewer results than requested. SQL LIMIT bounds materialization, not all FTS
matching and scoring work. Scores are not probabilities and are not exposed as
confidence values.

## Why these defaults

The [synthetic evaluation](evaluation.md) supports bounded candidate selection
and soft diversity on its fixtures. It does not establish general relevance on
real repositories. Other chunk sizes, weights, and query rerankers did not supply
sufficient evidence to replace the remaining defaults.

See [CLI reference](../reference/cli.md) for query limits and filters, and
[output reference](../reference/output.md) for source ranges.
