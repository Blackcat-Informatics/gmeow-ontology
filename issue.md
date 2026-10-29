# Issue #2030: Use native JSON for the complete canon-admission persistence channel

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, logic, dependencies, rust, complexity-hard

## Body

Migrate the complete CanonAdmissionPublication persistence operation to native PurRDF JSON, including its actual pipeline publisher and BundleView reader. This is a complete writer/reader child of native JSON parents 1776 and 1777. Principles 4 and 7.

Scope and acceptance:

- Implement the five remaining writers and all nineteen required reader owners for the full publication/admission family, ReasoningContract, ComplexityClass and SemanticProfileId. Reuse complete existing native owners. SourceDocument/Base/Origin are not nested publication fields and require no new readers for this operation.
- Preserve complete canonical named-record output bytes, schema/profile/scope/selection checks, strict unsigned schema/indices, ordered maps/sets, vector order, external enum tags, optional fields and typed validation of every duplicate map value before last-wins replacement. Outer unknown fields remain refused; inner ignored fields remain ignored. Public byte APIs are unchanged.
- Use an explicit compiler-private observational AST entrypoint in the existing Term/Formula JSON owner, sharing typed variant/member decoding while retaining raw observational terms and formulas that checked authoring constructors reject. Keep the default checked readers unchanged. Preserve the existing literal component owner and direct named-field complexity labels. No shadow AST, generic local JSON walker, silent fallback or weakened authoring admission.
- Canonical records use named fields. Native reading deliberately refuses incidental positional struct-array forms accepted by Serde; freeze and document this admission tightening explicitly. Genuine tuples, literals and collection arrays remain supported. Preserve all other observed acceptance/refusal behavior selected by this operation.
- Never restore live compiler authority: checked_evidence is omitted on writing and always absent after hydration, including injected input. Other digests, row IDs, statuses, indices and AST metadata remain observations; hydration must not recompute or certify them. The separate live evidence hash keeps its existing direct streaming Serde owner and memory contract.
- Before production edits, capture complete original publication bytes and full observational roundtrips/acceptance probes in the existing canon-admission controls, including raw AST values, skipped authority, ignored/unknown fields, option forms and the intentional positional-record difference. Compare original producer bytes literally. Reuse the actual BundleView inert-archive reader control and retain its logic-compile.frontend code, native cause and decoding context on refusal.
- Verify existing checked AST admission controls, independent design/source review, focused actual-consumer strict compilation, selected authored controls, helper hygiene, static producer recipe and normal signed hooks. Complete delivery, qualification and landing must fit the eight-hour budget, including readiness work.

Standing limits: no local optimized producer, canonical corpus or fixture construction, sync or full gate. Selected controls use only tiny independently authored RDF and inert archive bytes; no fake shipped asset or authenticated corpus receipt. CI remains explicitly waived rather than passing. Installed/embedded, authenticated corpus, browser, release, the separate streaming commitment migration and all original parent acceptance remain open.


## Comments (0)

