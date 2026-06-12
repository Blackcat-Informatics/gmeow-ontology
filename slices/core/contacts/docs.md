<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Contacts — channels, addresses, and ties that carry their own time

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/contacts` · **tier: core**
> Contact points (email, telephone, postal), temporally-scoped contact relationships, and
> the reified interpersonal-relationship relator — the schema.org / vCard / REL superset.

An address book flattens three different things into rows: *channels* (an email address, a
phone number), *tenure* (who held that address, when — people acquire and relinquish
addresses), and *ties* (who knows whom, in what capacity, over what period). GMEOW models
each honestly. Channels are first-class `ContactPoint` objects; tenure and ties follow the
**flat-first, reify-on-demand** pattern — flat shortcuts carry their period at the statement
level with the temporal slice's `validFrom`/`validUntil` clocks (Principles 2–3), and
promote to a relator (`InterpersonalRelationship`, `AddressTenure`) when the fact itself
needs identity, evidence, or confidence. A postal address, meanwhile, is **frame-relative**
(Principle 11): its components are coordinate values along the axes of the postal reference
frame — the as-written surface form — kept apart from the resolved, identifier-bearing
place hierarchy it denotes.

This slice is the superset layer over schema.org `ContactPoint`/`PostalAddress`, vCard, and
the REL vocabulary (Principle 5: model it correctly, bridge by reference); identity *trust*
between contacts (keys, certifications, owner-trust) lives in the cross-cutting trust
module, and the email *message* model lives in the email extension — this slice is the
contact half of the #287 email split.

## Channels

### gmeow:ContactPoint

A means of reaching an agent, attached via `hasContactPoint`. Three subkinds:
`EmailAddress`, `TelephoneNumber`, `PostalAddress`. The flat `gmeow:email` /
`gmeow:telephone` literals remain for the 80 % case where the channel needs no structure —
flat-first, never the only form.

### gmeow:EmailAddress

The structured channel: `addressValue` (the normalized addr-spec), `localPart`,
`domainPart` — all functional. `deliversToAccount` is the seam to the accounts slice: an
*address* is held by an agent; the *account* it delivers to is where messages reside. The
mail corpus's address book is this slice's named consumer (Principle 15). Envelope display
names and internationalized addresses are tracked by issue #134 and land here.

### gmeow:PostalAddress

An address **expressed in a reference frame** (`postalAddressFrame`, functional — exactly
one frame, default `referenceFramePostalAddress`). Its components — `streetAddress`,
`extendedAddress`, `postOfficeBox`, `addressLocality`, `addressRegion`, `postalCode`,
`countryCode` — are coordinate values along that frame's axes: the *as-written* form, all
non-functional because multi-source values conflict and must coexist ("CA" / "Canada" /
"CAN" are three records, not one truth).

### gmeow:addressPlace

The seam from surface form to geography: the `gmeow:Place` an address denotes (typically
the premises), from which `containedInPlace*` climbs the resolved, QID-bearing place
hierarchy with its coordinates, geometry, and external identifiers. Geocoding — turning the
written form into that place — is solver work, never an asserted equivalence
(Principle 12).

## Tenure

### gmeow:AddressTenure

The reified, time-scoped fact that an agent held a contact point over an interval — a
`TimeScopedRelation` with functional `tenuredContactPoint` and `addressHolder`. Use it when
the holding itself needs identity (a shared mailbox's succession of owners, an address
recycled across people — the mail corpus's hard cases); otherwise annotate
`hasContactPoint` with the statement clocks.

## Ties

### gmeow:hasMet · gmeow:hasWorkedWith · gmeow:hasUsed · gmeow:hasAgreement

The flat shortcuts (the REL-superset layer). `hasMet`/`hasWorkedWith` are symmetric
agent-agent records; `hasUsed` reaches any entity; `hasAgreement` joins an agent to an
agreement it is party to — the party-ship period rides this statement, while the
agreement's own term lives on the `Agreement` (no double-modelling). All carry their period
with `validFrom`/`validUntil` on the statement, exactly as the gmeow store keeps
valid_from/valid_until per claim.

### gmeow:InterpersonalRelationship

The promoted form: a standing tie as a `gufo:Relator` (mediating and existentially
depending on its players — the same idiom as genealogy's `KinRelationship` and names'
`NameUsage`), for when the tie must bear its own interval, confidence, or evidence.
Subkinds `ProfessionalRelationship` (reified `hasWorkedWith`) and
`AcquaintanceRelationship` (reified `hasMet`).

### gmeow:relationshipParty · gmeow:relationshipInterval

`relationshipParty` is non-functional — typically two parties, deliberately open for group
ties; the EL mediation axiom (issue #38) makes "a relationship mediates at least one agent"
a reasoner-visible fact, while the closed-world "exactly two" is SHACL's job (issue #39).
`relationshipInterval` carries the tie's period as a first-class `TimeInterval` (relators
carry intervals this way; `duringInterval` is reserved for situation-based time-scoped
relations).

```turtle
ex:tie a gmeow:ProfessionalRelationship ;
    gmeow:relationshipParty ex:ada , ex:grace ;
    gmeow:relationshipInterval ex:i1998to2004 ;
    gmeow:confidence 0.9 .                     # the promoted form earns evidence
```

## Solver and alignment notes

Address normalization, geocoding, and frame conversion are computations outside the logic
(Principle 12): the slice stores the as-written coordinates and the denoted place; nothing
asserts that two written variants "are" the same address. Projections downcast to
`vcard:ADR` / `schema:PostalAddress` (flat strings reassembled from the coordinate values)
and the flat ties to the REL vocabulary; the frame indirection and the statement clocks are
canonical and survive only here (Principle 4).

## Dependencies

Depends on `kernel`, `temporal` (clocks, intervals, `TimeScopedRelation`), `places`
(`addressPlace` and the place hierarchy), `accounts` (`deliversToAccount`), `agreements`,
and `observations`. Consumed by the mail corpus's address book and the email extension.
