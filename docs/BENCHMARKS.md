# Relevance and performance evaluation

Run from the repository root on Linux. Requires Cargo, the configured Clang/LLD
linker, Python 3 with SQLite FTS5, and ripgrep. No Python packages are required.

```sh
python benchmarks/run.py --output target/evaluation-1 --repetitions 20
python -m unittest discover -s benchmarks -p 'test_*.py'
```

The output directory must not exist. Each run builds an isolated release binary
with line-table debug information and `--no-rosegment` for LLD stack unwinding.
It retains the generated corpus, build, snapshots, and `report.json`. Snapshots
use a run-local XDG data directory; the normal Sift store is not touched.
Remove the selected output directory manually when finished.

## Corpus and relevance

`benchmarks/corpus` contains original, small Rust, Python, TypeScript/TSX, Go,
SQL, YAML, and Markdown fixtures. These are search inputs, not buildable apps.
`benchmarks/queries.json` declares relevant path/line locations for identifiers,
paths, error messages, keywords, natural language, and conversational filler.
Related-name distractors expose partial-identifier matches. The default workload
adds 100,000 deterministic Rust lines in 100 files; `--lines 0` uses only fixtures.
The report records actual bytes, files, lines, and a path/content SHA-256.

Three retrieval methods are compared:

- Sift: production query behavior, with ten results.
- Lexical: raw paths and raw Sift chunk text in a separate in-memory FTS5 table,
  equal-weight BM25, OR-connected ordinary word tokens, no code token variants,
  no overlap suppression. This deliberately unboosted baseline shares chunk
  boundaries, not preprocessing or ranking. It uses Python's SQLite, whose
  version is recorded separately from Sift's bundled dependency in Cargo.lock.
- rg: case-sensitive fixed-string content search for the entire query, sorted
  by path and line. No query rewriting or path-search fallback. Path queries
  and paraphrases can legitimately return no results. Results are matching
  lines, not chunks. The first five/ten lines define its top-k, not a ranker.

Hit@5/10 indicates at least one expected location retrieved. Recall@5/10 is the
fraction of expected locations covered, averaged per query. Duplicate hits do
not increase recall. Per-query results and categories permit failure inspection.
The report includes Sift text/JSON bytes, all-match rg JSON bytes, and a shared
path/range/snippet JSON representation of each engine's top ten. Native output
sizes are not directly comparable because envelopes and excerpt lengths differ.
Unique file and exact snippet counts expose basic diversity, not semantic
near-duplicate detection. This small synthetic suite is a regression seed, not
evidence of relevance on real repositories.

## Performance protocol

Each sample measures wall time from process launch through exit, including output
capture. Indexing includes discovery, publication, metadata validation, and handle
output. Peak RSS comes from Linux `wait4` for that individual child, not cumulative
child statistics. Database size is the final published SQLite file's size.
`info_startup` measures process plus metadata opening, not pure process startup.
Query latency includes startup, opening the snapshot, retrieval, and text output.
The performance suite adds a high-fan-out `pub const` query and a no-match
query to expose candidate-selection and startup costs. These do not enter the
relevance averages. Each query is warmed before measured repetitions; query order is deterministically permuted by SHA-256 of each
sample position. p50/p95 use nearest-rank percentiles. Raw samples are retained.
Baseline engines are used for relevance, not cross-language timing comparisons.

The report records CPU model, total RAM, OS/kernel, filesystem type, compiler,
revision/dirty status, build settings, and tool versions. Before making a speed
claim, select and retain one reference report, record the storage device, power
mode, CPU affinity, available RAM, and competing workloads alongside it. Use the
same hardware and environment for comparisons. Twenty samples is only a starting
point; increase repetitions for stable tail measurements.

Cache conditions are explicitly **uncontrolled OS cache** for indexing and the
first query, and **explicitly warmed** for timed queries. Corpus generation and
indexing already touch data. A new process is not a cold-cache test. The driver
does not evict caches or change privileged system settings. Cold-cache and
100,000-line goal claims remain pending a controlled reference-machine protocol.

## Flamegraphs

```sh
cargo install flamegraph --locked
python benchmarks/run.py --output target/evaluation-profile --flamegraphs
```

Install the Linux `perf` tool and arrange permission to profile your own
processes. The driver does not invoke sudo or modify perf permissions. Missing
flamegraph fails before building; profiling errors fail the run without deleting
the already-written timing report.

Profiling runs separately after measurements and writes `index.svg` and
`query.svg`. The query graph covers repeated fresh CLI processes across the
query suite, including startup. `--profile-repetitions 100` controls repetitions
independently of timing samples. The graph also includes the Python launcher;
focus on Sift stacks. The index graph profiles one full indexing run; very small
corpora may produce too few samples, so use the default scale or larger.
Profiler intermediate files stay in the output directory. These graphs are
sampling diagnostics, not timing samples or off-CPU I/O measurements.

## Measurement-gated follow-up

No production ranking/chunk defaults change in this work. Inspect failures by
category before testing exact-identifier/phrase boosts or filler removal. Expand
fixtures with multi-location expectations, long files, and repeated excerpts
before selecting a diversity policy. Sweep chunk size/overlap, field weights,
and candidate-selection strategies in isolated experimental builds, preserving
corpus hashes and relevance reports. These experiments and a real-project corpus
remain open; the synthetic padding alone does not exercise realistic candidate
fan-out or establish the 100,000-line performance goal.
