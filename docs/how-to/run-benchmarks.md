[Documentation](../README.md) / How-to guides

# Run benchmarks

Run from the repository root on Linux with Cargo, Clang/LLD, ripgrep, and Python 3
with SQLite FTS5. No Python packages are required. Use a new output directory
for every run.

## Run the suite

```sh
python -m unittest discover -s benchmarks -p 'test_*.py'
python benchmarks/run.py --output target/evaluation-1 --repetitions 20
```

The driver builds Sift in release mode and retains its corpus, snapshots, native
measurement helper, and `report.json`. It uses a run-local store, not your normal
Sift store. Inspect per-query retrieval and raw timings rather than only averages.

## Compare tuning variants

```sh
python benchmarks/experiments.py --output target/evaluation-sweep --repetitions 20
```

The sweep preserves isolated sources and binaries and writes `experiments.json`
and `summary.json`. It can consume several GiB. Remove run directories manually
when you no longer need the artifacts.

For defensible comparisons, supply your own machine notes with `--reference PATH`
and follow the [reference protocol](../reference/benchmarks.md). The default run
is warmed; do not label it a cold-cache measurement.

## Collect flamegraphs

Install Linux `perf` and arrange permission to profile your own processes, then:

```sh
cargo install flamegraph --locked
python benchmarks/run.py --output target/evaluation-profile --flamegraphs
```

Profiling runs separately from timings and writes `index.svg` and `query.svg`.
See [profiling details](../reference/benchmarks.md#flamegraphs) for interpretation
and [retained evaluation](../explanation/evaluation.md) for measured decisions.
