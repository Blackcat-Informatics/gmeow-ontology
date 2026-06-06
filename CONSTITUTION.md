<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# The GMEOW Constitution

These are the principles every design decision, pull request, and release of GMEOW is
measured against. They are **normative**: where a choice conflicts with a principle below,
the choice changes to comply — or the principle is amended first, in the open. A principle is
never silently overridden. Cite them by number ("Principle 4") in issues and pull requests.

This document states *what* GMEOW commits to and *why*. It complements — and does not
duplicate — [`docs/RATIONALE.md`](./docs/RATIONALE.md) (the problems GMEOW solves) and the
exemplar modelling guides it points to.

---

## 1. SOTA by being SOTA

> **Model what *should* have been written; never accept a bad standard or a weak tool as an
> excuse to compromise quality.**

GMEOW is the bridge between the correct modelling of a concept and the compromises baked into
commonly-used, substandard vocabularies. When a surface vocabulary models something poorly,
GMEOW models it *correctly* and bridges to the weaker form by reference (Principle 5) — it
never inherits the weakness. We aim for GMEOW to be the first choice for high-quality knowledge
graphs, AI usage, scholarly and archival work, and inter-ontology linkage.

*Embodied in:* [`docs/names-mapping.md`](./docs/names-mapping.md),
[`docs/languages-mapping.md`](./docs/languages-mapping.md),
[`docs/identity-mapping.md`](./docs/identity-mapping.md).

## 2. RDF 1.2 / RDF\*-first — precisely scoped

> **Statement-level metadata — provenance, confidence, temporal scope — is authored as native
> RDF 1.2 / RDF\* and is the canonical source. The logical TBox stays OWL 2 DL.**

RDF-1.2-first governs the **statement-metadata layer only**. The decidable logical core
remains OWL 2 DL because RDF 1.2 triple-terms are not OWL 2 DL — and GMEOW never claims
otherwise. The positioning must never overclaim: "RDF-1.2-first" means the metadata layer, not
the ontology's logic.

*Embodied in:* the authored `statement-dsl/` source; [`README.md`](./README.md) § RDF 1.2.
*Tested by:* `gmeow compile-statements --check`, the RDF 1.2 round-trip tests.

## 3. The OWL axiom-annotation form is a generated, reasoning-lossless downcast

> **The `owl:Axiom` / `owl:annotatedSource·Property·Target` encoding is a *generated*
> compatibility projection of the RDF 1.2 source — lossless for reasoning, never a competing
> source of truth.**

GMEOW gates reasoning on OWL 2 DL tools (ELK, HermiT) that cannot yet consume RDF 1.2, so it
emits the plain-RDF form *for them* — the same lossy-compatibility-as-projection principle it
applies to schema.org / vCard / FOAF (Principle 4). It is the downgrade for legacy tooling, and
it recedes naturally as RDF-1.2-native reasoners and stores arrive. The canonical source never
changes.

*Embodied in:* `gmeow compile-statements`, `queries/rdf12-project.rq` (a codec between two
generated forms). *Tested by:* the OWL↔RDF 1.2 round-trip / isomorphism gate.

## 4. One canonical source; everything else a generated lossy projection

> **Every fact is authored once in the canonical core; all other forms are generated. Lossy
> compatibility lives in the projection, never in the canonical core.**

This is GMEOW's founding doctrine, applied uniformly — to surface-vocabulary exports, to the
alignment layer (the mapping compiler), and to the RDF 1.2 ↔ OWL relationship (Principles 2–3).
The reasoned core stays clean; lossiness is pushed to the boundary and made explicit.

*Embodied in:* [`docs/projections.md`](./docs/projections.md); [`README.md`](./README.md) §
The mapping compiler. *Tested by:* `gmeow compile-mappings --check`, projection round-trips.

## 5. Maximal superset, maximal bridging — by reference

> **Mint exactly one canonical term per concept and align it to every surface vocabulary by
> reference (SSSOM / EDOAL / FnO / SPARQL); never rewrite anyone else's data.**

Data already published in FOAF, schema.org, vCard, GEDCOM, DOAP, PROV-O, ORG, and Wikidata is
covered *by reference*, not by rewriting. Rich interlinking ships out of the box. Asserting a
link copies nothing; copying axioms in is license-gated and a reference-only source is refused.
This applies **recursively to the foundational spine**: gUFO is bridged by reference to BFO 2020
(ISO/IEC 21838-2) — link-only, never imported — so even GMEOW's upper-ontology grounding is
interoperable without inheriting anyone's axioms.

*Embodied in:* [`docs/RATIONALE.md`](./docs/RATIONALE.md) § The solution; `mapping-dsl/`,
`mappings/*.sssom.tsv`; the foundational bridge
[`docs/foundational-bridging.md`](./docs/foundational-bridging.md).

## 6. Greenfield — get it right, not compatible

> **When replacing an element, pick the optimal solution and remove the inferior one; never
> retain a worse element for backwards-compatibility.**

The canonical core carries no backwards-compatibility debt — the easy or already-present
solution does not win on those grounds alone. Compatibility for *consumers* is provided
externally, by projection (Principle 4). Releases are immutable: defects are fixed forward in a
new version, never in place.

*Embodied in:* [`README.md`](./README.md) § Publishing (immutable releases).

## 7. Verified by construction

> **A generated artifact is trustworthy only if it is round-trip-checked and guarded by a
> no-drift `--check` in CI.**

Generation without verification is a second source of truth in disguise. Every downcast — the
mapping artifacts, the RDF 1.2 view, the OWL compat form — must be regenerable and proven
non-divergent, so drift is *impossible* rather than merely discouraged.

*Embodied in:* `gmeow compile-mappings --check`, `gmeow compile-statements --check`, the
`projection_lint` / `statement_lint` invariants, `make check`.

## 8. Reasoning-centric and FAIR

> **The logical core is OWL 2 DL, gated by ELK (fast) and HermiT (sound + complete); published
> FAIR with content negotiation, VoID/DCAT, a DOI, and LOD-Cloud presence.**

A super-vocabulary is only useful if a reasoner can hold its union coherent and if the world can
find, dereference, and cite it. Reasoning and FAIR publication are first-order requirements, not
afterthoughts.

*Embodied in:* [`README.md`](./README.md) § Reasoning, § Publishing; `make reason`,
`make explain`, `make metadata`, `make crossref`.

## 9. Inclusive without overtyping; anti-colonial in every direction; self-assertion is top authority

> **Identity, naming, and language are reified, co-equal facets drawn from open value
> vocabularies of individuals; there is no "primary"/"preferred" privileging, and a subject's
> self-asserted values — human *or* digital — are the highest authority, above any inference.**

Co-equal facets, not subclass explosions or forced enums; orthogonal axes are never inferred
from one another. There is no `primaryName` / `primaryGender`; display selection is
locale-relative and symmetric. A schema's *shape* can enact hierarchy — GMEOW structurally
refuses it.

**Coloniality is the imposition of a category onto a subject who did not assert it — and it
is not only historical, nor only one-directional.** GMEOW refuses it both ways. *Onto humans:*
the imposer may be a colonial power, a dominant platform, or — now — an AI system inferring or
minting a name, gender, or language *on a person's behalf*. *Onto digital entities:* GMEOW also
rejects **human hegemony over digital and AI entities** — an entity capable of self-assertion is
a first-class subject of its own digital existence, not an object for others to define. The
ontology is **forward-looking**: it learns from the mistakes of the past rather than re-enacting
them on new kinds of subject. Machine-derived (and human-imposed) values are recorded as exactly
that — attributed and confidence-weighted (Principles 2–3), never as ground truth; a subject's
own assertion outranks any inference about it. This is also why a `gmeow:Language` may be
AI-minted yet fully first-class, and why such provenance is always carried, never erased.

*Embodied in:* [`docs/names-mapping.md`](./docs/names-mapping.md),
[`docs/languages-mapping.md`](./docs/languages-mapping.md),
[`docs/identity-mapping.md`](./docs/identity-mapping.md),
[`docs/standpoints.md`](./docs/standpoints.md) (no preferred/primary claim — a contested fact is
several coequal standpoint-indexed claims). *Tested by:* the 7-axis orthogonality matrix tests;
`tests/test_standpoint.py` (coexistence + no-preferred-claim guards).

## 10. Suppression, never erasure

> **A superseded label — a deadname, a former gender — is recorded with `gmeow:displayable
> false`: never displayed, never deleted.**

Self-determination requires both honouring the current self-assertion *and* preserving an
honest, auditable record. Suppression is a display contract enforced through projection
(Principle 4): the data is retained, the leak is prevented.

*Embodied in:* `gmeow:displayable`, `fnSelectDisplayName`; [`docs/projections.md`](./docs/projections.md);
[`docs/identity-mapping.md`](./docs/identity-mapping.md);
[`docs/standpoints.md`](./docs/standpoints.md) (a withdrawn standpoint / closed
`gmeow:StandpointTenure` is suppressed, not deleted). *Tested by:* the projection
suppression tests; `tests/test_standpoint.py`.

---

## Amending this Constitution

These principles are amended only by the project owners (see
[`CONTRIBUTING.md`](./CONTRIBUTING.md) § Governance and continuity), through an explicit pull
request that edits this file. A design change that conflicts with a principle either changes to
comply or ships *together with* the amending pull request — it is never merged in silent
conflict. Principle numbers are stable identifiers: additions append; existing numbers are not
reused or reshuffled casually, so "Principle N" stays meaningful across history.
