# Current qualification

Observed production failure: the completed shared workflow run failed both cold generations and fixture-prefix at stage-export-pydantic with a model definition identity collision for gmeow:Agent. The kernel owns Agent; software contains a bare class redeclaration. The documentation model incorrectly turned that reference into a second owning term entry.

- PASS: make fmt.
- PASS: eight focused synthetic cases through make nextest-synthetic, selecting the three new documented_terms controls and all producer-independent Pydantic controls. Both owner/reference orders preserve the canonical owner, label and definition. Genuine conflicts and unowned entries remain diagnosable. The real native routed emitter receives only the canonical class entry and preserves cross-slice references, metadata, documentation and README. Missing metadata, colliding slice modules and conflicting term owners still fail. The stage dependency contract also passes. Build completed in 5m36; all eight tests passed in 0.037s.
- PASS: make clippy-prebuild with warnings denied across all producer-independent targets. No warnings or lint allowances were added.
- PASS: make lint, including the corpus-producer scanner, source policy and all remaining hygiene hooks.
- PASS: make producer-recipe, the static source/recipe admission boundary; it did not build or execute the optimized producer.
- PASS: staged whitespace check. The normal signed commit hooks are recorded by the actual commit output.

The fix indexes the existing validated ownership report once, before documentation indexing. Foreign references are not published as owning definitions. Unvalidated/conflicting claims remain visible; no first/last winner, richer-prose heuristic or duplicated ownership analyzer is introduced. The Pydantic collision guard is unchanged. DocsModel and Pydantic stage versions advance to invalidate stale semantic output.

No local optimized producer, corpus or fixture production, synchronization, full make check, generated Python package refresh or live Python import ran. The hosted producer failure is not represented as a subsequent pass. CI is waived for integration under the instruction to merge sequentially while ignoring CI. The parent issue remains open for real generated and Python import/schema acceptance; the repair does not redefine its contract. No independent dispatcher is exposed and no independent execution audit is claimed.

Final integration assessment: origin/main was fetched and git merge-tree computed a clean candidate with no intervening base-only changes. The four intended source files are committed and pushed; the normal signed hooks all passed. There are no conflicts or source synchronization requirements. The complete review/thread query returned no findings and no further pages. Existing focused qualification applies to the unchanged integration candidate. CI remains waived rather than passed evidence.
