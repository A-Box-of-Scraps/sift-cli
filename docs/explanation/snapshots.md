[Documentation](../README.md) / Explanation

# Why snapshots are immutable

A handle identifies stored content, not a running process. Every command opens
an explicit handle and exits. There is no daemon, watcher, hidden active index,
or automatic refresh.

## Stable evidence for exploration

Index once and query many times. Storing the original excerpts means a query's
text and offsets describe the same indexed revision, even after source edits.
That stability is useful for exploration, but it is not freshness: read current
source before editing. A staleness check reports observations without changing
the indexed evidence.

## Extension without mutation

Extension produces a new independent snapshot. Existing handles retain their
meaning, and deleting a base does not invalidate its descendants. This favors
simple ownership and failure recovery over shared-storage efficiency: an
extension copies the database and can require substantial temporary disk space.

Extension is additive refresh, not directory synchronization. Sources absent
from the new input remain. A fresh index is the way to remove old sources.

## Roots preserve source identity

A source is identified by root ID plus relative path. Two projects can each have
`src/main.rs` without overwriting one another. Root names make this distinction
readable and filterable; stored absolute locations provide provenance and support
staleness checks. Moving sources does not silently rebind that identity.

## Explicit ownership and cleanup

Published snapshots persist until explicitly deleted. Cleanup handles abandoned
staging directories, not expiration. Atomic publication prevents partial builds
from appearing as completed snapshots, but does not guarantee power-loss durability.

Use [manage snapshots](../how-to/manage-snapshots.md) for commands, or consult the
[snapshot reference](../reference/snapshots.md) for precise guarantees and limits.
