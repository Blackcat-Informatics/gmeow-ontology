# Quality semantic review

Reviewed child #1893, the selected plan, current quality module additions, mathematical result contracts, constraint admission and SHACL projection implementation. Read-only source review; no build, parser, generated projection or corpus execution was performed.

## Blocking finding

The current AggregateQualityResultsConstraint does not have a supported executable SHACL lowering. Its consequent conjoins existential requirements with nested universal implications. In crates/logic-compile/src/projections/shapes.rs, lower_negative(And) finds bare FILTER groups from the existential requirements, selects its all_scoped=false branch, and calls lower_positive on the entire conjunction. lower_positive rejects the nested Forall. try_project_block therefore fails and project_procedural_constraints omits the block, retaining unsupported residue instead of enforcement.

Repair without changing the generic compiler: keep presence/typed-existence/target requirements together in one existential-only guarded constraint; place all supplied value typing and all supplied estimator typing in separate top-level universally guarded constraints. Their guards should bind assessment, result and each value/procedure before requiring its mathematical type. The actual producer-selected procedural product must contain every constraint and execution must catch a malformed extra value or estimator even when a valid sibling exists.

## Semantic assessment

The two explicit subkinds are coherent. Base categorical results remain legal; QualityDimension remains an open nonfunctional vocabulary; results retain their own dimensions without imposing fixed physical dimensions on broad ISO categories or pairing unrelated axes and results. Assessment/Estimate separation respects math:Estimate disjointWith gmeow:Observation. Required observationResult paths are correctly expressed through allValuesFrom plus class-scoped ClosureEntry, without functional restrictions. New classes and the named constraint are owned by quality and annotated.

Imported math:Quantity validation remains necessary: quality allValuesFrom alone does not reject a typed Quantity missing its dimension. Tests must use retained quality plus math shapes, preserve input subclass closure where needed, and demonstrate missing result, wrong result, missing/mistyped value, missing/mistyped estimator, missing target, malformed extra results/values/procedures, categorical base and multivalued cases. No tests may regenerate projections or corpus.

## Limits and disposition

Request changes for the concrete projection blocker. Documentation, examples, translations, structural/competency assertions and read-only consumer tests are still being authored by the root and were outside this initial review snapshot. Source review does not qualify generated or corpus acceptance; those remain visibly unrun under the confirmed campaign exception.
