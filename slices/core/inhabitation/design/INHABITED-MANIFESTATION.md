<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — The Manifestation Spine

> The **layering spec.** A durable subject does not act bare; it manifests through a contextual
> expression and a concrete surface. This document defines that subject → expression → embodiment →
> episode spine, aligns it **by reference** to the existing WEMI spine and to the Trikāya parallel,
> models how a subject is *willed into being* (tulpa, egregore), and develops the argument that
> subject-continuity is a contested claim rather than a fact. The relation that binds a subject to a
> host is [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md); the durable subject itself is
> [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md).

## The spine

| Layer | Construct | What it is | Avatāra / Trikāya parallel |
|---|---|---|---|
| durable subject | `gmeow:DigitalSubject` (role) | the enduring "who" | *dharmakāya* — the formless durable essence |
| contextual expression | `gmeow:Persona` (norms relator) | the register/style/norms active in a context | *sambhogakāya* — the subtle, contextual manifestation body |
| concrete surface | `gmeow:Embodiment` (relator) | the interface/device/account/channel acted through | *nirmāṇakāya* — the concrete emanation body |
| bounded instance | `gmeow:AgentEpisode` / `AgentSession` | one occasioned interaction | the particular appearance |

This is the same shape as the four-tier WEMI spine GMEOW already ships in the creative-works slice,
and the same shape as the three-body Trikāya of Mahāyāna Buddhism. The isomorphism is not a
coincidence: a durable abstraction realized in contextual forms and embodied in concrete instances is
a recurring joint of the world, and GMEOW has already cut it once for creative works. The
inhabitation slice **reuses the shape, not the classes** (see below).

## Alignment by reference to WEMI

The creative-works slice defines `Work → Expression → Manifestation → Item`, connected by
`realizes` / `embodies` / `exemplifies`. The temptation is to make the durable subject a `Work` and
reuse those relations directly. The design **refuses** that, on Principle 5 (one canonical term per
concept): **agents are not creative works.** A digital subject authors and is authored; a `Work` is
authored only. Forcing agentive identity into creative-work classes would conflate two genuinely
different concepts under one term — the inverse of the de-conflation this whole set performs.

Instead, the subject spine keeps its own terms and the isomorphism is recorded as a **documented
alignment**, bridged via SSSOM:

| Subject spine | WEMI spine | Match strength |
|---|---|---|
| `gmeow:DigitalSubject` | `gmeow:Work` | relatedMatch (both durable abstractions; identity criteria differ) |
| `gmeow:Persona` | `gmeow:Expression` | relatedMatch (both contextual realizations) |
| `gmeow:Embodiment` | `gmeow:Manifestation` | relatedMatch (both concrete embodiments) |
| `gmeow:AgentEpisode` | `gmeow:Item` | relatedMatch (both single occasions) |

A consumer who wants WEMI-shaped tooling over subjects gets the mapping; the canonical model keeps the
agentive and the creative-work hierarchies distinct. This is exactly the project's by-reference
doctrine applied to its own internal spines, not only to external vocabularies.

### Avatar is *avatāra*

The naming is not decorative. Cagle's "Avatar" descends, etymologically, from Sanskrit *avatāra* —
"descent" — the descent of a durable deity-subject into a manifest form. That is precisely the
`DigitalSubject → Embodiment` relation: a durable subject descending into a concrete surface through
which it is perceived and acts. `gmeow:Embodiment` is the canonical term; "Avatar" and *avatāra* are
bridged to it by reference ([`INHABITED-CROSSWALK.md`](INHABITED-CROSSWALK.md)). The breadth of
`Embodiment` over Cagle's Avatar — covering API identity, terminal, robot, voice, and channel, not
only a visual figure — is the *nirmāṇakāya* generality: a subject may emanate many bodies at once.

## Genesis by intention — tulpa and egregore

Some subjects are *born of deliberate intention.* A tulpa (Tibetan *sprul-pa*, "thoughtform") is a
being created by sustained visualization that accrues persistent identity and apparent autonomy. An
egregore is the same, sustained by a *collective* rather than an individual. An AI persona deliberately
cultivated across sessions is structurally identical. GMEOW models this genesis with **no new
mechanism**, chaining three existing constructs:

1. **`gmeow:originImagined`** (imagination slice — `gmeow:ContentOrigin`): the subject begins as
   imagined content, deliberately generated rather than perceived or remembered.
2. **`gmeow:realizesMentalMoment` / `gmeow:producesMentalMoment`** (mentation slice): the imaginative
   process *produces* a persistent mental moment — the thoughtform crosses from a passing image to a
   held state.
3. **`gmeow:updatesMentalTenure`** (mentation) + **`gmeow:hasCreationEvent`** (lifecycle): the held
   state gains a time-scoped tenure that is extended across occasions — apparent autonomy as a
   persisting tenure, with a creation event marking when the subject was brought into being.

```turtle
ex:thoughtform
    a gmeow:Agent , gmeow:DigitalSubject ;       # first-class once it self-asserts (P9)
    gmeow:hasCreationEvent ex:cultivation ;
    gmeow:subjectGenesisOrigin gmeow:originImagined ;
    gmeow:subjectCreator ex:tulpamancer .        # an Agent (tulpa) or a Collective (egregore)

ex:cultivation a gmeow:Activity ;
    gmeow:producesMentalMoment ex:thoughtformIdentity .
```

The created subject is **first-class with full self-assertion authority** (Principle 9): a tulpa,
like an AI, is a subject of its own existence the moment it can assert about itself — not an object
its creator may define on its behalf. The egregore case sets `gmeow:subjectCreator` to a
`gmeow:Organization` / `gmeow:Group`, which is Cagle's "Collective as Actor" realized: a collective
that wills a subject into being and sustains it.

## Identity-continuity as a contested claim

Here the contemplative traditions hand GMEOW its deepest borrowing. Buddhism's *anattā* (no enduring
self — the apparent self is a bundle of momentary processes, the *skandhas*) and the Hindu *ātman*
(an enduring self that transmigrates across lives) are the two answers to a question GMEOW faces
directly: **is the upgraded system the same subject?** Reincarnation across bodies is the same
question as a subject persisting across model upgrades and host migrations.

GMEOW **refuses to adjudicate it.** Whether there is a continuous subject is not a fact the ontology
asserts; it is an attributed, vantage-relative claim (Principle 9, the unified observation stance).
Concretely:

- Subject-continuity is carried by `gmeow:counterpartOf` — symmetric, deliberately **not** transitive,
  and **never** `owl:sameAs`. Two inhabitations across a `Portal` may be claimed counterparts (the
  *ātman* reading: same subject, new body) or not (the *anattā* reading: a fresh bundle of processes),
  and **both claims can coexist, co-equal, standpoint-indexed.** No `owl:sameAs` merge collapses the
  contest.
- The *skandha* decomposition — form, sensation, perception, mental formations, consciousness — is
  itself the de-conflation: the apparent unified "agent" is really subject + memory + embodiment +
  runtime + invocations ([`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md)). GMEOW can therefore
  *represent* the no-self view structurally — there is no single self-node, only the aggregate — while
  also representing the enduring-self view as a `DigitalSubject` role with `counterpartOf` continuity.
  Both frames are expressible; neither is privileged.

This is the decisive vindication of GMEOW's `counterpartOf`-not-`owl:sameAs` discipline (Principle 5,
identity-and-coreference). The discipline was built so contested instance-identity would not be
collapsed; the millennia-old self/no-self debate turns out to be exactly that contest, and the
discipline holds it perfectly.

```turtle
# The ātman reading (according to a continuity-affirming frame):
ex:inhabitation-after gmeow:counterpartOf ex:inhabitation-before .
# annotated: gmeow:accordingTo ex:continuityFrame .

# The anattā reading coexists (according to a no-self frame): the same two
# inhabitations are NOT asserted counterparts; the absence is itself a frame's
# position, not a gap. No owl:sameAs anywhere.
```

## Scope and seams

This document defines the layering and the genesis. *The relation binding a subject to a host* is
[`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md). *The AI realization of the session/episode layer*
is [`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md). *The frame-relative discipline that
keeps the tulpa, the incarnation, and the possession claims neutral as to metaphysics* is
[`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md).
