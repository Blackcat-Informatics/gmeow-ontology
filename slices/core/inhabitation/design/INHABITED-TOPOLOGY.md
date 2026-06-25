<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — The Topology

> The **core formal spec.** This document defines the inhabitation relation and its players. It was
> revised after the foundational review (see [`INHABITED-REVIEW.md`](INHABITED-REVIEW.md) for the
> full disposition): the inhabitation is a **situation/tenure**, not a relator subclassing a
> situation; embodiment splits into a carrier role and a time-scoped assignment; locus is two
> orthogonal axes; control is its own observation, not deception. The durable subject it binds is
> [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md); the manifestation layering it points into is
> [`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md). Dispositions are fixed by
> [`INHABITED-CROSSWALK.md`](INHABITED-CROSSWALK.md).

## The relation, stated once

> **Subject S stood in an inhabitation relationship with host H over interval T, in a configuration
> (persona P, embodiment E, deployment D, memory view M) that itself held over sub-intervals of T.**

The foundational correction that shapes everything below: GMEOW's `gmeow:TimeScopedRelation` is a
`logic:Situation` (`temporal/module.ttl`), and a `logic:Situation` is disjoint from a `logic:Relator`
(an endurant aspect). The earlier draft typed `Inhabitation` as both, which is unsatisfiable in the
foundational projection. The relationship-over-time is therefore a **situation/tenure**, and the
configuration that varies within it is a **second, finer situation** — which also fixes the
"active-at-T" competency question (a single tenure could not say which persona held at T if the
persona changed mid-tenure).

## `gmeow:InhabitationTenure` — the relationship over time

```turtle
gmeow:InhabitationTenure
    a logic:Situation , owl:Class ;
    rdfs:subClassOf gmeow:TimeScopedRelation ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/inhabitation> ;
    skos:definition "The reified, time-scoped fact that a subject stood in an inhabitation
        relationship with a host over an interval — that S inhabited H during T. A
        gmeow:TimeScopedRelation (a logic:Situation), carrying its interval via
        gmeow:duringInterval (Principle 11), with per-branch role-filler edges to the subject and
        host. It records the relationship's existence over T; the configuration that held within T
        (persona, embodiment, deployment, memory view) is carried by gmeow:InhabitationConfiguration,
        because those facets may change while the tenure persists." .
```

Per-branch role-filler edges — never `gufo:inheresIn` (the `awareness`/`teleology` precedent):

| Property | Range | Cardinality | Note |
|---|---|---|---|
| `gmeow:inhabitationSubject` | `gmeow:Agent` | functional | the inhabitant (the agent filling the `Inhabitant` role) |
| `gmeow:inhabitedHost` | `gmeow:Entity` | functional | the host (filling the `InhabitedSystem` role) |
| `gmeow:duringInterval` | `gmeow:TimeInterval` | functional | inherited; carries its `hasTemporalFrame` (P11) |
| `gmeow:inhabitationLocusKind` | `gmeow:InhabitationLocusKind` | functional | self vs vessel (below) |

A note on the relator question. GMEOW models some relationship objects as *relators* that carry an
interval via a property (e.g. `gmeow:Membership a logic:Kind ⊑ logic:Relator`, carrying its period),
and others as *situations* `⊑ TimeScopedRelation` (e.g. `gmeow:AwarenessTenure`). For inhabitation the
situation form is chosen, because the configuration-varies-within-tenure structure is most naturally a
nest of situations. If a future consumer needs a persistent relationship object distinct from any one
tenure (an `InhabitationBond ⊑ Relator` that several tenures are tenures *of*), that is added then —
the tenure alone is sufficient for every example in this set (Principle 15).

## `gmeow:InhabitationConfiguration` — the facets, time-scoped

The configuration is a **maximal interval of constant configuration**: a sub-situation of a tenure
during which the active persona, embodiment, deployment, and memory view do not change. When any facet
changes, a new configuration opens. This is what makes "which persona/embodiment/deployment/memory
view was active at T?" answerable: it is the configuration whose interval contains T.

```turtle
gmeow:InhabitationConfiguration
    a logic:Situation , owl:Class ;
    rdfs:subClassOf gmeow:TimeScopedRelation ;
    skos:definition "A maximal sub-interval of an inhabitation tenure over which the active
        configuration is constant — the persona, embodiment(s), deployment, and memory view that
        held together over that sub-interval. A new configuration opens whenever any facet changes,
        so the active value of any facet at a time T is read off the configuration whose interval
        contains T. The invariant 'constant configuration over the interval' is stated and tested,
        not assumed." .

gmeow:configurationOfTenure a owl:ObjectProperty ; rdfs:range gmeow:InhabitationTenure .
gmeow:configurationPersona  a owl:ObjectProperty ; rdfs:range gmeow:Persona .          # expression profile
gmeow:configurationEmbodiment a owl:ObjectProperty ; rdfs:range gmeow:EmbodimentAssignment .
gmeow:configurationDeployment a owl:ObjectProperty ; rdfs:range gmeow:ModelDeployment . # AI profile
gmeow:configurationMemoryView a owl:ObjectProperty .   # derived view, or signed MemoryView (P12)
```

The facets the configuration references — `Persona`, `ModelDeployment` — live in the **expression** and
**AI** profiles, not the minimal core (see [`INHABITED-CONSUMER.md`](INHABITED-CONSUMER.md) on the
core/profile split that the review required).

## `gmeow:Inhabitant` and `gmeow:InhabitedSystem` — contingent role-mixins

A system is only *inhabited* while it stands in an inhabitation; an agent is only an *inhabitant*
while it occupies one. Both span multiple Kinds (an inhabitant may be a `Person` or a `SoftwareAgent`;
a host may be a `PhysicalObject` or a `SoftwareAgent`), so both are **`logic:RoleMixin`** (anti-rigid
non-sortals spanning Kinds), not `logic:Role` (which is a sortal tied to one Kind). This is the same
correction applied to `DigitalSubject` in [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md).

```turtle
gmeow:Inhabitant       a logic:RoleMixin , owl:Class ; rdfs:subClassOf logic:FunctionalComplex .
gmeow:InhabitedSystem  a logic:RoleMixin , owl:Class ; rdfs:subClassOf logic:FunctionalComplex .
```

## Locus: two orthogonal axes, not one

The earlier draft collapsed two independent questions into one value vocabulary. They are orthogonal —
a self-hosted runtime can be shared, an external vessel can be exclusive — so they are two axes:

- **`gmeow:inhabitationLocusKind`** (subject↔host relationship): `gmeow:locusSelf` (the subject
  inhabits its own substrate — invocation into self) vs `gmeow:locusVessel` (a distinct external
  vessel — evocation into a vessel). An open value vocabulary.
- **Tenancy cardinality** is **derived, not asserted**: a host is shared when two inhabitation
  tenures over it have overlapping intervals; exclusive otherwise. This is computed (Principle 12),
  never a `gmeow:locusSharedSubstrate` value, because it is a property of the *set* of tenures, not of
  any one inhabitation.

## `gmeow:Embodiment` — carrier role versus assignment

A device, avatar, account, terminal, or voice endpoint is an entity (or an entity in a role); the fact
that a subject *acts through it over time* is a time-scoped situation. Naming both "Embodiment"
recreated a conflation. They split:

```turtle
gmeow:EmbodimentCarrierRole
    a logic:RoleMixin , owl:Class ;
    skos:definition "The role an entity plays as the surface a subject acts and is perceived through —
        a device, avatar, account, terminal, voice endpoint, or channel. A RoleMixin: the same kind
        of carrier (an account, a robot) is a carrier only while it surfaces a subject." .

gmeow:EmbodimentAssignment
    a logic:Situation , owl:Class ;
    rdfs:subClassOf gmeow:TimeScopedRelation ;
    skos:definition "The time-scoped fact that a subject acts through a carrier over an interval, with
        the capabilities that carrier exposes — subject × carrier × interval × capabilities. The
        suppressible, retirable unit (Principle 10): retiring an assignment ends the surface's use, it
        does not delete the carrier." .

gmeow:assignmentSubject  a owl:ObjectProperty ; rdfs:range gmeow:Agent .
gmeow:assignmentCarrier  a owl:ObjectProperty ; rdfs:range gmeow:EmbodimentCarrierRole .
gmeow:assignmentCapability a owl:ObjectProperty .   # the capabilities this surface exposes
```

"Avatar" (Cagle) and the Sanskrit *avatāra* it descends from bridge to the **carrier**; the
descent-into-a-surface relation is the **assignment** ([`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md)).

## Co-tenancy and control — control is not deception

One host, several subjects, control shifting (possession, multiple tulpas, co-tenant agents) is real,
but the earlier draft modeled control with the deception slice's `heldStandpoint` ≠
`projectedStandpoint`. That records a divergence between what an agent *holds* and what it *presents* —
**not** which co-tenant causally controls the embodiment. Control is its own observation:

```turtle
gmeow:ControlAssessment
    a logic:Situation , owl:Class ;
    rdfs:subClassOf gmeow:Observation ;
    skos:definition "A standpoint-indexed, attributed, dated observation of which agent causally
        controls a host or embodiment over an interval, to what degree — the agency attribution a
        consumer needs to answer 'who was driving at T'. Distinct from the deception divergence
        (held ≠ projected standpoint), which records belief-versus-presentation, not control: a
        documented similarity, no axiom coupling." .

gmeow:controlAgent a owl:ObjectProperty ; rdfs:range gmeow:Agent .
gmeow:controlOver  a owl:ObjectProperty .       # the host or embodiment assignment
gmeow:controlDegree a owl:DatatypeProperty .    # partial / full / contested
gmeow:controlInterval a owl:ObjectProperty ; rdfs:range gmeow:TimeInterval .
```

A solver cannot compute control without control observations; there is no `gmeow:primaryInhabitant`
(Principle 9), and precedence among co-tenants is resolved over recorded `ControlAssessment`s, never
asserted (Principle 12). The deception divergence remains available as a *separate* claim where a
co-tenant's presented agency differs from its held one — a documented analogy, not the control model.

## Transitions: migration as a lifecycle event

A subject migrating host/runtime (Cagle's "portal") is a **lifecycle-family event**, modeled with the
`eventType` value pattern that birth/death/migration already use — not a new Event subclass. (GMEOW
does have Event subclasses — `ModelInvocation`, `ToolCall`, `Commit` — but those are computational/
provenance events; a migration is lifecycle, so it takes the value form for consistency.)

```turtle
ex:migration-7 a gmeow:Event ;
    gmeow:hasEventType gmeow:eventTypeInhabitationTransition ;
    gmeow:portalFrom ex:tenure-before ;
    gmeow:portalTo   ex:tenure-after ;
    gmeow:atTime "..."^^xsd:dateTime .
```

Two corrections the review required:

- **Ending a tenure is ontic; it does not suppress.** Closing an inhabitation
  (`gmeow:hasDestructionEvent`) is an ontic fact; it does **not** entail `gmeow:displayable false`.
  Suppression is a separate display contract (Principle 10) applied only when a value must be withheld
  — the lifecycle slice's exact discipline. The earlier "banishing = suppression" wording conflated
  the two; banishing *ends* a tenure, and may *additionally* be suppressed, but the two are
  independent.
- **Crossing a boundary requires evidence, not coincidence.** Seeing the same claim before and after a
  transition does **not** establish that it crossed — it may have been regenerated independently.
  Migration content is carried by a `gmeow:TransferManifest` (or per-claim derivation provenance)
  that records what was transferred ([`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md#the-migration-boundary)).

## The scene graph is computed, not asserted

Cagle's "scene graph" is a **projection** (Principle 12): a view over the active configurations'
embodiment assignments and role fillers, generated on demand, never materialized as triples.

## The holon: deferred at the domain layer, supplied at the foundation layer

A first-class *domain* `gmeow:Holon` Kind is deferred; the **foundation** holon kernel (issue #704,
in-flight) supplies `logic:Holarchy`, `logic:HolonicPosition` (the five-place entity × holarchy ×
context × interval × path relation), and `logic:Holon` (its lossy unary projection). The kernel's
doctrine — *"holon-ness is never a bare property of the entity; it is a position"* — is the same
anti-rigidity argument this slice makes, an independent cross-check. `gmeow:InhabitedSystem` aligns to
`logic:HolonicPosition` (host = `positionEntity`, tenure interval = `positionInterval`, claim frame =
`positionContext`), by reference. The references are forward-looking until #704 lands
([`INHABITED-REFERENCES.md`](INHABITED-REFERENCES.md)).

## Scope and seams

This document defines the relation and its players. The durable subject and continuity are
[`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md); the layering and genesis are
[`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md); the AI realization is
[`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md); the frame-relative neutrality (contested
inhabitation as an unasserted claim) is [`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md); the
full review disposition is [`INHABITED-REVIEW.md`](INHABITED-REVIEW.md).
