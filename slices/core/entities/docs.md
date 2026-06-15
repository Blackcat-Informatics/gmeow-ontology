<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Entities — people, organizations, groups, and software agents

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/entities` · **tier: core**
> The FOAF / schema.org superset layer: the agent sortals everything else describes.

This slice mints the concrete agent Kinds beneath the kernel's `gmeow:Agent` category —
the things the mail corpus, the identity facets, and every people-describing slice talk
about. The stance is deliberately thin: the sortals carry identity, while names, contact
points, locations, documents, and trust assessments live in their own slices and attach
*to* these Kinds. Where common vocabularies flatten a person into a bag of string fields,
GMEOW keeps the flat tier honest (a simple `name` literal for entities that need nothing
more) and routes everything with structure — person names above all — to the structured
model, downcasting to "First Last" only in the projection layer (Principle 10: projection
is where lossiness is allowed, never storage).

The module's one axiom block is doctrine made checkable (relator-mediation doctrine): distinct `gufo:Kind`
classes each supply their own principle of identity, so the core ultimate Kinds are
asserted pairwise disjoint — and only those whose disjointness is *true*.

## The agent sortals

### gmeow:Person

An individual human being, living or deceased. A `gufo:Kind` — the identity-supplying
sortal that the names slice's `PersonName`, the identity facets, and the relationship
relators all anchor to. Disjoint with Organization, SoftwareAgent, Location, ContactPoint,
CryptographicKey, Appellation, Language, and WritingSystem.

### gmeow:Organization

A structured group of agents able to act as a single agent — a company, institution,
association, or governmental body. Sub-organization decomposition specializes the
kernel's universal `gmeow:partOf` spine in its own slice; here only the Kind lives.

### gmeow:Group

A collection of agents treated as a unit *without* the formal structure of an organization
(`gufo:Collection`, under `gmeow:Entity` rather than `gmeow:Agent`). Deliberately
**excluded** from the disjointness axiom: a structured organization is arguably a group,
and asserting that overlap away would manufacture inconsistency (Principle 9).

### gmeow:SoftwareAgent

A software process or autonomous program acting on behalf of a person or organization.
Disjoint with `Person` — a human is never a bot — while delegation and attribution are
provenance-layer relations, not subclassing.

## The flat naming tier

### gmeow:name

The `rdfs:label` tier: a simple, language-tagged label for entities that do not need the
full naming apparatus (organizations, software agents, sources, keys). It carries **no
precedence** over an entity's other names (Principle 9 — no privileged selector). Persons'
names are `gmeow:PersonName` structures in the names slice; the flat given/family
rendering is produced by downcasting that model at projection time, never stored here.

### gmeow:description

The unstructured NOTE field — a biography, a remark, an annotation. Genuinely flat: unlike
names, dates, or roles, a note has no structure to reify, so no relator pairing exists.
Distinct from `skos:definition`, which documents ontology *terms*, not instances.
Non-functional; language-tag where applicable.

### gmeow:hasWebPage

The structured form of a "website / URL" field: the page is a first-class `gmeow:WebPage`
(documents slice) whose IRI is its URL, so it can itself carry a title, language, and
rights. Non-functional — an entity may have several pages. Prefer this over a bare URL
literal: the bridge into the document layer is the point.

## The disjointness doctrine (relator-mediation doctrine)

The `owl:AllDisjointClasses` axiom covers Person, Organization, SoftwareAgent, Location,
ContactPoint, CryptographicKey, Appellation, Language, and WritingSystem — cross-module
IRIs that resolve after merge. Deliberately excluded: `Group` (above) and the
`InformationObject` document / message / source family, where a Source may legitimately
also be a Document or CreativeWork. The rule is constitutional: add only disjointness
that is TRUE; benign overlap must never become an inconsistency (Principle 9).

## Alignment & boundaries

This is the layer that projects to FOAF (`foaf:Person`, `foaf:Organization`,
`foaf:Agent`) and schema.org (`schema:Person`, `schema:Organization`) — by reference and
lossy projection, never import (Principle 5). Entity resolution — deciding that two
person records denote one human — is a solver-layer computation over the identity facets
(Principle 12); the slice never asserts `owl:sameAs` shortcuts. Depends on kernel,
names, contacts, places, documents, language, and trust; consumed by every slice that
describes people, organizations, or things.
