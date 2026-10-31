<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Qualified breakthrough readings

This source extends the existing `CharacterArc` information object and
`ArcSample` observation. It adds no teller, matching engine, aggregate evaluator,
new status machine or second observation model. Every interpretation remains
attributed to its teller, and chronological succession remains distinct from cause
(Principles 4, 5, 9 and 17).

## Specific next steps

`arcTypeInsight`, `arcTypeDecision` and `arcTypeSettledQuestion` are individuals
of the existing open `ArcType` vocabulary. Their definitions name the general
breakthrough they narrow. This deliberately takes the definition-based option:
there is no new hierarchy property, authored `skos:broader` axiom, or forced
typing of an arc-type value as a `Concept`. `arcType` retains its existing
single-value semantics; an arc is not given both a general and a specific type.

Read only the selected `arcNextRecord`, with its actual subject attribution and
source evidence:

| Type | Required source meaning |
| --- | --- |
| `arcTypeInsight` | The subject's note explicitly bears an insight, realization or discovery tag recognized by the exact reading rule. |
| `arcTypeDecision` | The subject's note explicitly bears the rule's decision tag. |
| `arcTypeSettledQuestion` | The subject records setting the exact inquiry selected as the thread to `inquiryStatusResolved`. |

An open arc, a wording-only next step, or a note tagged both as decision and
insight/realization/discovery retains `arcTypeBreakthrough`. If several specific
interpretations remain applicable, keep the general type rather than inventing a
priority. Source tag resources are explicit inputs to the selected rule; matching
tag labels or retyping a tagged Note as a LearningEvent or Decision event does
not establish the source meaning. Resolved inquiry status concerns the pursuit,
not the truth of an answer. A current resolved value alone does not identify the
subject's status-setting act.

The realized narrower-type law requires the exact next-record reference. It
does not execute tag interpretation or prove the source classification. Different
tellers can retain distinct arcs and rival readings without merging their values.

## Conversation continuation threads

`threadKindConversation` selects the conversation containing the first
endpoint as its root. The known-positive-kind resource law now includes this
kind, while `ThreadKind` remains open and unknown remains distinct from none.

For a bound root conversation, include the root and each conversation C for
which a message m in C has `inReplyTo` or `groundedIn` a message n in an
already included conversation D. Both message memberships are explicit
`partOfThread` links. Repeat to a fixed point, retaining visited conversation
identities to terminate cycles. Include every message of every included
conversation, even messages with no internal reply or grounding edge.

A conversation continuing two roots belongs to both root threads. Deduplicate
by conversation and message identity, not text, timestamps or path count.
The root stays explicitly selected; no preferred conversation is inferred.

This is a structural contract over bound identities and exact source links, not
a word search or an unbound corpus scan. The existing `groundedIn` property
still requires an `EvidenceSpan` target. Its target must independently be known
as the message through actual `partOfThread` membership to supply a conversation
continuation; an arbitrary evidence span does not become a message by inference
from this thread kind. Its range is neither broadened nor copied into a new link.
The ordinary scene uses `inReplyTo`.

## Signal evidence and confidence configuration

`ResolutionSignal` mirrors the existing open `ImpasseSignal` quality-value
vocabulary. `resolutionSignalAct` records the stronger evidence category of an
attributed act; `resolutionSignalOwnWords` records a reading of the subject's
words through an exact reviewed cue release and reading rule. Neither category
is a source record, truth verdict, diagnosis or proof of causal resolution.
`resolutionSignal` belongs on `ArcSample`; the Refinement-domain
`refinementOutcome` is not reused.

Numeric degrees belong to `SignalConfidenceSetting` information objects,
not to shared signal individuals. Each setting names one signal through
`confidenceSignal`, one exact `SignalConfidenceRule` version through
`confidenceRule`, and one decimal `signalConfidence`. Both ResolutionSignal
and ImpasseSignal categories are admitted, resolving the wider-domain question
without changing either category's identity.

The rule version retains existing `versionOf`, `versionLabel`,
`contentDigest`, `wasDerivedFrom` and `wasAttributedTo` provenance plus its
`signalConfidenceReview` activity. The review retains `wasAssociatedWith`
and `eventTime`. A changed value, reason or rule configuration creates a new
version and digest. Distinct rule versions may configure the same signal
differently. Several tellers may explicitly use one unchanged version; that does
not make its settings global defaults.

Canonical local maxima, required paths, per-rule/per-signal uniqueness,
review-evidence and lineage-distinctness laws validate configuration structure.
A `ValueRangeConstraint` checks the closed decimal interval [0,1]. This is the
ordinary bounded degree used by existing provenance `confidence`, not a
calibrated probability distribution or the statistics `math:ConfidenceInterval`.
The scene represents the same bound with the existing `math:Interval`,
including both closed endpoint declarations; that mathematical description is
an explanatory reference, not a second execution or calibration mechanism.

On the exact signal statement, use existing `confidence` for the selected
degree, `accordingTo` for the teller, `wasDerivedFrom` for the setting and
actual source record, and a language-bearing `description` for why that signal
was read. Preserve the source wording's language and any RDF 1.2 base direction.
These annotations do not turn the teller's assessment into the subject's claim.
Configuration validation does not certify that an output was actually produced
by that rule or that its degree is empirically calibrated.

## Counts with explicit source scope

`arcBetweenCount` and `arcUndatedCount` are optional nonnegative-integer
reports on the existing arc. The minimal source bindings remain on that same
object; no count-report class or executor is introduced:

| Binding | Meaning |
| --- | --- |
| `arcFirstRecord` | Exact selected impasse endpoint. |
| `arcNextRecord` | Exact selected next-step endpoint, absent on an open arc. |
| `arcCountThread` | One of the arc's actual `arcThread` resources chosen for this report. |
| `arcCountRecord` | Every distinct record in the selected thread's retained finite source snapshot, including endpoints and undated records. |
| `arcCountTimeProperty` | Exact source time-property IRI fixing the clock used. |

Generic source-record ranges are `logic:Thing`, so an Event, Activity or other
source record is not coerced into an InformationObject or CreativeWork.
These links are not specializations of the narrower `arcEvidence` property.

Between count excludes both endpoint identities and every timestamp tie at
either endpoint. Count identities strictly within the selected endpoint values
once, regardless of duplicate evidence or continuation paths. Undated count
covers the entire retained cohort, including undated endpoints when present;
it counts records with no value for the selected clock, not just the records
already placed between the ends. Never prefilter undated records by time.

A supplied coarse date or unknown timezone is not an absent value. Retain it
and the supplied precision. A complete between count requires every dated
cohort record's relation to the interval to be decidable under the selected
clock and temporal interpretation. If comparison is unavailable or conflicting,
retain the source records and withhold that count rather than silently drop a
record, impute midnight or zero, or select a convenient clock value.
An available undated count can still report actual absence independently.

`recordedAt` is available for record-creation time at source precision;
`atTime` dates point occurrences; statement assertion clocks have still another
meaning. The selected property is explicit, not a substitute for those
distinctions. Do not order source records by narrative position or file order.
Equal endpoint values define an empty strict interval; reversed endpoint values
violate the endpoint law.

The count laws check source scope, thread membership, exact endpoint references,
clock IRI, endpoint comparison and agreement of supplied selected-clock values.
They preserve distinct original literal terms denoting the same value:
`termEqual` is value comparison, not a claim of RDF-term identity.
No law here recomputes aggregate cardinalities, proves cohort completeness or
certifies the structural conversation closure. Those are obligations of the
selected source-producing operation; absence of such capability does not
authorize a fabricated count.

## Ordinary source scene

Read [the existing frame scene](../examples/breakthrough.ttl),
[the retained records](../examples/qualified-source-records.ttl),
[the confidence payload](../examples/signal-confidence-v1.ttl), and
[the qualified readings](../examples/qualified-breakthrough.ttl) together.
The two payload digests cover their exact UTF-8 files, including final newlines.
The example supplies all three narrower readings, an ambiguous general reading,
a qualified act signal and a conversation population with one strictly
intervening record and one retained undated record. Its values are synthetic
source declarations, not claims of executed matching, review, counting or
reasoning.
