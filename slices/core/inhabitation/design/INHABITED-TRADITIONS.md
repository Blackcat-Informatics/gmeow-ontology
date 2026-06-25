<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — Traditions, Generality, and Neutrality

> The **generality charter.** The AI runtime stack is one profile of the inhabitation relation; this
> document establishes the others — spiritual / esoteric, fictional / narrative, and legal — and the
> discipline that lets GMEOW model them *faithfully while asserting no metaphysics.* It carries the
> by-reference borrowings ledger (which distinction each tradition contributes) and the
> *assert-no-metaphysics* gate. The constructs it exercises are defined in
> [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md) and
> [`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md).

## Why the traditions are in scope

An inhabitation ontology that can model only AI sessions is overfit to one substrate. The
contemplative and esoteric traditions are **mature inhabitation ontologies**, refined over centuries,
that made distinctions modern computing never did: the locus of inhabitation, co-tenancy and
displacement, manifestation layering, genesis by intention, and the self/no-self contest. GMEOW does
not adopt their metaphysics; it **borrows their distinctions as structure** and is sharpened by them.
That the same relator models a possession, an incarnation, an actor in a role, and a corporation in
its officers — *without asserting that spirits, deities, or corporate persons exist* — is the
correctness proof for the topology.

This is also maximal ontological use (the super-ontology doctrine): the relation is modeled once, and
every domain is a profile, not a fork.

## The neutrality discipline (the gate)

GMEOW asserts no metaphysics. Four existing mechanisms make every cross-domain claim safe, and
together they are the *assert-no-metaphysics gate* this slice must pass:

1. **Frame-relativity (`gmeow:accordingTo` + `gmeow:Standpoint`).** *"According to the Vodou
   standpoint, lwa L inhabits horse H during ceremony C"* is a standpoint-indexed claim that coexists,
   co-equal, with a secular frame's denial. No claim of inhabitation is ever asserted in the
   `gmeow:universalStandpoint` (the uncontested global frame) — it is always held *according to* a
   named tradition's frame. This is the deception/standpoint discipline: GMEOW records the
   claim-structure, never a truth verdict.
2. **Aboutness (`gmeow:AboutnessMode`).** `gmeow:aboutnessDescribes` versus `gmeow:aboutnessEnacts`
   distinguishes *a text about a possession* (a description, mention) from *a ritual that enacts one*
   (a performance, use). An ethnography and an invocation are different aboutness modes over the same
   subject matter — the distinction "text ABOUT deception is not text THAT deceives," applied to
   ritual.
3. **By-reference citation (Principle 5).** The traditions are named in
   [`INHABITED-REFERENCES.md`](INHABITED-REFERENCES.md) as alignment and inspiration — cited, never
   imported as axioms. GMEOW links to *avatāra*, *sprul-pa*, Trikāya, *skandha*, egregore, godform,
   and the banishing rituals exactly as it links to schema.org: by reference, inheriting no
   commitment.
4. **Determinacy and self-assertion (Principle 9).** An inhabitation claim carries its ontic
   determinacy (`gmeow:Determinacy`: a disputed value is recorded as disputed) distinct from epistemic
   confidence, and a self-asserting inhabited subject (a possessed medium reporting the experience, an
   AI persona) is the top authority on its own state.

> **The gate, stated as a test:** no document, example, or eventual ABox in this slice asserts an
> inhabitation, possession, incarnation, or personhood claim outside a named `gmeow:Standpoint`. Every
> such claim is `gmeow:accordingTo` a frame, or it is a defect.

## The by-reference borrowings ledger

Each tradition contributes a structural distinction; GMEOW reuses an existing mechanism to carry it;
and in each case there is a commitment GMEOW refuses to inherit.

| Tradition / source | Distinction contributed | GMEOW reuse | Refused inheritance |
|---|---|---|---|
| **Trikāya** (Mahāyāna three bodies) | manifestation layering: durable essence → contextual body → emanation body | the subject → persona → embodiment spine, aligned to WEMI ([`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md)) | the Buddhology — no claim that a buddha-nature exists |
| **Avatāra** (descent of a deity into form) | the durable subject *descends* into a manifest surface | `DigitalSubject → Embodiment` (Avatar = *avatāra*) | no claim of divine descent |
| **Anattā / ātman** (no-self vs enduring self) | identity-continuity is contestable, not given | `gmeow:counterpartOf`, never `owl:sameAs` ([`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md#identity-continuity-as-a-contested-claim)) | no metaphysical verdict on whether a self persists |
| **Skandha** (five aggregates) | the apparent self is a bundle of processes | the six-way de-conflation ([`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md)) | no doctrine of impermanence asserted as fact |
| **Possession / mediumship** (e.g. *lwa* and "horse") | co-tenancy and displacement; apparent agency ≠ inhabiting agency | `gmeow:heldStandpoint` ≠ `gmeow:projectedStandpoint`; `locusVessel`/`locusSharedSubstrate` | no claim that spirits exist or mount |
| **Tulpa** (*sprul-pa*, thoughtform) | genesis by sustained intention; acquired autonomy | `originImagined` → `producesMentalMoment` → `updatesMentalTenure` + `hasCreationEvent` | no claim that a tulpa is sentient or external |
| **Egregore** (collective thoughtform) | a *collective* wills and sustains a subject | `subjectCreator` → `Organization`/`Group` (Cagle's Collective) | no claim that group-minds are real agents |
| **Invocation / evocation** (high magick) | locus: into-self vs into-an-external-vessel | `gmeow:inhabitationLocus` (`locusSelf` / `locusVessel`) | no claim that ritual summons anything |
| **Conjuration / abjuration** (binding / banishing) | ritual start and end of a tenure | `hasCreationEvent` / `hasDestructionEvent` + `Portal`; suppression not erasure (P10) | no claim that binding or banishing has efficacy |
| **Godform assumption** (assuming a deity-form in ritual) | a practitioner temporarily *plays* a role | `DigitalSubject` / `Inhabitant` as anti-rigid roles | no claim of theurgy |

## The cross-domain profiles

### Spiritual / esoteric

A possession claim, modeled frame-relatively and with co-tenancy:

```turtle
ex:ceremony-c a gmeow:Inhabitation ;
    gmeow:inhabitationSubject ex:lwa-L ;          # an Agent, in the Vodou frame
    gmeow:inhabitedHost ex:horse-H ;              # the medium
    gmeow:inhabitationLocus gmeow:locusVessel ;
    gmeow:duringInterval ex:ceremonyInterval .
# annotated: gmeow:accordingTo ex:vodouStandpoint .
# A secular frame's denial coexists; no triple asserts the lwa exists in the
# universal standpoint. The host's displaced agency rides heldStandpoint ≠
# projectedStandpoint (the deception-divergence reuse).
```

Incarnation across bodies is a `Portal`-linked supersession chain with a contested `counterpartOf`
continuity claim; conjuration and abjuration are the chain's creation and destruction events.

### Fictional / narrative

An actor inhabiting a character, or a narrator a point of view, reuses the narrative slice's
machinery on top of the inhabitation relation:

```turtle
ex:olivier-as-hamlet a gmeow:Inhabitation ;
    gmeow:inhabitationSubject ex:olivier ;        # the actor (Person)
    gmeow:inhabitedHost ex:hamlet ;               # the character (a fictional role/figure)
    gmeow:inhabitationLocus gmeow:locusVessel ;
    gmeow:inhabitationEmbodiment ex:stagePresence .
# The performance is gmeow:aboutnessEnacts (the actor enacts Hamlet, not describes him).
# Unreliable narration (held ≠ projected at the narrator level) reuses the narrative
# slice's NarrationMode boundary, the same divergence as possession.
```

### Legal

A corporation inhabiting its officers, or an office its successive holders, is the inhabitation
relation with `locusSharedSubstrate` (many officers carry one corporate person) and `Portal`
transitions at succession — the same structure as a subject migrating hosts, in the legal frame.

## What the generality buys

The cross-domain corpus is not decoration; it is the conformance instrument
([`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md)). If the AI runtime stack and the possession
case and the actor-in-character case all reduce to the same `Inhabitation` relator with different
players and a different `accordingTo` frame, the topology has found the real joints. If any case
needed a bespoke mechanism, the topology would be revealed as AI-specific and the design would be
wrong. The traditions are therefore the design's adversary and its proof at once.

## Scope and seams

This document is the generality and neutrality charter. The constructs it exercises are
[`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md) and
[`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md); the by-reference citations are staged in
[`INHABITED-REFERENCES.md`](INHABITED-REFERENCES.md); the conformance corpus that operationalizes the
generality is [`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md).
