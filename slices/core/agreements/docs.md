<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Agreements — the relator that grounds every binding

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/agreements` · **tier: core**
> Mutual understandings reified as relators — and the `foundedOn` hook that lets every other relator say *why it binds*.

A flat `partyA contractsWith partyB` triple cannot carry a date, a clause, a name, or a
dispute. GMEOW therefore models an agreement the way UFO says it should be modelled: as a
**relator** (`gufo:Relator`) — a first-class truthmaker that *binds* its parties rather than
a predicate that merely connects them. The agreement node owns its identity, its
appellations, its temporal scope (statement-layer clocks), and its provenance; the parties
hang off it via `gmeow:hasParty`.

The slice is deliberately small because its real surface is everything else: in UFO, a
relator is *founded on* a set of commitments, and `gmeow:foundedOn` makes that foundation
explicit and reusable. A `Membership`, a `KinRelationship` by adoption, an employment
tenure, a licence — any relator anywhere in the graph can declare the agreement that grounds
it, instead of duplicating contractual structure per slice. Legal enforceability is a
*specialization*, not a property flag: `gmeow:Contract` is the SubKind for agreements a
legal system will enforce, and the handshake deal stays an honest `gmeow:Agreement`.

## The relator core

### gmeow:Agreement

A mutual understanding between two or more agents that creates obligations or rights,
reified as a `gufo:Kind` relator connecting its parties. Anything from a treaty to a verbal
promise; when the terms themselves need deontic structure, link a RightsStatement (rights
slice) rather than widening this class.

### gmeow:Contract

The legally enforceable SubKind of `gmeow:Agreement`. Enforceability is the identity-bearing
distinction — not a boolean on Agreement — so contract-specific machinery (governing law,
execution, breach events) has a typed home without burdening informal agreements.

### gmeow:hasParty

Binds the agreement to each `gmeow:Agent` it obligates. Non-functional and repeated: one
statement per party, any arity. Roles within the agreement (licensor/licensee,
employer/employee) belong to the consuming relator or a Participation, not to new
subproperties here.

### gmeow:AgreementName

An `gmeow:Appellation` SubKind for the formal title, short name, or multilingual version of
an instrument ("CETA", "Comprehensive Economic and Trade Agreement"). Each name is a
first-class object carrying its own `gmeow:nameLanguage` and `gmeow:nameScript`, so co-equal
multilingual names coexist instead of fighting over one literal (Principle 9 — no
`primaryName`).

### gmeow:foundedOn

The cross-cutting hook: any `gufo:Relator` → the `gmeow:Agreement` that grounds it — UFO's
foundation-on-commitments made a queryable edge. This is how licences-as-agreements (rights
slice), employment terms, and calendar RSVPs all resolve to one contractual spine instead of
three parallel ones.

## Worked shape

```turtle
ex:ceta a gmeow:Contract ;
    gmeow:hasParty ex:canada, ex:europeanUnion ;
    gmeow:hasAppellation ex:cetaShortName .

ex:membership-ca-ceta a gmeow:Membership ;
    gmeow:foundedOn ex:ceta .
```

The membership relator carries the tenure; the contract carries the binding; neither
duplicates the other.

## Solver layer & deferred alignment

The ontology asserts who is bound and on what foundation — nothing more. Obligation
fulfillment, breach detection, term expiry, and clause interpretation are solver-layer
computations over the relator plus its rights statements and statement-layer clocks
(Principle 12); the graph never carries an `isInBreach` flag for the same reason the
deception slice carries no `isFalse`. No mapping profile ships yet: alignment by reference
(schema.org's agreement-adjacent terms, FIBO contracts) is deliberately deferred to the
alignment window, where it lands as SSSOM rows rather than imported axioms.

## Dependencies

Depends on `kernel` (Agent, the relator grounding) and `names` (Appellation, for
`AgreementName`). Consumed by `rights` (licences are agreements), `employment` (terms of
engagement), and `calendar` (RSVPs as lightweight agreements) — each via `gmeow:foundedOn`,
never by re-modelling the contract.
