<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# State records

This slice retains property-named state assignments and their replacement
history. `StateAssignmentClaim`, `StateTransitionClaim` and
`StateSupersessionClaim` are information objects. They preserve what a source
recorded without making the record an occurrence or its quoted content true.
Principles 2, 4, 9, 10, 17 and 19 govern the model.

## One original assignment tuple

A `StateAssignmentClaim` is a native RDF 1.2 reifier of exactly one assignment:

```turtle
ex:priorA a gmeow:StateAssignmentClaim ;
    rdf:reifies <<( ex:agent ex:mode ex:quiet )>> .
```

The original subject is the bearer, the predicate IRI names the property, and
the original object is its assigned value. No slot resource, `slotBearer`,
`positionInSlot` or copied value field is required. The property IRI alone is
not the identity of a state position: the original bearer must also agree.
Objects retain native RDF identity, including literal lexical form, datatype,
language, direction, blank-node scope and recursive quoted triples.

Reification does not assert the quoted assignment. Several reifiers may retain
separate source or standpoint claims about the same tuple. Distinct values may
coexist without selecting a preferred winner. `accordingTo` retains an actual
standpoint, and the existing provenance relations retain source derivation;
this slice supplies neither a default author nor a fabricated source.

## Several replaced assignments

A `StateTransitionClaim` is itself the successor assignment. It links directly
through the existing non-functional `supersedes` relation to every predecessor:

```turtle
ex:next a gmeow:StateTransitionClaim ;
    rdf:reifies <<( ex:agent ex:mode ex:ready )>> ;
    gmeow:supersedes ex:priorA, ex:priorB .
ex:priorB a gmeow:StateAssignmentClaim ;
    rdf:reifies <<( ex:agent ex:mode ex:waiting )>> .
```

The transition needs at least one replaced assignment. An initial assignment
with no predecessor uses `StateAssignmentClaim`. Every predecessor must be an
assignment for the same original bearer and property. A transition can therefore
consolidate several distinct values or source records. Neither matching values
nor related property IRIs substitute for exact bearer and property identity.
Self-replacement fails the local position law.

Replacement order comes from the explicit edges. It is not derived from source
clocks, a global counter or a total ordering of concurrent records. This slice
does not compute a globally current value, claim a complete history or impose
an application-wide state machine. Predecessor records remain available.

## Evidence and duration belong to an edge

`StateSupersessionClaim` reifies one exact replacement edge:

```turtle
ex:edgeA a gmeow:StateSupersessionClaim ;
    rdf:reifies <<( ex:next gmeow:supersedes ex:priorA )>> ;
    gmeow:supersededStateLasted "PT15M"^^xsd:duration .
```

The original subject and object supply the actual successor and predecessor;
there are no copied endpoint fields. Its predicate must be `supersedes`, and
the edge must also be present between those actual typed records. Quoting an
edge is insufficient evidence that replacement was asserted. Evidence for
`priorB` belongs on a separate edge reifier even though its successor is the
same transition.

`supersededStateLasted` is optional and applies to the replaced assignment on
that edge. Retain a supplied measured duration or a supplied computed duration.
For a computed span, reify the exact duration assertion, use `wasDerivedFrom`
to cite the actual opening and closing assignment claims, and retain the source
calculation and each input's role with `statedRationale`. GMEOW does not invent
an omitted duration, timezone or precision, subtract timestamps or certify
arithmetic. Distinct attributed duration claims may coexist.

## Existing profiles keep their meaning

The strict persona `ModeTransition` profile retains its `PersonaSlot`, functional
`transitionFrom` and `transitionTo`, and persona-specific `transitionLasted`.
The new generic records neither loosen nor inherit those fields. A consumer
selects the model that describes its actual record rather than casting a
property IRI as a `PersonaSlot` or a duration-bearing record as an Event.

The epistemics `ClaimToken` remains the assertion act, an Event/Activity.
It is not a superclass of these information records. The memory-records
`MemoryItem` remains its observation role; storing a state record in memory
does not identify the record with the observation that remembers it.

[The design](design/STATE-RECORDS.md) states the canonical laws and their
native grounding. No authored SHACL surface or external state ontology supplies
a second semantic authority.
