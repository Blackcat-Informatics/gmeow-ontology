<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Sexuality — split-attraction orientation on the shared facet base

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/sexuality` · **tier: core**
> Sexual and romantic orientation as two separate, self-asserted, co-equal identity axes.

Where most models offer one `sexualOrientation` slot (if they offer anything), GMEOW applies
the **split-attraction model**: sexual orientation and romantic orientation are *separate,
mutually independent axes*, each a reified self-asserted facet on the gender slice's
`gmeow:IdentityFacet` base. Asexual-biromantic, aromantic-bisexual, and every other
combination are directly expressible because the axes never collapse into each other — and
neither is ever inferred from gender identity, gender expression, sex-assigned-at-birth,
pronouns, or honorifics. These two axes complete the **seven-axis orthogonality matrix**
(with gender's three and names' two), whose pairwise disjointness is a reasoner theorem
asserted in the gender module (issue #38) and whose bridge-absence is a closed-world lint
(`tests/test_identity_orthogonality.py`).

The governing tenets are exactly gender's, because the base is shared (Principle 9):
**self-assertion is the top authority** (`gmeow:selfAsserted`); facets are **co-equal** with
deliberately no preferred/primary orientation term; value spaces are **open vocabularies of
individuals**, never per-value subclasses, never a forced enum — inclusive without
overtyping. A superseded label is kept with `gmeow:displayable false`, **suppressed, never
erased** (Principle 10). And like all identity slices, this one is **core by commitment**
(Principle 16): orientation modelling is not an opt-in extension.

## The two axes

### gmeow:SexualOrientation

A person's self-asserted pattern of *sexual* attraction, as a reified facet (a
`gufo:Relator` and `gmeow:Observation`, inheriting `facetSubject`/`facetVantage`, the
validity clocks, and `displayable` from `IdentityFacet`). A separate axis from romantic
orientation and from every gender axis; nothing infers one from another.

### gmeow:hasSexualOrientation

The bearer property, Person → facet. Non-functional and contextual: identities shift, and
multiple co-equal facets coexist — a superseded one is suppressed, never deleted. MUST NOT
be inferred from `hasRomanticOrientation`, `hasGenderIdentity`, `sexAssignedAtBirth`,
pronouns, or honorifics.

### gmeow:sexualOrientationValue

Functional **per facet** — one value each; multiplicity is expressed by more facets. The
**single path** to the value (no flat-literal shortcut exists, deliberately): a seeded
individual, or a fresh `SexualOrientationValue` individual with `rdfs:label` when none fits.

### gmeow:RomanticOrientation

A person's self-asserted pattern of *romantic* attraction — the other half of the
split-attraction model, structurally a twin of `SexualOrientation` but a fully independent
axis: asexual yet biromantic is two facets on two axes, no tension, no inference.

### gmeow:hasRomanticOrientation

The romantic bearer property; same contract as its sexual sibling — non-functional,
contextual, suppression-not-deletion, and no inferential bridge to any other axis.

### gmeow:romanticOrientationValue

Functional per facet; the single path to an open `RomanticOrientationValue`.

## The value vocabularies

### gmeow:SexualOrientationValue · gmeow:RomanticOrientationValue

Two **separate** open vocabularies under `gufo:QualityValue`, kept apart so the
split-attraction distinction is explicit in the data (asexual ≠ aromantic — they are
different individuals in different value spaces, and the spaces are disjoint by the matrix
axioms). Seeds: heterosexual / homosexual / bisexual / pansexual / asexual / demisexual /
queer / questioning, and their romantic counterparts plus queerplatonic. The seeds are
**anchors, not a fence**: mint a fresh individual with `rdfs:label` for anything unseeded —
never a flat string, never a new subclass.

```turtle
ex:sam a gmeow:Person ;
    gmeow:hasSexualOrientation ex:samSexual ;
    gmeow:hasRomanticOrientation ex:samRomantic .

ex:samSexual a gmeow:SexualOrientation ;        # asexual …
    gmeow:sexualOrientationValue gmeow:orientAsexual ;
    gmeow:selfAsserted true .

ex:samRomantic a gmeow:RomanticOrientation ;    # … and biromantic, independently
    gmeow:romanticOrientationValue gmeow:romanticBiromantic ;
    gmeow:selfAsserted true .
```

The relator-mediation existentials (`someValuesFrom` the value classes) live here in EL form
for the reasoner; the closed-world "exactly one value per facet" is deliberately SHACL's job
(issue #39), keeping the logic small and decidable (Principle 12's boundary discipline).

## Alignment — and an honest lossy drop

GSSO, Homosaurus, and Wikidata alignments live in `mappings/gmeow-sexuality.sssom.tsv`.
schema.org and FOAF have **no orientation term at all**, so at the projection layer
orientation is a *documented lossy drop* (Principle 4): the projection records what it
cannot carry rather than flattening it into a string somewhere it doesn't belong. The
reified, self-asserted, co-equal, split-attraction machinery is canonical and exists only
here (Principle 5: superset by reference, never dumbed down at the source).

## Dependencies

Depends on `entities` and `gender` (the `IdentityFacet` base, `selfAsserted`, and the matrix
axioms). Consumed wherever persons are: identity facets are core by commitment
(Principle 16), and the gender slice's matrix axioms reference these classes by name.
