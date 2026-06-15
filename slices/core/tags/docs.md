<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Tags — open folksonomy, kept honest

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/tags` · **tier: core**
> The universal tagging building block: tag, scheme, and the reified act — three axes, never collapsed.

Tagging systems rot when three different things get smeared into one: *typing* (what a
thing ontologically is), *aboutness* (what a resource is about), and *tagging* (what label
a user chose to stick on it). This slice keeps them as three distinct, property-level
disjoint axes (Principle 9). A `Tag` is an information-object value — like a
`skos:Concept`, its identity is its IRI, its surface form is a label, and it carries no
inferential weight over `rdf:type`. A `TagScheme` is the namespace bucket that makes
folksonomy multi-tenant. And `Tagging` is the reified act, a `gufo:Relator` bearing
provenance, confidence, temporal scope, and retract-without-delete suppression
(Principle 10).

The slice is flat-first throughout: the `hasTag` shortcut covers the 80 % case, tag
hierarchy is optional, and nothing forces a scheme. Forced into core by dual extension
use (images and software both tag).

## The three classes

### gmeow:Tag

An open, user-minted tag. Synonyms are multiple labels on one IRI; homonyms are different
IRIs; coreference is asserted in data, never by collapsing tags into one. A tag is NOT a
type and NOT a property bag — no datatype value property is asserted on it. Seed
individuals (`tagUrgent`, `tagTodo`, `tagReview`) are illustrative anchors, not a fence.

### gmeow:TagScheme

A namespaced set of tags — a project vocabulary, a personal bucket, a controlled
vocabulary. Multi-tenant by design: many schemes coexist and a tag may sit in zero or
more of them via `tagInScheme` (non-functional — cross-listing is normal). The
counterpart of `skos:ConceptScheme` and `schema:DefinedTermSet`.

### gmeow:Tagging

The reified tagging act: a `gufo:Relator` mediating tagged resource, tag, tagger, and
optionally scheme and interval. Bears `wasAttributedTo`, `confidence`, and
`displayable false` for retraction without deletion (Principle 10). Structurally the
NameUsage/IdentityFacet idiom wearing a different hat — it inherits time-scoping,
confidence-weighting, and retract-without-delete for free. EL-visible mediation is
axiomatised: every Tagging has some tagged entity, some tag, and some tagger.

## The flat shortcut and the aboutness boundary

### gmeow:hasTag

The 80 % flat shortcut: entity → tag, non-functional, all tags co-equal. Period,
confidence, tagger, and suppression ride RDF-star statement annotations on the shortcut;
promote to a `Tagging` relator when the act itself must be a node. The pairing is
machine-usable — `gmeow:hasTag gmeow:pairsWith gmeow:Tagging` is asserted in the module,
so `gmeow describe` renders the promotion path from structure.

### gmeow:isAbout

Subject-matter aboutness: a resource is *about* another resource. Declared
`owl:propertyDisjointWith gmeow:hasTag` — the axis separation is an axiom, not a
convention. The third axis, `rdf:type`, cannot be made disjoint in OWL (it is not an
ObjectProperty in the ontology), so that leg of the trichotomy guard lives in SHACL and
the Python tests.

## The relator roles

### gmeow:taggingTagged · gmeow:taggingTag · gmeow:taggingScheme

The functional-per-relator roles: one tagged entity, one tag, and (at most) one scheme
per `Tagging`. Two acts applying two tags are two relators — which is exactly what lets
each carry its own provenance and confidence. The tag itself may still belong to many
schemes; functionality binds the *act*, not the tag.

### gmeow:taggingTagger

The agent who performed the act. Deliberately non-functional: a tag may be co-asserted by
multiple agents (collaborative curation), and those co-assertions coexist (Principle 9).

### gmeow:taggingInterval

The interval over which the tagging holds — a relator carries its period this way
(matching `usageInterval`, `relationshipInterval`), not via RDF-star annotations on the
relator node. Bridges to the temporal slice's `TimeInterval`.

## Optional structure

### gmeow:broaderTag · gmeow:narrowerTag · gmeow:relatedTag

SKOS-shaped tag relations — transitive broader, its inverse, and a symmetric associative
link. Strictly optional: folksonomy stays flat-first, and hierarchy is something a scheme
*grows*, never something the model demands. Transitive closure over `broaderTag` is the
reasoner's job; any heavier tag analytics (co-occurrence, suggestion) is solver-layer
work (Principle 12).

## Alignment & boundaries

Aligns lossily to SKOS (`Tag` ≈ `skos:Concept`), schema.org (`DefinedTerm`), W3C Web
Annotation (a Tagging as an annotation with a tagging motivation), and MOAT — by
reference and projection, never import (Principle 5); each target flattens away part of
the relator (most drop tagger or interval), which is why the canonical form lives here.
Depends on kernel and temporal; consumed by the images and software extensions and any
slice that lets users label things.
