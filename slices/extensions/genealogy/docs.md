<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Genealogy — evidence-centric kinship, derived ancestry

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/genealogy` · **tier: extension**
> Reified, typed kinship as observation; ancestry the reasoner derives, never asserts.

Genealogy is the discipline of *contested evidence*, and the slice models it that way. A
kinship claim is an observation — a claim-from-a-vantage in the universal stack
(issue #69) — not a brute fact. The standpoint doctrine (issue #51) governs everything
contested: disputed parentage, conflicting birth/death dates, competing civil vs parish
records are standpoint-indexed claims that **coexist**, none privileged (Principle 9). A
contested parentage is two `accordingTo`-annotated `hasParent` triples, or two reified
`ParentChildRelationship` instances each carrying `gmeow:accordingTo`. There is no
genealogy-specific dispute mechanism, no `preferredParent`, no `primaryKinship` — only
the cross-cutting issue #43 facility. A withdrawn claim sets `gmeow:displayable false`,
never deletion (Principle 10).

Equally important is what the slice does *not* own. Life events belong to the universal
events module (`LifeEvent` occurrences carrying `eventTypeBirth` / `…Marriage` /
`…NameChange` values, with `Participation` relators for principal/witness/officiant);
names belong to the names module (`PersonName`, linked to its conferring event via
`conferredByEvent`); sex and gender belong to the gender and sexuality modules. The slice
supersedes the unmaintained W3C SWAP gedcom vocabulary and is aligned to BIO, GEDCOM X,
schema.org, Wikidata, REL, and GeoNames. Its Principle-15 consumer, declared in the
manifest: **kin relationships for the family-history use of the mail corpus, and the
GEDCOM alignment**.

## The reified layer

### gmeow:KinRelationship

The root reified kinship relator — simultaneously a `gmeow:Observation` and a
`gufo:Relator`: a relationship *is* a claim from a vantage, able to bear its own events,
dates, sources, and standpoint-indexed sub-claims. The participants are co-observed
features (`relationshipParent`/`relationshipChild`/`hasPartner` are `observedFeature`
sub-properties — the issue #287 bridge declared here, so the observation spine never
knows the slice).

### gmeow:ParentChildRelationship

The parent-child relator, typed by nature via its four subkinds. Relator mediation is
axiomatized (EL `someValuesFrom`: a parent and a child exist) so ELK sees the structure;
closed-world cardinality is SHACL's (issue #39).

### gmeow:BiologicalParentChild · gmeow:AdoptiveParentChild · gmeow:StepParentChild · gmeow:FosterParentChild

The four natures of parenthood as subkinds — one of the few places GMEOW subclasses
rather than using a value vocabulary, because the nature changes the relator's identity
conditions (an adoption is a different relationship, not the same one re-labelled).

### gmeow:relationshipParent · gmeow:relationshipChild

The functional role properties of a `ParentChildRelationship`: one parent, one child per
relator. A child with two parents has two relators — which is exactly what lets each
parentage claim carry its own evidence and standpoint.

### gmeow:CoupleRelationship

The reified couple relator — marriage, civil union, or partnership — bearing marriage,
divorce, and related events. Its two partners attach via `gmeow:hasPartner` (an
`observedFeature` bridge like the parent/child roles).

### gmeow:Family

A `Group` subkind: a kinship group related by descent, marriage, or adoption. The
GEDCOM-style family record, grounded in the universal Group machinery rather than
reinvented.

## The flat layer (the 80 % case)

### gmeow:hasParent · gmeow:hasChild

The flat shortcuts, mutually inverse, deliberately non-functional: contested parentage
claims from multiple sources coexist as `accordingTo`-annotated statements (Principle 9).
`hasMother`/`hasFather` specialize `hasParent`. Both are sub-properties of
`gmeow:connectsTo` — kinship bonds are traversable links in the universal graph layer
(issue #80), safely, because `connectsTo` is neither symmetric nor transitive.

### gmeow:hasSpouse · gmeow:hasSibling

Symmetric flat shortcuts, also `connectsTo` sub-properties. Their symmetry stays local —
the connectivity spine imposes nothing back.

### gmeow:hasAncestor · gmeow:hasDescendant

The derived closure (issue #38, phase 2 of the reasoning-depth epic issue #35):
`hasAncestor` is transitive with `hasParent` as a sub-property, so the reasoner *entails*
the full ancestor closure that was never asserted; `hasDescendant` adds the DL inverse
(HermiT-complete). Both are non-simple (transitive) and are deliberately kept out of
every cardinality and functional axiom, preserving OWL 2 DL regularity. Never assert
ancestry directly — assert parentage and let the reasoner do its one job.

## Solver layer & alignment

Beyond the OWL-derived ancestor closure, genealogy computation — relationship-degree
calculation ("second cousin once removed"), pedigree collapse, generation numbering — is
the solver layer's work (Principle 12) over the asserted parentage graph. The GEDCOM X /
BIO / schema.org alignments are projections of the reified relators; the evidence-bearing
form is canonical, the flat GEDCOM record is the lossy down-projection.

## Dependencies

Depends on `kernel`, `entities` (Person, Group), and `observations` (the Observation
spine the relators sit on). Leans on events, names, gender, and connectivity by
convention — each contribution declared in the slice that owns it.
