<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — Traditions, Generality, and Neutrality

> The **generality charter**. The load-bearing rule: a contested inhabitation must
> be an **unasserted claim**, not an asserted base triple with a comment — directly asserting
> `inhabitationSubject ex:lwa` puts the relationship in the base graph and (range `Agent`) globally
> infers the lwa is an agent. The cross-domain cases are **profile mappings with documented
> differences**, not a correctness proof. Constructs are defined in
> [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md) and [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md).

## The neutrality discipline — claims, not asserted base triples

GMEOW asserts no metaphysics. `gmeow:accordingTo` annotations on a *directly-asserted* inhabitation
triple do not suffice: a statement annotation *attributes* a claim, but it cannot *un-assert* a base
proposition already in the graph, and the range axiom on `gmeow:inhabitationSubject` would entail that
the lwa is a `gmeow:Agent` everywhere.

The canonical form of a contested inhabitation is therefore a **`gmeow:InhabitationClaim`**, an
observation whose observed feature is an inhabitation configuration/description — the relationship is
*described*, not *asserted*:

```turtle
gmeow:InhabitationClaim
    a logic:SubKind , owl:Class ;
    rdfs:subClassOf gmeow:StandpointClaim ;
    skos:definition "A standpoint-indexed, attributed claim that an inhabitation holds — its observed
        feature is a gmeow:InhabitationDescription (a quoted configuration), NOT an asserted
        gmeow:InhabitationTenure. A gmeow:StandpointClaim (it carries gmeow:claimModality and
        gmeow:vantage): the relationship is described, never placed in the base graph, so no range
        entailment fires — claiming 'the lwa inhabits the horse' does not entail that the lwa is an
        Agent. Competing claims and refutations coexist (Principle 9)." .

gmeow:InhabitationDescription
    a logic:SubKind , owl:Class ;
    rdfs:subClassOf gmeow:Proposition ;
    skos:definition "A quoted, unasserted description of an inhabitation configuration — the observed
        feature of a gmeow:InhabitationClaim. Its gmeow:describedSubject and gmeow:describedHost are
        deliberately RANGE-OPEN (no rdfs:range), so describing a spirit as a subject does not infer it
        is a gmeow:Agent; the neutrality depends on this." .
# gmeow:describedSubject / gmeow:describedHost / gmeow:describedLocusKind — range-open by design.
```

```turtle
ex:possession-claim a gmeow:InhabitationClaim ;
    gmeow:vantage ex:vodouStandpoint ;
    gmeow:claimModality gmeow:unequivocal ;     # held unequivocally in the Vodou frame
    gmeow:observedFeature [ a gmeow:InhabitationDescription ;     # a quoted configuration, not asserted
        gmeow:describedSubject ex:lwa-L ;        # range-open: NOT inferred to be a gmeow:Agent
        gmeow:describedHost ex:horse-H ;
        gmeow:describedLocusKind gmeow:locusVessel ] .
# A secular frame's refutation is a coexisting gmeow:InhabitationClaim with gmeow:claimModality
# gmeow:refuted. Nothing in the base graph asserts ex:lwa-L a gmeow:Agent.
```

Equivalently, an RDF-1.2 reified-but-unasserted proposition carrying `accordingTo` + modality is
available where a consumer prefers the statement form. Either way the base graph stays silent on the
metaphysics; the claim layer carries the contest. This is the same form GMEOW's deception slice uses
for a `StandpointClaim` (held vs refuted), reused — and it is required for **every** contested case:
possession, incarnation, tulpa, corporate personhood, and contested subject-continuity.

> **The gate, as a test:** no `gmeow:InhabitationTenure`, `gmeow:inhabitationSubject`, or
> `gmeow:inhabitedHost` triple carrying a spiritual / fictional / legal claim appears in the asserted
> base graph; each is the observed feature of a `gmeow:InhabitationClaim`, vantage-indexed. Asserting
> one in the base graph is a defect.

The other neutrality mechanisms remain: `gmeow:AboutnessMode` (`aboutnessDescribes` vs
`aboutnessEnacts`) separates a text about a possession from a ritual that enacts one; by-reference
citation (Principle 5) names the traditions without importing their commitments; and
`gmeow:Determinacy` records a disputed value as disputed.

## The by-reference borrowings ledger

Each tradition contributes a structural distinction; GMEOW carries it with existing machinery; in each
case a commitment is refused. Each row is grounded in a scholarly study of the tradition, cited in full
in [`INHABITED-REFERENCES.md`](INHABITED-REFERENCES.md) — the citation grounds the *distinction*, never
the metaphysics.

| Tradition | Distinction contributed | GMEOW reuse | Refused inheritance |
|---|---|---|---|
| Trikāya | manifestation layering | subject → persona → embodiment (documented parallel, [`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md)) | the Buddhology |
| Avatāra | durable subject descends into a surface | `DigitalSubject` → `EmbodimentAssignment` | divine descent |
| Anattā / ātman | continuity is a contestable verdict | `IdentityContinuityAssessment`, never `owl:sameAs` | a verdict on whether a self persists |
| Skandha | the apparent self is a bundle of processes | the six-way de-conflation | impermanence as asserted fact |
| Possession / mediumship | co-tenancy; control is attributed, not given | `ControlAssessment`; `InhabitationClaim` | that spirits exist or mount |
| Tulpa | genesis by sustained intention | `originImagined` → `producesMentalMoment` → `updatesMentalTenure` | that a tulpa is sentient |
| Egregore | a collective wills a subject | `subjectCreator` → `Organization`/`Group` | that group-minds are agents |
| Invocation / evocation | locus: into-self vs into-a-vessel | `inhabitationLocusKind` | that ritual summons anything |
| Conjuration / abjuration | ritual start and end of a tenure | creation/destruction events (ending ≠ suppression) | that binding/banishing has efficacy |
| Godform assumption | temporarily *playing* a status | anti-rigid `RoleMixin` | theurgy |

## The cross-domain cases are profile mappings, not a correctness proof

One successful graph shape does not prove that disparate phenomena instantiate one ontological
relationship. The non-AI cases are **profile mappings with documented similarities *and*
differences** — adversarial stress tests that sharpen the topology, not a theorem that it is
universal:

- **Spiritual (possession).** Modeled as an `InhabitationClaim` with co-tenancy and a
  `ControlAssessment`; the *difference* is that traditions vary — some claim control, some identity
  replacement, some mere presence or attribution (Bourguignon 1976) — so the locus, control, and
  configuration vary by tradition and are recorded per claim, not assumed. The Vodou "horse"/`chwal`
  mounted by a *lwa* (Deren 1953; Métraux 1959) is the host/inhabitant relation with a control
  attribution, modeled frame-indexed and asserting nothing.
- **Fictional (actor as character).** *Difference:* this may be **role enactment**, not host
  occupation — an actor *performs* a character (`aboutnessEnacts`) rather than the character inhabiting
  the actor as a host. The profile mapping records this explicitly rather than forcing the
  host/inhabitant reading.
- **Legal (corporation and officers).** *Difference:* an officer **represents** a corporation; saying
  a corporation *inhabits* the officer may **reverse the dependence**. The profile documents the
  direction of dependence rather than assuming the AI-runtime reading.

Each profile mapping ships its similarities and its disanalogies; the generality claim is "the same
topology *can express* these, with documented divergences," not "these are one thing."

## What the generality buys

The cross-domain corpus is the design's adversary and a conformance instrument
([`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md)). It forces the constructs to be frame-neutral
(the `InhabitationClaim` form), to separate control from identity from presence (distinct
observations), and to record direction of dependence — disciplines the AI-only reading would not have
exposed.

## Scope and seams

This document is the generality and neutrality charter. The constructs are
[`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md) and [`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md);
the bibliographic records are [`INHABITED-REFERENCES.md`](INHABITED-REFERENCES.md); the conformance
corpus is [`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md).
