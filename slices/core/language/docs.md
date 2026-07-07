<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Language — first-class languages, registry-independent by design

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/language` · **tier: core**
> The slim language foundation every slice depends on: `Language` (now a
> `lang:SignSystem`), the scripts it binds via `lang:Orthography`, and the seed
> languages the framework itself speaks.

The standard pattern — `inLanguage "ja"` — makes a registry code the *identity* of a
language. GMEOW inverts this: a language is a **first-class information object with a
self-minted IRI**, and every registry (BCP-47, ISO 639, Glottolog, Wikidata) is an
*optional alignment*, never identity (Principle 5: bridge by reference). The payoff is
anti-colonial in both directions (Principle 9): a language is not less real for lacking a
committee-issued code — a conlang, an Indigenous language absent from a Western registry,
or an AI-generated language is exactly as first-class as English — and a richly-coded
language is not flattened to whichever single tag a schema happened to standardize.
Language sits in **core** (Principle 16) because every GMEOW literal depends on it: this is
the slim half of the slice-dependency doctrine dependency split, with the rich sociolinguistic machinery
(proficiency, varieties, diachronic states, conlang lineage) in the `languages` extension.

Internally, GMEOW's own literals carry **private-use tags** (`@x-gmeow-english`, never
`@en`) so that the canonical graph never pretends a registry tag is ground truth; standard
BCP-47 is *reconstructed on projection* (Principle 4: one canonical source, lossy
projections generated outward).

## The classes

### gmeow:Language

A system of signs and rules for communication or computation, under
`gmeow:InformationObject`. Identity is the IRI; codes are data (`languageCode`), registry
coreference is `skos:exactMatch` + `gmeow:authorityLink`. Its names are co-equal
`gmeow:Appellation`s — endonym and exonym, never preferred-vs-alternate (the names
doctrine); the scripts it is written in bind co-equally via the grounding layer's
`lang:Orthography`.

### lang:Script · lang:Orthography

A script is a first-class object — Latin, Han, Hiragana, Arabic, Braille, or a bespoke
conlang/AI script — now a **`lang:Script`** in the `lang:` grounding layer (the former
`gmeow:WritingSystem`). Its ISO 15924 code, when one exists, rides `skos:notation` (e.g.
`skos:notation "Latn"^^xsd:string`); the old `gmeow:writingSystemType` (alphabet /
syllabary / …) and `gmeow:textDirection` (ltr / rtl / …) facets are a **declared loss** in
the graft — they have no `lang:` equivalent and are no longer modelled.

A language binds to a script through a **`lang:Orthography`** individual —
`lang:orthographyFor <language>` and `lang:usesScript <script>` — one orthography per
(language, script) pair. A language co-mingling several scripts (Japanese = Han + Hiragana +
Katakana + Latin) simply has several co-equal `lang:Orthography` individuals, so multiple
concurrent scripts remain the normal case, not an edge case. The former reified
`gmeow:WritingSystemUsage` relator with its script-role and usage-interval facets is
retired in the graft (declared loss).

## The tag machinery

### lang:carrierTag (internal tags retired)

The per-language internal private-use tag (the former `gmeow:languageTag`, `"x-gmeow-english"`)
is **retired in the graft**: a language's self-minted IRI is now its identity, so most
languages carry no internal tag at all. The framework's own `@x-gmeow-*` authoring tags now
ride **`lang:carrierTag`** on exactly three carrier-variety individuals —
`lang:gmeowEnglish`, `lang:gmeowFrench`, and `lang:gmeowMandarin`, each a
`lang:LanguageVariety` of the corresponding sign system. Those three are the join key
between literal-space and IRI-space for GMEOW's own labels and definitions; no other
language needs one.

### gmeow:bcp47Tag

The optional **external** tag (`"en"`, `xsd:language`) used only when projecting into
vocabularies that demand standard tags. Non-functional: one language legitimately exposes
several BCP-47 tags across script/region/variant contexts — another reason a single flat
tag was never going to be identity.

### gmeow:languageCode

Any registry code — BCP-47 `"ja"`, ISO 639-3 `"jpn"`, a Glottocode — as a see-also
alignment value, **never identity**. Non-functional: a language carries codes in several
registries at once, and a code-less language carries none and loses nothing.

## The seed languages

### lang:english · lang:mandarin · lang:french

The languages the framework itself speaks — unified with the `lang:` grounding sign systems
(same identity). GMEOW's own labels and definitions are authored against these three via the
`@x-gmeow-english` / `@x-gmeow-mandarin` / `@x-gmeow-french` carrier tags (which now ride
`lang:carrierTag` on the carrier varieties, not the languages themselves). They are ordinary
data, not schema: any slice or dataset mints further languages the same way (the exhaustive
reference catalog is the design).

```turtle
ex:langKlingon a gmeow:Language ;            # code-less, fully first-class
    rdfs:label "tlhIngan Hol"@x-gmeow-klingon ;
    gmeow:languageCode "tlh" .               # alignment, not identity
```

## Formal languages and transliteration

### lang:signSystemKind — formal and programming languages

Grammar-defined languages with machine modality and no native speakers — programming,
markup, query, logic, schema languages. The old `gmeow:FormalLanguage` → `gmeow:ProgrammingLanguage`
subclass ladder is retired: `gmeow:Language` (now `rdfs:subClassOf lang:SignSystem`) carries
the distinction as data via **`lang:signSystemKind`** pointing at kind individuals —
`lang:formalLanguageKind` for formal languages, and `lang:programmingLanguageKind` (which
refines `formalLanguageKind` via `skos:broader`) for programming languages. A programming
language is thus written `a gmeow:Language ; lang:signSystemKind lang:programmingLanguageKind ;
lang:modality lang:writtenModality`, and remains the first-class target of `writtenInLanguage`
for the software slice's source trees.

### gmeow:transliterationScheme · gmeow:TransliterationScheme

*How* a romanization was derived — Hepburn vs Kunrei, Pinyin vs Wade-Giles — attached to a
`gmeow:Appellation` beside its `gmeow:romanization`. This is the bridge into the names
slice's doctrine that a romanization relates a name only to a transliteration of *itself*;
recording the scheme makes the derivation reproducible. The major schemes are catalogued as
FnO functions in the projection layer (`projections/transforms.fno.ttl`).

### gmeow:writtenInLanguage

The generic written-in relation (FRBR's language-of-expression, domain-wide): any
`InformationObject` — a document, an expression, a source tree, an inscription reading — is
written in a first-class `Language` IRI, never a code literal. Non-functional: content
mixes languages, and a codebase uses several. The registry-independent replacement for the
`inLanguage "ja"` anti-pattern GMEOW's own doctrine names.

## Solver and projection notes

Tag work is computation, not assertion (Principle 12): reconstructing a standard BCP-47 tag
from `bcp47Tag` (+ script/region context), executing a transliteration scheme, and matching
a name's language to an audience locale all happen in the solver/projection layer — the
canonical graph stores the first-class objects and their alignments, nothing derived.
Projections to schema.org (`schema:inLanguage`), Dublin Core (`dcterms:language`), and
Wikidata are downcasts; the private-use-tag discipline is canonical and internal.

## Dependencies

Depends on `kernel` and `names` (Appellation, romanization). Depended on by effectively
everything: every `x-gmeow-*` literal in every slice resolves here, names' romanizations
cite its schemes, and software's `writtenInLanguage` targets its classes (P16 slim core).
