<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Exact assignment and replacement evidence

## Semantic owners

The kernel owns `InformationObject`; coreference owns the existing
non-functional `supersedes`; provenance owns `wasDerivedFrom`; record-metadata
owns `statedRationale`; standpoint owns `accordingTo`. The new slice declares
those actual dependencies. It introduces no replica of the memory-records
observation role, no slot identifier, no copied bearer/value/endpoint fields and
no state-specific assertion-act superclass.

The grounding kernel owns the formal mechanisms. `logic:subClassOf` and the
class-local `logic:allValuesFrom` restriction express inheritance and the
transition's predecessor type. All procedural checks are canonical
`logic:Constraint` / `logic:Formula` trees. The six laws carry
`StateRecordIntegrityFailure` as their native failure class; diagnostics retain
the individual violated law. OWL and SHACL remain generated target views.

## Original RDF tuple authority

An assignment record positively binds its existing `rdf:reifies` object.
`termSameComponent` checks its subject, predicate and object components against
themselves, so an unavailable component cannot satisfy the assignment contract.
A second law compares every bound pair of reified terms by `componentWhole`;
a valid first tuple cannot conceal a different second tuple.

This reads the original native quoted term, including nested RDF objects. It
never asserts or trusts copied `reifiedStatementSubject`, predicate or object
fields. Reification remains distinct from assertion.

The underlying RDF authority is
[RDF 1.2 Concepts](https://www.w3.org/TR/rdf12-concepts/): RDF term identity and
triple terms preserve the actual tuple and literal metadata. The selected
bound identity intrinsic and its SPARQL target behavior are owned by
[LOGIC-VALIDATION](../../../grounding/logic/design/LOGIC-VALIDATION.md#bound-rdf-component-identity)
and its [authority ledger](../../../grounding/logic/design/LOGIC-REFERENCES.md#bound-component-identity-authorities).
This domain slice consumes that grounding; it does not add an external
correspondence or claim equivalence to an external state calculus.

## Canonical obligations

| Law | Required recorded evidence |
| --- | --- |
| `StateAssignmentTupleConstraint` | An existing reified triple with all three native components available. |
| `StateAssignmentTupleIdentityConstraint` | Every tuple of one assignment record has the same complete RDF identity. |
| `StateTransitionPredecessorConstraint` | An existing `supersedes` edge joins this transition to a typed predecessor assignment; both original tuples agree in subject and predicate. |
| `StateTransitionSlotIdentityConstraint` | Every supplied predecessor is a distinct record and every successor/predecessor tuple pair agrees in subject and predicate. |
| `StateSupersessionEdgeConstraint` | The original edge tuple agrees in all three positions with an existing `supersedes` edge between a typed successor transition and predecessor assignment. |
| `StateSupersessionTupleIdentityConstraint` | Every tuple of one supersession record has the same complete RDF identity. |

The transition's local all-values restriction applies to every predecessor;
the assignment laws therefore cover its tuple presence and uniqueness. The
universal position law checks all supplied predecessors, so one valid joined
witness does not conceal another predecessor on a different bearer or property.
The successor value need not equal any predecessor value. The local law forbids
self-replacement; no global acyclicity or complete-history claim is made here.

The edge law compares the quoted subject with the bound actual successor, the
quoted predicate with the exact `supersedes` IRI, and the quoted object with the
bound actual predecessor. Reversed edges, unrelated relations between the same
records and unsupported reified scalars cannot satisfy these comparisons.

## Native joined support

The existential assignment, transition-predecessor and supersession-edge laws
contain bound component identity and select the existing joined-support profile.
Their positive stored binary patterns bind the focus inside the witness and
bind every identity operand. Only the focus correlates with the outer guard.
Each existential binder is fresh throughout its law. These laws require existing
witnesses; they never create missing records or bind an output through a
component accessor.

Universal identity laws bind their operands positively in their guards. Native
RDF identity preserves exact term components and false-on-unavailable behavior;
`termEqual` value coercion is not substituted. The selected native and SPARQL
lowering contracts, private support identity and explicit refusal boundaries are
specified in
[positive existential integrity joins](../../../grounding/logic/design/LOGIC-VALIDATION.md#positive-existential-integrity-joins)
and [LOGIC-RUNTIME](../../../grounding/logic/design/LOGIC-RUNTIME.md).
This is a source-grounding claim; no execution or generated-surface result is
asserted by this design document.

## Edge-specific duration

The event owner's `Duration` quantity and persona owner's `transitionLasted`
have different existing contracts. `supersededStateLasted` is consequently a
new datatype property whose domain is only `StateSupersessionClaim` and whose
range is `xsd:duration`. It creates neither an Event nor a second quantity
ontology. It has no mandatory value or functional winner: absent duration
remains absent, and distinct attributed claims remain distinguishable.

A supplied computed span keeps derivation on the exact duration-assertion
reifier, with actual opening and closing assignment inputs through
`wasDerivedFrom` and the source's calculation/input roles through
`statedRationale`. The optional literal reports a duration; it does not execute
or verify the source's arithmetic. The strict persona profile is unchanged.
