<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Chat model design

Chat consumes the shared messaging core rather than importing the email
extension. All manifest dependencies are core. The generic message, body, thread,
reply and source-time authorities stay in one place (Principles 4 and 16).

## Source context

Room identity is represented as an entity carrying a source network and verbatim
`xsd:string` or `xsd:token` identifier literal. Required paths preserve both pieces
of scope. They introduce
no global key, single preferred identifier or forced token normalization.
`chatTransport` remains domain-free so media and rooms do not acquire Message type.

Response-turn grouping is explicit. The canonical chain
`chatTurn / turnOfThread` derives shared `partOfThread`. It does not invent turn
boundaries from dates, require one terminal message or infer successful output.
`chatTurn` is supplied only when the source identifies that grouping; its range
remains constrained without a required-path closure on every `ChatMessage`.
Sources without turns can retain known thread membership directly.
`entryOrdinal` retains known zero-based local order on parts, messages or turns;
source precision of record time remains governed by `recordedAt`. Source sending
and storing-system receipt use shared `sentAt` and `receivedAt` independently.

## Participant evidence

A `ChatMessageParticipant` is a relator mediating its message and a known endpoint.
Its endpoint is an anonymous class union of `Agent` and `OnlineAccount`.
The agent and account properties specialize that common endpoint, so account-only
evidence satisfies the required player path without manufacturing an agent or
requiring all possible mediation properties at once.

The occurrence requires its message, an endpoint and a role. The inverse message
navigation shares the same relation. A realized guarded-existence constraint
requires an author occurrence for a complete ChatMessage. It does not limit the
number of authors or impose a functional role across standpoints. No rule
infers account ownership from participation, and no email-address restriction
is inherited.

## Content retention obligations

| Object | Canonical obligation |
| --- | --- |
| ChatMessage | Required network and participant paths; a realized author-occurrence obligation; optional source-provided turn with a constrained range. |
| ChatRoom | Required transport and exact source identifier paths. |
| ChatTurn | Required Thread reference. |
| ChatMessageParticipant | Required message, Agent-or-account endpoint and role paths. |
| ChatMessagePart | Required nonnegative source ordinal and a realized incoming-message obligation. |
| UnreadableChatPart | Required original content digest and classifying reader. |
| DisagreeingChatText | Required original content digest, classifying reader and compared part. |
| Compared parts | Realized existence formula requiring a common containing ChatMessage. |

All value and required-path policies use `logic:allValuesFrom` plus explicit
class/property closure where presence is required. Procedural obligations are
actual `logic:Constraint` records with realized formula trees. The source authors
no separate SHACL, ShEx or OWL/RDFS constraint surface (Principle 17).

A digest names the original part’s content bytes. The retained occurrence’s
reader-specific classification does not rewrite those bytes into an error
message. Several reading contexts can retain the same content identity with
different classifications; no unreadability or disagreement class is declared
disjoint from a different reader’s successful interpretation. The ordinary scene
keeps that interpretation as the reader’s generated information object, separate
from the original message body.

## Separate assertions

An intended message and its failure outcome can coexist. A refused terminal reply
remains a source-designated terminal output, while its recipient need not have
seen it. No absence of outcome, reply edge, output role or image-text tag supplies
a default success or negative assertion.

Digest-only replied-to content uses the shared artifact-reference relation,
without inventing a source Message. Deliberate composition can coexist with
generative origin. Persona attribution remains the separate domain-free
`wornModality` vocabulary and does not need a chat-local duplicate.

## Source events and provider refusals

A ChatReaction is an Event occurrence, with one supplied reactionKey and one
reactionState. reactionPlaced and reactionRemoved seed an open ReactionState
vocabulary; a missing state never defaults to either. Separate occurrences retain
separate provider claims. actorIdentity and targetMessage are opaque source
identifier literals, not links resolving an Agent or Message. reactionResource,
providerEvent and providerEventType retain their source spelling and datatype.
providerOccurredAt preserves unparsed source timestamp text as `xsd:string`.
All retain exact source whitespace. Each identifier-bearing record retains providerProgram, chatTransport,
accordingTo and wasDerivedFrom evidence (Principles 2, 4 and 9).

Eight canonical scope laws guard the supplied actorIdentity, reactionResource,
targetMessage, providerEvent, providerEventType, providerOccurredAt,
sessionIdentity and sessionSender properties independently. Each law has a binary
property guard on its subject and the same provider, transport, source-context
and derivation obligation. No law requires an absent source field or assigns
another record class merely to obtain a validation target.

The source evidence, the RejectedMediaObservation and each RejectedMediaPart are
three different records. The observation has an actual observing vantage and
method, observes the retained provider record, and returns InformationObject
descriptors. That source records the sender string and the provider's unparsed
time text. The observation and source share provider, transport and source
context. Each descriptor requires its source reason and zero-based index;
numerically equal indexes within one observation identify one descriptor.
A sender's description remains description text. None of these assertions creates
a delivered Message, a body part, bytes, a MediaObject manifestation or a recipient
reading. An actual acquired artifact can be recorded separately.

sessionIdentity and sessionSender belong together on an Observation carrying
observedFeature, vantage, observationMethod, observationResult verdictHeld and
actual derivation evidence. This preserves a source claim of verification;
it neither performs authentication nor proves the provider trustworthy.
transportMatrix and transportGoogleChat identify open transport vocabulary
members, never providers, accounts or rooms.

messagePartText preserves only the supplied words of an ordered ChatMessagePart,
including language and RDF 1.2 base direction. Whole-message utteranceText belongs
to core messaging. Neither predicate assembles, translates or normalizes parts.
The [source-record scene](../examples/source-records.ttl) keeps reactions, rejected
descriptors, actual text, acquisition and session evidence distinct.

## Machine datatype admission

Ten existing machinery properties use anonymous canonical string/token unions.
The matching room, actor, target and media-refusal universal fillers agree; the
reaction-key universal and qualified maximum share that same union. Required
paths, cardinalities, domains, closure entries and all integrity formula trees
remain unchanged. No global identity rule or closed provider-code vocabulary
is added.

Preserve the supplied literal rather than converting its datatype from its
spelling. Provider occurrence-time text and human message wording retain their
separate contracts. The declarations introduce no token lexical checker,
normalization, authentication or token-preserving capture API.
