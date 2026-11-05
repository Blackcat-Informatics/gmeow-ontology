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
folksonomy multi-tenant. And `Tagging` is the reified act, a `logic:Relator` bearing
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

The reified tagging act: a `logic:Relator` mediating tagged resource, tag, tagger, and
optionally scheme and interval. Bears `wasAttributedTo`, `confidence`, and
`displayable false` for retraction without deletion (Principle 10). Structurally the
NameUsage/IdentityFacet idiom wearing a different hat — it inherits time-scoping,
confidence-weighting, and retract-without-delete for free. EL-visible mediation is
axiomatised: every Tagging has some tagged resource, some tag, and some Agent
tagger. Existing required paths and qualified maxima retain exactly one resource,
tag and resource-valued tagger per valid act. The target uses logic:Thing, so
Activities and class resources are admitted alongside enduring entities.

## The flat shortcut and the aboutness boundary

### gmeow:hasTag

The 80 % flat shortcut: resource → tag, nonfunctional, all tags co-equal.
Its universal logic:Thing domain admits an Activity or a class resource without
using an information wrapper. Tagging a class resource does not instantiate that
class; applying a label does not assert an EventType or classify the target as
Entity. Opaque resource references need no existing concrete target-type triple. Period,
confidence, tagger, and suppression ride RDF-star statement annotations on the shortcut;
promote to a `Tagging` relator when the act itself must be a node. The pairing is
machine-usable — `gmeow:hasTag gmeow:pairsWith gmeow:Tagging` is asserted in the module,
so `gmeow describe` renders the promotion path from structure.

### gmeow:isAbout

Subject-matter aboutness: an Entity is *about* another Entity. Both domain
and range remain Entity. The broader tagging target scope does not authorize
isAbout on Activity or class-resource targets merely because hasTag accepts
them. Declared
`owl:propertyDisjointWith gmeow:hasTag` — the axis separation is an axiom, not a
convention. The third axis, `rdf:type`, cannot be made disjoint in OWL (it is not an
ObjectProperty in the ontology). The native Rust conformance guard checks that no subproperty or
equivalence bridge collapses these axes. The declared disjointness and canonical
logic: validation remain independent of the target scope.

## The relator roles

### gmeow:taggingTagged · gmeow:taggingTag · gmeow:taggingScheme

The functional-per-relator roles: one tagged resource, one tag, and (at most) one scheme
per `Tagging`. Two acts applying two tags are two relators — which is exactly what lets
each carry its own provenance and confidence. The tag itself may still belong to many
schemes; functionality binds the *act*, not the tag.

### gmeow:taggingTagger

The Agent who performed the act. The existing class-local required path and
qualified maximum retain exactly one resource-valued tagger per valid Tagging;
no global functional-property assertion is added. Preserve actual source acts.
Only genuinely distinct tagging acts receive separate identities; do not invent
acts to split an account naming several collaborators. Independent actual acts
may coexist, without a preferred tag or tagger (Principle 9).

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
slice that lets users label resources.

The native logic:Relator discipline counts declared mediation roles and their
functional characteristics. It does not prove actual runtime relata are distinct.
The gUFO bridge is a validation-only, lossy view: gufo:mediates has an Endurant
range, while canonical tagging permits occurrence and class-resource targets.
The existing bridge drops canonical logic:mediates and records that loss; it
does not assert faithful gUFO mediation of an Activity or coerce it to Endurant.
No new bridge, enforcer or external typing commitment follows from this repair.

The existing contested-tagging competency scene retains photo suppression and
adds flat Activity, reified Activity and class-resource examples with the same
Tag identity. Its asserted-lane query checks those explicit identities and roles.
Such checks do not prove native no-coercion, generated validation or reasoned
consistency; those require their actual qualified execution.

## Preserve unresolved tag wording

`tagAsWritten` keeps a writer's exact tag wording when it does not resolve to
a tag in the relevant scheme. It is literal source content, independent of
`hasTag`, and does not mint a `Tag`, assert a scheme match or choose a preferred
spelling. Preserve supplied language and RDF 1.2 direction; an unstated language
remains an `xsd:string`. Retain attribution through existing statement metadata.
Resolved tags and unresolved wording may coexist without overwriting one another.
The notes owner specializes this role with `purposeAsWritten`.
