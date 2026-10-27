# Issue #2028: Use native JSON for complete semantic-unit identity preimages

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, logic, rust, complexity-hard

## Body

Use native PurRDF JSON for the complete semantic-unit identity preimage produced by AdmissionBuilder::push. This is a bounded identity operation under native JSON parents 1776 and 1777. Principles 4 and 7.

Scope and acceptance:

- Implement all seven missing native writers: SemanticUnitKind, SourceAssertion, PropertyCharacteristic, SourceClassDefinition, SourceContractDeclaration, AdmittedSemanticUnit and SemanticUnitDisposition. Cover all ten kind tags, all five admitted payload variants and all eight dispositions, including their full nested fields and empty containers. Reuse complete existing AtomicTerm, Formula and ReasoningContract writers.
- Route the actual builder through native compact encoding of the complete four-cell tuple: kind, anchor, documents, disposition. Preserve the gmeow-semantic-unit-v1 domain, exact compact preimages and resulting row IDs. Retain declaration order, external Rust variant tags, sorted maps/sets, vector order, raw strings and distinct program indices. Shared Serde and current infallible APIs remain intact.
- Before production edits, capture complete original tuple preimages and actual original unit IDs from the existing configuration_declarations_preserve_facets_without_switching_proof_semantics control. Add a bounded original-byte atlas for every local variant and complete nested records, including empty/nonempty constants, facts and documents, raw Unicode/NUL and distinct indices. Preserve the existing object-theory separation, configuration and projection-evidence assertions. Compare frozen references literally; no replacement-derived expected hashes or normalization.
- Retain the existing persisted-admission roundtrip control. Leave the live admission streaming evidence commitment, publication/hydration and authority checks unchanged. Row identity must not depend on a new serialization profile or version.
- Verify complete owner/caller closure, independent combined review, focused strict actual-consumer compilation, the existing authored controls, helper hygiene, static producer recipe and all normal signed hooks. Record tested scope and limits.

Standing limits remain: no local optimized producer, canonical corpus or fixture construction, sync or full gate. Only tiny independently authored inputs may run. No fabricated shipped asset or authenticated receipt. CI remains explicitly waived rather than passed. Complete native admission publication/hydration and all original parent production, installed consumer, authenticated corpus, browser and release acceptance remain open.


## Comments (0)

