# Issue #1975: Use native JSON for complete diagnostic observations

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

## Behavior

Replace the complete diagnostic-observation JSON channel with existing PurRDF native JSON mechanisms. Preserve all four observation records and the complete eight-field MetaDerivation, including internal traces, root and cluster evidence, directed glut edges, gate coordinates, category blocking, projections and full recorded failures.

## Production closure

Migrate every diagnostic meta-fixture, gate-matrix and report cache through the existing native cache operation; write the aggregate channel natively; migrate its authenticated reader and the existing direct MetaDerivation roundtrip. Keep rule execution, RDF projection, source selection, recorded-result semantics and output field ordering intact. Remove obsolete Serde derives only after confirming no remaining binary or JSON owner.

## Acceptance

- Complete native records preserve exact current compact JSON for admitted values and preserve set/map ordering and directed edge identity.
- Fixed two-cell meta edges and three-cell gate coordinates refuse missing, extra or wrongly typed cells with contextual errors. Fixed records reject duplicate known fields and positional struct arrays.
- Existing native cache coordination, full recorded diagnostic causes and authenticated reader boundaries remain intact; tests never produce the corpus.
- Focused authored component controls, affected strict diagnostics, ownership checks, independent source review and normal signed hooks pass. Actual authenticated corpus execution remains parent acceptance under #1776 and is reported accurately under the standing local-production restriction.

This independently useful source transport delivery does not close #1776, #1336 or the validator witness acceptance in #1973. No new parser, generic serialization bridge, reasoning implementation or corpus production is in scope. Principles 4, 7, 12, 15, 17 and 18 apply.


## Comments (0)

