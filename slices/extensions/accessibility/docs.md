<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Accessibility — features, barriers, and needs as co-equal facets

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/accessibility` · **tier: extension**
> Whether an entity can reach or use a location under constraints — said honestly, per facet.

Most vocabularies flatten accessibility to a boolean (`wheelchairAccessible true`). GMEOW
refuses the collapse: accessibility is a *family of orthogonal dimensions* — wheelchair,
step-free, visual, auditory, cognitive, clearance, life-support — and each dimension is an
open value facet (Principle 9: individuals, never subclasses; a new facet is data, not a
schema change). Features, barriers, and needs are co-equal: there is no "primary need", and
a location may honestly carry *both* a feature and a barrier for the same facet (a ramp at
the front entrance, stairs at the side). The flat shortcuts cover the 80 % case; promote to
a reified `AccessibilityAssertion` when provenance, confidence, temporal scope, or
suppression matter (Principle 10: suppression is `displayable false`, never deletion).
The slice is part of the the design Location-as-reference-frame design.

Its Principle-15 consumer, declared in the manifest: **accessibility facets over places and
routes, and the schema.org accessibility projection** — every term here either drives
accessible-route solving or down-projects to `schema:accessibilityFeature` and kin.

## The value vocabularies

### gmeow:AccessibilityFacet

The dimension being asserted — the seeds are `facetWheelchair`, `facetStepFree`,
`facetVisual`, `facetAuditory`, `facetCognitive`, `facetClearance`, `facetLifeSupport`.
An open vocabulary (Principle 9): a facet not among the seeds is a fresh individual with a
label, never a subclass. Facets are the shared currency of all three shortcut properties
and of the reified assertion.

### gmeow:AccessibilityPolarity

The sign of a reified claim: `polarityFeature` (the subject provides the facet),
`polarityBarrier` (it impedes it), or `polarityLimited` (it provides it under some
conditions but not all). Polarity exists *only* on the reified form — the flat shortcuts
encode polarity in the property name.

## The flat shortcuts (the 80 % case)

### gmeow:hasAccessibilityFeature

`Location → AccessibilityFacet`: the location positively provides the facet.
Non-functional — a location may support many facets, and may simultaneously carry
`hasBarrier` for the *same* facet under different local conditions. The properties are
deliberately **not** disjoint; disambiguation belongs to the reified form, not to schema
exclusions.

### gmeow:hasBarrier

`Location → AccessibilityFacet`: the location impedes the facet. The honest negative —
a barrier is asserted data, not the mere absence of a feature (open world: silence means
*unknown*, `hasBarrier` means *known impediment*).

### gmeow:hasAccessibilityNeed

`Entity → AccessibilityFacet`: what an entity requires in order to reach or use a
location. Needs are co-equal facets (Principle 9) — all asserted needs coexist. This is
the demand side that the solver matches against the supply side (features/barriers) when
computing accessible routes.

## The reified form (promote when metadata matters)

### gmeow:AccessibilityAssertion

A `gufo:Relator`-grounded claim that a location *or connection* (the connectivity slice's
`Connection` is an admissible subject) has a feature, barrier, or limited status for a
facet. Bears vantage, confidence, temporal scope, and suppression (`displayable false` —
Principle 10). Promote here whenever the claim itself must be a node — "accessible
according to the 2024 city audit, disputed by the user survey".

### gmeow:assertionSubject · gmeow:assertionFacet · gmeow:assertionPolarity

The three functional role properties of the relator: one subject, one facet, one polarity
per assertion. Relator mediation is axiomatized (`someValuesFrom` restrictions, EL-safe)
so ELK sees the doctrine; closed-world cardinality is SHACL's job.

## Solver layer & bridges

Accessible *routes* are computed, never asserted (Principle 12): the connectivity slice
declares the `routeKindAccessible` route kind, and the solver layer walks the connection
graph matching `hasAccessibilityNeed` against features and barriers to produce a path. The
OWL core only models the inputs.

## Dependencies

Depends on `kernel` and `places` (the `Location` domain of the shortcuts). Sibling to
`connectivity` — mutually independent extensions joined only at the solver layer and at
`routeKindAccessible` (slice-dependency doctrine refactor keeps that value individual beside its value
class, in connectivity).
