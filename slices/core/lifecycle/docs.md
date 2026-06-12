<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Lifecycle — existence over time, for anything

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/lifecycle` · **tier: core**
> Birth, founding, minting, death, dissolution, retirement — one facility for the existence of every entity.

Universal time-bounded existence facility: ontic existence-over-time for any gmeow:Entity, distinct from suppression (a display contract). Every entity may carry creation and destruction events, an existence interval, and supersession links. Revised forward, never deleted (Principle 10). Reuses the events module (gmeow:Event + EventType value vocabulary), the temporal module (TimeInterval / TimeScopedRelation / validFrom / validUntil), and the standpoint module (accordingTo / contested claims). Flat-first, reify on demand.

The slice draws one line hard: **destruction is an ontic fact; suppression is a display
contract; deletion does not exist.** A dissolved organization, a demolished place, a
retired reference frame — all remain in the graph, marked as no longer extant, because
the past is data (Principle 10). And because the model is event-typed by value
(Principle 9), no per-kind machinery is minted: a person's birth event is the same
`gmeow:Event` carrying both `gmeow:eventTypeBirth` and `gmeow:eventTypeCreation` —
co-equal values on one occurrence, never a subclass tower.

Flat-first governs the shape: the three direct hooks below cover the 80 % case; the
reified `gmeow:EntityExistence` is the promotion target when the existence claim itself
needs identity — contested bounds, attached evidence, standpoint indexing.

## The flat hooks (the 80 % case)

### gmeow:hasCreationEvent

Links any entity to the event that brought it into existence — the general form of
birth (person), founding (organization), minting (currency), or realization (reference
frame). Non-functional: competing standpoint-indexed creation claims coexist, none
privileged. The event side carries `gmeow:eventTypeCreation` (events slice), alongside
any more specific value such as `gmeow:eventTypeBirth`.

### gmeow:hasDestructionEvent

The mirror: the event that ended the entity's existence — death, dissolution,
destruction, retirement. Non-functional for the same standpoint reasons. Destruction is
ontic; it never implies `gmeow:displayable` false, and `gmeow:displayable` false never
implies destruction.

### gmeow:existenceInterval

The entity's span of existence as a `gmeow:TimeInterval` (temporal slice) — open-ended
(no end instant) while the entity is extant. Non-functional: in a multi-source merge,
different sources give different bounds, and those claims coexist as standpoint-indexed
statements. The interval carries its frame per Principle 11.

### gmeow:supersededBy

Links an entity to the entity that replaced it — Constantinople `supersededBy`
Istanbul, a deprecated release `supersededBy` its successor. The declared inverse of
the coreference slice's `gmeow:supersedes`; directional, never symmetric. The
superseded entity is retained (Principle 10) and may carry `gmeow:displayable` false;
the replacement itself may be recorded as an event typed
`gmeow:eventTypeSupersession` when it needs a date or participants.

## The reified form (promote on demand)

### gmeow:EntityExistence

The time-scoped fact that an entity existed over an interval — a gufo:Situation
specializing `gmeow:TimeScopedRelation`, carrying its period via
`gmeow:duringInterval`. Promote here when the existence claim is *contested* (two
sources disagree on when a place ceased), when evidence must attach to the claim
itself, or when standpoint indexing is needed. Open-world EL axioms require some
`gmeow:existenceEntity`; the closed-world "exactly one" is SHACL's
(`EntityExistenceHasEntityShape`).

### gmeow:existenceEntity

The functional post: exactly one entity per existence record — constitutive of the
situation's identity.

### gmeow:existenceCreationEvent · gmeow:existenceDestructionEvent

Optional event hooks on the reified record: an EntityExistence may record only an
interval when the creation event is unknown, and a still-extant entity's record simply
has no destruction event. Absence of an end is a statement about the world, not a gap
in the data.

## Solver layer & alignment

Existence *consistency* — creation precedes destruction, the interval brackets both,
participation does not outlive the participant — is gate and solver work (Principle
12): the slice asserts the claims; verification queries and the temporal solver check
their coherence on the reasoned graph. No external lifecycle vocabulary is imported;
the facility is expressed entirely through the events, temporal, and standpoint
machinery it reuses, with bridges riding those slices' alignments (Principle 5).

## Dependencies

Depends on `kernel`, `coreference` (`supersedes`), `events` (Event + the
creation/destruction/supersession type values), and `temporal` (TimeInterval,
TimeScopedRelation, the statement clocks). Consumed by birth/death and existence
claims for persons, places, organizations, and artifacts across the corpus.
