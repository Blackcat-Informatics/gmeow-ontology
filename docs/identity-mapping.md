<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Identity — gender & sexuality modelling guide

GMEOW models a person's gender and sexuality as **reified, self-asserted,
co-equal, time-scopeable facets** — never bare attributes. Two sibling modules
(`gender`, `sexuality`) share one base, `gmeow:IdentityFacet`. This guide is the
companion to the [names guide](names-mapping.md): together they make one claim —
**the axes of a person are orthogonal, and none is inferred from another.**

## The reframe

A flat model says `person.gender = "male"`. That conflates things that are
*independent* and erases change and self-determination. GMEOW separates them into
distinct, individually self-asserted axes:

| Real-world axis | What it is | Where it lives |
|---|---|---|
| **Pronouns / honorifics** | how a person is **addressed** | `names` (`gmeow:hasPronounSet`, `gmeow:honorific`) |
| **Gender identity** | what a person **is** | `gender` (`gmeow:hasGenderIdentity`) |
| **Gender expression** | how a person **presents** | `gender` (`gmeow:hasGenderExpression`) |
| **Sex assigned at birth** | a **recorded administrative** datum | `gender` (`gmeow:sexAssignedAtBirth`) |
| **Sexual orientation** | pattern of **sexual** attraction | `sexuality` (`gmeow:hasSexualOrientation`) |
| **Romantic orientation** | pattern of **romantic** attraction | `sexuality` (`gmeow:hasRomanticOrientation`) |

## Governing tenets

1. **Address ≠ identity.** What you want to be called has **no relation** to what
   you are. Pronouns are a form of address (a linguistic act); gender identity is
   what you are. Neither is ever inferred from the other.
2. **Sex ≠ gender.** `gmeow:sexAssignedAtBirth` is a *recorded* datum, **not** a
   self-asserted identity and **not** a `gmeow:IdentityFacet`. It never implies a
   gender identity/expression, nor is it implied by one.
3. **Self-assertion is the top authority** (`gmeow:selfAsserted true`). A
   third-party record (`false`) may coexist but never overrides the self-assertion.
4. **Co-equality.** A person bears **many** co-equal facets (bigender = two
   `gmeow:GenderIdentity` facets). There is deliberately **no** preferred/primary
   gender term — display *selection* is the consumer's, only display *suppression*
   is modelled.
5. **No erasure.** A superseded label (a former gender, like a deadname) is kept
   with `gmeow:displayable false`, **never deleted** — recorded for history,
   suppressed from display. `gmeow:displayable` is the single, shared display
   control across naming and identity.
6. **Split attraction.** Sexual and romantic orientation are **separate axes**, so
   *asexual yet biromantic* (and every other combination) is directly expressible.
7. **Inclusive without overtyping.** Each axis is an **open value vocabulary of
   individuals** (`gmeow:Gender`, `gmeow:SexualOrientationValue`, …) — never a
   per-value `Person` subclass, never a forced enum.

## The orthogonality matrix (a tested invariant)

`tests/test_identity_orthogonality.py` asserts that for **every pair** of the seven
axis properties there is **no** `rdfs:subPropertyOf` / `owl:equivalentProperty`
bridge and **no** shared range — no axis can be inferred from another. Building
gender and sexuality together is what makes the matrix *complete*: the claim
"orientation is independent of gender identity" is only a real test when both
exist to be held apart.

## The escape hatch (inclusive without overtyping)

The seed value vocabularies are **anchors, not a fence**. An identity not among the
seeds is expressed by **minting a fresh value individual** carrying `rdfs:label` and
pointing the facet's value property at it — the same idiom `names` uses for a custom
pronoun set. There is deliberately **no** parallel flat-literal field (e.g. no
`gmeow:gender` string): the `…Value` object property is the single path, so a
self-description is a first-class value, never a second-class string.

```turtle
ex:samGi a gmeow:GenderIdentity ;
    gmeow:genderValue ex:genderDemiflux ;   # a fresh individual, not a seed
    gmeow:selfAsserted true ; gmeow:displayable true .
ex:genderDemiflux a gmeow:Gender ; rdfs:label "demifluid (self-described)"@en .
```

## Interoperability (all external mappings are lossy)

Alignments live in `mappings/gmeow-gender.sssom.tsv` and
`mappings/gmeow-sexuality.sssom.tsv` (only identifiers **verified** against each
source are mapped; the vocabularies are open and extend there).

| GMEOW | External (closeMatch) | Note |
|---|---|---|
| `gmeow:genderValue` | `schema:gender`, `foaf:gender`, `wdt:P21` | all **conflate** sex/gender (lossy) |
| `gmeow:Gender` values | Homosaurus (`homoit…`), GSSO (`GSSO_…`) | the authoritative LGBTQ+ / sex-gender vocabularies |
| `gmeow:sexAssignedAtBirth` | `wdt:P21`, `fhir:administrative-gender` | loose neighbours; nothing is an exact birth-sex twin |
| `gmeow:sexualOrientationValue` | `wdt:P91`, Homosaurus | — |
| `gmeow:romanticOrientation*` | Homosaurus (aromantic …) | most vocabularies have **no** romantic axis (canonical) |

The reified, self-asserted, co-equal `IdentityFacet` machinery is **canonical** — no
maintained external RDF vocabulary models it faithfully.

## Projection

`gmeow project` downgrades GMEOW to consumer profiles via the EDOAL/FnO stack. For
gender, `fnSelectDisplayableGender` emits a **displayable** gender identity's value
label as `schema:gender` / `foaf:gender`; a `gmeow:displayable false` label is
**never emitted** — the same suppression contract as a deadname. Orientation and
sex-assigned-at-birth are **documented lossy drops** (no standard target term).

## What's deliberately non-standard (and why)

| GMEOW choice | The common alternative | Why GMEOW differs |
|---|---|---|
| Gender as a reified self-asserted facet | a single `gender` string | supports co-equality, fluidity, transition, suppression, provenance |
| Open value vocab + fresh individuals | a fixed enum (`male`/`female`/`other`) | no forced enum, no class explosion, culturally extensible |
| Sex assigned at birth kept separate | one `sex`/`gender` field | refuses the sex/gender conflation that erases identity |
| Split sexual vs romantic orientation | one `orientation` field | makes asexual/aromantic and mixed orientations expressible |
| Pronouns in `names`, not here | derive pronouns from gender | address is not identity; conflating them erases self-identification |
