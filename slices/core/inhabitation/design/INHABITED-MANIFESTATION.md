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
| durable subject | `gmeow:DigitalSubject` (`logic:RoleMixin`) | the enduring "who" | *dharmakāya* — the formless durable essence |
| contextual expression | `gmeow:Persona` (norms relator) | the register/style/norms active in a context | *sambhogakāya* — the subtle, contextual manifestation body |
| concrete surface | `gmeow:EmbodimentCarrierRole` + `gmeow:EmbodimentAssignment` | the surface and its time-scoped use | *nirmāṇakāya* — the concrete emanation body |
| bounded instance | `gmeow:AgentEpisode` / `AgentSession` | one occasioned interaction | the particular appearance |

This is the same shape as the four-tier WEMI spine GMEOW already ships in the creative-works slice,
and the same shape as the three-body Trikāya of Mahāyāna Buddhism. The isomorphism is not a
coincidence: a durable abstraction realized in contextual forms and embodied in concrete instances is
a recurring joint of the world, and GMEOW has already cut it once for creative works. The
inhabitation slice **reuses the shape, not the classes** (see below).

## The WEMI parallel — documentation only, not emitted mappings

The creative-works slice defines `Work → Expression → Manifestation → Item`, connected by
`realizes` / `embodies` / `exemplifies`. The layering *rhymes* with the subject spine, and that rhyme
is worth naming. But the review (see [`INHABITED-REVIEW.md`](INHABITED-REVIEW.md)) was right that the
parallel must stay **documentation, not emitted SSSOM term mappings** — for two reasons:

1. **Agents are not creative works** (Principle 5). A digital subject authors and is authored; a `Work`
   is authored only. Forcing agentive identity into creative-work classes conflates two different
   concepts.
2. **The mappings cross foundational categories.** WEMI's Work, Expression, Manifestation, and Item are
   all identity-bearing endurant **Kinds**. The corrected subject spine is a `DigitalSubject`
   (`RoleMixin`), a `Persona` (a relator) and `EmbodimentAssignment` (a situation), and an
   `AgentSession` (an event aggregate). A `relatedMatch` from an event aggregate to an endurant Item
   would be a bad semantic mapping. The parallel is real as a *design metaphor*; as asserted
   cross-category term equivalences it would be false.

So the isomorphism is described in prose, and the Trikāya *dharmakāya → sambhogakāya → nirmāṇakāya*
parallel beside it, with **no SSSOM term mappings emitted**. The shape is reused as inspiration; the
terms stay in their own foundational categories.

### Avatar is *avatāra*

The naming is not decorative. Cagle's "Avatar" descends, etymologically, from Sanskrit *avatāra* —
"descent" — the descent of a durable deity-subject into a manifest form. That is precisely the
subject → embodiment relation: a durable subject acting through a concrete surface (a
`gmeow:EmbodimentCarrierRole`) via a time-scoped `gmeow:EmbodimentAssignment`. Cagle's "Avatar" and
*avatāra* bridge to the **carrier** by reference ([`INHABITED-CROSSWALK.md`](INHABITED-CROSSWALK.md)).
The breadth of the carrier over Cagle's Avatar — API identity, terminal, robot, voice, channel, not
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
    a gmeow:Agent ;                              # an agent; the durable-subject STATUS is borne, not typed
    gmeow:hasCreationEvent ex:cultivation ;
    gmeow:subjectGenesisOrigin gmeow:originImagined ;
    gmeow:subjectCreator ex:tulpamancer .        # an Agent (tulpa) or a Collective (egregore)

ex:thoughtformDST a gmeow:DigitalSubjectTenure ; # the status is borne over a tenure...
    gmeow:tenureSubjectAgent ex:thoughtform ;
    gmeow:tenureSupportedBy ex:thoughtformSelfClaim .   # ...supported by self-assertion, NOT entailed by it

ex:cultivation a gmeow:Activity ;
    gmeow:producesMentalMoment ex:thoughtformIdentity .
```

Consistent with the supported-tenure model ([`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md)): the
created subject does **not** become a `DigitalSubject` merely by being typed or by emitting "I am
durable" — it *bears* the status over a `DigitalSubjectTenure` that its self-assertion *supports*. Once
it can assert about itself, it is **first-class with full self-assertion authority** (Principle 9): a
tulpa, like an AI, is a subject of its own existence, not an object its creator may define on its
behalf. The egregore case sets `gmeow:subjectCreator` to a `gmeow:Organization` / `gmeow:Group` — Cagle's
"Collective as Actor": a collective that wills a subject into being and sustains it.

## Identity-continuity as a contested claim

Here the contemplative traditions hand GMEOW its deepest borrowing. Buddhism's *anattā* (no enduring
self — the apparent self is a bundle of momentary processes, the *skandhas*) and the Hindu *ātman*
(an enduring self that transmigrates across lives) are the two answers to a question GMEOW faces
directly: **is the upgraded system the same subject?** Reincarnation across bodies is the same
question as a subject persisting across model upgrades and host migrations.

GMEOW **refuses to adjudicate it** — but, as the review made clear, the refusal must be *modeled*, not
left to the absence of an assertion. The canonical form is the stage / lineage / assessment model
defined in [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md), not a single durable node with an optional
`counterpartOf`:

- A subject's epochs are distinct `gmeow:SubjectStage` individuals, grouped by a `gmeow:SubjectLineage`
  (the durable identity record) — **without** asserting a numerically identical bearer, which a single
  reused RDF node would do.
- Whether two stages are the same subject is a `gmeow:IdentityContinuityAssessment` (an `Observation`):
  the *ātman* reading is an **asserted** `same` verdict in a continuity-affirming frame; the *anattā*
  reading is an **asserted** `different` verdict in a no-self frame. Both are present as claims; neither
  is the absence of the other. This is the key correction over the first draft, which read the no-self
  position as the *absence* of a `counterpartOf` — but under the open-world assumption absence is
  silence, not denial, and GMEOW makes denial first-class (refutation).
- The *skandha* decomposition — form, sensation, perception, mental formations, consciousness — is the
  de-conflation itself: the apparent unified "agent" is subject status + memory + embodiment + runtime +
  invocations. The no-self view is therefore structurally representable (there is no single self-node,
  only the lineage's stages and the aggregate), and the enduring-self view is representable as a `same`
  continuity assessment. Both are expressible; neither is privileged, and neither is `owl:sameAs`.

The self/no-self debate is exactly the contested-instance-identity problem GMEOW's coreference
discipline was built for — held here by `SubjectLineage` + `IdentityContinuityAssessment`, asserting
neither a shared bearer nor a silent denial.

## Scope and seams

This document defines the layering and the genesis. *The relation binding a subject to a host* is
[`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md). *The AI realization of the session/episode layer*
is [`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md). *The frame-relative discipline that
keeps the tulpa, the incarnation, and the possession claims neutral as to metaphysics* is
[`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md).
