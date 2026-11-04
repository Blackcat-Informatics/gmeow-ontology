<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Reference navigation

This extension preserves mentions, internal cross-reference occurrences, index
wording and attributed division prerequisites from an exact reference revision.
It records source evidence without resolving names, parsing locators or
executing a prerequisite model (Principles 4, 9, 12 and 16).

## Exact revision and local origin

`navigationSource` names the exact InformationObject revision artifact,
with its existing `contentDigest`. It is distinct from a mutable work identity,
path, locator string or timestamp. Its digest is supplied identity evidence;
the ontology neither hashes nor authenticates source bytes.

For Mention, `mentionIn` names the containing ContentBlock. For CrossReference,
`refersFrom` names its source ContentSegment, including a ContentBlock.
That resource’s original decoded text is the coordinate origin. The origin
explicitly repeats the same navigationSource binding; a resolved internal
target repeats it too. Extra revision bindings must agree. New editions
therefore do not silently reuse these local coordinates.

`spanStart` and `spanEnd` retain their existing zero-based, half-open Unicode
code-point convention. Start is inclusive, end exclusive, and the span is
nonempty. Offsets are local to the containing block or segment, before
normalization or other text rewriting. They are not whole-document offsets,
UTF-8 byte counts, grapheme counts or visual positions in right-to-left text.

`surfaceForm` retains one original wording literal with its supplied language
and direction. It is source data, not an analyzed language Form or a normalized
entity name. The caller is responsible for binding the exact original decoded
text. Numeric range ordering alone does not certify that the wording equals
the actual source substring.

The four original text carriers, `surfaceForm`, `indexTerm`, `indexSubTerm`
and `indexLocator`, specialize the existing domain-free `lang:literalValue`.
This common literal spine retains lexical form, datatype, language and
direction without turning their subjects into language Forms or Signs.

## Mentions and unresolved wording

Mention is an EvidenceSpan occurrence. Its optional `mentionOf` names the
explicitly resolved Entity; an unresolved occurrence retains no invented
referent. Competing attributed resolutions can coexist without identity merges
or a preferred winner. The referenced entity need not belong to the reference
revision or be a Person.

Do not use `mentions`, whose actual domain is Note. The existing
`selectorTextQuote`, `selectorTextPosition`, `selectorPage` and
`selectorLocator` still have Selector domains, so they are not aliases for
these fields. This extension does not change those owners or force a Mention
into Selector.

## Internal cross-references and generic reference meaning

CrossReference is another EvidenceSpan occurrence. Its optional `refersTo`
target is a ContentSegment in the same exact source revision. A section,
volume, figure or block can be a target without becoming CreativeWork.
Missing targets remain unresolved; CitationAct is not used as an internal
segment-reference wrapper.

`refersTo` specializes `references` for the occurrence itself. It does not
assert a generic reference triple on its containing block. Raw source queries
can reach a target with the supported inverse/sequence path:

```sparql
PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
SELECT ?source ?target WHERE {
  ?source ^gmeow:refersFrom/gmeow:refersTo ?target .
}
```

For an auditable result, retain the occurrence and revision explicitly:

```sparql
PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
SELECT ?reference ?source ?target ?revision WHERE {
  ?reference gmeow:refersFrom ?source ;
             gmeow:refersTo ?target ;
             gmeow:navigationSource ?revision .
}
```

These paths use the supplied links and need no inferred superclass or
subproperty triples. No resolver, new query engine or materialized source
`references` claim is implied.

## Optional actual language surfaces

When an actual language surface is supplied, `occurrenceSurface` points to
its `lang:SurfaceForm`. It retains the same original surfaceForm literal,
including supplied language and direction. One of its existing
`lang:SurfaceAnchor` records must name the same exact local origin,
start, end and `lang:codepointOffset`.

The surface keeps its own sign-system, script, encoding, normalization,
collation and analysis-level contracts. Explicit `lang:UnanalyzedProse`
can preserve raw words without an analyzed Form or fabricated denotation.
An absent surface record does not fabricate that metadata. Byte or
document-global anchors cannot stand in for the selected local coordinate
contract.

## Index wording and prerequisites

IndexEntry is an InformationObject with required `indexTerm`, original
`indexSubTerm` and `indexLocator` wording when supplied, optional
`indexConcept` and its exact navigationSource. The textual fields are
nonfunctional and preserve language and direction. A page range or
“see transport” remains textual evidence independently of resolution.
The Concept link does not create a definition or identify the locator string
with a Concept.

`assumes` relates ContentSegments directly. It does not inherit the existing
CreativeWork domains of `requires`, and it creates no universal or transitive
prerequisite relation. Keep the source and interpreting viewpoint on the exact
RDF 1.2 statement. Mutual or conflicting readings can coexist without an
invented universal ordering.

[The ordinary source scene](examples/source-navigation.ttl) includes resolved
and unresolved mentions and references, directional source wording, a supplied
raw language surface, index fields and attributed prerequisites.
[The design](design/REFERENCE-NAVIGATION.md) explains identity and grounding.
These are canonical source obligations and supported query paths, not claims
that extraction, resolution, hashing or contextual reasoning has run.

## Original recognized occurrences

`recognizerLabel` preserves a recognizer's verbatim string on the original
`EvidenceSpan` occurrence. Its domain is broader than `Mention`: a recognized
occurrence may cross block boundaries, while a Mention keeps its required
block-local origin. The code is a reported label, not class membership,
resolved identity, a score or evidence of a model run.

`rootTokenParse` links that original occurrence to a supplied
`lang:TokenParseReport`. The report describes the parser-selected root without
equating the occurrence with a token or manufacturing a token identifier,
head slot or composed-form tree. `occurrenceInAnalysisInput` identifies the
`lang:Analysis` whose selected input concerns the occurrence; it does not
turn the Analysis result into a text carrier.

Keep the actual recognizer vantage on each exact RDF 1.2 assertion. Several
labels, reports and analysis-input associations may coexist. Each parse report
retains its own `lang:parseInAnalysis`; do not join every report to every
occurrence input and require their contexts all to be equal.

A block-local projection is a separate occurrence description. Copying an
original span's label or root report onto each fragment does not establish that
each fragment was independently recognized or has the same root token.
Supplied offsets retain their own origin and existing code-point semantics;
these relations perform no coordinate conversion.

See [reported token syntax](../../grounding/lang/docs.md#reported-token-syntax)
and [analysis input descriptors](../../core/ai/docs.md#reported-analysis-inputs)
for the report fields and supplied input context.

## Existing terms for alias text and block-local offsets

Supplied alias wording uses the names slice's existing
`Entity → hasAppellation → Appellation → fullName` pattern. It adds a co-equal
name object, not an entity identity link or a preferred name. A fictional
Character can use this generic Entity route; the person-scoped `hasName`
would incorrectly type it as Person. Retain actual source and viewpoint on
the bearing statement. Unknown name language stays unknown; raw wording does
not authorize language interpretation or locale matching without actual frames.
See [supplied aliases](../../core/names/docs.md#supplied-aliases-and-unknown-name-language).

A Mention is the occurrence of wording in one exact block and revision,
independently of whether that wording is supplied as an entity's alias.
The existing fields already express the requested block-local character offsets:

| Field | Existing obligation |
| --- | --- |
| `mentionIn` | One exact containing ContentBlock, whose original decoded text is the coordinate origin. |
| `navigationSource` | One exact revision artifact on the Mention; the block repeats the same binding and extra revision bindings must agree. The artifact retains its supplied `contentDigest`. |
| `surfaceForm` | One original wording literal, preserving supplied language and direction. |
| `spanStart`, `spanEnd` | One nonempty zero-based half-open Unicode-code-point interval in that block: start inclusive, end exclusive. |
| `mentionOf` | Optional explicitly resolved Entity; competing attributed resolutions may coexist. |

Read the supplied occurrence and origin together:

```sparql
PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
SELECT ?mention ?block ?revision ?start ?end ?wording WHERE {
  ?mention a gmeow:Mention ;
           gmeow:mentionIn ?block ;
           gmeow:navigationSource ?revision ;
           gmeow:spanStart ?start ;
           gmeow:spanEnd ?end ;
           gmeow:surfaceForm ?wording .
  ?block gmeow:navigationSource ?revision .
}
```

These coordinates precede normalization or rewriting. Whole-document positions,
UTF-8 byte counts, grapheme indices and visual positions require an actual
conversion before they can claim this contract; changing a predicate does not
convert them. The source digest and agreement laws retain supplied identity and
consistency evidence, not authenticated bytes or proof of substring equality.
The query reads recorded links; it neither checks every law nor executes a
converter or resolver.

Original recognized occurrences and their block-local fragments remain distinct.
Do not propagate a label, root-token report, resolution or name-bearing claim to
every fragment merely because the original span has it. Character narrative-frame
obligations and actual language SurfaceForm obligations remain unchanged. No
new alias or offset property, coordinate convention or identity rule is required
by this existing-term mapping (Principles 2, 4, 7, 9, 11, 17 and 19).
