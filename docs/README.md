# Documentation

Sift indexes text into immutable local snapshots and returns ranked excerpts.
Start small, then follow links to the details you need.

**New to Sift?** Follow [Your first search](tutorials/first-search.md).

## Tutorials: learn by doing

- [Your first search](tutorials/first-search.md): install, index, query, and delete a snapshot.

## How-to guides: complete a task

- [Select inputs](how-to/select-inputs.md): directories, globs, ignore rules, and piped text.
- [Manage snapshots](how-to/manage-snapshots.md): extend, filter roots, check freshness, and clean up.
- [Run benchmarks](how-to/run-benchmarks.md): reproduce measurements and collect profiles.

## Reference: look up a contract

- [CLI](reference/cli.md): commands, flags, storage, limits, and exit statuses.
- [Inputs](reference/inputs.md): discovery rules, validation, and document identity.
- [Query output](reference/output.md): text rendering and the JSON schema.
- [Snapshots and roots](reference/snapshots.md): extension, retention, recovery, and portability.
- [Rust API](reference/rust-api.md): library entry points and types.
- [Benchmark protocol](reference/benchmarks.md): metrics, cache conditions, and experiments.

## Explanation: understand the design

- [How Sift searches](explanation/search.md): lexical retrieval and its limits.
- [Why snapshots are immutable](explanation/snapshots.md): handles, provenance, and explicit refreshes.
- [Evaluation and decisions](explanation/evaluation.md): retained synthetic measurements and their limits.
- [Historical design notes](explanation/design-history.md): original proposals, not the current contract.
