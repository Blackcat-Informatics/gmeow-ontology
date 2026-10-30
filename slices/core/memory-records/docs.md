<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Memory records

This slice describes memory prose, revision identity, reviews and source evidence.
It reuses the existing observation, provenance, audience and activity owners.
Declarations do not install a memory store, enforce access or execute revisions.

## Prose and lasting bank identity

`memoryText` holds a `MemoryItem`'s original words, with their language and any
RDF 1.2 base direction. `MemoryItem` remains the existing remembered-claim role
on an `Observation`. It is not recast as an `InformationObject` merely because
its content is prose. Preserve `memoryOf`, `memoryKind` and the claim's vantage;
the remembering agent, observed subject and author are not interchangeable roles.
Triple-based memory items need no prose. A note chosen by an agent uses the
existing note model; a summary uses the separate summary fields below.

A `MemoryBank` is the lasting `InformationObject` identity of a standing memory
body. Each revision is a `MemoryItem` linked to that identity with the existing
`versionOf`. A revision retains its own `memoryText`; the bank carries none of the
revision, summary or appraisal payload fields. It can have a label without holding
a revision's body. Superseding revisions retain the original source and history;
the existing `supersedes` and `displayable` mechanisms apply. Clearing a body in
a new revision does not authorize deleting the bank or previous revisions.

## Audience is separate from disclosure posture

`BankTier` specializes the existing open `ProjectionContext` audience vocabulary.
Consumers mint values for their own audiences; this slice privileges none and
seeds none. It records who a revision or claim may reach. The existing
`DisclosurePolicy` and `hasDisclosurePolicy` record the orthogonal disclosure
posture. No equivalence between audience and policy is asserted.

`bankTier` is a domain-free annotation property. It can annotate a revision or
an RDF 1.2 claim carrier, retaining a statement's own audience without changing
the statement into a revision. The ordinary object relation `eligibleForConsumer`
can explicitly use the same audience values when its eligibility claim is
intended. The annotation is not made a subproperty of that object property;
there is no silent annotation-to-object-property bridge or access certificate.
An audience classification alone never establishes permission to release text.

## Reviews of exact revisions

`BankReview` is an `Activity`, independent of any broader reading-act vocabulary.
It binds the exact `reviewedRevision`, an explicit `readToEnd` boolean, its
`reviewDisposition` values and the performing Agent through `wasAssociatedWith`.
The revision must belong to a `MemoryBank`; a moving latest pointer cannot stand
in for the revision actually read. False records a partial read; missing evidence
does not default to true or false.

`eventTime` records the instant of the review decision, including partial or
deferred reviews. It does not claim the entire reading took zero time or that
the revision was read to completion. Retain an `eventInterval` separately when
the source provides a reading duration.

| Disposition | Recorded decision |
| --- | --- |
| `bankReviewKeep` | Keep the reviewed revision. |
| `bankReviewReplace` | Replace it through a new revision. |
| `bankReviewAppend` | Append material in a successor revision. |
| `bankReviewClear` | Clear the current body in a new revision, retaining history. |
| `bankReviewRelabel` | Change a label or classification without assuming unchanged content. |
| `bankReviewRestoreSnapshot` | Restore a prior snapshot as a successor revision. |
| `bankReviewProposeWiderAudience` | Propose a wider-audience version, without authorizing release. |
| `bankReviewHandledPrivately` | Handle the matter in private conversation, without claiming the bank stayed unchanged. |
| `bankReviewDeferredChanged` | Defer because the bank changed during review. |

The disposition vocabulary is open and several decisions can coexist. No decision
alone proves that a revision was fully read, that a proposed mutation occurred or
that a store enforced its audience. Private handling requires no disclosure of
private conversation text. A subsequent mutation remains a separate act.

## Summaries and kept appraisals

`summaryText` holds the existing `Summary`'s exact summarizer-supplied words.
`summaryAnchor` holds the summarizer's passage identifying material drawn on:
it is a paraphrase anchor, not a quotation credited to the source author. Both
preserve language and direction and retain the source through `wasDerivedFrom`.
A literal field suffices; no extra extraction object is required for a summary
whose own content is text. A verbatim source extraction remains `TextExtraction`.

`BodyDistillation` retains the proposed consumer IRI for an appraisal kept by a
runtime on an agent's behalf. It is an `InformationObject`, not an agent-chosen
`Note` or an `Observation` relator. `distilledText` preserves the original appraisal
words and `distillationFor` identifies the beneficiary. The runtime is responsible
for keeping the record through `wasAttributedTo`, and the existing `accordingTo`
records the runtime standpoint under which it is offered. No parallel viewpoint
property is introduced. Keep the original appraisal record through
`wasDerivedFrom`, with its actual authorship separate from the runtime keeper.

Offering these words does not fabricate the beneficiary's self-assertion,
endorsement or decision to keep a note. Adoption would be a separate attributed
act or record. The intrinsic law retains the responsible runtime and its offering
standpoint; it does not verify authorship or infer adoption from text.

## Historical curation stays external source evidence

`CurationEpoch` describes a historical `Activity` from original external source
evidence. It is not a governed-store deletion receipt or a tombstone. The
[claim lifecycle rule](../../../docs/claim-lifecycle.md) requires pre-migration
repair receipts to remain outside migrated governed stores. Naming this class
does not permit laundering a historical receipt into a generic Activity inside
those stores. Its source description and all retained evidence stay on the
external side of that boundary.

`epochEvidence` references the original external documentary artifact.
`curationManifest`, `epochPredecessor`, `epochArchive` and `epochSuccessor` reference
the exact manifest, input ledger, retained archive and output ledger artifacts.
Each artifact has its own existing, domain-free `contentDigest`. The original
`curationManifestDigest`, `epochPredecessorDigest` and `epochArchiveDigest` names
are retained as algorithm-tagged literals agreeing with the corresponding source
artifact. They are not hashes of the epoch description. Several algorithm-labelled
digests can coexist; no digest is made globally functional.

The tag law checks an explicit algorithm token and nonempty payload. It neither
computes a hash nor verifies an algorithm, encoding, digest length or byte equality.
Keep the original bytes and source identity; a path or filename is audit location,
not a content identity. An archive explicitly reported empty still has its own
artifact and digest rather than an invented missing value.

The successor artifact `wasGeneratedBy` the historical epoch and `wasDerivedFrom`
its predecessor. Those are source production claims. The later documentary
evidence did not generate the earlier event, so `epochEvidence` is not mapped to
`wasDerivedFrom` on the historical Activity. Nothing changes the governed store's
erasure or receipt rules.

The [memory scene](examples/memory-and-review.ttl) separates revisions, claim
audiences, review decisions, summary authorship and runtime offering. The
[external curation scene](examples/external-curation.ttl) describes source evidence
outside a governed store. Both are ordinary ontology examples, not execution or
migration instructions.
