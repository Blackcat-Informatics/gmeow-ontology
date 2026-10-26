# Issue #1982: Use native JSON for complete logic vocabulary observations

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, logic, dependencies, rust, complexity-hard

## Body

Migrate the complete logic-vocabulary observation operation to PurRDF native JSON. The private Observation record carries source, members and preset_facets. Preserve the entire members map of recorded set outcomes and the independent recorded map-of-sets facet outcome, including complete diagnostic causes.

Update the actual CompileLogicStage publisher, the selector-authenticated vocabulary reader and both tiny authored test readers. Reuse native records, string maps/sets and the existing complete RecordedDiag outcome owner. Remove this record's obsolete Serde derives; there is no additional binary or nested compiler-IR owner. The stage cache retains the published artifact bytes.

Keep every record field required, tolerate unknown members, reject duplicate known fields and positional arrays, preserve declaration order and sorted containers, and retain last-wins map behavior while validating every supplied value before replacement. Preserve all source identities, default-graph selection, named-IRI typing, observation algorithms and thirteen authenticated vocabulary assertions. No generic JSON/RDF parser, source extraction or compatibility layer.

Acceptance:

- The complete publisher-to-authenticated-reader operation uses the intended native codecs, including both levels of recorded refusal.
- Both existing authored tiny RDF controls pass. Complete literal wire and causal-outcome controls cover sorted sets/maps, malformed records and malformed map entries even when a later duplicate would otherwise replace them.
- Actual pipeline and developer CLI callers compile under strict diagnostics. Source admission, helper ownership, independent review, formatting and normal signed hooks pass.

Tests never construct or discover corpus; corpus_logic_vocabulary_surface remains reserved for authenticated producer output. The standing exception excludes local optimized production, corpus construction, sync and the full gate. Actual authenticated corpus and installed/browser release acceptance remain unverified. Remaining contracts: #1776 and the full logic/release contracts. Principles 4, 7, 12, 17 and 18 apply.


## Comments (0)

