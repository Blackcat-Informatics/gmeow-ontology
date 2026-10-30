<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Exact-source navigation design

Mention and CrossReference specialize the existing EvidenceSpan category as
source-occurrence Kinds. IndexEntry specializes InformationObject. They retain
their actual information-object identity and do not become the referent,
analyzed form, Note, Selector, CitationAct or CreativeWork (Principles 4 and 5).

The exact revision is explicit through navigationSource and its existing
contentDigest. This is source identity, not a path or clock reading.
A local origin is independently explicit through mentionIn or refersFrom.
Canonical agreement laws bind that origin and any supplied internal target to
the occurrence’s exact revision and reject extra incompatible revision
bindings. Mention referents are different: an Entity mentioned by a text need
not be part of that text, so mentionOf does not impose revision membership.

Required source fields use class-local all-values restrictions,
ClosureEntries and qualified at-most-one restrictions. No global functional
carrier is added. Each occurrence has one local origin, original wording,
start, end and selected revision; optional resolution links remain
nonfunctional and do not force an identity winner. Index term/subterm/locator
wording remains nonfunctional.

The original offset unit comes from the actual spanStart/spanEnd definition:
zero-based, half-open Unicode code points. The source resource is the local
coordinate origin, before text rewriting. Two targetable formula laws require
ordered nonempty ranges. They do not calculate a substring, convert bytes or
graphemes, normalize text, or authenticate source data. These algorithmic
operations remain explicit caller responsibilities.

The optional language surface composes with existing lang structures.
The four raw text carriers also specialize the actual domain-free
lang:literalValue property, whose range is rdfs:Literal. This preserves their
complete literal values through a real canonical carrier relation without
imposing a language-form or sign type on the occurrence or index record.
A supplied surface’s text must agree with the original RDF literal, and a
matching SurfaceAnchor must carry the same local source, offsets and
codepointOffset. Shared surfaces may have other anchors; the law requires a
matching anchor rather than discarding other occurrences. Raw original prose
does not silently become a parsed Form. These are source and form layers,
not competing selector models (Principles 2, 11 and 19).

No selector-property domain is widened. Actual selectorTextQuote,
selectorTextPosition, selectorPage and selectorLocator domains still infer
Selector; Note’s mentions similarly infers Note. Domain-free numeric span
properties can be reused without those coercions.

RefersTo’s canonical subproperty bridge preserves the occurrence’s generic
references meaning. Source-to-target navigation uses the existing inverse and
sequence path mechanisms, or the equivalent explicit occurrence join.
This distinction matters because raw source querying does not assume ontology
closure. The documentation does not claim that source-block references were
materialized or that a resolution is true because a link was supplied.

IndexEntry keeps locator prose apart from a resolved Concept, whose actual
owner defines it as a SocialObject. Assumes has ContentSegment endpoints
without a requires superproperty: requires would infer CreativeWork on both
ends. No universal DAG, transitivity or resolution executor is invented.
Source attribution, recorder responsibility and interpreting viewpoint remain
separate on RDF 1.2 statements (Principles 9 and 17).

The reference-work consumer is an extension, not a new mandatory core engine.
Direct dependencies name documents, evidence, ai, citations, concepts, lang,
entities, provenance, sources and standpoint alongside logic and kernel.
Integer offset comparisons reuse the canonical logic relations and existing
code-point space; no physical quantity or duplicate mathematical coordinate
system is introduced. All validation obligations remain logic axioms or
realized formula trees. No existing owner module, test, fixture, competency
case, producer or parser is changed.
