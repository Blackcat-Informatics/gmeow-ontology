<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Refinement readings

These terms describe consumer-owned readings of records. `FirstOccurrence` and
`RelationOccasion` specialize the existing `Refinement` observation spine:
`observedFeature` names the source, `vantage` names the versioned reading process,
and `refinementOutcome` describes the interpretation. The result is the
process’s reading, never automatically the source subject’s assertion.

## Exact offered records and source chronology

A `ReadingScope` explicitly enumerates the source records offered to the selected
refiner through `scopeRecord`, including records it could not read. An empty
scope is valid. `scopeChronology` identifies the existing `TemporalFrame` in
which the original assertion-time values are interpreted. `scopeChronology`
specializes `hasReferenceFrame`, so the existing frame spine is inherited. The scope says which
records were offered; it asserts no completeness of the surrounding store.
`readingScope` associates this same scope with findings and limit aggregates
without making every aggregate an observation.

`FirstOccurrence` identifies an earliest source within that scope and declared
chronology. Ties can coexist. Missing or incomparable source times cannot be
repaired by assuming a clock value or claiming a global first. A process that
cannot support an earliest reading retains an unresolved refinement outcome
instead. The ontology records the attributed earliest claim; it does not execute
a search, certify that all offered records were interpretable or compare an
unrecorded source against it.

| Binding | Meaning |
| --- | --- |
| `relationPredicate` | Actual predicate IRI from the source relation. |
| `relationVault` | Store containing the source records; no access grant. |
| `relatedThing` | Actual referent IRI when the source supplies one. |
| `relatedName` | Original literal name when that is all the source supplies. |
| `relationAt` | Source assertion time at its supplied precision. |
| `relationIn` | Original occasion or context, such as a conversation. |
| `findingSummary` | A short account in its original language and direction. |

The two relational reading classes require a source, vantage, scope, predicate,
store, source time and either a related IRI or a source name. `relationAt` admits
a date-time, date, month or year without padding it into a more precise instant.
It is distinct from the refiner’s `observedAt` and from transaction-time
`recordedAt`. `relatedName` retains supplied language and base direction without
inventing a named referent. `findingSummary` requires a language-bearing literal,
including RDF 1.2 directional language strings.

## Milestones

A `Milestone` is an `InformationObject` annotation, attributed to its teller with
`wasAttributedTo`. Its `milestoneOf` points to the original source record and
specializes the existing `wasDerivedFrom` provenance relation. An open
`MilestoneKind` classifies the annotation. Consumers mint their own values.
The annotation is neither the life event itself nor the subject’s endorsement.

## Limits and actual losses

A `ReadingLimit` specializes the existing `QueryLossLedger`, grouping one
relation, store, input scope and reason for a selected refiner. `limitCount` reports the number of distinct excluded records.
`limitReason` draws from the open `ReadingLimitReason` specialization of
`QueryLossCause`:

- `limitNoAssertionTime`: no usable assertion time was supplied for ordering.
- `limitRecordUnnameable`: the selected operation cannot address a supplied
  blank-node record as a named record.
- `limitThingUnshareable`: its addressing contract cannot correlate a supplied
  blank-node or triple-term referent across records.

These describe the selected refiner’s limitations. RDF 1.2 can retain blank nodes
and triple terms. None of these values bans those terms, erases an input or
licenses a weaker implementation.

The aggregate inherits its information-object and empty-ledger semantics from
`QueryLossLedger`; it is separate from an actual `QueryLoss`. Zero is legal and supplies no `ledgerLoss`. For every actually
excluded source record, retain an identifiable `QueryLoss` through `ledgerLoss`;
its `lossConcerns` points to the original record and its `lossCause` retains the
same reason. Included losses must concern records in the declared input scope.
Recall can reference this specialized ledger directly through `queryLossLedger`.
Other ledgers can retain those same loss resources, preserving identity and
acknowledgement history.

The count is a reported distinct-record total. Keep every excluded record’s loss,
reuse an existing loss identity and avoid duplicate losses that inflate a count.
The intrinsic laws check zero-versus-member consistency, positive evidence and
scope/cause coherence. They do not certify the exact arithmetic of counts above
one or perform the consumer’s grouping computation.

The ordinary [archive scene](examples/archive-readings.ttl) separates an earliest
reading over two ordered records from a wider input that also contains an
undated record, retains a source-only name and attributes a milestone to its
process. No refinement edits its original sources (Principles 3, 4, 9 and 11).

## Open items and changing readings

`OpenItemReading` specializes `Refinement`. Its `observedFeature` is the original
record, its `vantage` is the versioned reader, and `readingScope` retains exactly
the records offered to that reader. `openItemAgent` identifies the awaited Agent,
not the reader. `openItemAt` preserves the holder's original opening date-time;
the reader's `observedAt` must never replace it. Missing source precision cannot
be repaired by inventing a clock value. A source that supplies no usable opening
date-time cannot satisfy this reading's selected contract.

The open `OpenItemKind` vocabulary initially distinguishes `openItemRequest`
and `openItemQuestion`. These are interpretations of records, separate from
request acts, `AttentionRequest` lifecycles, or a question's own inquiry status.

The open `OpenItemState` vocabulary initially distinguishes:

| Value | Reading within the offered scope |
| --- | --- |
| `openItemOpen` | No reply or settling act is linked. |
| `openItemReplied` | A reply is linked, without a settling act. |
| `openItemSettled` | A settling act is linked and its original record is retained through `settledBy`. |
| `openItemLetGo` | The records support a reading that the agent let the item go, distinct from settlement. |

The settled value requires evidence. The other three seeded states exclude
`settledBy`; custom states remain open to their own semantics. Each cited settling
record belongs to the reading's offered scope. These intrinsic laws do not prove
that a reply or resolution happened outside the recorded evidence.

A later reading uses the existing `supersedes` link to its predecessor. Preserve
both scopes, outcomes and vantages, and suppress an older reading from display
when appropriate rather than retracting it. A changed reading does not edit the
original request or question. No state defaults from silence, elapsed time or
an absent field.

## Recurrences

`RecurrenceReading` also specializes `Refinement` and binds its original source,
versioned vantage, `readingScope`, outcome and actual IRI-valued `relatedThing`.
`recurrenceMember` retains each exact record counted in the current segment.
`recurrenceSettledBy` optionally retains the record of the settling act that began
that segment. Both relations specialize `wasDerivedFrom`, and both kinds of
evidence must belong to the offered scope. An omitted boundary says it was not
supplied; it does not claim that settlement never occurred.

Membership is by RDF identity, not by matching text. A member contributes once
to the retained set; chronological segmentation and arithmetic remain attributed
consumer computations. A recurrence says nothing about why something appeared,
the agent's preference, or a global absence of a settling act.

## Activity figures and day parts

`HabitFigure` is a `Refinement` with explicit source, scope, outcome and vantage.
`habitActivity` retains the source's activity designation or description without
mistaking a category for an individual occurrence. `habitMember` retains actual
`Activity` occurrences as evidence, through the existing derivation spine. Every
member is in the offered scope, including when the scope consists of act records.

`habitChosenCount` and `habitSituationCount` are nonnegative reported counts. Their
denominators and grouping belong to the declared consumer method; this vocabulary
introduces no hidden equality between membership and a count, or inequality
between the two counts. A zero figure need not fabricate a counted act. These
figures do not diagnose a habit or infer preference.

`habitHolder` names the Agent whose civil-day reckoning is used. This role is
separate from the process `vantage`. `habitTemporalFrame` names the actual
`TemporalFrame` and specializes `hasReferenceFrame`; `habitDayPart` selects the
temporal owner's open `DayPart` vocabulary. Morning, afternoon, evening and night
have no universal fixed hours. Their boundaries belong to that holder's reckoning
in the supplied frame. A new holder or frame produces a distinct reported figure,
not a new universal meaning for a shared day-part value.

The holder's civil-day frame and the scope's source assertion-time chronology
are separate bindings. They may differ; neither silently converts the other.

The ordinary [state-reading scene](examples/state-readings.ttl) retains a changed
open-item interpretation, recurrence boundary evidence and a holder-relative
activity figure without introducing an executor or a statistical certificate.
