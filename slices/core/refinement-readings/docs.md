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
- `limitCueSuppressed`: a cue matched words the reading read, but a negating cue
  within the declared negation scope suppressed that match from counting.
- `limitCueUngrounded`: a cue matched, but evaluation of its declared grounding
  rules did not establish the match as grounded.
- `limitCueUnread`: supplied words fell outside a cue's declared speaker or
  field selection and were not read by that cue.

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

Cue limits retain the original words and any detected match, together with the
selected cue, negating evidence and scope, or evaluated grounding rules and
supplied facts as applicable. Unread words remain offered inputs through
`scopeRecord`. A reading records its source through `observedFeature` and its
versioned process through `vantage`; a limit aggregate retains its reporting
agent through `wasAttributedTo`. Preserve source authorship separately. None of
these reasons turns a refiner's interpretation into the source author's claim
or turns an uncounted cue into a silent negative verdict.

Retain a source record actually excluded from the selected reading as a
`QueryLoss` with `lossConcerns` naming that original record and `lossCause`
naming the cue reason. A `ReadingLimit` groups those losses through `ledgerLoss`
under the same `limitReason`, input `readingScope`, relation and store. A
suppressed or ungrounded match alone does not show that its entire source
record was excluded: other contributions may remain usable. Keep such match
decisions as evidence without inventing an excluded-record loss. When records
are excluded, count distinct source records for the stated reason, not matches;
several excluded matches within one record do not inflate `limitCount`.

These values classify reported outcomes; they do not execute cue detection,
negation or grounding. Missing rules, an evaluation that never ran and an
execution failure do not establish an ungrounded outcome. The existing
scope/cause and count-evidence laws remain unchanged and do not certify the
consumer's cue decisions or exact record-count arithmetic (Principles 4, 9
and 11).

The ordinary [archive scene](examples/archive-readings.ttl) separates an earliest
reading over two ordered records from a wider input that also contains an
undated record, retains a source-only name and attributes a milestone to its
process. No refinement edits its original sources (Principles 3, 4, 9 and 11).
