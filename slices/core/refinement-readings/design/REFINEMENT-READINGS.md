<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Scoped relational findings and reading limits

`FirstOccurrence` and `RelationOccasion` inherit their actual ontological sort
through `Refinement` and `Observation`. They reuse source identity,
versioned-process vantage and outcome rather than introducing a parallel
reading engine. Milestone and limit aggregates inherit `InformationObject` and
remain attributed annotations. Shared relation and scope properties have no
global domain that would turn those annotations into observations.

A `ReadingScope` records the exact offered inputs. Source chronology references
an existing `TemporalFrame` through a true subproperty of `hasReferenceFrame`;
it does not misuse transaction concurrency protocols
or turn coarse dates into timestamp instants. Original source assertion time,
refiner observation time and record transaction time remain distinct.

The mathematical set and membership APIs were inspected. `math:hasElement`
declares mathematical objects as its members, so arbitrary source records are
not forced into that sort merely to give their input enumeration a set label.
The scope enumeration is operational provenance, not a new mathematical set
calculus. Likewise, a language-bearing `findingSummary` is text, not an invented
lexeme, lexical sense or analyzed `lang:SurfaceForm`. Native tagged and directional
literals preserve language without making an unsupported linguistic analysis.

`ReadingLimit` is a SubKind of the existing `QueryLossLedger` and uses its
canonical `ledgerLoss` membership directly. It does not specialize `QueryLoss`:
a zero-count aggregate must not fabricate an unavailable input.
Its reason vocabulary genuinely specializes `QueryLossCause`; actual members
reuse `lossConcerns`, `lossCause` and the existing ledger identity. The selected
refiner can report inability to correlate particular RDF terms without making
that limitation a property of RDF 1.2.

Canonical domains, ranges, local value restrictions and required paths supply
the structural grounding. Local single-value restrictions apply only to fields
that define one relational reading or aggregate group; there are no new global
functional declarations. Multiple attributed reports remain separate records.
Eight realized integrity formulas enforce resource-or-name presence, actual IRI
bindings, source scope membership, loss scope/cause coherence, empty zero counts
and retained evidence for positive counts. They use targetable outer universal
guards and supported native term predicates (Principle 17).

Earliest-in-source status and the exact distinct-record count are attributed
consumer computations. No formula claims to execute source relation selection,
repair missing assertion times, certify global absence or verify count arithmetic
beyond the explicit intrinsic checks. No parser, ordering executor or statistical
framework is added.

External grounding belongs to the logic, language and mathematics owners under
[the grounding contract](../../../../docs/GROUNDING.md). Existing upper sorts and
provenance/observation relation owners supply the applicable bridges. Scoped
first-occurrence claims and zero-capable limit aggregates have no justified
exact external equivalence from this source review; no downstream duplicate
alignment or unsupported preservation claim is authored (Principles 5 and 19).
