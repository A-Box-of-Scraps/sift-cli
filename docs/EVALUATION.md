# Section 2 reference evaluation

## Scope and reproduction

This completes the synthetic evaluation, not validation on real repositories.
The retained machine-readable summary is `benchmarks/results/section2.json`;
reference conditions are in `benchmarks/results/reference.json`.

```sh
python benchmarks/experiments.py --output target/evaluation-section2-reference \
  --repetitions 20 --reference benchmarks/results/reference.json
```

Choose a new output directory when repeating. Full reports, raw samples,
experimental sources, binaries, and snapshots from the recorded run remain in
`target/evaluation-section2-reference`. The checked-in summary records hashes of
those reports, sources, and binaries, plus relevance and timing summaries. It
omits full excerpts and raw samples. Reproduction on another machine requires
new reference notes, not reuse of this hardware description.

Reference: AMD Ryzen 5 7600X, 32,454,612 KiB RAM, Samsung SSD 9100 PRO 2TB NVMe,
LUKS/device-mapper and btrfs, CPU governor `performance`. Linux
7.2.3-arch1-2, rustc 1.97.1, release builds with debug level 1 and LLD
`--no-rosegment`. CPU affinity allowed CPUs 0-11; no pinning or workload isolation.
Available RAM at default-run start was 15,689,240 KiB; load average was
2.71/1.56/1.09. Background work was uncontrolled. These are local observations,
not stable tail-latency guarantees.

Input: **116 files, 3,788,098 bytes, 100,360 lines**, including 100,000 generated
lines. Corpus SHA-256:
`4aa8b6f78c91509e3da7bca26b5f09c268137b791a3610b8108f0b640bf1bfa5`.
All variants used the same corpus and 18 relevance queries. Indexing and queries
were explicitly warmed, with 20 measured repetitions. No cache eviction was
performed. See `BENCHMARKS.md` for the separate cold-input-cache protocol.

## Relevance and diversity

| Method | Hit@5 | Recall@5 | Hit@10 | Recall@10 |
| --- | ---: | ---: | ---: | ---: |
| Original streaming/overlap-only Sift | 94.44% | 90.74% | 100% | 100% |
| Bounded overlap-only Sift | 94.44% | 90.74% | 100% | 100% |
| Bounded, diversity-aware Sift | 100% | 100% | 100% | 100% |
| Unboosted lexical | 88.89% | 85.19% | 88.89% | 85.19% |
| Exact fixed-string rg | 50% | 48.15% | 50% | 48.15% |

The diversity fixture expects three locations: a repeated worker excerpt and two
other implementations. Original Recall@5 was 1/3; diversity selection retrieves
all three. Adding conversational filler originally missed both expected alternate
implementations at five; diversity selection retrieves both. For these queries,
near-duplicate pairs at ten decrease from 21 to 15. Duplicates remain because
fallback deliberately fills otherwise unused slots. Maximum hits per file at ten
can still exceed two; the preference is soft.

The production change retains BM25 order within preferred and deferred groups,
uses a 10-times-limit candidate pool, defers near copies and third/subsequent
hits from one root/file, and rechecks byte overlap during fallback. It keeps
single-file queries useful. Diversity can promote partial lexical matches ahead
of exact copies; it does not impose an exact-match predicate.

Sift text/JSON sizes, rg all-match JSON sizes, normalized output bytes, and
literal-query hit counts are retained per query. They are not interchangeable
payload formats. In particular, `rotate_session_token_missing` has zero literal
hits but retrieves the expected related function through OR-connected components.
This is explicitly scored as fallback coverage, not exact-identifier correctness.

## Query-ranking ablations

Coverage, whole-identifier preference, literal phrase preference, filler removal,
and combined identifier/phrase/coverage preference were compared with BM25 at
20, 100, and 500 candidates. All retained the overlap-only baseline's aggregate
Hit@5/Recall@5 of 94.44%/90.74% and 100% at ten. None solved the duplicate-heavy
failures. This does not rule out ordering improvements within the top five or
benefits on a larger corpus.

At 100 candidates, pooled diagnostic Python/SQLite p95 was 0.086 ms for BM25 and
4.496-4.714 ms for the rerankers. These timings include Python tokenization and
sorting, and must not be extrapolated to native Rust. The ablations are ranking
prototypes, not production switches. With no aggregate retrieval improvement,
keep production OR terms, identifier handling, and filler handling unchanged.

## Release-build performance sweep

Times below are warmed p95. All non-legacy diversity variants achieved 100%
Hit/Recall at five and ten. Index timing differences between variants that only
change query code illustrate environmental noise, not an indexing improvement.

| Variant | Index ms | Database bytes | `pub const` query ms |
| --- | ---: | ---: | ---: |
| Default: 32/4, 2048 bytes, weight 3, pool 10x | 184.85 | 17,600,512 | 9.52 |
| Legacy streaming, no candidate limit | 182.98 | 17,600,512 | 13.70 |
| Pool 10x, overlap-only | 179.43 | 17,600,512 | 9.24 |
| Chunk 16/2, 2048 bytes | 184.97 | 16,289,792 | 10.00 |
| Chunk 32/0, 2048 bytes | 170.69 | 15,572,992 | 9.07 |
| Chunk 64/8, 4096 bytes | 173.34 | 20,189,184 | 10.76 |
| Path/body weight 1:1 | 180.52 | 17,600,512 | 9.43 |
| Path/body weight 6:1 | 181.99 | 17,600,512 | 9.50 |
| Candidate pool 2x | 183.18 | 17,600,512 | 8.88 |
| Candidate pool 50x | 182.03 | 17,600,512 | 15.59 |

Default indexing p50/p95 was **178.34/184.85 ms**, with peak child RSS
**11,404 KiB**. Pooled query p50/p95 was 2.94/4.02 ms across 400 samples, equally
weighting the 20 query texts. The high-fan-out query has its own 9.52 ms p95;
pooling must not hide that cost. No-match p95 was 3.15 ms and metadata/process
startup p95 was 2.88 ms. Maximum query child RSS was 8,952 KiB.

The native measurement helper prevents Python's pre-exec RSS high-water mark
from dominating child measurements. Tests explicitly allocate a large Python
heap and check that measured child RSS does not inherit that size.

The defined **warmed synthetic** indexing goal, p95 <= 5 seconds for at least
100,000 lines, passes on this reference. Cold-cache, larger-byte-per-line, and
real-repository performance remain unassessed.

## Decisions

- Adopt bounded candidate materialization and soft lexical/file diversity. They
  improve this suite's top-five coverage and reduce high-fan-out latency compared
  with the original selection. SQL LIMIT does not bound FTS scoring work.
- Keep a 10x pool as a conservative headroom choice, not a measured optimum. The
  2x pool also passes this small suite and is faster; 50x costs more with no gain.
  All bounded pools can miss useful results past the cutoff.
- Retain 32 lines, four overlap lines, and the 2048-byte cap. Zero overlap saves
  time and space here, but this corpus does not establish boundary-sensitive
  recall well enough to remove overlap. Larger chunks increase database size.
- Retain 3:1 path/body weights. Neither alternative improves measured retrieval.
- Do not add production query reranking or filler stopwords on this evidence.
- Expand real-project and boundary-sensitive fixtures before making broader
  claims. This is follow-up evidence gathering, not an unfinished section-2
  implementation or a claim that synthetic results generalize.
