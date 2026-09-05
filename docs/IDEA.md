# sift

Status: initial idea and discussion notes with explicit decisions recorded below.
CLI syntax and implementation details remain proposals unless stated otherwise.

## Idea

A Rust CLI for indexing and querying files, designed for LLMs and AI coding
agents. It should fill a similar discovery role to rg, but return ranked,
relevant code excerpts rather than an exhaustive list of matching lines.

Priorities:

- Fast local indexing: target 100,000 lines in a few seconds.
- Compact output by default, with structured JSON available.
- Accept files, directories, glob patterns, and piped input.
- Explicit index handles, with no required daemon or hidden active index.
- Low setup cost. Avoid mandatory models, remote services, or GPUs.

The speed target is a goal, not a measured claim. Define reference hardware,
input bytes, file counts, languages, and cold/warm cache conditions before
treating it as an acceptance criterion.

## Initial CLI sketch

```sh
sift index src/
sift index src/main.rs
sift index 'src/**/*.ts'

handle=$(sift index src/)
sift query "$handle" "token validation"
sift query "$handle" "validateToken" --json
sift query "$handle" "foo" --path src/foo/bar
sift info "$handle"
sift delete "$handle"
```

Decided: indexing writes only the handle to stdout; progress and diagnostics
go to stderr. Query takes a required positional handle and a query string.
Recommended order: `sift query <handle> <query>`, consistent with info and delete.
Commands documented here are not implemented yet.

### Input selection and query interface

- Support files, directories, and quoted glob patterns as indexing inputs.
  Allow multiple inputs too: `sift index src/ docs/ 'examples/**/*.rs'`.
  Globs complement multiple inputs rather than replacing them.
- Compact text is the default; `--json` provides structured output suitable for jq.
- Decided: the query contract is "search-engine-style text": one free-text
  interface accepting descriptive keywords, identifiers, and natural-language
  questions, without separate query modes. Optimize the initial lexical retrieval
  implementation for short descriptive queries; accepting sentences does not
  promise semantic understanding. Do not interpret input as regex or raw FTS syntax.
  Keep path/root filters explicit. Future semantic or hybrid retrieval should
  preserve this interface. Benchmark keyword and natural-language versions of
  the same exploration questions.
- Expose `--limit`; propose a default of 5 results, subject to retrieval tests.
  It is a maximum, not a requirement to fill all slots. Do not expose a score
  threshold or output-byte-budget option initially. Keep excerpts bounded by
  default so a small result count also produces compact output.
- Include `info <handle>` for snapshot metadata and `delete <handle>` for removal.

### Proposed subtree filtering

Use an explicit metadata filter, separate from query keywords:

```sh
handle=$(sift index src/)
sift query "$handle" "foo" --path src/foo/bar
```

`--path` selects an exact indexed file or a directory and its descendants,
using path-component boundaries, not a textual prefix. Apply it during candidate
selection before ranking and limiting results; do not require reindexing.

Preserve a stable path namespace from indexing so an input of `src/` can produce
paths such as `src/foo/bar/file.ts`. Interpret filters against the stored namespace,
not the querying process's current directory. The exact root/corpus naming rules
for absolute paths and extended indexes still need specification.

Do not interpret `foo in src/foo/bar` as a guaranteed filter: it remains keyword
text. Explicit flags avoid ambiguity and accidental interpretation of source text.

Anonymous stdin documents have no filesystem path and do not match path filters.
A display name supplied with `--name` is not automatically a filesystem path.
Structured input with explicit source paths could support filtering later.

### Proposed exit status contract

- 0: operation completed successfully, including a query with no results.
- 1: operational failure, such as an unavailable handle, read failure, or corrupt index.
- 2: invalid invocation, such as missing arguments or an invalid option value.

This is sift's proposed convention, not a claim that POSIX mandates these exact
meanings. Empty query results should be represented clearly in both text and JSON.
Errors go to stderr, without emitting a success-shaped payload on stdout.

### Piped input

The original idea included `cat src/**/**.ts | sift index`. Concatenated content
does not preserve file boundaries or paths. Support that as one anonymous
document, with stream-relative line numbers, not as identifiable source files.

For file-aware indexing, propose a separate path-list mode:

```sh
rg --files -0 src/ | sift index --files0-from -
cat notes.txt | sift index --stdin --name notes.txt
```

Quoted globs would be expanded by sift; unquoted globs may be expanded by the
shell. Define directory recursion and ignore behavior consistently. Proposed
defaults: respect ignore files, skip binaries, avoid following symlinks, and
report skipped oversized files. Offer explicit overrides.

## Decision: immutable indexes and stateless commands

Decided: sift is immutable and stateless at the command/session level. Index
artifacts persist on disk; stateless does not mean there is no stored data.

- Indexing creates a local, immutable index artifact.
- The returned handle identifies that artifact, not a running process.
- Queries open the named artifact and exit.
- Commands use explicit handles, with no hidden active or most-recent index.
- No daemon or persistent session is required.
- Publish an artifact only after indexing completes successfully.
- A published handle always identifies the same indexed snapshot.
- Reindexing or extending a snapshot returns a new handle, never mutates the old one.

### Decision: extension creates a new snapshot

Support adding data to an existing snapshot by creating a new snapshot. Proposed
CLI syntax:

```sh
code=$(sift index src/)
project=$(sift index docs/ --extend "$code")

sift query "$project" "authentication"
```

Here, `project` contains code and docs; `code` remains unchanged and queryable.
Failed or interrupted extension must leave the original snapshot usable.
Parallel callers can extend the same handle independently.

Extension semantics:

- Same source identity and unchanged content: skip.
- Same source identity and changed content: replace its chunks in the new snapshot.
- Different source identity: add.
- Sources absent from the new input: retain. Extension is not synchronization.
- Source identity includes corpus/root identity, not only a relative path, so
  unrelated corpora with matching relative paths do not overwrite each other.

The exact source-identity scheme remains to be specified. Internal storage may
reuse unchanged data; immutable handles do not mandate full copies or complete
reprocessing. Storage reuse is an implementation choice, not a v1 requirement.
Any shared storage must remain available while a retained snapshot references it.

Implement basic indexing first, then extension. Mutable in-place append is not
part of the design. Multi-handle querying remains a separate possible feature.

A path is the simplest initial handle. An opaque ID resolved through a cache
directory is another option. Handles are references, not access credentials.
Cache location, deletion, retention, and portability need explicit contracts.

Proposed snapshot semantics: retain indexed chunk text so query snippets and
line ranges describe the same source revision. Record content hashes to allow
staleness checks. Do not silently present old line numbers as current file
locations. Reindexing should produce a new handle; incremental reuse can follow.

## What to index

## Decision: modular library and replaceable backends

This is an initial design intended for rapid iteration, especially during
benchmarking. Keep a reusable Rust library separate from the CLI. Expose typed
primitives that other callers can compose without invoking shell commands.

Suggested boundaries:

- Input adapters: files, directories, path lists, and text streams.
- Documents and provenance: source identity, corpus, logical path, original text.
- Chunking and enrichment: generic text chunks and optional language-aware data.
- Retrieval backend: build/open snapshots and retrieve ranked candidates with filters.
- Result processing: ranking adjustments, overlap removal, and snippet selection.
- Snapshot management: handles, publication, extension, and deletion.
- Presentation: compact text and JSON, kept out of the retrieval core.

SQLite FTS5 and Tantivy should be alternative backend implementations behind a
small shared contract. Public document, query, filter, and result types must not
expose SQL rows, FTS query strings, or backend-specific document addresses.
Translate typed queries and filters inside each backend adapter. Keep backend
configuration explicit; avoid an abstraction that prevents useful experiments.

Suggested library operations include index, extend, query, info, and delete,
with independently testable ingestion, chunking, and result-processing stages.
These are conceptual primitives, not finalized Rust signatures.

Record backend name, format version, and preprocessing configuration in snapshot
metadata. Replacing a backend does not imply existing index files are compatible:
initially rebuild a corpus to compare backends. Do not promise cross-backend
extension or portable scores. Apply equivalent filters and output budgets in
benchmarks, and measure cold indexing and query startup as well as retrieval.

## Proposal: root-qualified source paths

Separate source identity from its displayed filesystem path. Identify a file by
`(root_id, root_relative_path)`; give each source root a readable name and retain
its original filesystem location as provenance. Display a qualified path such as
`app:src/auth.ts` or `dependency:src/auth.ts` when disambiguation is needed.
Root names are unique within a snapshot; IDs provide stable identity across extension.
Use "root" in the public API and CLI rather than the retrieval term "corpus".

For project-local inputs, propose a common indexing root, defaulting to the
indexing working directory. Thus indexing `src/` retains `src/...` rather than
stripping it, and extending with `docs/` from the same root adds to that source root.
An explicit root override should support other calling locations.

For external material such as decompiled dependencies, register a separate source
root with an explicit name and filesystem location. On extension, require explicit
mapping when a root or name conflicts; do not silently combine unrelated sources or infer
identity from basename alone. Root relocation/rebinding remains future work.

Proposed query behavior: `--path src/auth` searches matching logical paths in all
indexed roots; an optional `--root app` restricts it to one named root. This flag
selects stored root metadata, not a new filesystem location to scan. Returned
results always include root identity. Structured JSON keeps root and path separate.
`sift info <handle>` should list root names and original filesystem locations.
The CLI flags and root-detection details in this section are proposals, not decisions.

## Indexed data

Use a code chunk as the retrieval unit, with a file record as its parent.

Suggested data:

- File path relative to the indexed root, language, and content hash.
- Chunk ID, parent file ID, start/end lines, and byte offsets.
- Original chunk text for faithful snippets.
- Searchable fields: path, identifiers, and body text including comments.
- Later: enclosing symbol, symbol kind, signature, and documentation.
- Index schema version and tokenizer/chunking configuration.

Proposed first chunker: bounded line windows with modest overlap, preserving
exact source offsets and capping bytes for pathological long lines. Benchmark
different sizes instead of assuming one optimal window. Later, test
syntax-aware function/class chunks with a maximum size and a fallback for
unsupported or invalid source files.

## Retrieval options

### First candidate: lexical retrieval

Start with an inverted index and BM25 ranking, plus code-aware tokenization:

- Keep exact identifiers and also split camelCase and snake_case.
- Normalize searchable variants without altering stored source.
- Tokenize path components and retain useful short code identifiers.
- Boost exact symbol, phrase, and path matches over loose body matches.
- Combine field evidence; do not present a raw score as a confidence value.

Tantivy is a Rust full-text search library with BM25, making it a candidate
backend rather than something we need to recreate immediately. [1][2]

Optional substring retrieval can be evaluated separately. SQLite FTS5's
trigram tokenizer is an example of substring indexing, not a semantic-search
solution. [3] Avoid adding a second index until query tests justify it.

### Semantic retrieval, optionally later

Consider embeddings if lexical retrieval misses conceptual queries whose
wording differs from the code. Proposed hybrid design: independently retrieve
lexical and vector candidates, then combine their ranks, for example with
reciprocal rank fusion. Treat this as an experiment, not a dependency for v1.

Measure model loading, embedding generation, storage, and query embedding
latency separately. Do not assume the few-second indexing target holds with
embeddings enabled. If added, cache vectors by content hash and model version.
Choose brute-force or approximate vector search based on measured chunk counts
and latency, not lines of code alone.

Tree-sitter provides syntax trees and incremental parsing. [4] It is a candidate
for later structural chunking and symbol extraction, not a relevance scorer.

## Agent-oriented output

Proposed compact text shape:

```text
src/auth.ts:42-58
<short source excerpt>
```

JSON should use a documented schema with index handle, result IDs, paths,
line ranges, snippets, and an explicit truncation indicator. Include only
fields that help the caller act; make scores and ranking diagnostics optional.

Prefer a small result limit and bounded excerpts. Expose only `--limit` initially;
defer configurable byte or token budgets until a concrete need emerges.
Deduplicate overlapping chunks and avoid returning many near-identical hits
from one file. A later expansion command could retrieve more context by ID.

## Evaluation before committing to complexity

- Measure end-to-end time from process launch to a queryable artifact,
  including traversal, reading, chunking, indexing, and final commit.
- Report input bytes, files, lines, chunk count, peak memory, and index size.
- Measure query startup and p50/p95 latency as separate concerns.
- Test identifier, path, error-message, and natural-language intent queries.
- Measure whether the desired location appears in the top 5 or 10 results,
  and the output size needed to expose it.
- Compare with rg for exact discovery and with an unboosted lexical baseline.
- Test edits, deletions, empty input, large lines, ignored files, and Unicode.

## Tentative direction and open questions

Suggested starting point: Rust, explicit immutable handles, lexical retrieval,
bounded chunks, compact text plus JSON, and no mandatory embedding model.

Questions to discuss:

1. Are exact code lookups or natural-language intent queries the main use case?
2. Is the common lifecycle index-once/query-many, or reindex after each edit?
3. Should indexes be disposable snapshots or long-lived reusable artifacts?
4. Which languages and repository sizes should define the first benchmarks?
5. Should JSON or compact text be the default agent interface?

## Research references

## Discussion update: exploration first

- Primary use: help agents discover relevant information through natural-language
  questions or search-engine-style keywords, rather than broad matching output.
- Codebases are the main target, including mixed-language repositories.
- Other textual data should work without requiring language-specific parsers.
  Non-text formats would need explicit extraction support, outside the initial scope.
- Use a generic text retrieval core, with optional code-specific enrichment.
- SQLite is a candidate, not a settled choice. Evaluate SQLite FTS5 against
  Tantivy before choosing the backend.

### SQLite versus Tantivy

SQLite FTS5 provides full-text indexing, BM25 ranking, column weights, and
custom tokenizer support. Regular tables could store file/chunk metadata and
original text, with FTS5 indexing searchable fields. External-content tables
can avoid duplicating source text, but require keeping content and index in
sync. [3]

Tantivy uses its own search-index format, not SQLite underneath. Its index is
organized into immutable segments with separate component files and metadata;
its filesystem directory implementation reads files through memory mapping. [1]

Tentative preference: prototype SQLite FTS5 first for a unified local artifact
and metadata model. This is a simplicity preference, not a throughput claim.
Do not introduce SQLite plus Tantivy together unless a measured need emerges.

### Search behavior proposal

Accept ordinary text without requiring users to write backend query syntax.
Build a safe internal query representation rather than passing raw input to
FTS5 MATCH. Retrieve candidates using useful terms, then favor query coverage,
exact identifiers, phrases, and path evidence. Avoid requiring every word in
a conversational question to occur in a chunk.

Return diverse, non-overlapping excerpts within a small output budget.
Evaluate lexical search specifically on conceptual questions: accepting a
natural-language input does not imply semantic understanding. If vocabulary
mismatch is a frequent failure, test optional hybrid retrieval against the
same relevance and latency benchmarks.

### Clarified lifecycle: index as you go

The intended workflow is index once, query many times, then use targeted tools
such as rg and direct file reads after discovering the relevant locations.
Indexing is an explicit exploration action, not a step after every request or
edit. Reindex when substantial changes make the snapshot less useful, or index
newly discovered information as a separate corpus.

Example: an agent exploring Java code could decompile a dependency JAR with
CFR, then index the extracted source as another searchable snapshot.

Proposed implications:

- No watcher, daemon, or automatic per-edit reindexing in the initial scope.
- Reuse handles across many queries; keep independent corpora addressable.
- Prefer fast initial indexing over complex incremental-update machinery.
- Preserve snapshot provenance and make clear that results describe indexed
  content. Read current source before editing; indexed line ranges may be stale.
- Return useful excerpts plus paths that support precise follow-up tools.
- Consider querying multiple handles later, but do not require index merging
  or mutating an existing snapshot to add a new corpus.
- Optional semantic enrichment could be amortized across multiple queries,
  but must still justify its cold-start and indexing costs through benchmarks.

The intended division of responsibility is discovery with sift, precise
verification with rg or file reads, then edits against current source.

### Reference list

[1] Tantivy project (primary source).
[2] Tantivy search benchmark (primary source; documents BM25 usage).
[3] SQLite FTS5 documentation (primary source; trigram tokenizer).
[4] Tree-sitter project and parser documentation (primary sources).

These references motivate candidates, not performance guarantees for sift.
