<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Organization — modelling & interoperability guide

GMEOW's organization slice is a **W3C Organization Ontology (ORG) superset** that
reuses the universal event, lifecycle, temporal, place, and standpoint machinery
rather than minting parallel mechanisms (Principle 4). It de-conflates concepts
that surface vocabularies routinely collapse:

| Collapsed in surface vocab | GMEOW de-conflation |
|---|---|
| "CFO" = the person *and* the title | `gmeow:Post` (seat) ⟂ `gmeow:Membership` (holder) |
| "department" = org type *and* sub-organization | `gmeow:subOrganizationOf` (structural) + `organizationType` (value) |
| "founding date" = flat date on org | `hasCreationEvent` → universal `Event` with time, place, roles, standpoint |
| "merged company" = single successor truth | Coexisting `accordingTo`-annotated `successorOrganization` claims |

## Post — the seat independent of the holder

**`gmeow:Post`** is a `gufo:RoleMixin` that represents a seat or position (the
CFO chair) independently of who currently holds it. A Post is linked to exactly
one Organization via the functional property `gmeow:postIn`. A `gmeow:Membership`
may `gmeow:fillsPost` the Post, making the distinction between the seat and the
sitter explicit.

This enables:

- **Vacant posts**: a Post exists with no Membership filling it.
- **Succession in a post**: two Memberships (successive tenures) fill the same Post.
- **Org mismatch detection**: SHACL warns when a Membership's `membershipOrganization`
  differs from the Post's `postIn` organization.

## Organization type — an open value vocabulary

`gmeow:OrganizationType` is a `gufo:QualityValue` vocabulary (individuals, never
subclasses — Principle 9). The seed values are:

- `company` — for-profit business
- `nonprofit` — not-for-profit organization
- `governmentBody` — public-sector body
- `educationalInstitution` — school, university, college
- `association` — society, union, or membership body
- `collaboration` — consortium, joint venture, standards body

These mirror ORG's `FormalOrganization` / `OrganizationalUnit` /
`OrganizationalCollaboration` distinctions as **co-equal values**, not disjoint
subclasses. A single organization may carry several types (a university that is
also a nonprofit), and competing standpoint-indexed type claims coexist without
privileging one (Principle 9).

## Site — organizational location

`gmeow:hasSite` links an Organization to a `gmeow:Location` (from the places
module), with `gmeow:siteType` marking the purpose:

- `headquarters` — principal office
- `branch` — subsidiary location
- `registered` — legal / registered office

Reusing the places module means a site is a first-class `Place` with coordinates,
geometry, containment, and gazetteer coreference — not a flattened address string.

## Multi-organization change events

Creation and destruction of a single organization reuse the universal lifecycle
hooks (`hasCreationEvent` / `hasDestructionEvent`, with `eventTypeCreation` /
`eventTypeDestruction`). **Do not duplicate them.**

Transitions that relate **two or more** organizations use the event module's
universal `Event` with new `EventType` values:

- `eventTypeMerger` — two+ orgs combine
- `eventTypeSplit` — one org divides into two+
- `eventTypeSpinOff` — a part breaks away
- `eventTypeAcquisition` — one org acquires another
- `eventTypeRename` — same entity, new name (reuses the names module)

`gmeow:predecessorOrganization` and `gmeow:successorOrganization` link the orgs
an event relates. A rename additionally uses the names module plus a temporal
tenure (Facebook → Meta).

### Contested succession

Post-merger or post-coup, rival successor claims coexist as **standpoint-indexed
statements** (Principle 9). Two `accordingTo`-annotated `successorOrganization`
claims on the same event are both retained — there is no `preferredSuccessor`.
Withdrawn claims stay with `displayable false`, never deletion (Principle 10).

## Legal identity

`gmeow:legalIdentifier` and `gmeow:industryClassification` are **reified** via the
`gmeow:Identifier` class to avoid conflation when an organization carries multiple
codes (e.g. LEI + ROR + NAICS). Each Identifier node bundles `gmeow:identifierValue`
(the string) and `gmeow:identifierScheme` (`lei`, `ror`, `naics`, `isicV4`, etc.).
Reification ensures SPARQL projections pair the correct value with its scheme.
Optional `gmeow:jurisdiction` links to a `gmeow:Location`.

## Purpose

`gmeow:organizationPurpose` is free text. Hierarchical resolution and cross-scheme
mapping are **solver-side** computations (Principle 12); the logical core stays
OWL 2 DL.

## Alignment by reference

The mapping layer extends the existing shared alignment files:

| GMEOW term | W3C ORG | schema.org |
|---|---|---|
| `Post` | `≡ org:Post` | lossy → `OrganizationRole` |
| `postIn` | `≈ org:postIn` | — |
| `fillsPost` | `≈ org:holds` | — |
| `hasSite` | `≡ org:hasSite` | `→ schema:location` |
| `organizationType` values | `≈ org:FormalOrganization` etc. | `→ schema:Corporation` etc. |
| `predecessorOrganization` | `≈ org:originalOrganization` | — |
| `successorOrganization` | `≈ org:resultingOrganization` | — |
| `organizationPurpose` | `≡ org:purpose` | — |
| `industryClassification` | `≈ org:classification` | `→ schema:naics` / `schema:isicV4` |
| `legalIdentifier` (lei) | — | `→ schema:leiCode` |
| `hasIdentifier` (generic) | — | scheme-dependent |

All alignments are by reference (SSSOM / EDOAL / SPARQL projection) — never
axiom copying (Principle 5).
