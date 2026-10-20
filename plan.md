# Restore validated ownership at the documentation-to-model boundary

The shared production run completed with both cold generations and fixture-prefix failing at stage-export-pydantic: model definition identity collision for gmeow:Agent. Source inspection identifies two documentation entries: the canonical kernel definition and the bare logic:Class redeclaration in software. DocsModel currently assigns every typed subject to the module it was parsed from, even when the existing ownership report has validated a different canonical owner.

Principles 4, 7, 17 and 18 apply. Reuse native ownership analysis rather than a second ownership heuristic. Preserve the strict Pydantic identity-collision guard, mandatory metadata and the shared native renderer. No silent first/last winner, invented owner, source-coat deletion or output suppression is allowed.

The complete parent issue and seven comments were retrieved. Earlier routed-emitter work is merged; this increment repairs its observed production refusal. The parent stays open: real generated acceptance and the explicitly authorization-gated live Python import/schema lane are not established by synthetic Rust tests. This plan does not narrow that parent contract. CI is waived for merging this repair, not represented as passed. No local optimized producer, corpus/fixture production, synchronization or full gate is authorized. Normal signed hooks and ghprsq remain mandatory. No independent dispatcher is exposed; self-assessments will be labelled honestly.

## Completeness contract

| Requirement | Demonstration |
| --- | --- |
| Validated owner wins over a foreign reference | Build synthetic multi-slice DocsModel through the actual catalog/ownership path; one Agent entry with the real owner, label and definition, whether the owner sorts first or last |
| Unowned data remains diagnosable | Synthetic unowned declaration remains visible in the documentation inspection model; no invented validation status or silent disappearance |
| Conflicting ownership is not hidden | Two genuine owner claims remain visible and the Pydantic routing boundary still rejects the collision |
| Actual renderer succeeds | Synthetic owner/reference corpus passes DocsModel discovery, native SHACL compilation, real Pydantic routing/emission and README generation, with the correct owning module and metadata |
| Cache identity follows behavior | Advance DocsModel version and Pydantic implementation version; retain current source/manifest input admission |
| Existing failures remain hard | Run current missing-digest/owner/documentation and colliding-module controls unchanged |

## Task 1: Reproduce and correct ownership selection

Add independent synthetic owner/reference controls through DocsModel::from_slice_dir. Preserve authored dataset content. After module and PathShape term discovery, consult the already computed validated owner table once and retain only the canonical documentation entry for a term with validated ownership. Preserve unvalidated/conflicting entries for diagnostics; never guess an owner. Bump the documentation model version. The ownership table is indexed once, with no quadratic per-term search or repeated source parsing.

## Task 2: Verify the actual projection boundary

Add a synthetic routed-renderer control with the canonical class declaration and a foreign bare redeclaration in a different slice. Require the real emitter to return the owning slice module, canonical docstring, IRI and definition digest. Preserve and extend refusal tests for conflicting owners. Bump the Pydantic stage implementation identity. Run focused producer-independent Make selections, strict Clippy, source hygiene and static producer-recipe admission. No test may produce corpus.

## Task 3: Publish, integrate and clean

Commit normally, push and open one PR referencing the owner without closing it. Publish actual focused results and unrun corpus/Python acceptance. Address available review feedback and merge sequentially through ghprsq while ignoring CI. Verify archive/landing, remove only this owned branch/worktree and update the shared queue. Do not restart or cancel any hosted workflow.

## Prior art and rejected alternatives

The existing ownership analyzer validates rdfs:isDefinedBy against physical source; it already distinguishes references, conflicts and unowned terms. The existing docs model already receives its report but uses it only for dependency edges. Reuse that authority. Choosing the first documented entry, the richest prose or the term with a digest would hide genuine conflicts and remain order-sensitive. Removing the software declaration would repair only Agent and leave the same bug for other references. Weakening Pydantic collision checks would silently route metadata to a false owner. Keep those alternatives out of the implementation.
