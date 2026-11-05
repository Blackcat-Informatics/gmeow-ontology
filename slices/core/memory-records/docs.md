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

## Trace-retention classifications

`TraceMarkClass` is an open quality-value vocabulary for a curator’s
trace-retention classification of an existing [Annotation](../notes/docs.md).
`traceMarkClass` links the actual annotation to its supplied classification
values. The role is optional and nonfunctional: several attributed values may
coexist, and unrelated annotations need no category. Members are individuals,
not subclasses, EventTypes, events, confidence levels or an importance ranking.
Local category names do not define a universal closed taxonomy.

The annotation retains its actual target and motivation under the existing
Notes contract, together with actual curator attribution and standpoint. A
source record is not made an Event because it describes one. Do not invent a
record resource from an unresolved producer identifier, supply a default
motivation, manufacture a body or use an Observation-domain `vantage` relation
on the annotation. Body-free annotations remain possible; a classification
does not supply or require trace content.

`traceMarkHighFrictionCorrection` classifies a teller’s reading that one act,
using the same tool and arguments, was tried again in an episode after an
earlier attempt did not succeed. The curator classifies that reading for trace
retention; this does not assert the underlying event, equality of tools or
arguments, retry execution, truth, curator endorsement or confidence. Retain
the actual source reading and its attempt, episode, tool, argument and outcome
evidence, with the curator and teller kept distinct. A repeated label or missing
success evidence cannot supply the classification.

The supplied-value closure checks Annotation and TraceMarkClass endpoints.
These terms do not implement a mark record, packet anchor, trace join, packet
ordering, finality watermark, producer frontier, lost-mark give-up or recorder
deletion. A classification or absent category is no proof of retention,
completeness, finality or permission to discard traces (Principles 4, 9, 11,
17 and 19).

## Curator trace marks and exact packet anchors

`TraceMark` specializes the existing [Annotation](../notes/docs.md). Its sole
`annotationTarget` is the actual `InformationObject` event record, not an
`Event` occurrence. A local field naming the marked record corresponds to this
existing target role; this vocabulary adds no second target predicate, adapter
or migration. Do not manufacture a record resource from an unresolved identifier
or type a record as an Event because it describes one.

Each coherent mark requires its actual annotation target and motivation under
Notes' inherited exactly-one contract. No motivation is primary, automatic or
guessed from the mark's category. Keep at least one actual curator through
`wasAttributedTo`, and retain source and standpoint attribution for each exact
assertion separately. An inherited optional body may hold a real curator note;
the mark does not carry the recorder's raw trace content and requires no body.
No Observation-domain vantage or invented model run supplies curator context.

At least one supplied `traceMarkClass` value is required locally on a TraceMark,
without a maximum. The existing role stays optional and nonfunctional for other
Annotations. The open `TraceMarkClass` vocabulary retains actual local category
identities and source-specific meanings. A local category's name does not mint
a universal member, EventType, event occurrence or preferred importance rank.
Coexisting curator readings remain distinct marks; no global winner is chosen.

`traceMarkAt` retains exactly one original temporal literal placing the event
described by the target record: `xsd:dateTime`, `xsd:date`, `xsd:gYearMonth` or
`xsd:gYear` at the supplied precision. Preserve datatype, lexical form and
timezone information. This is not record creation, annotation creation or the
time the recorder processed it. Exactly one actual complete
[TemporalFrame](../temporal/docs.md) is required through `hasReferenceFrame`.
It retains the actual temporal interpretation while inheriting the existing
temporal and [reference-frame](../places/docs.md) profile laws. Do not borrow
Events' UTC default, invent midnight or an unstated clock, increase precision,
or use a frame to reinterpret an XSD datatype's calendar. Required actual
interpretation is not a performed chronology or calendar conversion.

### Explicit packet status

`traceMarkAnchor` retains every actual supplied `TraceAnchor`, without a
maximum or a preferred packet. Each anchor carries its own complete required
packet/profile context. If a mark is held without a packet identity, retain
the explicit affirmative `traceMarkWithoutPacket` flag instead. Missing anchors
and missing flag together are invalid; absence never becomes an affirmative
without-packet declaration or a default false value.

The optional flag retains at most one original `xsd:boolean` literal per
coherent mark. Each individual `true`/`1` or `false`/`0` lexical alias is admitted
unchanged and interpreted by Boolean value. The literal maximum does not admit
simultaneous alias assertions. An affirmative flag excludes every anchor. A
false flag requires at least one actual anchor; false alone supplies no packet
evidence. Contradictions fail rather than being repaired by dropping a source
assertion, rewriting a literal or inventing a packet.

The three packet laws stay separate. Presence is the disjunction of an atomic
anchor witness and an atomic flag witness. A positively bound false flag
requires an atomic anchor witness. A positively bound affirmative flag forbids
atomic anchor existence. Boolean `termEqual` comparisons retain the meaning of
individual lexical aliases without a joined affirmative existential or an
absence-based default. These structural obligations establish no trace join,
retention, deletion permission, ledger completeness or finality.

### Source-packet interpretation

`TraceAnchor` is an `InformationObject` descriptor, distinct from an actual
packet artifact or trace. Exactly one `anchorPacket` retains the complete
original `xsd:hexBinary` digest literal unchanged. Exactly one
`anchorSourceProfile` names the actual applicable `InformationObject`
declaration of its source interpretation. This profile belongs on each anchor;
it is not inferred from a sibling anchor, surrounding mark, profile name or
default. Several anchors may share an actual applicable profile without
becoming the same anchor or packet.

The actual full profile must identify the digest algorithm and construction,
including any keyed, domain-tag, truncation or output convention, and the exact
packet byte-preimage meaning, encoding and any canonicalization used. It must
also explain the selected episode/round source fields. It need not embed the
packet's bytes in the mark, but its actual meaning and applicability must be
source-supported. A typed link or recognizable digest width does not establish
those facts. An importer selecting the normalized role must fail if required
source interpretation is absent or invalid; preserve raw incomplete evidence
under its original meaning rather than inventing a profile or claiming success.

`xsd:hexBinary` alone identifies no digest algorithm. Do not infer an algorithm
from width, normalize the spelling or retype historical literals as the distinct
[Math algorithm-digest datatypes](../../grounding/math/docs.md#algorithm-tagged-digest-literals).
No literal is converted to BLAKE3, SHA-256 or MD5 by this role. This property
does not specialize [contentDigest](../sources/docs.md): on the anchor descriptor,
that property would identify the descriptor's own bytes. A supplied packet
digest does not create a packet resource, establish byte verification or
compute a trace join. Keep any independently supplied actual packet artifact
and its own content digest separate.

### Optional episode and round terms

`anchorEpisode` and `anchorRound` each optionally link at most one scalar
[StructuredValue](../record-metadata/docs.md) occurrence. The occurrence's
`structuredTerm` preserves the exact original RDF term: IRI, blank node,
literal or triple term, including literal datatype, lexical form, language and
direction. A source resource identifier can remain unresolved; it is neither
stringified nor inferred to be an Activity. A round ordinal is not retyped,
renumbered or assigned an unstated zero/one base. The anchor's actual source
profile interprets each supplied field.

The scalar carrier exposes source evidence without manufacturing a source
episode or round event. Existing `StructuredTermIntegrityConstraint` owns
whole-term uniqueness, and `StructuredValueBranchConstraint` owns payload
branch exclusivity. The new `TraceAnchorScalarConstraint` requires only an
atomic positive `structuredTerm` witness for each selected optional occurrence,
with fresh variables for episode and round. No required field is added when
absent. A container or explicit null stays source evidence outside these
selected scalar identifier/ordinal roles; an empty carrier fails instead of
becoming a default value. Independently supplied resolved episode/round evidence
remains distinct from the raw term and its interpretation.

### Trace finality remains a separate contract

A mark, flag, profile or anchor implements no ordered packet ledger, time/digest
comparator, monotonic finality watermark, producer committed frontier or explicit
lost-mark give-up with its reason. Without-packet status is not evidence that
in-memory marks were lost, a producer committed, or a recorder may advance a
frontier or discard traces. The vocabulary computes no digest, performs no
join, authenticates no source and executes no storage action. Unknown source
interpretation remains unknown rather than being hidden by a successful
structural result (Principles 4, 7, 9, 11, 12, 17 and 19).

## Recorded trace-packet ledger evidence

`TracePacketLedger` is an actual `InformationObject` ledger record.
`tracePacket` retains the source's actual observed-packet memberships, without
a maximum or preferred packet. It is not required on every ledger record.
Missing members establish neither an empty inventory nor complete observation.
No immutable snapshot, commit boundary, ledger profile, history completeness
or recorder-ingestion operation is imposed by this vocabulary. Retain actual
ledger identity, source attribution and any real recording metadata supplied.

Each `TracePacket` is a source-recorded packet descriptor specializing the
existing `TraceAnchor`. It inherits exactly one original `anchorPacket`
`xsd:hexBinary` digest and one actual `anchorSourceProfile` declaration, with
their existing construction/preimage interpretation requirements. Optional
`anchorEpisode` and `anchorRound` retain their original scalar RDF terms and
existing integrity laws. No anchor contract is duplicated, algorithm guessed,
literal retyped or packet artifact created from a hash. The ledger record
itself is not an anchor: do not put `anchorSourceProfile` on the ledger to
manufacture a ledger-level interpretation or digest requirement.

`tracePacketAt` retains the original temporal value the source uses to order
the described packet. On recorded `TracePacket`, exactly one supplied
`xsd:dateTime`, `xsd:date`, `xsd:gYearMonth` or `xsd:gYear` literal is required,
together with exactly one actual complete [TemporalFrame](../temporal/docs.md)
through `hasReferenceFrame`. Preserve datatype, lexical form, precision and
timezone information. The actual temporal and [reference-frame](../places/docs.md)
owners retain frame completeness; no frame laws are copied here. Do not invent
midnight, UTC, an epoch conversion or a more precise instant, round a timestamp,
or use a frame to reinterpret an XSD datatype's calendar.

The role's domain is `TraceAnchor`, not `TracePacket`. A source-backed key time
on an anchor therefore does not assert that its packet was observed or entered
in a ledger. Required time/frame remains local to recorded `TracePacket`;
unrelated anchors acquire no required time. A future genuine frontier key can
retain an actual source time without manufacturing a recorded-packet row, but
this unit supplies no frontier protocol or cursor interpretation.

Ordering time is distinct from the marked event's `traceMarkAt`, an occurrence's
`eventTime` and record transaction `recordedAt`. It neither specializes those
roles nor borrows Events' UTC default. A selecting importer must fail when a
recorded packet's required original time, frame or inherited anchor evidence
is missing or invalid. Preserve incomplete raw source under its actual meaning
rather than inventing missing context, silently dropping selected data or
claiming normalized success.

These terms retain ordering inputs; they execute no time-then-digest comparator
and add no pairwise order relation, ordinal or commit-history requirement.
The native integrity ordering path currently handles finite numeric values;
it cannot be relabeled as timestamp or digest-byte comparison. The existing
PurRDF temporal/binary owner and any future GMEOW comparison exposure retain
their separate qualification requirements. No order-verification result is
asserted by a supplied timestamp or structurally valid ledger membership.

The full finality protocol remains separate: inclusive never-backward marking
watermarks, complete applicable producer frontiers, and explicit give-up with
reason for lost in-memory marks before advancement. Without-packet status is
not a give-up receipt. Missing membership is not unmarkedness, finality, an
age-policy verdict or permission to discard traces. No trace join, retention,
deletion action, producer commitment, completeness proof or consumer adoption
follows (Principles 4, 7, 9, 11, 12, 17 and 19).

## Reported trace feed positions

`tracePacketPosition` optionally retains the source-reported feed-position
descriptor on a `TraceAnchor` as its complete original
`xsd:nonNegativeInteger` literal. Preserve its datatype and lexical form.
Interpret it only through that anchor's actual `anchorSourceProfile`, including
the source's numbering convention. The generic datatype can represent zero;
this does not override a source convention that excludes zero or choose an
origin when none was supplied. Do not calculate a descriptor from surrounding
rows, a timestamp, a digest or missing evidence.

The field is nonfunctional: several supplied descriptors can coexist as
attributable claims, and omission remains unstated. Retain each assertion's
actual source, declaring perspective and recorder on its exact-statement
reifier through `wasDerivedFrom`, `accordingTo` and `wasAttributedTo`.

Its TraceAnchor domain retains the existing required `anchorPacket` digest and
actual applicable `anchorSourceProfile`; a missing profile is never synthesized.
A position alone establishes no recorded `TracePacket` type or `tracePacket`
ledger membership. It supplies no packet count, clock, comparison, frontier,
numbering law, finality commitment, retention operation or consumer-adoption
verdict (Principles 2, 4, 7 and 17).
