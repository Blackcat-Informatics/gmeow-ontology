<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Shared messaging design

Messages are intended communicative artifacts, not certificates of successful
transport. This permits retention of refused communication without inventing
receipt, and keeps the body structure independent of a particular network.

The core owns `Message`, `BodyPart`, `Thread`, `hasBodyPart`, `inReplyTo`,
`partOfThread`, `sentAt` and `receivedAt`. Their public IRIs remain the same
across email and chat. Email owns its RFC and MIME specializations, header fields,
address relators and provider facets. The email `MessageParticipant` still
requires email context; chat has its own occurrence model.

## Canonical structure

Principles 4 and 17 place class, property, range and restriction declarations in
`logic:`. Generated OWL/RDFS and validation surfaces are views of these sources.
The universal part spine remains shared: `hasBodyPart` and `partMedia` specialize
`hasPart`, while `partOfThread` specializes `partOf`.

| Assertion | Canonical meaning |
| --- | --- |
| Message typing | Intended or addressed communication; no success inference. |
| Body-part membership | A transport-neutral constituent, with source order retained separately. |
| Sending and receive dates | Source dateTime observations, with no record-creation bridge. |
| Delivery and reply outcomes | Open attributed values, never a default from absent edges. |
| `sourceMedia` | Actual source-byte derivation; referenced source has its own digest. |
| `repliesToContent` | Answered content identity without asserting a source Message. |
| `originComposed` | Deliberate communicative or committing composition; nonexclusive origin. |
| Illustrative-text tag | Positive content annotation, without transcription or closed-world negation. |

Realized guarded-existence formulas require `contentDigest` on every
`sourceMedia` and `repliesToContent` target. They do not fabricate unknown bytes,
require that those bytes are locally retained, or invent a digest datatype.
Other value obligations use ordinary universal restrictions. No required-path
closure imposes a send time, receive time, body or success claim on every Message.

A `MediaObject` remains a concrete publication artifact under `Manifestation`.
Its source bytes are only an `InformationObject`, so unpublished generator output
is not falsely asserted to be a publication. A body-part wrapper references the
media artifact rather than inheriting that publication identity. The illustrative
media file retains its existing Expression/Work embodiment context.

## Projection ownership

The messaging alignment set carries the close matches for shared Message,
Thread, sending time, receive time and direct reply. Each cell retains its
manual-curation justification, confidence and semantic comment. A close match
does not upgrade into equivalence because the source vocabulary is shared.

The SIOC container, membership and direct-reply recipes retain their identifiers,
source patterns, explicit target edges and preservation declarations. Email’s
post typing and address-based creator recipes remain email-specific. Digest-only
content references never enter the message-only reply projection. Principle 16
keeps both transport extensions dependent on the core, without extension-to-
extension imports.

## Boundaries

The model records explicit failures rather than diagnosing them from silence.
It does not make sender-guard refusal and provider rejection permanently disjoint
message classes: reports can describe different attempts or standpoints. Retain
that context on the statement. No outcome is a truth judgment.

Content origin, authorship, veridicality and fulfillment remain separate. An
intention’s deliberate composition needs no extra exclusive origin solely because
it commits rather than communicates. `ContentOrigin` is not `LanguageOrigin`, and
matching local-name prefixes imply no schema alias.

A digest-only content node asserts an identified artifact, not a fictitious act of
sending it. Conversely, `sourceMedia` does assert derivation and must not stand
for mere input exposure. These relations are separate because their information
claims differ.

## Whole wording and part wording

utteranceText is a nonfunctional Message property over the union of xsd:string,
rdf:langString and rdf:dirLangString. The same literal policy applies to chat's
messagePartText on its own ordered part. Multiple attributed renderings may
coexist; neither predicate computes a whole utterance from pieces or asserts
delivery (Principles 2, 4 and 9).
