# Relevance and performance evaluation

Run from the repository root on Linux. Requires Cargo, the configured Clang/LLD
linker, Python 3 with SQLite FTS5, and ripgrep. No Python packages are required.

```sh
python benchmarks/run.py --output target/evaluation-1 --repetitions 20
python benchmarks/experiments.py --output target/evaluation-sweep --repetitions 20
python -m unittest discover -s benchmarks -p 'test_*.py'
```

Output directories must not exist. The driver builds a release binary with
line-table debug information and `--no-rosegment` for LLD stack unwinding.
`--binary PATH` instead uses a caller-supplied release binary and records its
hash; its build settings are unknown to the driver. Each run retains the corpus,
snapshots, measurement helper, and `report.json`. Snapshots use a run-local XDG
data directory; the normal Sift store is not touched. Remove output directories
manually when finished. A full sweep can consume several GiB.

The completed reference evaluation and decisions are in `EVALUATION.md` and
`benchmarks/results/section2.json`.

## Corpus and relevance

`benchmarks/corpus` contains original Rust, Python, TypeScript/TSX, Go, SQL, YAML,
and Markdown fixtures. These are search inputs, not buildable apps.
`benchmarks/queries.json` declares relevant path/line locations for identifiers,
paths, error messages, keywords, natural language, filler, query coverage, and
repeated implementations. Long repeated excerpts and near-copy files exercise
result diversity. The default workload adds 100,000 deterministic Rust lines in
100 files; `--lines 0` uses only fixtures. Reports record actual bytes, files,
lines, and a path/content SHA-256.

Three retrieval methods are compared:

- Sift: production query behavior, with ten results.
- Lexical: raw paths and raw Sift chunk text in a separate in-memory FTS5 table,
  equal-weight BM25, OR-connected ordinary word tokens, no code token variants,
  and no overlap suppression. It shares chunk boundaries, not preprocessing or
  ranking. Python's SQLite version is recorded separately from Sift's bundled
  dependency in Cargo.lock.
- rg: case-sensitive fixed-string content search for the entire query, sorted
  by path and line. No rewriting or path-search fallback. Paths and paraphrases
  can return no results. Results are matching lines, not chunks. Its first
  five/ten lines define top-k, not a relevance ranker.

Hit@5/10 indicates at least one expected location retrieved. Recall@5/10 is the
fraction of expected locations covered, averaged per query. Duplicate hits do
not increase recall. Per-query results and categories permit failure inspection.
The missing-whole-identifier case explicitly expects component fallback; its
literal-query hit count is zero. A retrieval hit is not proof of an exact match.

Reports include Sift text/JSON bytes, all-match rg JSON bytes, and a shared
path/range/snippet JSON representation of each engine's top ten. Native sizes
are not directly comparable because envelopes and excerpt lengths differ.
Diversity metrics include unique files/snippets, maximum hits per file, and
near-duplicate pairs at ten. Near duplicates use Jaccard similarity of lowercase
whitespace-token sets, with a threshold of 0.85. This detects lexical copies,
not semantic equivalence. The small synthetic suite is a regression seed, not
evidence of relevance on real repositories.

## Performance and reference protocol

Before assessing the indexing goal, retain a reference JSON containing `name`,
`storage`, `power_mode`, and `workloads`, and pass it with `--reference PATH`.
Record the physical storage model and filesystem stack, power/governor settings,
CPU affinity, available RAM, and competing activity. The driver records CPU,
total/available memory, affinity, load average, OS/kernel, filesystem, compiler,
revision/dirty status, binary hash, and tool versions. Use the same corpus hash,
hardware, cache protocol, and build settings for comparisons.

For this evaluation, "a few seconds" means **p95 <= 5 seconds for at least
100,000 actual input lines**, under the named cache condition. Always report
files and bytes alongside lines. A warm synthetic pass is not a general speed
or cold-cache guarantee. Twenty samples is a starting point, not a confidence
interval. Sweep variants run sequentially; workload drift can affect comparisons.

`benchmarks/measure.c` is compiled with Clang. Its monotonic timer covers
fork/exec through child exit, including output capture. The native helper's own
startup is excluded. Indexing includes discovery, publication, metadata
validation, and handle output. Peak RSS comes from the helper's `wait4`, not
Python's: launching directly from Python can include the driver's inherited
pre-exec memory high-water mark. The helper keeps that floor small. RSS is not
system-wide memory consumption or filesystem-cache size.

Database size is the published SQLite file's size. `info_startup` measures
process plus metadata opening, not pure process startup. Query latency includes
startup, snapshot opening, retrieval, and text output. The performance suite adds
a high-fan-out `pub const` query and a no-match query; neither enters relevance
averages. Nearest-rank p50/p95 and raw samples are retained. Baseline engines are
used for relevance, not cross-language timing comparisons.

### Warm cache

The default performs one excluded full index warmup before indexing samples.
Every query is explicitly warmed before timed repetitions. Query sample order
is deterministically permuted by SHA-256 of each sample position. The separate
`first_query` field is the first measured query on the last snapshot, but is
**not cold**: corpus generation, indexing, and metadata reads already touch data.
Warmup is an operational condition, not a promise that all data remains resident.

### Cold input cache

```sh
python benchmarks/run.py --output target/evaluation-cold --repetitions 20 \
  --reference reference.json --cache-command '/absolute/path/to/cache-helper'
```

This explicit, operator-supplied command runs before each indexing sample, before
`first_query`, and before each separately reported `cold_query` sample. There is
no index warmup in this mode. Warm query samples are collected separately after
rewarming the suite. The command is split into arguments without an implicit
shell; failure aborts the run. The driver never invokes sudo or changes cache or
perf permissions itself.

The operator must arrange and verify eviction of corpus and snapshot payload
pages, finish pending writes before eviction, and record the method and evidence
in the reference notes. A successful helper exit alone does not prove eviction.
Use a dedicated host or disposable VM, not a shared machine where cache eviction
can disrupt others. Reboot-based tests need equivalent externally orchestrated
samples. Device/controller caches and libraries touched by the launcher remain
separate conditions. No cold-cache measurements are claimed in the retained
reference evaluation.

## Experiments and production selection

`experiments.py` copies sources into isolated directories, shares a release build
cache, and preserves each variant's binary and source hash. It runs the same
corpus/query suite for each variant and writes `experiments.json`, full per-run
reports, and compact `summary.json` for retention. No workspace sources or normal
snapshots are modified. Experimental chunk metadata changes with the constants;
use the matching experimental binary to read those snapshots.

The sweep compares:

- Original streaming, unbounded-candidate, overlap-only selection (`legacy`).
- Bounded candidates with and without diversity.
- 16 lines/2 overlap, 32/0, default 32/4, and 64/8 with a 4096-byte cap.
  Other variants retain the 2048-byte cap.
- Path/body BM25 weights 1:1, 3:1, and 6:1.
- Candidate budgets of 2, 10, and 50 times the requested result limit.

Separate Python/SQLite diagnostic ablations compare BM25 with coverage,
whole-identifier boost, literal phrase boost, filler removal, and combined
identifier/phrase/coverage sorting, at candidate caps 20/100/500. Stable sorting
preserves BM25 order for ties; selection only suppresses byte overlap. Coverage
is the fraction of distinct code-aware query terms present in path plus body.
Identifier matches use whole raw tokens; phrase matches use case-folded,
whitespace-normalized literal text. Filler removal is a fixed experimental list,
with an empty-query fallback. Its tokenizer matches the ASCII fixture cases;
it is not a replacement for Rust's Unicode tokenizer. Diagnostic latency includes
Python reranking and is **not** a prediction of a native implementation's cost.

The measured production policy fetches at most `10 * limit` BM25 candidates,
prefers no more than two hits per root/file and non-near-duplicate excerpts,
then fills unused slots from deferred hits in original order. Byte overlap is
always rejected, including during fallback. These are soft diversity preferences,
not hard deduplication or file quotas. Single-file queries can still fill their
limit. A bounded pool can miss alternatives beyond the cutoff or underfill after
overlap removal. SQL LIMIT bounds materialization, not FTS matching/scoring work.

Chunk size/overlap, field weights, and OR-connected query terms remain unchanged.
The evidence and reasons for not adopting other experiments are in `EVALUATION.md`.

## Flamegraphs

```sh
cargo install flamegraph --locked
python benchmarks/run.py --output target/evaluation-profile --flamegraphs
```

Install Linux `perf` and arrange permission to profile your own processes.
Missing flamegraph fails before building; profiling errors fail the run without
deleting its already-written timing report.

Profiling runs separately after measurements and writes `index.svg` and
`query.svg`. The query graph covers repeated fresh CLI processes, including
startup, and also includes the Python launcher; focus on Sift stacks.
`--profile-repetitions 100` is independent of timing samples. The index graph
profiles one full indexing run. Very small corpora may produce too few samples.
Profiler intermediate files remain in the output directory. Graphs are sampling
diagnostics, not timing samples or off-CPU I/O measurements.
