# Issue #1979: Use native JSON for complete CL ingestion observations

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Migrate the complete CL ingestion observation operation to native JSON: the actual cache, publisher, outer recorded outcome and authenticated reader, including both local observation records and every nested plain/annotated result.

The complete closure includes DerivedQuad and DerivationId; BudgetStatus; AnnotationCertification; AnnotatedFactKey and AnnotatedTupleKey; AnnotationDerivation<i64> and AnnotatedQuad<i64>; ProviderTupleSource; PreservationClaim and PreservationKind; and SemiringLaw, AnnotationQueryClass and all three AnnotationLineageContract variants. Carry all binary, positional tuple and provider lineage sources even when the present CL fixture leaves them empty. Reuse existing native RDF term, frontend diagnostic and complete recorded-cause owners and PurRDF's record/collection/variant mechanisms. Do not add a generic JSON bridge, RDF parser, annotation framework or second solver.

Preserve field order, exact strings, sorted sets, vector order, nested RDF term identity and existing enum wire names. Display, ontology and contract-hash spellings are separate projections. Every declared record field remains required; refuse positional arrays and duplicate known fields while retaining unknown-field tolerance. Annotation weights use the full signed i64 domain and refuse signed zero, fractional/exponent tokens and overflow. Generalize the existing strict signed admission helper and migrate its actual i32 callers rather than duplicate it.

Native admission must refuse contradictory graph and graph_component values and PreservationClaim's prohibited ValidationOnly polarity through actionable errors. Do not repair or normalize those inputs. Preserve the documented distinction between transport hydration and execution: no parsing, materialization, dictionary reconstruction or source/corpus discovery occurs during hydration. Retain shared Serde where genuine binary, CBOR or other JSON owners require it; remove obsolete local observation and annotation Serde only after the caller census supports removal.

Acceptance:

- The complete selected source operation uses native JSON from cache to authenticated reader, preserving failures and terminal cache refusal. No outer-only migration or lost lineage.
- Authored complete wire controls cover nonempty fact/tuple/provider lineage, exact RDF terms, all enum variants, signed boundaries, complete causal failures and malformed/invariant refusal.
- Relevant existing tiny authored materialization controls pass without discovering or producing corpus.
- Actual affected production consumers compile under strict diagnostics. Source ownership, real producer-recipe admission, independent design/source review and normal signed hooks pass.

The standing exception excludes local optimized production, corpus construction, sync and the full local gate. Actual authenticated corpus, installed/embedded and browser execution remain acceptance of parent #1776 and the release contracts; this complete source child does not close them. Principles 4, 7, 12, 15, 17 and 18 apply.


## Comments (0)

