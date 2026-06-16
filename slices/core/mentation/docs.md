<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# mentation

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/mentation` · **tier: core**

The occurrent (perdurant) half of mind — mental processes and experiences that unfold in time:
perceiving, reasoning, imagining, remembering, dreaming. The companion to the endurant mental-moment
stack (belief, knowledge, desire, emotion) in the cognition / epistemics family. Where the endurant
slices model the *states* an agent is in, this slice models the *events* that produce or update them.

In gUFO's ontological split, *endurants* are continuants that persist through time (modes, qualities,
states — gmeow:MentalMoment and its subclasses), while *perdurants* (occurrents) are processes and
experiences that have temporal parts and unfold by happening. GMEOW's endurant mental side is rich;
this slice completes the picture with the perdurant umbrella so the full arc — from process to
resulting state — can be queried as a single timeline.

## Classes

### gmeow:MentalProcess

A mental occurrence that unfolds in time — the perdurant (occurrent) counterpart of the endurant
`gmeow:MentalMoment`: a perceiving, a reasoning, an imagining, a remembering, a dreaming. The
kernel-level umbrella under which every mental event lives, so an agent's mental life can be queried
as a single occurrent stream. A `gmeow:Event` borne by exactly one agent (`gmeow:experiencer`); the
kind of process is a `gmeow:mentalProcessType` value, never a subclass (Principle 9).
`gmeow:Inference` (inference slice #581) and `gmeow:LearningEvent` (learning slice #584) reparent
under it from their own slices.

**Stereotype:** `owl:Class, gufo:EventType` · `⊑ gmeow:Event`

### gmeow:Experience

The phenomenally-conscious, qualia-bearing subset of `gmeow:MentalProcess` — a mental occurrence
there is something it is like to undergo: a perception-as-experienced, a felt emotion-episode, a
dream. Distinguished from sub-personal mental processing that carries no first-person character. The
one genuine subclass in this slice; finer kinds remain `gmeow:mentalProcessType` values.

**Stereotype:** `owl:Class, gufo:EventType` · `⊑ gmeow:MentalProcess`

### gmeow:MentalProcessType

The kind of a mental process — a closed-but-open value vocabulary (individuals, never subclasses;
the `gmeow:EventType` idiom): `gmeow:processPerception`, `gmeow:processAttention`,
`gmeow:processReasoning`, `gmeow:processImagining`, `gmeow:processDeliberation`,
`gmeow:processRecollection`, `gmeow:processMindWandering`, `gmeow:processDreaming`. New kinds are
minted as individuals, never as subclasses of `gmeow:MentalProcess` (Principle 9: no overtyping).

**Stereotype:** `owl:Class, gufo:AbstractIndividualType` · `⊑ gufo:QualityValue`

## Properties

### gmeow:experiencer

The agent whose mental process this is — the one undergoing the perceiving, reasoning, or dreaming.
Functional: one process, one experiencer (a mental occurrent inheres in exactly one agent; two agents
reasoning about the same thing are two processes).

**Domain:** `gmeow:MentalProcess` · **Range:** `gmeow:Agent` · **Functional**

### gmeow:mentalProcessType

The kind of a mental process — a `gmeow:MentalProcessType` value (`gmeow:processPerception`,
`gmeow:processReasoning`, `gmeow:processDreaming`, …). NOT functional: a single occurrence may
carry several type values (a reverie that is both imagining and mind-wandering). Mirrors
`gmeow:eventType`.

**Domain:** `gmeow:MentalProcess` · **Range:** `gmeow:MentalProcessType` · **Non-functional**

### gmeow:realizesMoment

Relates a mental process to a mental moment it produces or updates — the occurrent-to-endurant
bridge across the gUFO endurant/occurrent divide: a reasoning realizes a belief, a perceiving
realizes a perceptual claim. NOT functional — one process may settle several moments. Range is
intentionally **open** at this tier (the doxastic-spine precedent): it will point at a
`gmeow:MentalMoment` once the endurant umbrella lands with kernel/cognition #556; until then it may
point at any reified mode, belief-state, or claim the process produces. Renamed from the design's
`realizes` to avoid colliding with the WEMI `gmeow:realizes` (Expression → Work) — Principle 4.

**Domain:** `gmeow:MentalProcess` · **Range:** open (intentionally no `rdfs:range`) · **Non-functional**

## Value individuals — `gmeow:MentalProcessType`

### gmeow:processPerception

A perceptual episode — sensing and registering the environment (or an internal state) as it occurs.
The occurrent that typically realizes a perceptual observation or claim.

### gmeow:processAttention

An episode of selective attention — focusing cognitive resources on a subject, sensation, or task
while backgrounding the rest.

### gmeow:processReasoning

A reasoning episode — inference as it unfolds in time, drawing a conclusion from premises. The
occurrent face of a `gmeow:Inference` (inference slice).

### gmeow:processImagining

An episode of imagining — entertaining quasi-perceptual or suppositional content as-if, decoupled
from current perception and from belief (the imagination slice's faculty in action).

### gmeow:processDeliberation

A deliberation episode — practical weighing of options, reasons, or consequences toward a choice or
intention.

### gmeow:processRecollection

A recollection episode — retrieving a stored memory into present awareness (the act, not the stored
content). Forgetting is modelled as suppression elsewhere, not as a process here.

### gmeow:processMindWandering

A mind-wandering episode — undirected, stimulus-independent thought that drifts without a task goal
(daydreaming, spontaneous reverie).

### gmeow:processDreaming

A dreaming episode — an offline, typically sleep-bound `gmeow:Experience` of imagined content;
composed into the dreaming extension (#589) with awareness mode and content-origin.

## Doctrine highlights

- **Occurrent umbrella** — `gmeow:MentalProcess ⊑ gmeow:Event` is the single home for every mental
  occurrence; one SPARQL query returns an agent's whole mental timeline.
- **Value-vocab, not taxonomy** (Principle 9) — the kind of a process is a `gmeow:mentalProcessType`
  value (a `gmeow:MentalProcessType` individual), never a subclass of `gmeow:MentalProcess`. The one
  genuine subclass is `gmeow:Experience` — the phenomenally-conscious subset.
- **Open-range bridge** — `gmeow:realizesMoment` has no `rdfs:range` at this tier (the doxastic-spine
  precedent). It will range over `gmeow:MentalMoment` once the endurant umbrella lands with
  kernel/cognition #556; until then it points at any mode, belief-state, or claim the process
  produces.
- **Renamed from `realizes`** (Principle 4) — the design used `realizes`; renamed to `realizesMoment`
  to avoid colliding with the WEMI `gmeow:realizes` (Expression → Work) in the creative-works slice.
- **Reparenting hooks stay open** — `gmeow:Inference` (#581) and `gmeow:LearningEvent` (#584) declare
  `rdfs:subClassOf gmeow:MentalProcess` from their own slices, never pre-declared here.

## Dependencies

Depends on `kernel` (`gmeow:Agent`, `gmeow:Event` via `events`), `events` (the event spine and
`gufo:EventType` stereotype), `temporal`, and `observations`.
