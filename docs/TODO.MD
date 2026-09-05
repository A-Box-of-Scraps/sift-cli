# Remaining work

## Current baseline

Implemented: separate immutable SQLite databases, explicit UTF-8 file indexing
under one root, bounded overlapping chunks, content hashes and provenance,
code-aware token variants, FTS5 ranking, recursive directory/glob discovery,
ignore controls, NUL-separated file lists, anonymous and typed documents, and
`index`, `query`, and `info` commands.
Queries support `--path`, `--limit`, compact text, and versioned JSON.

Current limits: Linux-only publication, no root rebinding, and an 8 MiB
per-document cap. Named filesystem roots and synthetic documents can coexist
through independent snapshot extension. Invalid UTF-8,
NUL-containing files, and symlinks fail explicit indexing or are skipped during
discovery. See `INPUTS.md` for discovery, adapter, and error behavior. Chunking
uses 32 lines, 4 overlap lines, and a 2048-byte cap.

`IDEA.md` retains the original design discussion, including historical status
notes and proposals. The checklist below describes work after phase two,
including completed input adapters.

## 1. Input discovery and adapters

- [x] Add recursive directory indexing and quoted glob expansion, including
  mixed file/directory/glob inputs and deduplication.
- [x] Define consistent ignore-file and hidden-file behavior. Keep symlinks
  disabled by default; specify any overrides before implementing them.
- [x] Add `--no-ignore` (all ignore files) and `--no-gitignore` (Git ignore
  rules only), independently of hidden-file and symlink behavior.
- [x] Define skip-versus-fail behavior for discovered binaries, invalid text,
  oversized files, unreadable files, unmatched globs, and empty selections.
  Report skips and progress on stderr without contaminating handle output.
- [x] Add `--files0-from -` for NUL-separated file lists.
- [x] Add `--stdin` and optional `--name` for one anonymous document, with
  stream-relative line numbers. Display names must not imply filesystem paths.
- [x] Allow library callers to index typed documents without temporary files.
- [x] Test ignored trees, duplicate selections, unusual filenames, symlink
  changes, read failures, and files modified during ingestion.

## 2. Relevance and performance evaluation

Evaluation driver and protocol: `benchmarks/run.py` and `BENCHMARKS.md`. Optional
`--flamegraphs` emits indexing and repeated-query SVGs separately from timings.
Reference measurements, isolated tuning experiments, and decisions are recorded
in `EVALUATION.md` and `benchmarks/results/section2.json`. The corpus is synthetic;
the warmed reference result is not a cold-cache or real-repository guarantee.

- [x] Build a mixed-language relevance corpus with expected locations for
  identifier, path, error-message, keyword, and natural-language queries.
- [x] Measure top-5/top-10 retrieval and output size against rg for exact
  discovery and an unboosted lexical baseline.
- [x] Define reference hardware, input bytes/files/lines, and cold/warm cache
  conditions before assessing the 100,000-line indexing goal.
- [x] Benchmark process-to-published-index time, query startup, p50/p95 query
  latency, peak memory, and database size using release builds.
- [x] Evaluate query coverage, exact identifier and phrase boosts, and handling
  of conversational filler. Current OR-connected components can match related
  identifiers even when the whole identifier is absent.
- [x] Improve result diversity beyond byte-overlap suppression, including
  near-duplicate excerpts and repeated hits from one file.
- [x] Benchmark chunk sizes, overlap, field weights, and candidate-selection
  cost before changing defaults or adding complexity.

## 3. Snapshot lifecycle and roots

Implemented behavior and support limits: `LIFECYCLE.md`.

- [x] Add `delete <handle>` with ownership/path validation that prevents
  arbitrary directory deletion. Define behavior with concurrent readers.
- [x] Define retention, abandoned-staging cleanup, and storage reclamation.
- [x] Add named roots and multiple roots per snapshot, with explicit conflict
  handling and a query-side `--root <name>` filter.
- [x] Specify root identity and relocation/rebinding rules before extension.
  Do not merge unrelated sources merely because their relative paths match.
- [x] Add `index ... --extend <handle>` by building a new independent snapshot:
  unchanged sources are skipped, changed sources are replaced, new sources are
  added, and sources absent from extension inputs are retained.
- [x] Test failed/interrupted extension and independent parallel extensions;
  original handles must remain unchanged and queryable.
- [x] Add explicit staleness checks using stored hashes. Keep query excerpts
  tied to indexed content rather than silently reading current source text.
- [x] Define artifact portability, supported format compatibility, and safe
  publication on other platforms. Test crash recovery and specify whether
  power-loss durability is a supported guarantee.

## 4. Public contracts and hardening

- [ ] Document implemented CLI/API behavior, root-relative input/filter rules,
  limits, snapshot semantics, and exit statuses in `docs/`.
- [ ] Document the JSON schema, result-ID scope, byte/line ranges, truncation
  semantics, and schema compatibility expectations.
- [ ] Expand CLI regression tests for invalid queries, broken output pipes,
  corrupt artifacts, unsupported formats, and filesystem permission failures.
- [ ] Review terminal rendering of untrusted source text and unusual paths
  while preserving faithful source text in structured output.
- [ ] Add repeatable CI checks for formatting, Clippy, and tests, plus release
  installation/package validation for supported targets.

## 5. Optional experiments, gated by measurements

- [ ] Introduce a small shared backend contract and prototype Tantivy for a
  controlled comparison with SQLite FTS5. Keep public types backend-neutral.
- [ ] Evaluate syntax-aware chunking and symbol/language enrichment with a
  bounded generic fallback for unsupported or invalid source files.
- [ ] Evaluate substring retrieval only if lexical query tests justify it.
- [ ] Evaluate optional embeddings/hybrid retrieval only for measured
  vocabulary-mismatch failures; include model startup and indexing costs.
- [ ] Consider multi-handle queries, context expansion by result ID, and
  unchanged-data reuse only when concrete workflows justify them.

No daemon, hidden active index, automatic watcher, or mutable in-place append is
planned. Optional experiments are not requirements for the initial release.
