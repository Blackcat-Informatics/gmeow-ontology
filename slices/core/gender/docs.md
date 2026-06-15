<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Gender — self-asserted identity facets, never inferred, never erased

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/gender` · **tier: core**
> The shared `IdentityFacet` base, and the gender-identity / gender-expression /
> sex-assigned-at-birth axes of the seven-axis orthogonality matrix.

Most schemas have a `gender` column: an enum, single-valued, often silently inferred from a
title or a pronoun. GMEOW refuses every part of that. A gender is a **reified, self-asserted
claim** — an `IdentityFacet`, the same relator idiom as names' `NameUsage` — never a bare
property; the value space is an **open vocabulary of individuals**, never an enum and never a
`Person` subclass (Principle 9: inclusive without overtyping); a person bears **many co-equal
facets** with deliberately *no* primary/preferred term, because a primary-gender slot encodes
the same hierarchy a primary-name slot does (Principle 9, anti-colonial in both directions —
the seeds neither force Western categories nor appropriate culturally-specific ones:
Two-Spirit carries a scope note restricting it to self-referential Indigenous North American
use). **Self-assertion is the top authority** (Principle 9): a facet the person asserted
outranks any registry or import, and is never silently overwritten.

A superseded label is **suppressed, never erased** (Principle 10): `gmeow:displayable false`
— the deadname analog — keeps the history while preventing the leak. And none of this is
optional: identity slices sit in **core by commitment** (Principle 16) — GMEOW refuses to
make respectful identity an extension someone might not load.

## The seven-axis orthogonality matrix

Seven identity axes span three modules — **GenderIdentity**, **GenderExpression**, and
**SexAssignedAtBirth** here; **SexualOrientation** and **RomanticOrientation** in the
sexuality slice; **PronounSet** and **Honorific** in names — and they are **pairwise
disjoint with no inferential bridge in any direction**. Address ≠ identity: pronouns say how
a person is *addressed*, never what they *are*. Sex ≠ gender: sex-at-birth is a recorded
administrative datum, not an identity. Nothing infers expression from identity, orientation
from gender, or anything from anything. The disjointness is a reasoner **theorem**
(`owl:AllDisjointClasses` over both the facet classes and their value spaces, OWL 2
EL-visible — relator-mediation doctrine); the bridge-*absence* half, which OWL cannot say, stays a
closed-world lint (`tests/test_identity_orthogonality.py`).

## The facet base

### gmeow:IdentityFacet

The shared root: a reified, self-asserted claim about an aspect of a person's identity — a
`gufo:Relator` *and* a `gmeow:Observation` in the universal claim stack (observation-spine bridge),
mediating the person (`facetSubject`) and an open identity value, with the asserting agent
as `facetVantage`. Carries the optional `validFrom`/`validUntil` clocks and the
`gmeow:displayable` control. The sexuality slice's two orientation facets subclass this same
base.

### gmeow:selfAsserted

The authority marker: `true` when the person asserted the facet themselves (the top
authority), `false` for a third-party record. Non-functional — a multi-source merge carries
both, coexisting rather than contradicting (Principle 9's standpoint stance). Also usable as
a statement-level RDF 1.2 annotation on a quoted claim.

## The three axes in this slice

### gmeow:GenderIdentity · gmeow:hasGenderIdentity · gmeow:genderValue

What a person **is**. `hasGenderIdentity` is non-functional — bigender is two co-equal
facets, genderfluid and transition are facets over time — and MUST NOT be inferred from
pronouns, honorifics, expression, or sex-at-birth. `genderValue` is functional *per facet*
(one value each; multiplicity is more facets) and is the **single path** to the value: there
is deliberately no flat `gmeow:gender` shortcut to grow stale.

### gmeow:GenderExpression · gmeow:hasGenderExpression · gmeow:expressionValue

How a person **presents** — a separate axis: a masculine-presenting woman and a
feminine-presenting man are directly expressible with no tension, because expression is
never derived from identity (or vice versa). Same shape: non-functional bearer property,
functional-per-facet value path to an open `GenderExpressionStyle`.

### gmeow:sexAssignedAtBirth

A **recorded administrative datum** — not an `IdentityFacet`, not self-asserted, never
equated with or implied by gender identity/expression. Non-functional so a correction and a
prior record coexist (multi-source honesty). Historical/genealogical records keep external
`gedcom:sex`; this is GMEOW's native, value-vocabulary-backed term.

### gmeow:intersexVariation

An optional free-text note for inclusivity where `gmeow:saabIntersex` alone is insufficient.
Deliberately a plain note: GMEOW does not model clinical sex characteristics or a DSD
taxonomy — that depth would be overtyping in exactly the sense Principle 9 forbids.

## The value vocabularies

### gmeow:Gender · gmeow:GenderExpressionStyle · gmeow:SexAssignedAtBirth

Open value vocabularies of individuals under `gufo:QualityValue` — never `Person`
subclasses, never a closed enum. The seeds (woman, man, non-binary, agender, genderfluid,
genderqueer, bigender, demigirl, demiboy, Two-Spirit, questioning; feminine through neutral;
the four coarse sex-at-birth values) are **anchors, not a fence**: an identity not seeded is
a *fresh individual* carrying `rdfs:label` — the names module's custom-`PronounSet` idiom —
never a flat literal. The three value spaces are themselves pairwise disjoint, so a raw
value can never cross axes.

```turtle
ex:robin a gmeow:Person ;
    gmeow:hasGenderIdentity ex:facetNb , ex:facetPrior .

ex:facetNb a gmeow:GenderIdentity ;            # current, self-asserted
    gmeow:genderValue gmeow:genderNonBinary ;
    gmeow:selfAsserted true .

ex:facetPrior a gmeow:GenderIdentity ;         # superseded — suppressed, kept
    gmeow:genderValue gmeow:genderMan ;
    gmeow:displayable false .                  # Principle 10: never deleted
```

## Boundaries, solver, and alignment

Closed-world cardinality ("exactly one value per facet", "exactly one subject") is
deliberately **not** OWL — it is SHACL's job (phase 3, SHACL closure gate), keeping the reasoned core
EL-friendly (Principle 12's boundary discipline: the ontology states the relata, the gates
check the counts). Alignment is lossy by design: GSSO, Homosaurus, Wikidata, schema.org,
FOAF and HL7/FHIR mappings live in `mappings/gmeow-gender.sssom.tsv`, and flat targets like
`schema:gender` receive a documented downcast — the reified, self-asserted, co-equal
machinery is canonical and survives only here (Principle 4). The full rationale is in
[`identity-mapping.md`](../../../docs/identity-mapping.md).

## Dependencies

Depends on `kernel`, `entities`, `names` (the displayable control and the relator idiom),
`observations` (the claim stack), and `sexuality` (the matrix axioms span both). Consumed by
every person-bearing dataset: identity facets on persons are core by commitment
(Principle 16).
