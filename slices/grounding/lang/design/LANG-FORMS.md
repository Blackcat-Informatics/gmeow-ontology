<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Language — Sign Systems and the Form AST

> The **form-core charter** of the GMEOW Language design set: the sign-system reference layer and
> the typed form AST. It makes precise the claims the manifesto ([`LANG.md`](LANG.md)) states once —
> that a language is not a tag, that a string is not a form, and that form identity is independent
> of encoding, script, and rendering. The meaning layer that gives forms denotations is in
> [`LANG-MEANING.md`](LANG-MEANING.md); the rendering and translation maps between forms are in
> [`LANG-TRANSLATION.md`](LANG-TRANSLATION.md); the lossy lowerings of everything here are in
> [`LANG-PROJECTIONS.md`](LANG-PROJECTIONS.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by canonical OWL/`logic:` authorities in `module.ttl`,
> their generated validation projections, competency queries, and the projection loss ledger —
> not a claim that any implementation already realizes X except as those gates demonstrate.

## Purpose

The form core names *what language is made of* before any meaning is attached: the sign systems
forms belong to, the scripts and orthographies that write them, the grammars that license them,
and the structured forms themselves — from morphs through lexemes and word forms to composed
phrases, sentences, and texts. Two regions factor the core, each with a single load-bearing
commitment.

- **Sign-system reference layer** — *a language is an individual with structure, never a tag.*
- **Form AST layer** — *a string is a surface; the form is a tree.*

## The sign-system reference layer

The reference layer lets GMEOW name a sign system — English, Turtle, IPA, the GTS grammar, Braille,
a controlled vocabulary — without collapsing its identity into a BCP-47 tag, a MIME type, or a file
extension.

Core classes: `lang:SignSystem`, `lang:LanguageVariety`, `lang:Script`, `lang:Orthography`,
`lang:Grammar`, `lang:GrammarRule`, and `lang:SignSystemKind`.

Core properties: `lang:signSystemKind`, `lang:varietyOf`, `lang:usesScript`,
`lang:orthographyFor`, `lang:grammarFor`, `lang:grammarRuleOf`, and `lang:informalGloss`.

The governing rule:

> **External identifiers name alignments, not identity.** A GMEOW sign system may align to a BCP-47
> tag, an ISO 639 code, a Wikidata QID, or a MIME type, but the GMEOW term remains the local source
> of truth. Tags are generated identifiers — the existing `gmeow:bcp47Tag` discipline, inherited.

A sign system is a first-class individual, not a subclass proliferation. The slice does **not**
mint a new OWL class per language; English, Turtle, and IPA are individuals of `lang:SignSystem`
distinguished by `lang:signSystemKind` individuals (`lang:naturalLanguageKind`,
`lang:formalLanguageKind`, `lang:notationalKind`, `lang:gesturalKind`, …), following the
kind-individual pattern the `notation` slice already uses for `gmeow:SymbolicSystemKind`. This
keeps the TBox small and the ABox expressive, matching the project's frame-relative modeling.

Three consequences of taking sign systems seriously as individuals:

- **Varieties are systems, not qualifiers.** Quebec French, Middle English, and RFC-2119
  requirement prose are `lang:LanguageVariety` individuals related to their parents by
  `lang:varietyOf`, time-scoped through the `temporal` slice where the variety is a stage. A
  BCP-47 tag like `fr-CA` is a *projection* of a variety individual, never its definition.
- **Scripts and orthographies are separated.** A script (`lang:Script` — Latin, Cyrillic, Han) is a
  character repertoire; an orthography (`lang:Orthography`) is a convention binding a sign system
  to a script (Serbian has one language, two orthographies). Transliteration between orthographies
  is a rendering-layer map ([`LANG-TRANSLATION.md`](LANG-TRANSLATION.md)), grafting under the
  existing `gmeow:TransliterationScheme`.
- **Grammars are first-class.** A `lang:Grammar` names the sign system it licenses
  (`lang:grammarFor`) and carries its rules as `lang:GrammarRule` individuals. Formal grammars
  (Turtle's, GTS's) are held natively and *projected* to EBNF/ABNF; a grammar file is a rendering
  of the grammar object, not the grammar. Formal-language-theoretic facts *about* a grammar — the
  generated language as a set, automaton equivalence, complexity class — are `math:` objects
  referencing the `lang:Grammar` individual, per the manifesto's one-way razor.

**Alignment is a mapping record, not a bespoke predicate.** External authority links follow the
established repository pattern: `gmeow:TermEquivalence` reification records in the slice's
`mappings/equivalences.ttl`, lowered as `logic:Correspondence`. The language slice introduces no
free-standing `authorityLink` predicate; a Wikidata QID or an ISO 639-3 code is an alignment
carrying its preservation judgment in the loss ledger, exactly as every other slice records its
external links.

## The form AST layer

A form that is meant to be analyzed, denoted, translated, matched, or projected is a **structured
object**, never an opaque string. This is the semiotic instance of the project-wide principle that
the canonical form is the maximal, explicit, checkable one and the surface string is a projection
of it — the same commitment `math:` makes for formulas and `logic:` makes for rules.

Core classes: `lang:Form` (the abstract root), `lang:SurfaceForm`, `lang:Lexeme`, `lang:WordForm`,
`lang:Morph`, `lang:MorphFeature`, `lang:ComposedForm`, `lang:FormSlot`, `lang:FormRole`, and
`lang:UnanalyzedProse`.

Core properties: `lang:inSignSystem`, `lang:realizes`, `lang:surfaceText`, `lang:inScript`,
`lang:encoding`, `lang:unicodeNormalization`, `lang:lexemeOf`, `lang:inflectionOf`,
`lang:partOfSpeech`, `lang:morphFeature`, `lang:featureKey`, `lang:featureValue`,
`lang:formSlot`, `lang:slotIndex`, `lang:slotForm`, `lang:slotRole`, and `lang:formHead`.

The layer is deliberately stratified the way a century of linguistics stratifies it, because each
stratum has a distinct identity criterion:

- **`lang:SurfaceForm`** — a concrete realization: text (`lang:surfaceText`) with declared script,
  encoding, and Unicode normalization form. Two surface forms with different normalization are
  different surface forms of (possibly) the same form. This is the *only* stratum where byte
  identity means anything.
- **`lang:Morph` / `lang:MorphFeature`** — the smallest meaningful units and their typed
  feature-value pairs (`Number=Plur`, `Tense=Past`), UD-alignable by construction. Morphology is
  never a free string.
- **`lang:Lexeme` / `lang:WordForm`** — the dictionary word versus its inflections. `cats` is a
  `lang:WordForm`, `lang:inflectionOf` the lexeme *cat*, carrying `Number=Plur`; the lexeme, not
  the spelling, is what senses attach to ([`LANG-MEANING.md`](LANG-MEANING.md)). This is the
  OntoLex form/lexical-entry split, held with GMEOW's claim rigor.
- **`lang:ComposedForm` / `lang:FormSlot`** — phrases, sentences, and texts as trees over indexed
  slots. A slot carries an integer `lang:slotIndex` (constituent order), the constituent form
  (`lang:slotForm`), and optionally a `lang:slotRole` (subject, object, modifier — a
  `lang:FormRole` individual, UD-relation-alignable). `lang:formHead` marks the head constituent
  where headedness is analyzed.

### The hard rules of the form AST

1. **Inferential weight requires analysis.** A form that denotes, is grammar-governed, or is
   translated participates as a structured `lang:Form`; it is not represented only by a string
   literal.
2. **Analyzed or explicitly unanalyzed — never silently either.** Every `lang:SurfaceForm` either
   `lang:realizes` an analyzed form or is typed `lang:UnanalyzedProse`. Unanalyzed prose is lawful
   and expected — most `@x-gmeow-english` fields will hold it — but it is a *recorded status*, so
   "this string has no structure yet" is a queryable fact rather than an ambient assumption.
3. **Every form names exactly one sign system.** `lang:inSignSystem` is mandatory and functional on
   every `lang:Form`. Code-switching is represented compositionally: the composed form belongs to
   its matrix system while embedded constituents carry their own — the mixture is structure, not a
   tag soup.
4. **Constituent order is indexed, not list-ordered.** Composed-form constituent order is carried
   by `lang:FormSlot` individuals with integer `lang:slotIndex`, not by RDF list ordering — the
   identical discipline as `math:ArgumentSlot`, for the identical reason: two encodings of one
   form must be one form. Slot indexes are unique per composed form; strict canonical mode requires
   them zero-based and contiguous.
5. **Morphology is typed.** Morphological content is carried as `lang:MorphFeature` pairs with a
   declared key and value drawn from a feature inventory, never as an unparsed feature string. The
   inventory aligns to UD's universal features and extends past them where a language demands it.
6. **Surface forms declare their material identity.** A `lang:SurfaceForm` carries its script and,
   where byte identity is load-bearing (hashing, anchoring, corpus offsets), its encoding and
   Unicode normalization form. The prose-hash discipline (`candidateSourceHash`) hashes surface
   forms, and an unhashable surface — one with undeclared normalization — is ill-formed for that
   use.
7. **A grammar licenses; it does not own.** A form may declare the `lang:GrammarRule` that licenses
   it, but form identity never depends on the grammar — grammars change, forms persist, and the
   licensing link is versioned through the `versions` slice like any evolving claim.

### Form identity and content addressing

Form identity follows the project's content-addressing discipline: a form's identity key is
computed over its structural content — sign system, stratum, features, and slot structure with
constituent keys — and **never** over any surface string, encoding, or rendering. Two consequences
are normative. First, re-encoding, re-normalizing, or re-transliterating a text creates new
surface forms but no new forms — the `lang:realizes` fan-out widens, the form stands. Second,
alpha-like invariances are explicit, not accidental: the identity key of a composed form includes
slot indexes and roles, so *word order and grammatical function are identity-bearing*, while
whitespace, casing conventions, and normalization live on the surface stratum and are not. The
Rust-side key computation and interning discipline are specified in
[`LANG-RUNTIME.md`](LANG-RUNTIME.md).

### A worked example — "cats chase mice"

The sentence as a composed form: three word forms over two lexemes and a verb, with UD-style slot
roles, one surface realization, and the sign system explicit throughout.

```ttl
ex:sentCatsChaseMice
    a lang:ComposedForm ;
    lang:inSignSystem lang:english ;
    lang:formHead ex:wfChase ;
    lang:formSlot ex:sent_s0 , ex:sent_s1 , ex:sent_s2 ;
    lang:realizes ex:sentSurface .   # inverse shown for readability; canonical direction is surface→form

ex:sent_s0 a lang:FormSlot ; lang:slotIndex 0 ; lang:slotForm ex:wfCats ; lang:slotRole lang:subjectRole .
ex:sent_s1 a lang:FormSlot ; lang:slotIndex 1 ; lang:slotForm ex:wfChase ; lang:slotRole lang:predicateRole .
ex:sent_s2 a lang:FormSlot ; lang:slotIndex 2 ; lang:slotForm ex:wfMice ; lang:slotRole lang:objectRole .

ex:wfCats
    a lang:WordForm ;
    lang:inSignSystem lang:english ;
    lang:inflectionOf ex:lexCat ;
    lang:morphFeature ex:featPlur .

ex:featPlur a lang:MorphFeature ; lang:featureKey "Number" ; lang:featureValue "Plur" .

ex:lexCat
    a lang:Lexeme ;
    lang:inSignSystem lang:english ;
    lang:partOfSpeech lang:noun .

ex:sentSurface
    a lang:SurfaceForm ;
    lang:surfaceText "cats chase mice" ;
    lang:inScript lang:latinScript ;
    lang:unicodeNormalization "NFC" .
```

Constituent order (`cats` before `chase` before `mice`) is explicit and inversion-safe: swapping
slot forms is a different sentence, and `lang:FormSlotIndexUniquenessConstraint` forbids two slots
sharing index `0`; SHACL is its generated validation view. The
byte string is one surface hung off the form; an NFD copy, a shouting-case copy, or a Braille
transcription would be further surfaces of the *same* composed form. What the sentence *means* —
its denotation into a `logic:` formula — attaches to the form, never to the surface, and is the
worked example of [`LANG-MEANING.md`](LANG-MEANING.md).

## Grafting the existing slices

The reference layer is the substrate the existing domain slices were migrated onto, per the
manifesto's grafting posture and the greenfield principle:

| Existing term | Relation to the grounding layer |
|---|---|
| `gmeow:Language`, `gmeow:FormalLanguage`, `gmeow:ProgrammingLanguage` | became individuals/kinds under `lang:SignSystem`; the class ladder collapsed into `lang:signSystemKind` individuals |
| `gmeow:WritingSystem` | grafted to `lang:Script` + `lang:Orthography` (the conflation of repertoire and convention is the removed inferiority) |
| `gmeow:TransliterationScheme` | became a rendering-layer map between orthographies ([`LANG-TRANSLATION.md`](LANG-TRANSLATION.md)) |
| `gmeow:bcp47Tag`, `gmeow:languageCode` | retained as projection/alignment surfaces of sign-system individuals |
| `gmeow:NotationSystem`, `gmeow:SymbolicSystem` (notation slice) | notational sign systems; their projection-profile machinery grafted under the rendering layer |

The migration is **executed** — with each grafted term passing the affected slices'
tests and the regenerated bundle, never a big-bang rename.
