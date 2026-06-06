<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Standpoints — contested facts as coexisting claims

GMEOW records a contested fact as **several standpoint-indexed claims that
coexist, none privileged** — never one ground truth with a winner. This is the
doctrine document for the standpoint facility (`ontology/modules/standpoint.ttl`);
its companion is the [alignment & projection reference](standpoint-mapping.md).

## The epistemic thesis

History is not an immutable gestalt. It is a collection of **biased, refined, and
variably recorded opinions and observations** — and so is the geographic,
biographical, and organizational record built on top of it. A fact is therefore a
**standpoint-indexed, time-scoped, attributed, revisable observation**, never
ground truth.

The canonical Wikidata/Wikipedia edit wars — Crimea's sovereignty, the Sea of
Japan / East Sea name, "genocide" vs "armed clash" as an event framing — all share
one cause: a **flat model with a single slot** that two parties must both own.
Whoever writes last wins; the loser reverts. The fight is *structural*.

GMEOW dissolves the contention surface: reify the claim, index it to a standpoint,
and refuse any privileged winner. There is then **nothing to overwrite, so nothing
to revert** — contradictory base triples coexist without logical contradiction
because each is scoped to *whose frame it holds in*.

## The three orthogonal axes (no inferential bridge)

A claim carries up to three independent kinds of metadata. Collapsing any into
another is the modelling error the facility exists to prevent.

| Axis | Property | Question it answers |
|---|---|---|
| **Standpoint** | `gmeow:accordingTo` | *Whose frame* is this true in? |
| **Source** | `gmeow:wasAttributedTo` / `gmeow:mappedFrom` | *Which source* recorded it? |
| **Confidence** | `gmeow:confidence` | *How sure are we* of it? |

A neutral archive can record a partisan claim (source ≠ standpoint); a claim can be
high-confidence-that-S-holds-it yet about a frame we disagree with (confidence ≠
standpoint). The three never bridge — the same discipline as the identity-axis
orthogonality matrix. `gmeow:accordingTo` is an `owl:AnnotationProperty`, so it
rides the RDF-1.2 statement layer and the generated OWL downcast stays OWL 2 DL.

## The two clocks (keep them apart)

A standpoint is **itself temporal** — recognition is granted and withdrawn, naming
preferences shift, historiographic positions are adopted then revised. Two clocks
must stay separate:

1. **Fact-time** — when the *claimed fact* holds (Crimea was Ukrainian 1991–2014).
   Carried by the statement's `gmeow:validFrom` / `gmeow:validUntil`.
2. **Standpoint-time** — when the *standpoint held that position* (a country
   recognized Kosovo from 2008). Independent of fact-time: a 2025 standpoint may
   assert a claim about an 1850 fact, and revise it again in 2030.

The lightweight case rides `validFrom`/`validUntil` on the statement. When the
*change of position itself* is the fact of interest, promote to a reified
`gmeow:StandpointTenure` (the `AddressTenure` idiom): recognition granted in 2008
and withdrawn in 2030 is an opened-then-closed tenure with `gmeow:displayable
false` — retained, never deleted (suppression, not erasure).

## Modality and the standpoint poset

The facility realises **Standpoint Logic** (Gómez Álvarez & Rudolph), giving it a
formal grounding rather than an ad-hoc shape:

- **`gmeow:standpointModality`** — the **belief-value axis**, at least as expressive
  as *both* the Standpoint-Logic operators *and* the CRMinf belief value
  (true/false/probable/possible): `gmeow:unequivocal` (□, settled true),
  `gmeow:probable` (likely), `gmeow:conceivable` (◊, possible), and
  `gmeow:refuted` (□¬, settled **false** — the standpoint *denies* the proposition).
  Optional; absent reads as unequivocal. Orthogonal to confidence (our certainty)
  and `accordingTo` (whose frame). Refutation is what lets GMEOW distinguish a
  standpoint's explicit **denial** from its **silence** — a distinction flat models,
  and even the recent attributions work, leave ambiguous.
- **`gmeow:sharpens`** — the standpoint partial order: S₁ sharpens S₂ when every
  precisification S₁ admits is admitted by S₂ (the more specific frame).
- **`gmeow:universalStandpoint`** — the universal standpoint `*`, the top of the
  poset: the uncontested global facts every standpoint shares. **An unindexed
  statement is held according to the universal standpoint** — which is exactly why
  the common, uncontested case needs no `accordingTo` at all.

## Worked example — Crimea, end to end

Two contradictory base triples coexist, each indexed to a standpoint, neither
privileged (`statement-dsl/examples.ttl`):

```turtle
ex:crimea gmeow:containedInPlace ex:russia , ex:ukraine .   # both asserted

# according to the RU standpoint, from 2014
[ owl:annotatedSource ex:crimea ; owl:annotatedProperty gmeow:containedInPlace ;
  owl:annotatedTarget ex:russia ; gmeow:accordingTo ex:standpoint-ru ] .
# according to the UA/UN standpoint, from 1991
[ owl:annotatedSource ex:crimea ; owl:annotatedProperty gmeow:containedInPlace ;
  owl:annotatedTarget ex:ukraine ; gmeow:accordingTo ex:standpoint-un ] .
```

- **SHACL-clean.** `gmeow:StandpointCoexistenceShape` documents that this is
  *explicitly permitted*; no shape constrains coexistence. `gmeow:NoPreferredClaimShape`
  *would* fire if either claim were crowned with a `preferredRank`/`primary*` selector.
- **Reasoning-safe.** The two base triples do not make the reasoned graph
  inconsistent (`containedInPlace` is non-functional; places are not pairwise
  disjoint), so coexistence survives `make reason` — it is not merely tolerated by
  SHACL, it is sound under the reasoner.
- **Projected five ways — every standpoint kept, never a winner.**
  `standpoint-owl2.rq` re-expresses every claim in the **Standpoint-OWL 2**
  `standpointLabel` form for a standpoint-aware reasoner; `standpoint-crminf.rq` in
  the **CRMinf** argumentation/belief model (the full belief value, denial included);
  `standpoint-prov.rq` as **PROV-O** qualified attributions; `standpoint-oa.rq` as
  **W3C Web Annotations**; `standpoint-schema.rq` as **schema.org Claims** (one per
  standpoint, no single verdict). All preserve every standpoint.
  There is **no** projection that selects one standpoint: collapsing a contested fact
  to a chosen frame in a down-projection would re-create the single winning slot — it
  is picking a winner by another name, and the facility forbids it. See
  [standpoint-mapping.md](standpoint-mapping.md).

## SOTA, and how GMEOW transcends it

GMEOW's design is the academic state of the art — and supersets it.

| Prior work | What it offers | Where it falls short | GMEOW's response |
|---|---|---|---|
| **Standpoint Logic / EL / Standpoint-OWL 2 / Monodic-S5** (Gómez Álvarez & Rudolph) | `□_S`/`◊_S` operators, the standpoint poset (∪/∩/⊆/`*`), the `standpointLabel` OWL encoding, **tractable per-/cross-standpoint reasoning** | Tooling is young; values are opaque strings | **Aligns directly** — `accordingTo` = `standpointLabel`, modality = □/◊, `sharpens` = ⊆, `universalStandpoint` = `*`; generates the tool-consumable projection |
| **Attributions / "Shards of Knowledge"** (2025) | per-viewpoint predicates, supported-by/disputed-by, viewpoint hierarchies | **Static viewpoints (no time)**; **neutral-stance ambiguity** (silence ≠ rejection ≠ no-data); representation paradox | `StandpointTenure` solves the temporal gap; the three axes disambiguate silence/rejection/low-data; **View-Preserving** (no forced propagation) avoids the paradox |
| **CRMinf** (CIDOC-CRM Argumentation model) | I1 Argumentation / I2 Belief (temporal) / I4 Proposition Set / J5 holds to be a belief value (true/false/probable/possible) | heavyweight; no standpoint poset or modal operators; cultural-heritage-scoped | **At least as expressive**: proposition = the reified statement, temporal belief = `StandpointTenure`, belief value = `standpointModality` (incl. `refuted` = false); generated as a lossless projection. GMEOW *adds* the modal poset + the two clocks |
| **CKR** (Contextualized Knowledge Repository) | OWL2 contexts (time/space/topic) + coverage + propagation | propagation forces cross-context consistency | GMEOW factors time/place into their own modules, keeps only the *perspective* axis, and is View-Preserving |
| **RDF contextualization** (reification, named graphs, singleton property, NdFluents, **RDF***) | mechanisms to annotate triples | reification doesn't entail the triple; named graphs overloaded | GMEOW is **already RDF-1.2/RDF*-first** — the benchmarked-best mechanism |
| **schema.org ClaimReview, Wikidata rank, MVP-OWL, ClaiMaker, nanopublications** | fact-check verdicts, ranked statements, multi-viewpoint classes, source-anchored claims, assertion+provenance bundles | a **preferred rank / single verdict re-creates the one slot** the edit war is fought over | GMEOW **refuses `preferredRank`** (Principle 9); keeps references/qualifiers as alignment targets; reuses the nanopub assertion/provenance split + adds the standpoint axis |

The transcendences worth naming: **static viewpoints → `StandpointTenure`**;
**neutral-stance ambiguity → three orthogonal axes**; **single winner → no
`preferredRank`**; **representation paradox → View-Preserving coexistence**.

## Doctrine

- **Principle 9 — no single slot to win.** A contested fact is several
  standpoint-indexed claims; there is no `preferredRank`/`primary*`. Enforced three
  ways: the `NoPreferredClaimShape` (SHACL), the `no_preferred_rank` statement lint,
  and a term-absence test.
- **Principle 10 — suppression, never erasure.** A withdrawn claim or a closed
  `StandpointTenure` sets `gmeow:displayable false`; it is retained, not deleted.
- **Principles 2/3 — RDF-1.2-first, DL-clean.** The axis is an annotation property;
  the OWL downcast stays OWL 2 DL.
- **Principle 4 — one canonical source.** The Standpoint-OWL 2 projection is
  *generated* from the one graph and is **lossless** — it carries every standpoint.
  GMEOW ships **no winner-selecting projection**: a down-projection that picks one
  frame is the single-slot edit war deferred to projection time (Principle 9). A
  flat, perspectiveless format cannot represent "according to whom," so contested
  facts are carried only by the lossless path, never resolved to a chosen side.
- **Principle 5 — align by reference.** Standpoint Logic, PROV-O, nanopublications,
  CKR, the rest — folded by reference, never imported.
