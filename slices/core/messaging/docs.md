<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Shared messaging

`Message` represents a communicative artifact addressed or intended for other
agents. A refused outgoing message is still part of what its author intended to
say. Merely retaining a message proves neither sending nor delivery. Email and
chat share this core and retain their own transport-specific structures.

## Content and conversation structure

`hasBodyPart` links a message to `BodyPart` information objects. The shared part
is transport-neutral. Email supplies MIME structure; chat supplies ordered typed
occurrences. Preserve a source sequence with `entryOrdinal` on each occurrence,
scoped to its containing message. A file reused in several messages can have
separate wrappers and positions.

`Thread` groups messages into a conversation. `partOfThread` retains membership;
`inReplyTo` retains a direct message-to-message answer. A response turn is a
smaller grouping supplied by the chat vocabulary, while a room is a network-scoped
channel. Neither is interchangeable with the entire thread.

`sentAt` keeps the source’s sending-time claim. For email, this includes the RFC
5322 `Date` header and therefore need not be independent evidence of actual
transmission. `receivedAt` keeps the storing system’s receive time, including
provider internal dates. It does not prove that the intended recipient read the
message. Both retain their standard `xsd:dateTime` range and supplied timezone
information. Record creation can independently use `recordedAt` at source
precision; these clocks have no recording-time subproperty bridge.

## Keep failures explicit

`deliveryOutcome` retains an explicitly reported outcome. The initial values
separate refusal by the sender’s guard (`deliveryRefusedByGuard`) from rejection
by a provider (`deliveryRejectedByProvider`). The intended message remains
available for recall even when its recipient never saw it.

`replyOutcome` can retain `replyFailedCognition`: a reported answering attempt
failed because the recipient’s cognition failed. A missing reply link alone
cannot establish that cause. It could mean no known answer, an answer not yet
made, or incomplete history.

These outcome relations are nonfunctional. Several reports may coexist, and a
later successful attempt does not erase an earlier failure. Preserve recipient,
attempt, observation time and standpoint in the attribution when those distinctions
matter. Absence of a failure assertion is not a success assertion, and an outcome
does not determine the truth or importance of the message’s content.

## Separate media occurrences from their bytes

`partMedia` links a body-part wrapper to the separate `MediaObject` it carries.
The wrapper and the media file can retain different content identities. The
shared model chooses explicit references, so an importer need not identify a
transport wrapper with a published media artifact.

`sourceMedia` links delivered media to the `InformationObject` whose original
bytes it was produced from before delivery encoding. The source carries its own
`contentDigest`; the delivery file carries another digest for its own bytes.
The source may be known only by digest and need not be stored or published. A
reference does not pretend that the original bytes are available for retrieval.

This relation asserts actual byte-source derivation and specializes
`wasDerivedFrom`. Do not assert it merely because an artifact was present in a
creation context. More general derivation chains can still use `wasDerivedFrom`
when they do not have this specific media-source role.

The artifact-reference representation replaces the need for a direct
`sourceDigest` property. It reuses the existing algorithm-labelled, nonfunctional
`contentDigest` contract. No digest datatype, parser or lexical guarantee is
introduced here. Multiple known digest algorithms can describe the same artifact.

## Reply to content known only by digest

`repliesToContent` identifies the words or content a message answers when only
their digest identity is known. Create a reusable `InformationObject` with
`contentDigest`, then reference that object. It need not be stored locally or
asserted to be a transmitted message.

This chooses the content-artifact alternative to a `repliesToDigest` literal.
It is not a subproperty of `inReplyTo`, because that would turn an unknown content
artifact into a message. When the actual source message is separately known,
retain its `inReplyTo` edge independently.

## Composition and image text

`originComposed` is an individual of existing `ContentOrigin`: an agent deliberately
authored content to communicate or commit. Speech, a written message and a stated
intention can share that source distinction without collapsing communication and
conative acts into one act type. The origin says neither that a statement is true
nor that an intention was fulfilled.

Deliberate composition and generative production can both apply. A model-produced
message can retain `originGenerated` together with `originComposed` when its
communicative composition is supported. Composed content is not thereby imagined,
remembered or perceived; retain those origins only when they independently apply.
There is no exclusive origin or preferred attribution (Principle 9).

`ContentOrigin` and `LanguageOrigin` are separate vocabularies. The latter’s
`originPidgin` and `originCreole` classify how languages arose, rather than the
source of a piece of content. A shared local-name prefix does not mix their
classes. No aliases or renaming are needed to preserve that distinction.

`tagContainsIllustrativeText`, attached with `hasTag`, positively marks an image
whose pixels contain text belonging to the picture, such as a sign or drawn label.
It does not provide searchable transcription. Missing the tag does not mean the
image contains no text. Keep any known transcription or image-region description
separately; the tag remains useful when only presence is known.

The [content-reference scene](examples/content-references.ttl) retains separate
media identities, a digest-only reply target and explicit failure reports. The
[design rationale](design/MESSAGING.md) describes ownership and canonical laws.

## Verbatim whole utterances

utteranceText retains the whole wording when a source records one Message as
one utterance. Preserve an untagged string, language tag or RDF 1.2 base direction
as supplied. Multipart chat wording belongs to each messagePartText in chat;
do not concatenate parts, use labels as content, or invent a language.

## Shared message kinds

MessageKind and hasMessageKind belong to this transport-neutral slice.
The open vocabulary permits overlapping behavioral and report kinds on any
Message without choosing a primary category or inferring delivery.
Email retains its specialized report-kind individuals; environment records
supplies messageKindWorldNotice. The shared relation imposes no email-address
or participant requirements on other notice transports.
