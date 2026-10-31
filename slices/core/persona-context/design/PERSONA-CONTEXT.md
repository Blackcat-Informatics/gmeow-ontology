<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Persona attribution and transition records

The unit of attribution is a particular record. The unit of change is a
source-scoped information record retained by an activity. Existing `Persona`
objects remain the sole expression-policy vocabulary, with their existing
`personaBearer` and register obligations. `PersonaSlot` identifies a producer
stream for a bearer; it does not supply a rival persona, a general state calculus
or the logical-path meaning of the existing `State` class.

## Identity and scope

A transition belongs to one slot and retains one previous persona, one replacement,
one producer clock reading and one source sequence value. A supplied duration is
also singular. Canonical property-characteristic records express those intrinsic
record fields. They do not make `wornModality`, cause, boundary or audience-ring
attributions functional. Several causes or audience contexts can explain one
change, and independently sourced transition accounts remain separate records.

A slot record has one bearer. Distinct producers and restarted counters use
distinct slot identities, including when the bearer and human-readable slot name
are unchanged. Sequence is comparable within that identity and may continue
across several containing activities. No formula orders timestamps or creates
missing sequence values. No uniqueness axiom merges records sharing a sequence
number, and no axiom converts sequence to `entryOrdinal`.

A ring member is specific to one consumer scheme and retains one local rank.
Another scheme can give a similar audience description a different member and
rank. Equal ranks within a scheme are allowed. There is no closed enumeration of
rings and no rank-to-person relationship inference. Scheme-scoped singular fields
make each consumer’s declaration unambiguous without selecting a global scheme.

## Canonical obligations

Principles 4 and 17 put the declarations and obligations in `module.ttl`.
OWL, SHACL and ShEx remain generated views. No surface constraint is authored
separately.

| Object or relation | Authored obligation |
| --- | --- |
| `wornModality` | Domain-free relation with `Persona` range, deliberately nonfunctional. |
| `ModeTransition` | `InformationObject`, with class-scoped closed-world required paths for slot, previous persona, replacement, clock, sequence and cause. |
| Owning activity | Realized guarded-existence formula requiring an incoming `modeTransition` from a containing activity. The property’s domain is existing `Activity`. |
| Slot bearer | Required `Agent` reference and a canonical functionality record. |
| Transition identity fields | All-values restrictions and canonical functionality records; no reasoner-created existential witnesses for required fields. |
| Persona bearer consistency | Realized integrity formulas compare the previous and replacement personas’ bearers with the slot bearer. |
| Optional duration | `xsd:duration` all-values restriction and at most one source-reported value, measured or computed with stated derivation; no required path or GMEOW-computed elapsed value. |
| Automatic boundary | Realized guarded-existence formula requires a boundary when that cause is attributed. |
| Audience-forced change | Realized guarded-existence formula requires an audience ring when that cause is attributed. |
| Audience member | Required scheme reference and nonnegative rank with intrinsic singular fields; no closed value list. |

Required paths use `logic:allValuesFrom` with explicit class/property closure
entries. The functionality records set the separate at-most-one bounds. The
activity and conditional obligations use actual `logic:Constraint` records with
realized `logic:Formula` trees. They neither mint missing source data nor replace
provenance with guessed defaults.

The two bearer constraints join the existing `personaBearer` vocabulary.
They do not infer that one persona is the bearer’s authentic identity. The
existing core persona obligations supply bearer and register presence; this
slice consumes them by reference rather than restating their definitions.

## Distinct meanings remain distinct

`activatedIn` describes the condition or situation type that activates a persona.
It is not the inverse of record attribution. No axiom bridges it to
`wornModality` or infers a transition from an activating condition alone.

`transitionAt` is a producer-clock observation of the change; `recordedAt` is
creation time of a record at supplied precision. Duration is the source-reported
tenure of the single replaced persona. A source-computed span is admitted when
the exact duration assertion’s native RDF 1.2 reifier links through
`wasDerivedFrom` to the actual opening and closing state-claim reifiers and
retains the source’s calculation and input roles through `statedRationale`.
These existing domain-free relations preserve exact claim provenance without
introducing an arithmetic executor, a new derivation profile or a general state
calculus (Principles 2, 4 and 17). No law here claims to inspect nested claim
tuples or verify subtraction, clock compatibility, precision or timezone.
Original source values and attribution remain intact; missing duration or clock
information is never synthesized.

The record is not an `Event` or `Activity`, and the event-domain `hasDuration`
property is not applied to it.

The causes and boundaries are open value vocabularies under `logic:QualityValue`.
They preserve source explanations without implementing a hold system, audience
policy or lifecycle scheduler. The distinction between periodic wake-up and
waking follows the producer’s account, without a biological assertion.

## Source ownership

This core slice depends only on the core grounding, kernel, norms, provenance,
record-metadata and names slices. It references their existing terms without
moving their declarations or copying their source examples. Names are consumed
for the personas’ register values in the illustrative scene. The manifest is the
single identity, tier and dependency declaration for this slice.

## Audience register remains distinct from worn state

personaMode has an open domain and Persona range, without functionality or a
subproperty bridge to wornModality. Audience-facing register and worn state are
separate source claims about existing bearer-bound personas (Principles 4 and 9).
The relation supplies no identity resolution, preference, deception judgment or
automatic persona transition.
