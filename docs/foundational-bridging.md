<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Foundation grounding correspondences

> **Status: realized and shipped.** The canonical source is
> [`slices/grounding/logic/mappings/grounding-bridges.ttl`](../slices/grounding/logic/mappings/grounding-bridges.ttl)
> and
> [`slices/grounding/logic/mappings/foundation-bridges.ttl`](../slices/grounding/logic/mappings/foundation-bridges.ttl).
> It compiles to content-addressed `logic:Correspondence` records in the
> `graph/correspondence-laws` named graph of `generated/dist/gmeow.gts`.
> The target-specific SSSOM tables under `generated/mappings/` are generated review views,
> not alignment authorities.

This is the concrete grounding instance of the correspondence calculus in
[`LOGIC-CORRESPONDENCE.md`](../slices/grounding/logic/design/LOGIC-CORRESPONDENCE.md)
and its applied-category-theory rationale in
[`take1.md`](./APPLIED_CATEGORY_THEORY/take1.md). It applies four constitutional
rules together:

- **Principle 4:** one canonical source; SSSOM and other interchange dialects are
  projections of it.
- **Principle 5:** external ontologies are bridged by reference; their
  commitments do not silently become GMEOW truth.
- **Principle 7:** catalog size, target validity, orientation, policy, generated
  views, and shipped-bundle presence are executable gates.
- **Principle 17:** `logic:` is the rich source; prior logical and upper-ontology
  formalisms are typed views of it.

## Ownership and orientation

All formal and upper-ontology grounding is owned by the `logic:` grounding
slice. Every catalog row is oriented the same way:

```text
canonical logic: source  --get / projection-->  external target view
```

The external vocabulary is never the source from which GMEOW derives its
meaning. Inverse ingestion is the correspondence's `put` direction; it does not
reverse semantic ownership.

Each authored row is a native alignment cell — a reified RDF-1.2 match statement
(`skos:*Match` / `owl:equivalent*`) carrying `gmeow:sssomFile` — whose reifier is
also an explicit `logic:GroundingCorrespondence`. A grounding row must state all
three judgments:

- `logic:morphismClass` — its position on the law spine;
- `logic:morphismKind` — an institution morphism or a commitment-shifting bridge;
- `logic:preservationKind` — what the target view preserves.

The compiler rejects incomplete combinations. In particular,
`logic:CommitmentShiftingBridge` requires `logic:BridgeView`; it cannot emit an
equivalence claim.

## The core six catalogs

| Target | Rows | Morphism policy | Preservation policy | Coverage contract |
|---|---:|---|---|---|
| **gUFO** | 55 | `AffineCorrespondence` + `InstitutionMorphism` | `ValidationOnly`; seven explicit `Unsupported` rows | Every class in the vendored gUFO surface has a row, and five relation rows join the audited relations; the floor is measured per term kind below |
| **BFO 2020** | 13 | `BridgeView` + `CommitmentShiftingBridge` | `ValidationOnly` | The category spine from entity/continuant/occurrent through material entity, disposition, quality, role, and process boundary |
| **OBO / RO** | 6 | `BridgeView` + `CommitmentShiftingBridge` | `ValidationOnly` | BFO `part of`/`precedes` and RO overlap, causal, disjointness, and membership relations |
| **SUMO** | 24 | `BridgeView` + `CommitmentShiftingBridge` | `ValidationOnly` | Foundational categories and structural relations used by the logic foundation |
| **OWL / RDFS** | 33 | `WellBehavedLens` + `InstitutionMorphism` | `SoundUnderApproximation` | The complete adapter, restriction, property-characteristic, and projector construct surface |
| **SHACL Core / AF** | 15 | `AffineCorrespondence` + `InstitutionMorphism` | `ValidationOnly` | Validation shapes, property paths, constraints, rules, targets, and logical constraint operators |

The counts are pinned deliberately. Extending an adapter or target surface
requires extending the catalog and its coverage test in the same change.

## The additive foundation catalog

`foundation-bridges.ttl` adds 25 curated, by-reference rows without importing
any target TBox:

| Target | Rows | Disposition |
|---|---:|---|
| **DOLCE+DnS Ultralite (DUL)** | 6 | Entity, Object, Event, Quality, Situation, and InformationObject as commitment-shifting `BridgeView`s |
| **IAO** | 1 | Information content entity as a close, validation-only bridge |
| **OBI** | 2 | The planned-process backbone: `logic:Plan` → protocol and `logic:Enactment` → planned process, each with its explicit lossy drop |
| **PATO** | 1 | Biomedical quality root as a close, validation-only bridge |
| **YAMATO 2021-08-08** | 9 | Version-pinned particular, independent entity, object, event, process, quality, amount-of-matter, quality-value, and role bridges |
| **OpenCyc 2012-05-10** | 6 | Permanent identifiers for Individual, Collection, Event, Role, InformationBearingThing, and Microtheory |

All 25 are `BridgeView` + `CommitmentShiftingBridge` + `ValidationOnly`.
This is deliberate: shared labels do not erase differences in identity,
participation, process/event, role, collection, or microtheory commitments.
YAMATO uses the versioned
`http://www.hozo.jp/owl/YAMATO20210808.miz.owl#` namespace. OpenCyc uses the
permanent `http://sw.opencyc.org/concept/` identifiers published by the
[OpenCyc KB repository](https://github.com/therohk/opencyc-kb).

### gUFO is a projection floor, not the canon

The gUFO catalog holds 55 rows: 50 class-side rows covering the complete vendored class
surface from `logic:`, and 5 relation rows. 48 rows are validation-only affine
correspondences. Seven rows are explicitly unsupported rather than silently flattened:

- `logic:Disposition` against gUFO's coarser `gufo:IntrinsicMode`;
- the five temporary-situation reifiers, represented canonically by
  `logic:Fluent` plus RDF 1.2 statement metadata; and
- `gufo:externallyDependsOn`, whose gUFO domain is disjoint from the relator type a
  `gmeow:Commitment` has, so the generated view does not emit the edge.

The vendored gUFO source remains an interoperability and conformance input
during the transition away from external terms in domain slices. It does not
outrank `logic:` as the grounding authority.

### The gUFO floor is measured over every vendored term

The row count mixes class coverage, property coverage and explicit replacements, so
it does not measure the floor. The floor check derives its denominator from the
declarations of the vendored source itself: every term in the gUFO namespace, by the kind
its declaration gives it. The source declares 49 classes, 40 object properties and 7
data properties, and no annotation property, individual or datatype of its own. The
external annotation properties, the `xsd:date` datatype, `time:Instant` and the two
disjointness axioms the file also contains are not gUFO terms and are outside the
denominator.

Each term is joined to every catalog cell that names it as target, and each cell keeps
its own source identity, so a many-to-one mapping stays visible: `logic:Aspect` is the
source for `gufo:Aspect`, `gufo:IntrinsicAspect` and `gufo:ExtrinsicAspect`, and
`gufo:IntrinsicMode` is the target of both `logic:Mode` and `logic:Disposition`.

A cell is judged on the native claim, and its target claim is reported beside it:

| Judgment | Native evidence required |
|---|---|
| Earned counterpart | The cell is an exact, close or narrow match. The source is declared with the target's kind and carries a `gmeow:graphBoxRole`. For a property, the source carries every characteristic gUFO states for the target, as a type or through a characteristic assertion. |
| Native replacement | The cell is a related match that states a non-blank `gmeow:lossyDrop`, and the source is declared with the target's kind, carries a `gmeow:graphBoxRole`, and carries every characteristic gUFO states for a property. |
| Validation-only match | A cell exists, but it is a broad match, or it is a related match with no non-blank `gmeow:lossyDrop`, or the source is not declared with the target's kind, lacks a `gmeow:graphBoxRole`, or lacks a characteristic gUFO states. The catalog validates; the native claim is not earned. |
| Uncovered | No cell names the term. |

An earned counterpart establishes that the native source relates to the gUFO target at the same or a wider
relation (a `skos:narrowMatch` cell states that the source includes the view), that the source is declared
with the target's kind, that it is a member of a graph box, and that it carries the target's characteristics.
It does not establish structural subsumption, disjointness, or domain and range parity. One wider native term
may therefore earn several disjoint gUFO terms, as `logic:Aspect` does for the intrinsic and extrinsic aspects.

A term takes the strongest judgment among its cells. The target claim, validation-only
or unsupported, never raises or lowers the native judgment: `logic:Mode` earns the
`gufo:IntrinsicMode` counterpart, and `logic:Disposition` is a validation-only broad match whose
gUFO downprojection is unsupported.

| gUFO kind | Terms | Earned counterpart | Native replacement | Validation-only match | Uncovered |
|---|---:|---:|---:|---:|---:|
| Class | 49 | 42 | 7 | 0 | 0 |
| Object property | 40 | 2 | 0 | 3 | 35 |
| Data property | 7 | 0 | 0 | 0 | 7 |
| Annotation property, individual, datatype | 0 | 0 | 0 | 0 | 0 |

The three dependence judgments are measured with their own evidence.
`gufo:historicallyDependsOn` is an earned counterpart of `logic:historicallyDependsOn`,
which carries the transitive characteristic. `gufo:externallyDependsOn` is a validation-only
match: `gmeow:commitmentBeneficiary` is a broad match, and its unsupported target claim does
not change that judgment. `gufo:inheresIn` is a validation-only match, because `gmeow:bearer`
is a broad match to the gUFO relation. The temporary-situation reifier classes are covered by
the `logic:Fluent` replacement that the worked example `criticism-fixes.ttl` exercises. The event
and situation types are covered by the `logic:Event` and `logic:Situation` replacements.

The floor reads the grounding correspondences only. The core alignment module
(`slices/core/kernel/mappings/equivalences.ttl`) also names `gufo:isComponentOf`,
`gufo:isCollectionMemberOf` and `gufo:isSubCollectionOf` under `gmeow:partOf`. Those alignments
are outside the grounding catalog and carry no `logic:preservationKind`, so the floor does not
measure them, and the three properties count as uncovered.

`logic:Disposition` is a broad match to `gufo:IntrinsicMode`, because gUFO's mode is the coarser
class. `logic:Mode` is the earned counterpart of that term. `logic:Event` and `logic:Situation` are
related matches to `gufo:EventType` and `gufo:SituationType`: their sorts are the type-level targets
of the projection and make no identity claim, so they are native replacements.

Of the other two relation rows, `gufo:isProperPartOf` is an earned counterpart of
`logic:properPartOf`. `gufo:mediates` is a validation-only match: `logic:mediates` carries
neither the asymmetry nor the irreflexivity gUFO states for it, and no `gmeow:graphBoxRole`.

The universal claim, that every gUFO term has a native counterpart or a stated
replacement, does not hold. Its exceptions are the two decided terms `gufo:inheresIn` and
`gufo:externallyDependsOn`, which the foundation charter excepts by decision; they are listed below the
table and are not in it. The other 43 gap terms preclude the claim and fall into these families:

| Family | Terms |
|---|---|
| Temporary-situation role properties | `concernsConstitutedEndurant`, `concernsNonRigidType`, `concernsQualityType`, `concernsQualityValue`, `concernsReifiedQualityValue`, `concernsRelatedEndurant`, `concernsRelationshipType`, `concernsTemporaryWhole` |
| Qualified standing | `standsIn` and its five qualified specializations |
| Parthood and constitution specializations | `constitutes`, `isAspectProperPartOf`, `isCollectionMemberOf`, `isComponentOf`, `isEventProperPartOf`, `isObjectProperPartOf`, `isSituationProperPartOf`, `isSubCollectionOf`, `isSubQuantityOf` |
| Temporal boundaries | `hasBeginPoint`, `hasEndPoint` and their four data-property forms |
| Event and participation relations | `broughtAbout`, `contributedToTrigger`, `manifestedIn`, `participatedIn`, `wasCreatedIn`, `wasTerminatedIn` |
| Quality values | `hasAssociatedQualityValueType`, `hasQualityValue`, `hasReifiedQualityValue`, `hasValueComponent` |
| Type categorization | `categorizes`, `partitions` |
| Derivation | `isDerivedFrom` |
| Mediation characteristics | `mediates` |

The coverage test holds a family list that includes the two decided exceptions, and the check
requires that list to equal the measured gaps exactly: covering a term removes it from its family,
and a gap can never be absent from the list. Full semantic coverage requires qualifying evidence for
every promised counterpart; the universal claim holds only when every family other than the two
decided exceptions is empty.
Characteristic parity is measured; domain and range parity are not machine-compared.

### BFO, OBO, and SUMO are bridge views

BFO, OBO/RO, and SUMO make ontological commitments that do not coincide with
the GMEOW foundation. Their rows therefore carry
`logic:BridgeView`, `logic:CommitmentShiftingBridge`, and
`logic:ValidationOnly`. The SSSOM predicate may express a curated close or
related match, but the compiler never upgrades those rows to
`owl:equivalentClass` or an equivalent preservation claim.

BFO target classes and labels are checked against the offline snapshot at
`imports/targets/bfo.ttl`. BFO/OBO/SUMO target axioms remain outside the
object-level reasoned closure.

### OWL and SHACL are dialect boundaries

OWL/RDFS is a sound-under down-projection of the richer `logic:` source. Its 33
rows cover the named constructs actually handled by the adapter, restriction
frontend, and OWL projector; adding compiler support without a catalog row is a
test failure.

SHACL Core and SHACL-AF validate rather than entail. Their 15 rows are therefore
institution morphisms marked `ValidationOnly`, including the deliberate
`logic:onClass` split between OWL qualified restrictions and SHACL class
targets.

## What ships

The correspondence compiler emits a content-addressed record for every row.
Every shipped grounding record has:

- `rdf:type logic:Correspondence` and
  `rdf:type logic:GroundingCorrespondence`;
- exactly one `logic:sourceEndpoint` and `logic:targetEndpoint`;
- exactly one morphism class, morphism kind, and preservation kind; and
- a link to the authoring cell through the correspondence frontend.

These records ride the `graph/correspondence-laws` named graph in
`generated/dist/gmeow.gts`. That graph is meta-level correspondence data: it is
part of the shipped ontology, but it is not injected into object-level closure.
This is the intended distinction from ordinary documentation-only projections.

The generated SSSOM views are:

```text
generated/mappings/gmeow-logic-gufo.sssom.tsv
generated/mappings/gmeow-logic-bfo.sssom.tsv
generated/mappings/gmeow-logic-obo.sssom.tsv
generated/mappings/gmeow-logic-sumo.sssom.tsv
generated/mappings/gmeow-logic-owl.sssom.tsv
generated/mappings/gmeow-logic-shacl.sssom.tsv
generated/mappings/gmeow-logic-dul.sssom.tsv
generated/mappings/gmeow-logic-iao.sssom.tsv
generated/mappings/gmeow-logic-pato.sssom.tsv
generated/mappings/gmeow-logic-yamato.sssom.tsv
generated/mappings/gmeow-logic-opencyc.sssom.tsv
```

The retired `gmeow-foundational.sssom.tsv` is an orphan and must not return.

## Extending the grounding surface

1. Add or update the `logic:` source term in
   `slices/grounding/logic/module.ttl` with the required annotations.
2. Add the correspondence cell to the appropriate canonical catalog under
   `slices/grounding/logic/mappings/`, oriented `logic:` → external target.
3. Declare morphism class, morphism kind, and preservation kind explicitly.
4. Extend the pinned target-surface test. For a by-reference ontology, add only
   the smallest legal validation snapshot needed to verify target IRIs.
5. Regenerate from canonical sources; never edit a generated SSSOM table or the
   GTS bundle by hand.

```bash
make validate
make check
GMEOW_CONFORMANCE_FILTER=conformance_foundational_bridging \
  cargo nextest run -p gmeow-validate --test conformance_suite
cargo nextest run -p gmeow-pipeline --test correspondence_laws_bundle
```

## Explicit non-foundation dispositions

The OBO Sequence Ontology (`SO_`) and Emotion Ontology (`MFOEM_`) remain cited
lineage and domain-specific alignment targets, not foundation bridge rows.
Neither supplies a trustworthy identity-strength counterpart for a `logic:`
foundation term: SO describes biological sequence entities, while MFOEM
describes emotion and mental-functioning phenomena. Fabricating a broad
foundation correspondence merely to make the inventory look complete would
be semantically false. Their eventual term-level use must be routed through an
owned grounding term with a specific warrant; until then the disposition is
**citation/reference only**, not an unspecified deferral.

## References

- gUFO — Almeida et al., *gUFO: A Lightweight Implementation of the Unified
  Foundational Ontology (UFO)*.
- BFO 2020 — ISO/IEC 21838-2:2021 and the OBO Foundry BFO release.
- DOLCE+DnS Ultralite (DUL) — Ontology Design Patterns DUL release.
- YAMATO — Mizoguchi, *Yet Another More Advanced Top-level Ontology* and the
  version-pinned Hozo OWL export.
- OpenCyc — the permanent-identifier OWL export in
  <https://github.com/therohk/opencyc-kb>.
- SUMO — Suggested Upper Merged Ontology, published OWL translation namespace.
- W3C OWL 2, RDF Schema, SHACL Core, and SHACL Advanced Features.
- Trojahn et al., *Foundational ontologies meet ontology matching: a survey*.
