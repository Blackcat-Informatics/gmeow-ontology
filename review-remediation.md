# Review remediation

This is the implementation author's assessment, not an independent completion audit. The captured submitted review contains nine actionable threads and 26 additional comments. Pagination is exhausted in the stored review and thread captures.

| Comment | Disposition |
| --- | --- |
| 4330949415 | Addressed: resolve equivalent output paths and symlinks, require distinct destinations, publish mandatory ledger first and use atomic file replacement. CLI regression cases cover lexical aliases, dangling symlinks and existing hard links. Installed CLI execution remains unrun locally. |
| 4330949466 | Addressed: enrich project/lift source and reference read/parse failures with the actual path and operation while retaining the original typed cause. CLI regression cases are authored; installed CLI execution remains unrun locally. |
| 4330949473 | Addressed: a selected STATO close correspondence adds both the native class and closeMatch predicate to the emitted loss boundaries. The synthetic case inspects the real ledger. |
| 4330949487 | Addressed: canonical integer parsing detects signed, zero-padded and whitespace zero denominators. The same correction covers inbound cube denominators. |
| 4330949505 | Addressed: variable names trim XML whitespace before validation and identity minting. A regression checks that x and spaced x share a declaration. |
| 4330949527 | Addressed native-ownership defect: a SKOS predicate on an external identifier no longer establishes a canonical symbol. Native typed mathematical subjects and unique exact alignments are accepted. Declined reverse closeMatch reconstruction because an approximate outbound correspondence cannot license exact native identity; this is documented and tested. |
| 4330949546 | Declined optional cache narrowing: the current complete mappings-product dependency is correct and required. The implementation selects the alignment graph for semantic input. Narrowing authenticated dataflow invalidation is an unmeasured optimization, not a correctness repair. |
| 4330949559 | Addressed: the loss-ledger example now refers to the target artifacts it records, with matching French and Chinese entries. |
| 4330949575 | Addressed with canonical qualified restrictions and closed-world required paths. Unqualified cardinality is forbidden by repository law; allValuesFrom plus maxQualifiedCardinality and explicit ClosureEntry express the intended one-source/one-target contract. |

Of the additional comments, 24 are LGTM. The numeric-range uncertainty was a real defect: literalValue is an object property to Number. Writers, readers, synthetic fixtures, CLI fixtures and the canonical producer example now use situated Number objects with an explicit number system and exactness. Exact computable numbers also carry RationalValue integer pairs; bare RDF numeric leaves are rejected. Named exact numbers retain their identity instead of deriving magnitude from labels.

The nested registry output question is not a cleanup conflict. generator_registry is metadata, not the executing DAG. all_output_paths sorts and deduplicates path descriptions and has no production caller; the live metadata CLI serializes records. Superset/orphan checking uses bundle projection keys, not first-match registry ownership. Therefore the math subtree cannot be deleted or shadowed by registry ordering. The source pipeline transports the actual math product through its declared carrier, archive and sink dependencies.

Strict producer-independent Clippy passed on the settled review fixes. All 24 focused tests passed. These do not prove authenticated bundle/fanout or installed-command acceptance. User instruction: "merge them one by one, ignoring CI." Hosted checks will not be awaited or called passes. The parent issue stays open for its full acceptance; no follow-up ticket substitutes for it.

All nine original threads have posted replies and are resolved. A refreshed review added four findings, all addressed in the final small change:

- 4331437321: atomic publication now preserves existing destination permissions. New files use ordinary creation permissions through tempfile Builder, letting the operating system apply the caller's umask without reading or changing it. Metadata failures other than absence hard-fail. The CLI regression checks both creation and replacement modes; local installed CLI execution remains unrun.
- 4331437350: duplicate symbol/variable regression coverage was consolidated, retaining the additional positive exact OpenMath reconstruction in the original test.
- 4331437353: the generated-ledger test positively requires the emitted record to be discoverable by the native lint's actual type selector, preventing a vacuous absence-of-errors pass.
- 4331437363: the source restriction uses logic:Thing and a separately qualified maximum, matching the module's existing generic object cardinality pattern. Mathematical expressions and rendering records are admitted independently of their distinction from MathematicalObject. Actual codec-specific source support is checked by the executing production path.

The settled numeric and first-review changes passed all 24 focused tests and strict producer-independent Clippy. The two changed test cases also passed after consolidation and the positive lint assertion. Production library code did not change after the strict pass. The CLI remains outside the authorized producer-independent build, and the ontology constraint still needs authenticated generated/corpus acceptance; these are explicitly unverified, not passes.

Refresh the review surface before merging. No independent dispatcher is available and no independent audit PASS is claimed.

The refreshed second review also contains 14 additional comments: 12 LGTM, one archive-transport concern explicitly refuted by the reviewer, and one uncertainty about ledger-first publication. That uncertainty is refuted by the actual math_interchange call sequence: write_math_bytes publishes the ledger before the target artifact, with distinct resolved paths. The document describes this ordering, not an unsupported two-file transaction guarantee. No further source change is needed for those additional comments.
