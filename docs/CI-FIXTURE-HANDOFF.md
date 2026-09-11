<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# CI fixture production and transfer

Corpus production uses the authenticated O3/full-LTO producer. Test compilation
starts independently and cannot produce corpus fixtures. CI preserves two cold
generations on every pull request; neither generation restores an action cache.

| Job | Required inputs | Responsibility |
| --- | --- | --- |
| `rust-prebuild` | Canonical Rust sources | Compile the producer-independent workspace test inventory. |
| `fixture-prefix` | Authenticated producer executable and receipt | Produce the selected stable-stage receipts. |
| `fixture-complete` | Prefix selector and action artifact; authenticated generated tree and bundle | Produce slice-specification, docs, and bundle-derived actions, then finalize the selector. |
| `rust-archive` | Rust build products; complete selector and action artifact; exact bundle import | Verify the selected corpus read-only, finish consumer compilation, and authenticate one nextest archive. |

Slice specifications consume the generated validation and constraint shapes, so
they run after the authenticated generated tree arrives. Their completed actions
remain independently reusable; moving their job earlier cannot remove that input
dependency.

An action cache accelerates a producer. Its absence means that producer computes
the requested actions. A fixture artifact transfers the exact output required by
a downstream consumer. Its absence is an error. Neither archive construction nor
a test runner may turn a missing artifact into another corpus production pass
(Principles 4, 7, and 17).

Each fixture producer exports the SHA-256 of its finalized selector as a job
output. The next job requires that exact digest before loading any selected
action. Read-only verification then authenticates the recorded action contexts,
receipts, blobs, source identity, and producer profile. The final nextest archive
receipt binds the same selector and the optimized executable's receipt. Cache
keys never stand in for those checks.

Successful update runs record reusable closure receipts in
`.cache/gmeow-sync/stage-fixture-candidate-v2.json`. This candidate is separate
from the finalized test selector; only the explicit fixture producer publishes
that selector. Ordinary synchronization cannot replace its bundle-import or docs
actions or invalidate a digest already handed to a runner. Before reusing a
candidate, the producer rebinds the current DAG, rehashes every declared raw input,
rederives action contexts, and authenticates the recorded stable products.
Retaining these small closure receipts avoids reconstructing uncached cumulative
carriers merely to discover reusable outputs. A candidate miss causes explicit
production; the runner selector is never used as a candidate fallback.

Only producer jobs restore candidates. Their action caches contain the candidate
and bounded actions, never a finalized selector. The completion job downloads the
required current-run prefix artifact after restoring its cache; that artifact
also carries the prefix's current candidate for the completed producer cache.
Archive and test jobs consume the final required artifact and its exact selector
digest. They never restore the producer candidate.

GitHub caches are immutable. Prefix and complete producers therefore restore
reusable stores and explicitly save under separate, fresh run/attempt keys.
Restore order prefers complete stores before prefix stores at the current source
revision, then complete and prefix stores from compatible revisions. This keeps
work from an interrupted current revision reachable even when an older revision
has a complete cache. Each action's own content key determines admissible reuse.
A newly completed action is published atomically by its producer, and CI saves
these bounded receipts even if a later action fails. Only a successful producer
can publish its finalized selector artifact. Cumulative carrier snapshots are
never transferred or cached.

The cold generations, fixture producers, archive, and test shards retain separate
timing evidence. A warm action hit must authenticate the same product identity;
changed inputs recompute only their dependent actions, while corruption fails.
The cache is bounded by the shared action store's quota and reachability rules.
