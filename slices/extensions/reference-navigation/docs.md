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

## Curated bibliographic descriptions

Use `curatedBibliographicDescription` to associate an actual InformationObject
with each supplied `CuratedBibliographicDescription`. This source-wording report
follows IndexEntry’s evidence-preserving pattern. It is not an IndexEntry and
does not inherit its required index term or navigationSource. A supplied work
or volume remains the subject; the description is a separate curation account.
Distinct coherent accounts retain distinct descriptions even when words agree.

`bibliographicTitleWords` and `bibliographicAuthorWords` select independent
StructuredSequences. Each item retains one complete supplied wording string,
not a word token. Distinct StructuredValue occurrences, structuredTerm and
entryOrdinal retain repetitions and source order. Title and author positions
are independent: unequal lengths are valid, and equal indices establish no
pairing, translation, authorship, name identity or preferred variant. Preserve
full `xsd:string`, `rdf:langString` and `rdf:dirLangString` literals with actual
language and direction; do not guess tags or normalize source wording.

`bibliographicAuthorityReferences` selects a third independent sequence. Each
scalar structuredTerm is an actual resource typed AuthorityReferenceRecord,
with that record’s own required target IRI, authority-source wording and local
field laws. An IRI-shaped string is not a record resource. Repeated references
use distinct sequence-item occurrences and may reuse an actual record identity.
Different actual reference records stay distinct even when targets agree.
Membership adds no inferred authorityReference edge, property chain, match
strength, identity merge or authorship on the described subject.

The five scalar roles, `bibliographicWorkKind`,
`bibliographicOriginalAuthorWording`, `bibliographicCollectionKind`,
`bibliographicCollectionNote` and `bibliographicPublicationDateText`, retain
complete supplied word literals through Lang’s literal carrier. Original-script
wording establishes no Agent, Appellation, linguistic analysis or relation to
an author-sequence item. Kind wording establishes no controlled type, collection
identity or membership. Date text preserves its source precision; it supplies
no dateTime, timezone, midnight or seasonal interval. Separately justified
interpretations use their actual existing contracts.

Each sequence relation and scalar field is independently optional and locally
at most one per coherent description. The subject association is nonfunctional.
Blank supplied strings remain valid. Explicit empty sequence requires evidence
that the source actually supplied it. An omitted relation makes no empty or null
assertion; a schema default proves no historical presence. Never fill missing
fields with empty strings, empty sequences, inferred names or guessed dates.
No universal navigationSource or digest is required for these descriptions.
If an actual exact source revision is independently supplied through that
existing relation, its existing source and digest obligations still apply.

BibliographicWordSequenceConstraint requires a scalar term for every selected
title or author item and checks the full word-literal datatype union for every
supplied term. BibliographicAuthoritySequenceConstraint requires a scalar term
for every selected authority item and checks every term’s actual record type.
Generic branch, whole-term identity and ordinal laws remain unchanged. These
are ordinary canonical Formulas selecting generated procedural validation;
no new native failure marker, independent reasoner discharge or runtime result
is claimed (Principles 4, 5, 9, 17 and 19).

Keep the actual source on each exact statement, including sequence membership,
item payload, ordinal and reference fields. A record-wide provenance edge does
not attribute its outgoing assertions or propagate to a target. Naming the
reported authority in authorityReferenceSource is distinct from naming the
curation artifact that supplied a statement. Do not invent an asserting Agent
or standpoint when only the supplied source artifact is known.

The following fictional scenario explicitly supplies one information-object
subject and three curation artifacts as background resources. The remaining
assertions are the facts those curations actually supply; every one is both
asserted and individually reified below with its own supplied source. The
reference records are the complete records from the authority-reference unit,
not literal locators or invented resolved authors.

Curation A supplies two equal title strings at distinct positions, one author
string, five scalar fields and two distinct authority-reference records. Its
original-author field explicitly supplies a Japanese tag; its author-sequence
string is untagged. This establishes no transliteration relation. Its date text
is retained without defining a calendar or season interval. The two authority
records share a target but retain different notes and record identities.

Curation B explicitly supplies an empty title sequence and a sequence reusing
the first actual authority record. It supplies no author-sequence information.
Curation C supplies a description association and type, with no sequence or
scalar information. Thus B’s explicit empty titles differ from C’s absent title
information. No case is inferred from a schema default. B’s reuse of a record
does not attribute A’s record fields to B or transfer their provenance.

```turtle
@prefix ex: <https://example.org/curated-bibliography-scene/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

# Explicit background resource identities supplied by this fictional scenario.
ex:subject a gmeow:InformationObject .
ex:curationA a gmeow:InformationObject .
ex:curationB a gmeow:InformationObject .
ex:curationC a gmeow:InformationObject .

# Actual assertions supplied by the named curation artifacts.
ex:subject gmeow:curatedBibliographicDescription ex:descriptionA .
ex:descriptionA rdf:type gmeow:CuratedBibliographicDescription .
ex:subject gmeow:curatedBibliographicDescription ex:descriptionB .
ex:descriptionB rdf:type gmeow:CuratedBibliographicDescription .
ex:subject gmeow:curatedBibliographicDescription ex:descriptionC .
ex:descriptionC rdf:type gmeow:CuratedBibliographicDescription .
ex:descriptionA gmeow:bibliographicTitleWords ex:titlesA .
ex:descriptionA gmeow:bibliographicAuthorWords ex:authorsA .
ex:descriptionA gmeow:bibliographicAuthorityReferences ex:referencesA .
ex:descriptionA gmeow:bibliographicWorkKind "reference volume"@en .
ex:descriptionA gmeow:bibliographicOriginalAuthorWording "ソラ"@ja .
ex:descriptionA gmeow:bibliographicCollectionKind "coastal studies"@en .
ex:descriptionA gmeow:bibliographicCollectionNote "" .
ex:descriptionA gmeow:bibliographicPublicationDateText "Spring, year of the amber tide"@en .
ex:titlesA rdf:type gmeow:StructuredSequence .
ex:titlesA gmeow:structuredItem ex:titlesAItem0 .
ex:titlesAItem0 rdf:type gmeow:StructuredValue .
ex:titlesAItem0 gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger .
ex:titlesAItem0 gmeow:structuredTerm "Amber coast"@en .
ex:titlesA gmeow:structuredItem ex:titlesAItem1 .
ex:titlesAItem1 rdf:type gmeow:StructuredValue .
ex:titlesAItem1 gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger .
ex:titlesAItem1 gmeow:structuredTerm "Amber coast"@en .
ex:authorsA rdf:type gmeow:StructuredSequence .
ex:authorsA gmeow:structuredItem ex:authorsAItem0 .
ex:authorsAItem0 rdf:type gmeow:StructuredValue .
ex:authorsAItem0 gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger .
ex:authorsAItem0 gmeow:structuredTerm "Sora" .
ex:referencesA rdf:type gmeow:StructuredSequence .
ex:referencesA gmeow:structuredItem ex:referencesAItem0 .
ex:referencesAItem0 rdf:type gmeow:StructuredValue .
ex:referencesAItem0 gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger .
ex:referencesAItem0 gmeow:structuredTerm ex:firstReference .
ex:referencesA gmeow:structuredItem ex:referencesAItem1 .
ex:referencesAItem1 rdf:type gmeow:StructuredValue .
ex:referencesAItem1 gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger .
ex:referencesAItem1 gmeow:structuredTerm ex:secondReference .
ex:firstReference rdf:type gmeow:AuthorityReferenceRecord .
ex:firstReference gmeow:authorityReferenceTarget <https://example.org/authority/amber> .
ex:firstReference gmeow:authorityReferenceSource "Amber catalogue"@en .
ex:firstReference gmeow:authorityReferenceRelation "author reference"@en .
ex:firstReference gmeow:authorityReferenceNote "Check the author heading."@en .
ex:secondReference rdf:type gmeow:AuthorityReferenceRecord .
ex:secondReference gmeow:authorityReferenceTarget <https://example.org/authority/amber> .
ex:secondReference gmeow:authorityReferenceSource "Amber catalogue"@en .
ex:secondReference gmeow:authorityReferenceRelation "author reference"@en .
ex:secondReference gmeow:authorityReferenceNote "Retain the earlier catalogue wording."@en .
ex:descriptionB gmeow:bibliographicTitleWords ex:titlesB .
ex:titlesB rdf:type gmeow:StructuredSequence .
ex:descriptionB gmeow:bibliographicAuthorityReferences ex:referencesB .
ex:referencesB rdf:type gmeow:StructuredSequence .
ex:referencesB gmeow:structuredItem ex:referencesBItem0 .
ex:referencesBItem0 rdf:type gmeow:StructuredValue .
ex:referencesBItem0 gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger .
ex:referencesBItem0 gmeow:structuredTerm ex:firstReference .

# Each supplied assertion has its own reifier and actual source.
ex:claim1 rdf:reifies <<( ex:subject gmeow:curatedBibliographicDescription ex:descriptionA )>> ;
    logic:provenance ex:curationA .
ex:claim2 rdf:reifies <<( ex:descriptionA rdf:type gmeow:CuratedBibliographicDescription )>> ;
    logic:provenance ex:curationA .
ex:claim3 rdf:reifies <<( ex:subject gmeow:curatedBibliographicDescription ex:descriptionB )>> ;
    logic:provenance ex:curationB .
ex:claim4 rdf:reifies <<( ex:descriptionB rdf:type gmeow:CuratedBibliographicDescription )>> ;
    logic:provenance ex:curationB .
ex:claim5 rdf:reifies <<( ex:subject gmeow:curatedBibliographicDescription ex:descriptionC )>> ;
    logic:provenance ex:curationC .
ex:claim6 rdf:reifies <<( ex:descriptionC rdf:type gmeow:CuratedBibliographicDescription )>> ;
    logic:provenance ex:curationC .
ex:claim7 rdf:reifies <<( ex:descriptionA gmeow:bibliographicTitleWords ex:titlesA )>> ;
    logic:provenance ex:curationA .
ex:claim8 rdf:reifies <<( ex:descriptionA gmeow:bibliographicAuthorWords ex:authorsA )>> ;
    logic:provenance ex:curationA .
ex:claim9 rdf:reifies <<( ex:descriptionA gmeow:bibliographicAuthorityReferences ex:referencesA )>> ;
    logic:provenance ex:curationA .
ex:claim10 rdf:reifies <<( ex:descriptionA gmeow:bibliographicWorkKind "reference volume"@en )>> ;
    logic:provenance ex:curationA .
ex:claim11 rdf:reifies <<( ex:descriptionA gmeow:bibliographicOriginalAuthorWording "ソラ"@ja )>> ;
    logic:provenance ex:curationA .
ex:claim12 rdf:reifies <<( ex:descriptionA gmeow:bibliographicCollectionKind "coastal studies"@en )>> ;
    logic:provenance ex:curationA .
ex:claim13 rdf:reifies <<( ex:descriptionA gmeow:bibliographicCollectionNote "" )>> ;
    logic:provenance ex:curationA .
ex:claim14 rdf:reifies <<( ex:descriptionA gmeow:bibliographicPublicationDateText "Spring, year of the amber tide"@en )>> ;
    logic:provenance ex:curationA .
ex:claim15 rdf:reifies <<( ex:titlesA rdf:type gmeow:StructuredSequence )>> ;
    logic:provenance ex:curationA .
ex:claim16 rdf:reifies <<( ex:titlesA gmeow:structuredItem ex:titlesAItem0 )>> ;
    logic:provenance ex:curationA .
ex:claim17 rdf:reifies <<( ex:titlesAItem0 rdf:type gmeow:StructuredValue )>> ;
    logic:provenance ex:curationA .
ex:claim18 rdf:reifies <<( ex:titlesAItem0 gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger )>> ;
    logic:provenance ex:curationA .
ex:claim19 rdf:reifies <<( ex:titlesAItem0 gmeow:structuredTerm "Amber coast"@en )>> ;
    logic:provenance ex:curationA .
ex:claim20 rdf:reifies <<( ex:titlesA gmeow:structuredItem ex:titlesAItem1 )>> ;
    logic:provenance ex:curationA .
ex:claim21 rdf:reifies <<( ex:titlesAItem1 rdf:type gmeow:StructuredValue )>> ;
    logic:provenance ex:curationA .
ex:claim22 rdf:reifies <<( ex:titlesAItem1 gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger )>> ;
    logic:provenance ex:curationA .
ex:claim23 rdf:reifies <<( ex:titlesAItem1 gmeow:structuredTerm "Amber coast"@en )>> ;
    logic:provenance ex:curationA .
ex:claim24 rdf:reifies <<( ex:authorsA rdf:type gmeow:StructuredSequence )>> ;
    logic:provenance ex:curationA .
ex:claim25 rdf:reifies <<( ex:authorsA gmeow:structuredItem ex:authorsAItem0 )>> ;
    logic:provenance ex:curationA .
ex:claim26 rdf:reifies <<( ex:authorsAItem0 rdf:type gmeow:StructuredValue )>> ;
    logic:provenance ex:curationA .
ex:claim27 rdf:reifies <<( ex:authorsAItem0 gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger )>> ;
    logic:provenance ex:curationA .
ex:claim28 rdf:reifies <<( ex:authorsAItem0 gmeow:structuredTerm "Sora" )>> ;
    logic:provenance ex:curationA .
ex:claim29 rdf:reifies <<( ex:referencesA rdf:type gmeow:StructuredSequence )>> ;
    logic:provenance ex:curationA .
ex:claim30 rdf:reifies <<( ex:referencesA gmeow:structuredItem ex:referencesAItem0 )>> ;
    logic:provenance ex:curationA .
ex:claim31 rdf:reifies <<( ex:referencesAItem0 rdf:type gmeow:StructuredValue )>> ;
    logic:provenance ex:curationA .
ex:claim32 rdf:reifies <<( ex:referencesAItem0 gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger )>> ;
    logic:provenance ex:curationA .
ex:claim33 rdf:reifies <<( ex:referencesAItem0 gmeow:structuredTerm ex:firstReference )>> ;
    logic:provenance ex:curationA .
ex:claim34 rdf:reifies <<( ex:referencesA gmeow:structuredItem ex:referencesAItem1 )>> ;
    logic:provenance ex:curationA .
ex:claim35 rdf:reifies <<( ex:referencesAItem1 rdf:type gmeow:StructuredValue )>> ;
    logic:provenance ex:curationA .
ex:claim36 rdf:reifies <<( ex:referencesAItem1 gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger )>> ;
    logic:provenance ex:curationA .
ex:claim37 rdf:reifies <<( ex:referencesAItem1 gmeow:structuredTerm ex:secondReference )>> ;
    logic:provenance ex:curationA .
ex:claim38 rdf:reifies <<( ex:firstReference rdf:type gmeow:AuthorityReferenceRecord )>> ;
    logic:provenance ex:curationA .
ex:claim39 rdf:reifies <<( ex:firstReference gmeow:authorityReferenceTarget <https://example.org/authority/amber> )>> ;
    logic:provenance ex:curationA .
ex:claim40 rdf:reifies <<( ex:firstReference gmeow:authorityReferenceSource "Amber catalogue"@en )>> ;
    logic:provenance ex:curationA .
ex:claim41 rdf:reifies <<( ex:firstReference gmeow:authorityReferenceRelation "author reference"@en )>> ;
    logic:provenance ex:curationA .
ex:claim42 rdf:reifies <<( ex:firstReference gmeow:authorityReferenceNote "Check the author heading."@en )>> ;
    logic:provenance ex:curationA .
ex:claim43 rdf:reifies <<( ex:secondReference rdf:type gmeow:AuthorityReferenceRecord )>> ;
    logic:provenance ex:curationA .
ex:claim44 rdf:reifies <<( ex:secondReference gmeow:authorityReferenceTarget <https://example.org/authority/amber> )>> ;
    logic:provenance ex:curationA .
ex:claim45 rdf:reifies <<( ex:secondReference gmeow:authorityReferenceSource "Amber catalogue"@en )>> ;
    logic:provenance ex:curationA .
ex:claim46 rdf:reifies <<( ex:secondReference gmeow:authorityReferenceRelation "author reference"@en )>> ;
    logic:provenance ex:curationA .
ex:claim47 rdf:reifies <<( ex:secondReference gmeow:authorityReferenceNote "Retain the earlier catalogue wording."@en )>> ;
    logic:provenance ex:curationA .
ex:claim48 rdf:reifies <<( ex:descriptionB gmeow:bibliographicTitleWords ex:titlesB )>> ;
    logic:provenance ex:curationB .
ex:claim49 rdf:reifies <<( ex:titlesB rdf:type gmeow:StructuredSequence )>> ;
    logic:provenance ex:curationB .
ex:claim50 rdf:reifies <<( ex:descriptionB gmeow:bibliographicAuthorityReferences ex:referencesB )>> ;
    logic:provenance ex:curationB .
ex:claim51 rdf:reifies <<( ex:referencesB rdf:type gmeow:StructuredSequence )>> ;
    logic:provenance ex:curationB .
ex:claim52 rdf:reifies <<( ex:referencesB gmeow:structuredItem ex:referencesBItem0 )>> ;
    logic:provenance ex:curationB .
ex:claim53 rdf:reifies <<( ex:referencesBItem0 rdf:type gmeow:StructuredValue )>> ;
    logic:provenance ex:curationB .
ex:claim54 rdf:reifies <<( ex:referencesBItem0 gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger )>> ;
    logic:provenance ex:curationB .
ex:claim55 rdf:reifies <<( ex:referencesBItem0 gmeow:structuredTerm ex:firstReference )>> ;
    logic:provenance ex:curationB .
```

All 55 supplied curation facts above have exact-statement provenance. The
background resource declarations provide the scene’s explicit framing; they
are not claims that any curation inferred an Agent, author, CreativeWork type
or authority identity. The quoted triples alone would not assert these facts;
the separate underlying assertions do. No target is dereferenced, no source
bytes are authenticated, and no generated, installed, consumer or runtime
acceptance is demonstrated by this source scene.
