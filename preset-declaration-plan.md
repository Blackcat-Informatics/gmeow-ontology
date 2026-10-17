# Distinguish preset declarations from applied probabilistic contracts

The full plan.md and issue 1770 remain authoritative. This increment repairs the concrete probabilistic-profile blocker without inventing a probability model or excluding required source. Principles 4, 7, 17, 18 and 19.

Observation: extract_contracts reads both ReasoningPreset and ReasoningContract owners and unconditionally applies the source-dependent probability-model requirement. The canonical ProbabilisticProfile is a reusable preset declaration, not an inference request. The existing classical proof admission already catalogues configurations as metatheory and does not execute them. Native probabilistic evaluation independently refuses unmodelled probabilistic facts.

Task 1: reproduce the canonical preset declaration refusal with a synthetic source using both structural typing spellings. Preserve the exact native preset ID and uncertainty facet in its compiled declaration. Prove real direct contracts and dual-typed preset/contracts still hard-fail without a model. Existing selected/default-graph support and malformed-facet cases remain required.

Task 2: apply the source-dependent model requirement to actual contract owners, including dual-typed preset/contracts. Preserve compatibility checking and unknown/malformed preset rejection. Explain the declaration/execution distinction in the governing design; add no ontology term, compatibility path, fake model or dependency.

Task 3: exercise the actual complete source-admission and FOF projection: preset plus ordinary domain facts reaches Complete; an applied probabilistic contract still blocks without its model; empty configuration-only source cannot manufacture an authorized theory. Run affected source/contract/FOF tests, native probabilistic refusal tests, strict producer-independent Clippy, source lint and normal hooks. Publish one coherent commit/PR with the unchanged full-goal requirements explicit. Independent reviewer tooling is unavailable; do not fabricate a review. Request concrete published CodeRabbit review.

Full corpus admission, prepared native canon, installed blinded proof behavior, exact production measurements and full hosted qualification remain required. Synthetic controls do not establish a new full-canon blocker count. Local optimized producer/fingerprint and machine probes remain forbidden. No test produces corpus.

## Source-grounded correction

The ontology makes ReasoningPreset a subclass of ReasoningContract. Distinguishing only explicit dual typing would make semantics depend on redundant type syntax and is rejected. Configuration parsing must instead remain independent of the runtime model selected by a query. Delete the graph-wide model lookup and its unconditional compile error; retain static compatibility/facet checks. Preserve direct, preset and dual-typed configurations as metatheory. Actual native probabilistic queries still refuse unmodelled facts through their selected-model guard. A runtime positive/negative pair proves an unselected model in the world cannot authorize the query, while explicit model selection produces the expected half marginal. Retain no duplicate declaration-time authorization path. Intermediate candidate logs do not qualify the final source.

Stage setup removed the prior generic local exclusion and exposed generated process receipts to authored-source lint. Keep the exact selected Stage directory locally excluded as separate evidence, following the existing ghprsq archival workflow. No source/hook/scanner rule is changed, and all receipts remain available for the separate merge archive.

## Final implementation decision

The ontology defines presets as contracts. Replace the earlier explicit-type distinction with the declaration/execution distinction: remove graph_declares_probability_model and its now-unused has_predicate helper, and remove the source-dependent model error from configuration extraction. Preserve all declaration facets and existing static compatibility checks. The final synthetic cases cover direct and dual-typed contracts, preset catalogue FOF output and refusal of an empty catalogue. The real probabilistic query pair proves that a ProbabilityModel merely present in the world does not authorize an unmodelled query, while explicitly selecting full independence gives the expected 0.5 marginal. This exercises the existing production evaluator; no model guard is deleted or bypassed.

The selected Stage directory is locally excluded from source scans and will be captured separately by ghprsq, consistent with the already-landed stack. The initial lint failure is retained and no hook/source checker is weakened.

## Exact configuration support and broader regression findings

The first expanded 218-case run passed 215 cases and exposed three failures. Two modal expectations still described the removed world-prepending representation and now assert all three canonical context/relation/argument coordinates. The third showed that admitted configuration support was simultaneously reasserted as object-theory facet typing. Record only the exact facet-type quads consumed by a successfully admitted declaration, before statement admission. Preserve arbitrary domain typings/properties of the same resource, reject excluded support as before, and retain a blocked empty catalogue. The support set is built once and queried by exact quad identity; no per-statement configuration rescan or broad vocabulary filter is introduced. The corrected final selection expands to all source/FOF tests plus native probabilistic runtime tests.
