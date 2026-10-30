<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Chat model design

Chat consumes the shared messaging core rather than importing the email
extension. All manifest dependencies are core. The generic message, body, thread,
reply and source-time authorities stay in one place (Principles 4 and 16).

## Source context

Room identity is represented as an entity carrying a source network and verbatim
string identifier. Required paths preserve both pieces of scope. They introduce
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
| ChatRoom | Required transport and exact string identifier paths. |
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
