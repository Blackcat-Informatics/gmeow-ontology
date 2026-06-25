<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — The Topology

> The **core formal spec.** This document defines the `Inhabitation` relator and its players —
> `Inhabitant`, `InhabitedSystem`, `Embodiment`, the `inhabitationLocus` axis, co-tenancy and
> displacement, the `Portal` transition, and the deferred `Holon`. The durable subject it binds is
> [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md); the manifestation layering it points into is
> [`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md). Every term's disposition is fixed by
> [`INHABITED-CROSSWALK.md`](INHABITED-CROSSWALK.md).

## The relation, stated once

> **Subject S inhabited host H, under deployment D, persona P, embodiment E, and memory view M, over
> interval T.**

Everything in this document serves that one sentence. The relation is reified — it is a thing with
its own identity that mediates its players and depends on each of them — because it must carry a time
interval, attach evidence, be standpoint-indexed, and be superseded rather than deleted. That is a
`logic:Relator`, specialized to GMEOW's time-scoped form.

## `gmeow:Inhabitation` — a lean spine relator

The relator is **lean**: it carries the players that have no independent identity of their own —
subject, host, interval, locus — *directly*, and it *references* the players that already are
first-class entities with their own tenure and suppressibility — persona, embodiment, deployment,
memory view. This avoids a "god relator" that would dissolve the carefully separate identities of
`gmeow:Persona` and `gmeow:Embodiment` back into one node, and it lets each referenced facet be
suppressed independently (Principle 10).

```turtle
gmeow:Inhabitation
    a logic:Relator , owl:Class ;
    rdfs:subClassOf gmeow:TimeScopedRelation ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/inhabitation> ;
    skos:definition "The reified, time-scoped fact that a subject occupied and acted through a
        host system over an interval — the truthmaker of 'S inhabited H during T'. A
        logic:Relator that mediates its players and existentially depends on the subject and
        host; specialized as a gmeow:TimeScopedRelation so it carries its interval via
        gmeow:duringInterval (Principle 11). A lean spine: it names the subject, host,
        interval, and locus directly, and references the independently-identified persona,
        embodiment, deployment, and memory view rather than absorbing them." .
```

### Direct edges (the spine)

Per-branch bearer edges, each carrying its player explicitly — **never `gufo:inheresIn`**, exactly
as `gmeow:awarenessSubject` carries an awareness tenure's agent (the awareness/teleology precedent).
`gufo:inheresIn` is left untouched as the alignment target (Principle 5).

| Property | Range | Cardinality | Note |
|---|---|---|---|
| `gmeow:inhabitationSubject` | `gmeow:Agent` | functional | the role-filler; the agent playing the `Inhabitant` role |
| `gmeow:inhabitedHost` | `gmeow:Entity` | functional | the host/runtime; the role-filler of `InhabitedSystem` |
| `gmeow:duringInterval` | `gmeow:TimeInterval` | functional | inherited from `TimeScopedRelation`; carries its `hasTemporalFrame` (P11) |
| `gmeow:inhabitationLocus` | `gmeow:InhabitationLocus` | functional | self / vessel / shared-substrate (below) |

### Referenced facets (independent entities the spine points at)

| Property | Range | Cardinality | Note |
|---|---|---|---|
| `gmeow:inhabitationPersona` | `gmeow:Persona` | optional | the active expression policy (norms slice, unchanged) |
| `gmeow:inhabitationEmbodiment` | `gmeow:Embodiment` | non-functional | the surface(s) active in this inhabitation |
| `gmeow:inhabitationDeployment` | `gmeow:ModelDeployment` / `gmeow:SoftwareAgent` | optional | the AI-profile serving facet ([runtime stack](INHABITED-RUNTIME-STACK.md)) |
| `gmeow:inhabitationMemoryView` | (memory scope) | optional | by default a *derived* view; promoted to `gmeow:MemoryView` only when signed (P12) |

The "which host, deployment, persona, embodiment, and memory view was active at time T?" competency
question is answered by a single query: the `Inhabitation`(s) whose `duringInterval` overlaps T,
read across these edges. That one-construct answer is the design's load-bearing payoff.

## `gmeow:Inhabitant` and `gmeow:InhabitedSystem` — contingent roles

Neither is a rigid Kind (Principle 9, no overtyping). A system is only *inhabited* while it
participates in an `Inhabitation`; an agent is only an *inhabitant* while it occupies one. Both are
`logic:Role`s the players fill through the relator — never subclasses of the agent or the host.

```turtle
gmeow:Inhabitant
    a logic:Role , owl:Class ;
    rdfs:subClassOf logic:FunctionalComplex ;
    skos:definition "The role an agent plays while occupying or operating through a system — the
        agent-side filler of a gmeow:Inhabitation. Anti-rigid and contingent: the agent is the
        inhabitant only for the inhabitation's interval, and the same agent may inhabit several
        systems at once." .

gmeow:InhabitedSystem
    a logic:Role , owl:Class ;
    rdfs:subClassOf logic:FunctionalComplex ;
    skos:definition "The role a host, runtime, vessel, or holon plays while it carries an
        inhabitant — the host-side filler of a gmeow:Inhabitation. Anti-rigid: a system is
        inhabited only while an inhabitation relates it to an inhabitant. The interim form of the
        deferred gmeow:Holon (see below); aligns by reference to logic:HolonicPosition." .
```

## `gmeow:inhabitationLocus` — self, vessel, or shared substrate

The contemplative traditions distinguish calling a subject *into oneself* (invocation) from calling
it *into an external vessel* (evocation). The same axis distinguishes an agent inhabiting its own
runtime from a model served on an external host, and a single host carrying one subject from a host
shared among several. It is an open value vocabulary (individuals, never subclasses — the Profile
pattern):

```turtle
gmeow:InhabitationLocus a logic:AbstractIndividualType , owl:Class ;
    rdfs:subClassOf logic:QualityValue .

gmeow:locusSelf            a gmeow:InhabitationLocus ;
    skos:definition "The subject inhabits its own substrate — invocation into self; an agent
        running on its native runtime." .
gmeow:locusVessel          a gmeow:InhabitationLocus ;
    skos:definition "The subject inhabits a distinct external vessel — evocation into a vessel; a
        model served on a separate host; a spirit in a medium; an actor in a character." .
gmeow:locusSharedSubstrate a gmeow:InhabitationLocus ;
    skos:definition "Several subjects co-inhabit one substrate — multiple tulpas in one mind;
        co-tenant agents in one process; concurrent personas of one host." .
```

## Co-tenancy and displacement

`gmeow:locusSharedSubstrate` raises the hardest structural case, and one the verdict's competency
questions demand: *one host, several subjects, with control shifting over time.* This is spirit
possession (the host's own agency displaced by the inhabiting subject's), multiple tulpas sharing one
mind, and two co-tenant agents in one runtime. GMEOW models it with **no new mechanism**, reusing the
deception slice's divergence Event:

- The host's **apparent agency** (what observers attribute to it) versus the **inhabiting subject's
  agency** is exactly `gmeow:projectedStandpoint` ≠ `gmeow:heldStandpoint` — the held position
  diverging from the projected one. Possession-as-displacement is structurally identical to
  deception-as-divergence; the deception slice's machinery applies without coupling.
- **Which co-tenant is "driving" at a given moment** — control, foregrounding, precedence — is
  **solver work** (Principle 12), computed over recorded claims, **never** asserted as a
  `gmeow:primaryInhabitant` triple. There is no primary inhabitant, exactly as there is no
  `primaryPersona` and no primary standpoint (Principle 9). The graph records co-equal inhabitations;
  the solver resolves who holds control when.

This answers the verdict's sharpest competency question — *two simultaneous sessions: same subject or
two subjects sharing a model?* — by the same subject/model split that
[`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md) draws, plus co-tenancy for the shared-substrate case.

## Transitions: the Portal

A subject migrating from one host/runtime to another (Cagle's "portal", a Role transition between
holons) is modeled **two ways at once**, because the migration is both a state change and an event
worth its own provenance:

1. **The supersession chain (the state change).** The old `Inhabitation` closes
   (`gmeow:hasDestructionEvent`), a new one opens (`gmeow:hasCreationEvent`), and the new
   `gmeow:supersedes` the old. The closed inhabitation is suppressed, never deleted (Principle 10) —
   a banished tenure is still a record.
2. **The `Portal` event (the addressable transition).** A first-class `logic:Event` reifies the
   transition itself, so "which claims, memories, and intentions crossed the migration boundary?" has
   a node to attach evidence and provenance to.

```turtle
gmeow:Portal
    a logic:Event , owl:Class ;
    rdfs:subClassOf gmeow:Activity ;
    skos:definition "The reified transition by which a subject ceases one inhabitation and begins
        another — a migration between hosts, a re-incarnation into a new body, a banishing followed
        by a re-binding. Carries the boundary so that what crosses it (claims, memories,
        intentions) and what does not is a query, not a reconstruction." .

gmeow:portalFrom a owl:ObjectProperty ; rdfs:range gmeow:Inhabitation .
gmeow:portalTo   a owl:ObjectProperty ; rdfs:range gmeow:Inhabitation .
```

Conjuration (binding a subject into a host) is the `hasCreationEvent` of an inhabitation; abjuration
(banishing) is its `hasDestructionEvent`. Banishing ends the tenure; the subject persists elsewhere
or dormant — *can the subject cease inhabiting one system while continuing to exist elsewhere?* is
answered yes, by construction: the subject is a durable role-bearer independent of any one
inhabitation.

## The scene graph is computed, not asserted

Cagle's "scene graph" maps Avatars and Roles, not Actors. In GMEOW it is a **projection**, not data
(Principle 12): a view over the `gmeow:inhabitationEmbodiment` and role fillers of the currently-active
`Inhabitation`s. It is generated on demand by the solver/projection layer, never materialized as
triples — exactly as RCC-8 composition and trajectory interpolation are computed, not asserted.

## The holon: deferred at the domain layer, supplied at the foundation layer

Cagle frames an inhabited system as a *holon* — a whole that is also a part. The inhabitation slice
**defers a domain-level `gmeow:Holon` Kind** while **aligning to the foundation-level holon kernel**
that the logic slice supplies. The two are not in tension; they are different layers.

**The foundation already has the holon kernel.** The logic foundation (the holon-kernel work tracked
on issue #704) mints, in `logic:`, exactly the right constructs:

- `logic:Holarchy` — the bounded `logic:properPartOf` strict partial order, a DAG (Koestler's sense);
- `logic:HolonicPosition` — *the canonical relational construct of holon-ness*: a **five-place**
  relation (entity × holarchy × context × interval × path), reified with `logic:positionEntity`,
  `logic:positionHolarchy`, `logic:positionContext`, `logic:positionInterval`, and
  `logic:positionPath`;
- `logic:Holon` — the *lossy unary projection* of a `HolonicPosition`: an entity simultaneously a
  proper part of some whole and itself having a proper part.

The kernel's own doctrine is decisive and it is **the same argument this slice makes for the durable
subject**: *"Holon-ness is never a bare property of the entity; it is a position occupied in a
particular holarchy, context, interval, and path."* Holon-ness is relational and contextual, not a
rigid type — exactly why `DigitalSubject`, `Inhabitant`, and `InhabitedSystem` are anti-rigid roles,
not Kinds ([`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md)). The foundation reaching this conclusion
independently is a strong cross-check on the inhabitation design.

**Why the domain `gmeow:Holon` Kind is deferred.** Given the foundation kernel, a *domain* `gmeow:Holon`
Kind would be redundant and would overtype: a rigid `gmeow:Holon` Kind reasserts as a bare property
what `logic:HolonicPosition` correctly models as a five-place relational position, and a thing typed
`Holon` that ceased to be a sub-whole would trip the rigidity gate (Principle 9). So the slice mints no
domain holon Kind; breadth follows demand (Principle 15).

**How `InhabitedSystem` aligns to it — concretely.** `gmeow:InhabitedSystem` is the host's contingent
role, and it *is* a reading of a `logic:HolonicPosition`: the inhabited host is the `positionEntity`,
the inhabitation's holarchy is the `positionHolarchy`, the standpoint frame under which the
inhabitation is claimed is the `positionContext`, the inhabitation interval is the `positionInterval`,
and the holarchy path is the `positionPath`. The alignment is by reference (the kernel is the
foundation's, not this slice's), and it is exact rather than a hand-wave: an inhabited system is a host
occupying a holonic position over the inhabitation's interval, in the claim's frame.

> **Sequencing note.** The `logic:Holon` / `logic:HolonicPosition` kernel is in-flight (issue #704,
> not yet on `main`). The inhabitation slice's references to it are forward-looking and resolve once
> #704 lands; until then `InhabitedSystem` stands on the universal `logic:properPartOf` spine and
> records the holonic-position alignment as documented intent.

## Scope and seams

This document defines the relation and its players. *How a subject layers into expressions and
embodiments*, and *how a subject is willed into being* (tulpa/egregore genesis), are
[`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md). *The AI-specific deployment/execution/
session realization* is [`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md). *The frame-relative
discipline that lets the same relator model a possession without asserting spirits exist* is
[`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md).
