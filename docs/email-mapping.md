<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Gmeow `mail_message` → GMEOW mapping

How the `gmeow` mail store's `mail_message` facet (and its compound parts) maps to
GMEOW email terms, so the importer can emit GMEOW RDF. Each email becomes a
`gmeow:EmailMessage`; participants are reached through `gmeow:EmailAddress` (the
seam to people and accounts); residence and address tenure are time-scoped.

## Facet metadata fields

| gmeow facet field | GMEOW term | Object kind |
|---|---|---|
| `rfc_message_id` | `gmeow:rfcMessageId` | literal (identity key) |
| `subject` | `gmeow:subject` | literal |
| `date` | `gmeow:sentAt` | dateTime |
| `from` | `gmeow:from` | → `gmeow:EmailAddress` |
| `sender` | `gmeow:sender` | → `gmeow:EmailAddress` |
| `reply_to` | `gmeow:replyTo` | → `gmeow:EmailAddress` |
| `to` | `gmeow:to` | → `gmeow:EmailAddress` |
| `cc` | `gmeow:cc` | → `gmeow:EmailAddress` |
| `bcc` | `gmeow:bcc` | → `gmeow:EmailAddress` |
| `thread_id` | `gmeow:partOfThread` | → `gmeow:Thread` |
| `internal_date` / `received_at` | `gmeow:receivedAt` | dateTime |
| `size_estimate` | `gmeow:sizeEstimate` | integer |
| `label_ids` | `gmeow:residesIn` (→ `gmeow:Mailbox`) and/or `gmeow:hasKeyword` | per Gmail label¹ |
| `gmail_message_id` | `gmeow:identifier` (provider id) | literal |
| `history_id` | — (provider sync cursor; not modelled) | — |
| `classification_label_values` | `gmeow:hasKeyword` | → `gmeow:MessageKeyword` |

¹ Gmail system labels split: `INBOX`/`SENT`/`DRAFT`/`SPAM`/`TRASH` → a `gmeow:Mailbox`
with `gmeow:mailboxRole` (`inbox`/`sent`/`drafts`/`junk`/`trash`) reached by
`gmeow:residesIn` (time-scoped via `gmeow:MailboxResidence`); `UNREAD`/`STARRED` →
`gmeow:hasKeyword` (`keywordSeen` absent / `keywordFlagged`); `CATEGORY_*` → keywords.

## Compound parts

| gmeow part role | GMEOW term |
|---|---|
| `rfc822_headers` | `gmeow:hasHeader` → `gmeow:MessageHeader` (`headerName`/`headerValue`) |
| `email_body` | `gmeow:hasBodyPart` → `gmeow:BodyPart` (`gmeow:mediaType`) |
| `mime_structure` → attachments[] | `gmeow:hasAttachment` → `gmeow:Attachment` (`filename`, `mediaType`) |
| `attachment` (bytes) | the `gmeow:Attachment`'s content; a PDF attachment is also a `gmeow:Document` |

Derived artifacts (text extraction, AI summary, embedding) are separate
`gmeow:InformationObject`s linked to the attachment/body by `gmeow:wasDerivedFrom`,
carrying `gmeow:confidence` and `gmeow:wasGeneratedBy` (a `gmeow:SoftwareAgent`).

### MIME body part metadata (issue #133)

GMEOW models the MIME body-part tree using the existing `BodyPart` / `Attachment` / `hasBodyPart` spine, with the universal `hasPart` / `partOf` relation for internal multipart structure (Principle 12: decoding and reconstruction are computations, not OWL entailments).

| Source facet / header | GMEOW term | Object kind |
|---|---|---|
| MIME part ID | `gmeow:partId` | literal |
| `Content-ID` header | `gmeow:contentId` | literal |
| `Content-Type` charset parameter | `gmeow:charset` | literal |
| `Content-Disposition` | `gmeow:hasContentDisposition` | → `gmeow:ContentDisposition` (`contentDispositionInline` / `contentDispositionAttachment`) |
| `Content-Transfer-Encoding` | `gmeow:hasContentTransferEncoding` | → `gmeow:ContentTransferEncoding` (`transferEncodingBase64`, `transferEncodingQuotedPrintable`, `transferEncoding7bit`, `transferEncoding8bit`, `transferEncodingBinary`) |
| multipart subtype | `gmeow:hasMultipartType` | → `gmeow:MultipartType` (`multipartTypeAlternative`, `multipartTypeMixed`, `multipartTypeRelated`, `multipartTypeSigned`, `multipartTypeEncrypted`, `multipartTypeReport`, `multipartTypeDigest`, `multipartTypeParallel`) |

**Multipart structure.** A `gmeow:MultipartBodyPart` is a `BodyPart` that contains other body parts via `hasPart` / `partOf`. The multipart subtype is recorded with `hasMultipartType`. Child parts may be `BodyPart`, `InlinePart`, or `Attachment` instances.

**Inline parts.** A `gmeow:InlinePart` is a `BodyPart` displayed inline within the message body. It is reached from the message via `hasInlinePart` (a subproperty of `hasBodyPart`). An inline image in an HTML message is an `InlinePart` with `contentDispositionInline` and a `contentId` referenced by a `cid:` URL.

**Disposition.** `InlinePart` and `Attachment` are not declared disjoint (Principle 9). The explicit disposition is recorded with `hasContentDisposition`; the kind is a separate facet that may or may not align with the disposition value.

**Decoded content.** Decoded plain-text or HTML body content is not stored as a literal property on the part. Instead, it is modeled as a derived `gmeow:InformationObject` (typically `gmeow:TextExtraction`) linked to the source `BodyPart` by `gmeow:wasDerivedFrom`, carrying provenance and confidence (Principle 12).

## Threading

`References` / `In-Reply-To` headers → `gmeow:references` / `gmeow:inReplyTo`
(→ `gmeow:EmailMessage`); `thread_id` → `gmeow:partOfThread`.

## Thread subject normalization

The raw `gmeow:subject` is the canonical RFC 5322 Subject header (Principle 4).
Threading systems (JMAP, IMAP, Gmail) strip reply/forward prefixes (`Re:`,
`Fwd:`, `AW:`, `SV:`, etc.) to group messages into conversations. GMEOW models
the result of this normalization explicitly:

- `gmeow:threadSubject` — the base subject, attached to the `gmeow:Thread`.
  This is a derived display value, not an identity key.
- `gmeow:subjectPrefix` — the prefix(es) removed from an individual message's
  subject. Non-functional; nested prefixes produce multiple values.

Prefix stripping and base-subject computation are importer/projection behavior
(Principle 12). The algorithm follows RFC 5256 § 2.1 base-subject rules where
applicable, but GMEOW does not mandate a specific implementation; the importer
asserts the result.

## Trust indicators (forward-looking)

These headers are preserved in `rfc822_headers` but not yet parsed by gmeow. When
parsed, map per the `messaging-trust` module:

| Source | GMEOW term |
|---|---|
| `Received:` chain | `gmeow:hasRelayHop` → `gmeow:RelayHop` (`relayFrom`/`relayBy`/`relayAt`/`relayProtocol`/`hopOrdinal`) |
| `Authentication-Results:` (DKIM/SPF/DMARC/ARC) | `gmeow:hasAuthenticationResult` → `gmeow:AuthenticationResult` (`authMethod`/`authResult`/`authServer`) |
| `DKIM-Signature:` | `gmeow:hasSignature` → `gmeow:DKIMSignature` (`signingDomain`/`signatureAlgorithm`/`verificationStatus`) |
| S/MIME / PGP signature | `gmeow:SMIMESignature` / `gmeow:PGPSignature` |

## Participant model: three layers

GMEOW distinguishes three layers for email addresses:

1. **`EmailAddress`** — the stable, normalized contact point. Carries structural
   facts (`gmeow:addressValue`, `gmeow:localPart`, `gmeow:domainPart`) and links
   to agents (`AddressTenure`) and accounts (`deliversToAccount`).
2. **`MessageParticipant`** — the contextual occurrence of an address in a
   message header or envelope. Carries `displayName`, `rawAddressValue`,
   `participantRole`, `participantHeader`, and `participantOrdinal`. Scoped to
   the occurrence, never a global claim about the EmailAddress.
3. **`AddressTenure`** — the time-scoped fact that an agent held an address.

### Flat shortcuts vs. reified relator

The existing flat properties (`gmeow:from`, `gmeow:to`, `gmeow:cc`, `gmeow:bcc`,
`gmeow:sender`, `gmeow:replyTo`) remain the 80% shortcut. Promote to
`MessageParticipant` when any of the following matters:

- Raw syntax, comments, or quoting must be preserved
- Envelope vs. header distinction (e.g. envelope-from vs. From)
- Resent-* roles
- Display-name variance across messages
- Ordering within a recipient list
- Provenance, confidence, or validation status
- Malformed or spoofed values that must not contaminate contact identity

### Normalization rules (importer-side, Principle 12)

- `addressValue` is the normalized addr-spec (SMTP-lower-case local part,
  Punycode A-label for IDN domains).
- `localPart` is the mailbox portion; case-folding and dot-removal are
  computation-layer concerns.
- `domainPart` is the Punycode A-label for IDN domains; the Unicode U-label
  is a projection-downcast concern.
- Malformed or missing addr-specs: `participantAddress` is omitted from
  the `MessageParticipant`; the `rawAddressValue` retains the malformed text
  for audit.

## Identity & temporal grounding

- An `EmailAddress` is held by an `Agent` over time → `gmeow:AddressTenure` (a
  `gmeow:TimeScopedRelation`); the same address `gmeow:deliversToAccount` an
  `gmeow:OnlineAccount`. This is the seam joining message ↔ person ↔ account.
- A message's residence in a mailbox/label is time-varying → `gmeow:MailboxResidence`
  (a `gmeow:TimeScopedRelation` with `gmeow:duringInterval`); the convenience
  `gmeow:residesIn` may carry `gmeow:validFrom`/`gmeow:validUntil` as RDF-star
  annotations for the current/simple case.

## Behavioral metadata — MessageKind and raw-header facets (issue #137)

Gmeow ingests complete mail archives including delivery status notifications,
abuse reports, read receipts, and auto-generated responses. These categories
are **overlapping** (a bounce is also auto-generated) and are modelled as an
open value vocabulary rather than subclasses, so no single category is forced
to win (Principle 9).

### MessageKind vocabulary

| Kind | gmeow individual | RFC / standard |
|---|---|---|
| Delivery Status Notification (DSN) | `gmeow:messageKindDeliveryStatusNotification` | RFC 3464 |
| Bounce (hard or soft) | `gmeow:messageKindBounce` | RFC 3464 (specialised DSN) |
| Abuse Reporting Format (ARF) | `gmeow:messageKindFeedbackReport` | RFC 5965 |
| Read receipt (MDN) | `gmeow:messageKindReadReceipt` | RFC 3798 |
| Auto-generated response | `gmeow:messageKindAutoGenerated` | RFC 3834 |

The property `gmeow:hasMessageKind` relates a `gmeow:Message` to one or more
`gmeow:MessageKind` values. It is non-functional: a single message may carry
multiple kinds.

### Raw-header-backed datatype properties

The canonical source for these values is the `rfc822_headers` part
(`gmeow:hasHeader` → `gmeow:MessageHeader`). The datatype properties below are
convenience projections (Principle 4):

| Source header | GMEOW term | Range | Notes |
|---|---|---|---|
| `X-Priority` | `gmeow:priority` | Literal | Typically 1–5; non-standardised across clients |
| `Importance` | `gmeow:importance` | Literal | `high`, `normal`, `low` |
| `User-Agent` / `X-Mailer` | `gmeow:userAgent` | Literal | Raw header string |
| `Auto-Submitted` | `gmeow:autoSubmitted` | Literal | RFC 3834 values |
| `Precedence` | `gmeow:precedence` | Literal | `bulk`, `list`, `junk`, etc. |

### Software agent identification

When the importer parses `User-Agent` or `X-Mailer` and identifies the sending
software, it may assert:

| Source | GMEOW term | Object kind |
|---|---|---|
| Parsed `User-Agent` / `X-Mailer` | `gmeow:sentBySoftware` | → `gmeow:SoftwareAgent` |

The raw header value always remains on `gmeow:userAgent` (Principle 4).

### Read receipt / disposition notification

| Source | GMEOW term | Object kind |
|---|---|---|
| `Disposition-Notification-To` | `gmeow:dispositionNotificationTo` | → `gmeow:EmailAddress` |
| Presence of above header | `gmeow:readReceiptRequested` | `xsd:boolean` |

The boolean `gmeow:readReceiptRequested` is a convenience projection derived
from the presence of `Disposition-Notification-To`. It is intentionally
**non-functional**: different sources or parsers may disagree or strip the
header (Principle 9). The canonical underlying fact is the
`gmeow:dispositionNotificationTo` address (or addresses). The raw header is
also preserved in `rfc822_headers`.

## Mailbox hierarchy (issue #132)

Mailboxes form a tree within an account. The canonical hierarchy spine is
`gmeow:parentMailbox` / `gmeow:childMailbox`, which specialize the universal
`gmeow:partOf` / `gmeow:hasPart` relations. All other hierarchy-derived values
are projection-layer conveniences (Principle 12).

### Hierarchy spine

| Source | GMEOW term | Notes |
|---|---|---|
| JMAP `parentId` | `gmeow:parentMailbox` | Direct parent in folder tree |
| JMAP `parentId` (inverse) | `gmeow:childMailbox` | Inverse of `parentMailbox` |

### Provider-derived UI state

| Source | GMEOW term | Range | Notes |
|---|---|---|---|
| JMAP `sortOrder` | `gmeow:mailboxSortOrder` | `xsd:integer` | Sibling ordering; mutable provider state |
| Derived path string | `gmeow:mailboxPath` | `rdfs:Literal` | Display path (e.g. `INBOX/Work/Projects`); computed from transitive `parentMailbox` |
| Derived count | `gmeow:mailboxTotalMessages` | `xsd:integer` | Rollup over `MailboxResidence` / `residesIn` |
| Derived count | `gmeow:mailboxUnreadMessages` | `xsd:integer` | Rollup over residence + absence of `keywordSeen` |

These are **not asserted as canonical facts** in the ontology; they are computed
by the importer/projection layer and may carry provenance or standpoint
annotations when needed (Principles 2–3).

### Lifecycle and destruction

A destroyed mailbox is **retained, not erased** (Principle 10). Use the lifecycle
module rather than a boolean flag:

| JMAP concept | GMEOW pattern |
|---|---|
| `isDestroyed` | `gmeow:hasDestructionEvent` → `gmeow:Event` + `gmeow:displayable false` |

### System vs user classification

JMAP `isSystem` is a provider classification, not an ontic identity distinction.
GMEOW does **not** model this as subclasses (`SystemMailbox` / `UserMailbox`)
because there is no identity difference — a user-created folder may later be
promoted to a special-use role, and a provider may auto-create folders with no
standard role (Principle 9).

System/user origin is a **projection concern**; the canonical signal is
`gmeow:mailboxRole` for JMAP special-use roles (`inbox`, `archive`, `drafts`,
`sent`, `trash`, `junk`, `templates`). A mailbox without a role is treated as
user-created by convention.

## Calendar invitations and event descriptions (issue #139)

When gmeow ingests an email with a `text/calendar` MIME part or a `.ics`
attachment, the message and the event it describes are structurally linked in
GMEOW without duplicating the event model.

### Event description bridge

| Source | GMEOW term | Object kind | Notes |
|---|---|---|---|
| `text/calendar` attachment | `gmeow:calendarAttachment` | → `gmeow:Attachment` | Subproperty of `hasAttachment`; keeps the attachment first-class |
| Parsed iCalendar VEVENT | `gmeow:describesEvent` | → `gmeow:Event` | Reuses `events.ttl` event spine; non-functional |
| iCalendar `UID` | `gmeow:calendarUid` | Literal | Non-functional; competing UIDs may coexist |
| iCalendar `METHOD` | `gmeow:hasCalendarMethod` | → `gmeow:CalendarMethod` | Value vocabulary: request, reply, cancel, publish, counter, decline-counter |
| Message kind | `gmeow:hasMessageKind` | → `gmeow:messageKindCalendarInvitation` | Overlaps with other kinds (auto-generated, etc.) per Principle 9 |

The social act of invitation — organizer, invitee, acceptance/decline status,
RSVP — is modeled via `gmeow:EventInvitation` from the calendar module
(`calendar.ttl`). The email is the **carrier**; the `EventInvitation` is the
social act. An email may carry zero, one, or many `EventInvitation` relators
(a group invitation parsed into multiple per-invitee relators, or a single
message carrying both an invitation and a cancellation).

### iCalendar alignment

| GMEOW | iCalendar (RFC 5545/5546) | Relationship |
|---|---|---|
| `gmeow:describesEvent` | `VEVENT` component | The email describes the event the VEVENT represents |
| `gmeow:calendarUid` | `UID` | Direct correspondence |
| `gmeow:hasCalendarMethod` | `METHOD` | Value vocabulary aligned to iTIP METHOD values |
| `gmeow:calendarAttachment` | `ATTACH` with `VALUE=URI` or inline | The attachment carrying the calendar data |

### Cancelled invitations

A cancelled invitation is **retained, not erased** (Principle 10). The email
remains in the store with its `hasCalendarMethod gmeow:calendarMethodCancel`
and the original `describesEvent` link intact. Suppression (hiding from UI)
is handled through the projection layer, never by deletion.
