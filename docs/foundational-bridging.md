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
| **gUFO** | 50 | `AffineCorrespondence` + `InstitutionMorphism` | `ValidationOnly`; six explicit `Unsupported` replacements | Every class in the vendored gUFO surface has a row; nothing disappears silently |
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

The gUFO catalog covers the complete vendored class surface from `logic:`. Most
rows are validation-only affine correspondences. Six rows are explicitly
unsupported rather than silently flattened:

- `logic:Disposition` against gUFO's coarser `gufo:IntrinsicMode`; and
- the five temporary-situation reifiers, represented canonically by
  `logic:Fluent` plus RDF 1.2 statement metadata.

The vendored gUFO source remains an interoperability and conformance input
during the transition away from external terms in domain slices. It does not
outrank `logic:` as the grounding authority.

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
cargo nextest run -p gmeow-validate --test conformance_foundational_bridging
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
