# Issue #2011: Preserve typed validation failures in complete rationale literal transport

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

The live crate check reports a string error return in the documentation model's competency rationale literal adapter. Its shared validator also stringifies the native datatype IRI failure before the serialization boundary.

Make the complete literal transport validation operation return `gmeow_errors::Diag`, retaining a private typed source for native datatype failures, native component refusals and invalid direction tokens. Preserve the native IRI error as a causal source and retain the native component API's actual static refusal text. Continue using the existing native validators in the same order; do not implement a second RDF validator or normalize the original literal fields. Principles 4 and 7.

Acceptance:

- Wire both existing Serde callbacks through the typed validation operation. Keep their existing four named fields, nullable facets, field order, original lexical bytes and admission semantics. Invalid datatype serialization and deserialization must both fail.
- Keep `Diag` as the first-party result error. Use a private typed source through the existing conversion path; do not introduce a new diagnostic code, weaken a census, or repurpose an unrelated failure class. The generic Serde `Error::custom` boundary may retain only display text; do not claim end-to-end native cause preservation through arbitrary serializers.
- Extend the existing malformed transport control to prove native IRI cause retention before that boundary and malformed datatype emission refusal. Retain the existing exact UTF-8/null wire control and all malformed admission inputs. No generic RDF conformance suite or corpus production.
- Run focused strict compilation of actual host consumers, the two authored transport controls and applicable normal hooks. Compile-only developer CLI qualification may support the existing read-only crate check; no optimized producer, canonical corpus, synchronization or full gate.

Remaining contract: #1761, including the other source-policy findings, all authentic browser attestations and full gate acceptance. This child does not claim the whole crate check passes. CI is waived under the standing user instruction.


## Comments (0)

