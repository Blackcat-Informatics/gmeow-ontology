# Issue #1977: Use native JSON for complete GMN dictionary observations

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

## Behavior

Migrate the complete GMN dictionary observation channel to native JSON, from its actual publisher through the authenticated reader and cost-feed consumer. Preserve all ten local observation types, every typed GMN/ingest success or failure, raw glyph membership, record/scoped/process observations, costs, stamps, notation and the complete recursive grammar tree.

## Dependencies and boundaries

Implement native codecs at the existing GMEOW owners for Gmn1Error, UncoveredTerm, StandaloneReport, QuadCoverage and construct categories, GrammarRule and all RuleExpr variants, IngestDiagnostic and LangFailure. Reuse existing PurRDF records, collections, numbers and variants, and the existing RecordedDiag codec. Preserve genuine binary or other protocol Serde owners; do not add a generic bridge, parser or alternate dictionary implementation. The existing validated dictionary packet codec is a separate native owner and remains intact.

Hydration preserves the observed grammar tree verbatim; it never reruns canonicalization, source parsing or dictionary validation. Preserve declaration order, exact strings, sorted containers, nullable options, fixed two-cell acceptance coordinates, full recorded causes and all typed refusal payloads. Fixed records reject duplicate known fields and positional arrays, with existing unknown-field tolerance retained.

## Acceptance

- Complete named records and recursive variants roundtrip with expected compact wire identity and precise malformed-input refusal, without losing fields or changing tree/operand order.
- All publisher, authenticated reader and cost-feed consumers use the intended native implementation, with no outer-only migration.
- Independently authored tiny complete observation and nested-family controls protect wire fidelity, typed errors, exact numeric domains and recursive grammar branches; tests never produce the corpus.
- Actual affected production callers compile under strict diagnostics; ownership, source admission, independent review and normal signed hooks pass.

Actual authenticated corpus, installed/browser execution and the full dependency migration remain acceptance of parent #1776 under the standing local-production restriction. This complete source operation does not close #1776 or the full GMN release issues #1858 through #1861. Principles 4, 7, 12, 15, 17 and 18 apply.


## Comments (0)

