<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Grounding correspondences — gUFO, BFO, OBO, SUMO, OWL, and SHACL

> **Status: realized and shipped.** The canonical source is
> [`slices/grounding/logic/mappings/grounding-bridges.ttl`](../slices/grounding/logic/mappings/grounding-bridges.ttl).
> It compiles to content-addressed `logic:Correspondence` records in the
> `graph/correspondence-laws` named graph of `generated/dist/gmeow.gts`.
> The six SSSOM tables under `generated/mappings/` are generated review views,
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

Each authored row is both an ergonomic `gmeow:TermEquivalence` frontend cell and
an explicit `logic:GroundingCorrespondence`. A grounding row must state all
three judgments:

- `logic:morphismClass` — its position on the law spine;
- `logic:morphismKind` — an institution morphism or a commitment-shifting bridge;
- `logic:preservationKind` — what the target view preserves.

The compiler rejects incomplete combinations. In particular,
`logic:CommitmentShiftingBridge` requires `logic:BridgeView`; it cannot emit an
equivalence claim.

## The six catalogs

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
```

The retired `gmeow-foundational.sssom.tsv` is an orphan and must not return.

## Extending the grounding surface

1. Add or update the `logic:` source term in
   `slices/grounding/logic/module.ttl` with the required annotations.
2. Add the correspondence cell to
   `slices/grounding/logic/mappings/grounding-bridges.ttl`, oriented
   `logic:` → external target.
3. Declare morphism class, morphism kind, and preservation kind explicitly.
4. Extend the pinned target-surface test. For a by-reference ontology, add only
   the smallest legal validation snapshot needed to verify target IRIs.
5. Regenerate from canonical sources; never edit a generated SSSOM table or the
   GTS bundle by hand.

```bash
make validate
make sync
make sync SYNC_MODE=check SYNC_OUTPUTS=generated
cargo nextest run -p gmeow-validate --test conformance_foundational_bridging
cargo nextest run -p gmeow-pipeline --test correspondence_laws_bundle
```

## Related bridge-view lineage

DOLCE/DUL and YAMATO remain useful by-reference refinement sources. Their
quality, quantity, process, and event distinctions inform the canonical
`logic:` foundation, but they do not yet have a pinned shipped catalog in this
six-target surface. Any future catalog must use the same orientation and must
enter as a commitment-shifting `BridgeView` unless a stronger preservation law
is actually discharged.

## References

- gUFO — Almeida et al., *gUFO: A Lightweight Implementation of the Unified
  Foundational Ontology (UFO)*.
- BFO 2020 — ISO/IEC 21838-2:2021 and the OBO Foundry BFO release.
- SUMO — Suggested Upper Merged Ontology, published OWL translation namespace.
- W3C OWL 2, RDF Schema, SHACL Core, and SHACL Advanced Features.
- Trojahn et al., *Foundational ontologies meet ontology matching: a survey*.
