<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Chat conversations

A `ChatMessage` specializes the shared `Message` with transport context,
participant occurrences and ordered parts. Source-provided response turns retain
additional grouping when available. It includes intended output
refused before dispatch. Its type and output role do not prove successful delivery.

## Network, room and turn

`ChatTransport` is an open vocabulary of source network or transport values.
Use deployment-specific values when a protocol label would not identify the
network. `chatTransport` has no domain restriction, so a message, room or carried
artifact can retain its source network without becoming another kind of object.

`chatRoom` references a `ChatRoom` entity. That entity retains its network with
`chatTransport` and its exact source identifier with `chatRoomIdentifier`.
The identifier is `xsd:string`, preserving case, whitespace and punctuation.
It is not globally unique, and matching strings on different networks do not
merge rooms. Direct conversations without a source room can omit `chatRoom`.

This selects the room-entity representation. An importer using a literal-valued
local `chatRoom` field should create the scoped room node, put the original string
on `chatRoomIdentifier`, and reference it from the message. Do not replace source
strings with whitespace-normalized tokens or put opaque room keys into predicates.

A `ChatTurn` is the producer’s response unit in a `Thread`. It can group incoming
messages, auxiliary output and the terminal reply. `chatTurn` assigns messages;
`turnOfThread` assigns the turn’s conversation. The canonical property chain
supplies shared `partOfThread` membership along those two links.

Some chat sources supply no response-turn grouping. Omit `chatTurn` in that case
and retain any known `partOfThread` membership directly. A message remains a
`ChatMessage` without an invented turn. When a turn is supplied, its reference
must still identify a `ChatTurn` with its own conversation context.

`outputRoleTerminalReply` and `outputRoleAuxiliary` retain the source’s intended
role of outgoing messages in the turn. Roles remain attributed and nonfunctional;
they do not demand a unique winning output or a reply that actually arrived.
Incoming messages need no output role.

## Participants without email assumptions

`ChatMessageParticipant` reifies a source occurrence. Link it through
`hasChatParticipant` or the inverse `chatParticipantMessage`. Every occurrence
retains its message, endpoint and at least one `chatParticipantRole`. A complete
chat record retains an author occurrence; more than one supported author may
coexist.

`chatParticipantEndpoint` accepts an existing `Agent` or `OnlineAccount`.
`chatParticipantAgent` and `chatParticipantAccount` retain the more specific
source evidence and specialize that endpoint. An account-only record supplies no
unknown holder, and neither relation infers `holdsAccount`. The initial roles are
`chatRoleAuthor` and `chatRoleRecipient`; the role vocabulary remains open.

Use `entryOrdinal` when participant order is supplied. That position belongs to
the occurrence in its message, not to the agent or account globally. Email’s
address-based participant model remains separate.

## Preserve parts and their source order

`ChatMessagePart` specializes transport-neutral `BodyPart`. Attach each occurrence
with shared `hasBodyPart` and retain a zero-based `entryOrdinal` local to the
message. Reused files can have distinct wrapper occurrences and positions.
Missing bodies are not replaced with synthetic content.

A media wrapper uses shared `partMedia` to reference its `MediaObject`. The media
can separately reference digest-identified pre-delivery source bytes through
`sourceMedia`. This keeps wrapper, delivered file and generator source identity
apart. A positive `tagContainsIllustrativeText` on an image says its pixels contain
text; absence of the tag proves nothing about text presence.

## Retain what the reader could not interpret

`UnreadableChatPart` describes a retained occurrence that the identified reader
could not read on reception. It keeps the exact original content named by
`contentDigest`, its source position and `chatPartReader`. This is a classification
of that retained reading context, not a universal claim that those bytes cannot
be parsed. Another reader can produce an interpretation of the same bytes.

`DisagreeingChatText` keeps original sender text that the identified reader found
to disagree with accompanying typed parts. It retains `contentDigest`,
`chatPartReader` and `disagreesWithPart` pointing to the compared occurrence.
Both parts remain in the same message. Neither the text nor the typed parts are
rewritten to force agreement, and the attributed comparison is not automatically
a logical contradiction.

Keep reader error-report prose with the reader’s own activity rather than
substituting it into conversation content. A digest identifies the preserved
content bytes, not a newly serialized diagnostic description of the wrapper.
Locations can record where those bytes were retained; they are audit metadata,
not the content identity.

The [conversation scene](examples/conversation.ttl) retains account-only
participation, a room and response turn, original parts, a reader-specific
classification and an intended reply refused by a guard. See the
[design rationale](design/CHAT.md) for canonical obligations and scope.
