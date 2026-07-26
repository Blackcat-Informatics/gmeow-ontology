<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Accounts — online accounts, centralized and decentralized, as peers

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/accounts` · **tier: core**
> The FOAF `OnlineAccount` superset plus the decentralized-identity layer: handles,
> Nostr keys, ActivityPub actors, and domain-verified identifiers.

FOAF gave the web `foaf:OnlineAccount` and then the web moved on: today an agent's online
presence spans platform accounts (a code forge, a social service) *and* decentralized
identities (a Nostr keypair, a federated ActivityPub actor) that have no "service provider"
in the FOAF sense. GMEOW models both in one class, as **co-equal accounts** (Principle 9's
co-equality stance applied to digital presence): a self-sovereign key-based identity is not
an exotic footnote to a platform handle, and a platform handle is not less real for being
custodial. There is no primary-account slot, for the same reason names has no primary name
— an agent holds many accounts via `holdsAccount`, and selection is the consumer's
frame, never the graph's hierarchy.

An account is an `InformationObject`, not the agent: the **account ≠ agent** distinction is
load-bearing exactly the way appellation ≠ person is in names. Accounts are *claimed* by
agents, get suspended, abandoned, and reused — facts about the account, not the person.
A defunct account is suppressed or time-scoped, never deleted (Principle 10): account
history is provenance for everything the mail corpus and the memory products ingest. The
slice is deliberately slim (Principle 16's small-core discipline) with named consumers —
the mail corpus, email delivery, and finance (Principle 15).

## The core pair

### gmeow:OnlineAccount

An account an agent holds with an online service — a social profile, a code-forge account,
or a decentralized identity. An `InformationObject` under `kernel`, so it bears the shared
machinery: external identifiers, `wasAttributedTo` provenance, and statement-level clocks
(`validFrom`/`validUntil`) for the holding period — flat-first, like the contacts slice's
tenures.

### gmeow:holdsAccount

Agent → account. Non-functional in both directions by design: an agent holds many accounts,
and (honestly modelled) a shared or transferred account relates to more than one agent over
time — disambiguate with the statement clocks. No inference runs from an account's display
name back to the agent's identity facets: handles are *address*, not identity (the
seven-axis matrix's address ≠ identity tenet, applied to the digital realm).

## Identifiers on the account

### gmeow:accountName

The handle or user name identifying the account *on its service* — a service-scoped string,
meaningful only paired with the service, and never a name of the *person* (a person's names
are co-equal `gmeow:PersonName`s in the names slice; an account handle never outranks or
implies them).

### gmeow:nostrPubkey

The Nostr public key (npub / hex) identifying a key-based, self-sovereign account. The key
*is* the account's identity on the protocol — no provider exists to ask — which is exactly
why decentralized accounts needed first-class modelling rather than a stretched FOAF
`accountServiceHomepage`.

### gmeow:activityPubActor

The ActivityPub actor IRI of a federated-social account — a dereferenceable identity in
fediverse space. Carried as `xsd:anyURI` data, not conflated with the account's own GMEOW
IRI: the actor IRI is what the *protocol* calls the account, an alignment in the same
spirit as language's registry codes — never identity within the graph.

### gmeow:nip05

A Nostr NIP-05 identifier (`user@domain`) verifying a Nostr account against a DNS domain —
the protocol's own claim-verification bridge. Recording it lets the trust module (keys,
certifications, owner-trust live there, not here) weigh a domain-backed account above an
unverified key without this slice ever ranking accounts itself.

```turtle
ex:kit a gmeow:Person ;
    gmeow:holdsAccount ex:forgeAcct , ex:nostrAcct .   # co-equal

ex:forgeAcct a gmeow:OnlineAccount ;
    gmeow:accountName "kit-dev" .

ex:nostrAcct a gmeow:OnlineAccount ;
    gmeow:nostrPubkey "npub1examplekey…" ;
    gmeow:nip05 "kit@example.ca" .
```

## Seams, solver, and alignment

The contacts slice joins in through `gmeow:deliversToAccount` (an email *address* delivers
to an *account* — the seam the mail corpus walks from address book to mailbox), and finance
reaches accounts for service relationships. Key verification — checking a NIP-05 document,
resolving an ActivityPub actor, proving control of a key — is solver-layer work
(Principle 12): the graph records the identifiers and the trust module records the
attestations; nothing here computes or asserts verification outcomes. Alignment is by
reference (Principle 5): `OnlineAccount`/`holdsAccount`/`accountName` map to FOAF's
`OnlineAccount`/`account`/`accountName`, with the decentralized-identity properties as
GMEOW's canonical superset — FOAF-bound projections drop them as documented lossy drops
(Principle 4).

## Dependencies

Depends only on `kernel` — accounts sit near the bottom of the core stack so that contacts
(`deliversToAccount`), the email extension, finance, and organization can all build on them
without cycles. Consumers: the mail corpus, email delivery, finance, and organization's
`acceptsPaymentMethod` (Principle 15, named in the manifest).

## Online-presence history

### gmeow:OnlineService · gmeow:accountService · gmeow:accountServiceHomepage · gmeow:serviceShutdownDate · gmeow:serviceStatus

The **service** an account is held with is a first-class `gmeow:OnlineService`
(`gmeow:accountService`, functional). Its liveness rides the flat
`gmeow:serviceStatus` (live / shut-down) and `gmeow:serviceShutdownDate` — so a
historical service (Yahoo 360, shut 2009-07-13) stays first-class with its shutdown
recorded (→ `schema:dissolutionDate`). `gmeow:accountServiceHomepage` is FOAF's
`accountServiceHomepage` idiom.

### gmeow:AccountStatus · gmeow:accountStatus

The holder's usage status of an account — an **open value vocabulary** (active /
dormant / historical). A retired account is `accountStatusHistorical` with
`validUntil` in the past, retained for the record (P10), never deleted.

## Payment method vocabulary

### gmeow:PaymentMethod

The method or instrument used to make a payment — an **open value vocabulary**
(cash, cheque, credit card, bank transfer, cryptocurrency), a VALUE never a
subclass (Principle 9). Domain-general enough that both `finance` (`Payment` ⊑
`FinancialTransaction`, via `gmeow:paymentMethod`) and `organization`
(`gmeow:acceptsPaymentMethod`, a business's accepted methods) reuse the same
individuals rather than each minting its own; `accounts` is its core home
because it is a payment-instrument identity fact independent of any one
transaction or organization.
