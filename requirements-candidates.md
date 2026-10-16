# Requirement candidates — issue #1757

Lines from the issue body and comments that matched a lexical rule, copied
verbatim with the rule that fired. This is a CANDIDATE list: rules both
over-match (a "should" in passing) and under-match (a requirement stated
without a keyword). The issue-analyzer keeps or drops each row, adds what
the rules missed, and accounts for every silent comment. Code fences are
skipped.

| ID | Source | Line | Rules | Text |
|---|---|---|---|---|
| B1 | body | 5 | modal | The implementation was committed after the normal repository commit hook passed. This issue owns the work deliberately left after merging that concrete implementation; it must not be represented as completed by #1740. |
| B2 | body | 14 | modal | - Do not introduce a narrow producer feature, crate, or separate compilation lineage as a performance measure: `make check` builds the complete required Rust surface, so shifting code between Cargo invocations is not removed work. Retain the authenticated O3/fat-LTO producer contract and optimize only measured work removed from the end-to-end gate, including duplicate execution, repeated lowering/parsing/materialization, avoidable corpus reads, and serial scheduler dependencies. |
| B3 | body | 16 | modal | - Reconcile and close the remaining #1704 child issues only after their own acceptance evidence is present. In particular, #1748 still requires the complete program-aware admitted native run; exporter/source-admission code alone is not that certificate. Existing PurRDF #317 remains upstream-owned and GMEOW must not duplicate its generic conformance suite. |
| B4 | body | 17 | modal | - Review current CodeRabbit findings and required CI on the post-merge follow-up PR before its final merge. |
| B5 | body | 21 | modal | Keep standpoint, provenance, complements, loss evidence, logical stage identities, required checks, exact source/context/program identities, and both cold generations. Bounded-corpus evidence never authorizes unrestricted rewrites. Generic RDF 1.2, SPARQL, SHACL, and codec conformance remains exclusively PurRDF-owned. |
| B6 | body | 33 | modal | The merged change also excludes machine-leased Cargo output directories from producer resolution identity. Output placement does not select packages, features, targets, or sources, and must not make a freshly built producer receipt stale on the next invocation. |
| B7 | body | 43 | modal | The cold update pipeline was stopped after `stage-source-load` exceeded ten minutes. This is an incomplete diagnostic observation, not a completed gate or a speedup claim. The next implementation must expose certified/slow probe counts per slice, identify the residual native families, extend only proof-backed shared analyses, and complete the full cold pipeline plus both independent CI generations. Workspace compilation remains required either way, so moving code behind a narrow producer feature or crate is explicitly outside the optimization target. |
| C3.1 | comment 3 (paudley, 2026-09-17T05:52:13Z) | 11 | modal | This is measured O3/full-LTO producer-build evidence, not a speedup claim. It confirms the optimized producer build/link surface is itself a major critical-path owner and does not saturate the four-core runner. The narrow producer surface remains required follow-on work in #1757; scheduler changes alone cannot remove this 18.5-minute pre-generation cost. |
| C4.1 | comment 4 (paudley, 2026-09-17T05:58:04Z) | 27 | modal | - Hosted CI final candidate b6c4441ba: lint passed; producer-build and rust-prebuild were still running at merge finalization. The two cold generations and remaining required jobs were not claimed. Issue #1757 durably owns their completion and every resulting repair. |
| C7.1 | comment 7 (paudley, 2026-09-22T09:02:22Z) | 13 | scope-out | 3. Add a focused synthetic regression that proves the removed operation stays removed and that old/new graph and byte projections are identical. Tests will consume supplied fixtures only; they will not produce corpus data. |
| C7.2 | comment 7 (paudley, 2026-09-22T09:02:22Z) | 15 | modal | 5. Complete a fresh `make check`, authenticated fixture production, and the required focused nextest inventory for correspondence laws, expression selection, native literal identity, source admission, old-graph removal, and ordinary/heavy conformance selectors. |
| C7.3 | comment 7 (paudley, 2026-09-22T09:02:22Z) | 17 | modal | 7. Reconcile every remaining #1704 child only against its own acceptance evidence, including #1748's complete program-aware admitted native run. Review and resolve current CodeRabbit findings and all required CI checks, then merge only through `ghprsq`. |
| C13.1 | comment 13 (paudley, 2026-09-22T21:12:41Z) | 21 | modal | A fresh optimized producer rebuild and identical 24 GiB serial measurement remain required before claiming #1757 fixed. |
| C14.1 | comment 14 (paudley, 2026-09-22T22:55:33Z) | 17 | modal | The next bounded production run must still prove that the 631,136-fact summary completes under the same 24 GiB address-space ceiling before this defect is considered fixed. Principles 4, 7, 12, 15, 17, and 18 remain preserved. |
| C15.1 | comment 15 (paudley, 2026-09-22T23:51:40Z) | 30 | modal | The rebuilt producer is staged. The next bounded production run must measure the batched global refinement and confirm downstream reasoning/output behavior before this optimization is considered complete. Principles 4, 7, 12, 15, 17, and 18 remain preserved. |
| C18.1 | comment 18 (paudley, 2026-09-23T02:26:13Z) | 14 | modal | The remaining critical path is now explicit: every broad rule computes the same seeded join once to prune a head domain and again to publish its conditional support, then repeats those full scans on subsequent fixed-point iterations. The next change must fuse the exact prune/publication pass and make closure scheduling delta-aware; no all-head Cartesian approximation is involved. |
| C19.1 | comment 19 (paudley, 2026-09-23T03:17:15Z) | 7 | modal | I have corrected the scheduler to a deterministic FIFO queue: a self-rescheduled rule goes behind already-pending peers, while a per-rule queued bit prevents duplicates. The trace now carries `rule_index` and `visit` so the next production sample can distinguish one recursive rule from structurally identical rules. The full focused value-flow selection passes after this correction: 18 tests, including concrete fixed-point coverage, protected-grammar exclusions, compact rectangles, and the 4,096 x 4,096 dynamic-predicate regression (1.38 s execution for the selection). A new production build/measurement is still required before claiming the scheduler improvement. |
| C22.1 | comment 22 (paudley, 2026-09-23T06:38:33Z) | 12 | modal | The next exact narrowing seam is the property-rule guard abstraction. `FlowRule` currently carries the positive `analysis_body` but deliberately drops pure guards. For these rules that drops `CardinalityEqual(?count, 1)`, so value flow evaluates every cardinality row and then every broad head value. Any repair must preserve accepted cardinality lexical forms such as integer `01`; substituting one literal spelling would be unsound. The appropriate direction is to carry a pure cardinality-domain constraint using the same native count parser, then measure whether the rule head domains narrow before adding broader batching machinery. |
| C24.1 | comment 24 (paudley, 2026-09-23T18:32:40Z) | 25 | modal | A fresh authenticated producer build and capped production replay are still required before making a runtime claim. They have not started while the host is running the PurRDF SIMD frontier benchmark and unrelated Lillith compilation. No completion, full-gate, CI, or merge claim is made. |

## Comments that matched no rule

- comment 1 (paudley, 2026-09-17T05:29:42Z)
- comment 2 (paudley, 2026-09-17T05:50:11Z)
- comment 5 (paudley, 2026-09-17T07:01:47Z)
- comment 6 (paudley, 2026-09-17T09:06:52Z)
- comment 8 (paudley, 2026-09-22T16:45:15Z)
- comment 9 (paudley, 2026-09-22T17:57:26Z)
- comment 10 (paudley, 2026-09-22T18:42:40Z)
- comment 11 (paudley, 2026-09-22T18:46:23Z)
- comment 12 (paudley, 2026-09-22T20:00:23Z)
- comment 16 (paudley, 2026-09-23T00:46:12Z)
- comment 17 (paudley, 2026-09-23T01:41:29Z)
- comment 20 (paudley, 2026-09-23T04:08:41Z)
- comment 21 (paudley, 2026-09-23T05:49:01Z)
- comment 23 (paudley, 2026-09-23T07:31:50Z)
