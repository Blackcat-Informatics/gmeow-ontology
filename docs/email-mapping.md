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

## Threading

`References` / `In-Reply-To` headers → `gmeow:references` / `gmeow:inReplyTo`
(→ `gmeow:EmailMessage`); `thread_id` → `gmeow:partOfThread`.

## Trust indicators (forward-looking)

These headers are preserved in `rfc822_headers` but not yet parsed by gmeow. When
parsed, map per the `messaging-trust` module:

| Source | GMEOW term |
|---|---|
| `Received:` chain | `gmeow:hasRelayHop` → `gmeow:RelayHop` (`relayFrom`/`relayBy`/`relayAt`/`relayProtocol`/`hopOrdinal`) |
| `Authentication-Results:` (DKIM/SPF/DMARC/ARC) | `gmeow:hasAuthenticationResult` → `gmeow:AuthenticationResult` (`authMethod`/`authResult`/`authServer`) |
| `DKIM-Signature:` | `gmeow:hasSignature` → `gmeow:DKIMSignature` (`signingDomain`/`signatureAlgorithm`/`verificationStatus`) |
| S/MIME / PGP signature | `gmeow:SMIMESignature` / `gmeow:PGPSignature` |

## Identity & temporal grounding

- An `EmailAddress` is held by an `Agent` over time → `gmeow:AddressTenure` (a
  `gmeow:TimeScopedRelation`); the same address `gmeow:deliversToAccount` an
  `gmeow:OnlineAccount`. This is the seam joining message ↔ person ↔ account.
- A message's residence in a mailbox/label is time-varying → `gmeow:MailboxResidence`
  (a `gmeow:TimeScopedRelation` with `gmeow:duringInterval`); the convenience
  `gmeow:residesIn` may carry `gmeow:validFrom`/`gmeow:validUntil` as RDF-star
  annotations for the current/simple case.
