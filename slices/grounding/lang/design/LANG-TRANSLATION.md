<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Language — Rendering, Translation, and Paraphrase

> The **rendering-and-translation charter** of the GMEOW Language design set: the reified
> content→form rendering relation, transliteration, paraphrase, and translation as audited
> correspondence between sign systems. This is the charter that owns the relation both sibling
> grounding layers use without grounding — `math:`'s "MathML is a rendering" and `logic:`'s
> parse/emit dialect surfaces — and the one that turns GMEOW's multilingual documentation trees
> from a pipeline artifact into a queryable translation corpus. Forms are defined in
> [`LANG-FORMS.md`](LANG-FORMS.md); the meanings translation must preserve are in
> [`LANG-MEANING.md`](LANG-MEANING.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by canonical OWL/`logic:` authorities in `module.ttl`,
> their generated validation projections, competency queries, and the projection loss ledger —
> not a claim that any implementation already realizes X except as those gates demonstrate.

## Purpose

This layer names the **maps** of the semiotic substrate: content into form (rendering), form into
form across orthographies (transliteration), form into form across sign systems (translation), and
form into form within one system (paraphrase). One commitment governs all four:

> **Every map declares what it preserves.** A rendering, transliteration, translation, or
> paraphrase without a preservation judgment is ill-formed — the loss ledger discipline, applied
> to the maps language is made of.

## Rendering — the content→form relation, grounded once

Core classes: `lang:Rendering`, `lang:RenderingKind`.
Core properties: `lang:renderedContent`, `lang:renderingForm`, `lang:renderingKind`,
`lang:renderingConvention`, `lang:renderingPreservation`.

A `lang:Rendering` is a reified record connecting a **content object** — a `logic:` formula, a
`math:` expression, a GMEOW term, a document node, a `lang:Form` — to a **form** that presents it,
under a named convention (a notation system, a serialization grammar, a documentation template).
Rendering is the general theory behind several relations the repository already uses piecemeal:

| Existing surface | As a grounded rendering |
|---|---|
| `math:ExpressionRendering` / `math:rendersAs` | a `lang:Rendering` whose content is a `math:` expression and whose convention is a mathematical notation system |
| the `notation` slice's projection profiles (`gmeow:ProjectionFunction`, `gmeow:ProjectionLoss`) | rendering conventions with their declared loss, grafted under `lang:renderingPreservation` |
| Turtle/GTS emission of any graph | a rendering whose convention is a formal grammar ([`LANG-FORMS.md`](LANG-FORMS.md)); parsing is the same record read backwards ([`LANG-RUNTIME.md`](LANG-RUNTIME.md)) |
| docs-tree page generation | renderings of term content under a documentation convention, per language tree |

The grafting direction is clean and the migration is **executed**: the mathematics slice declares
`lang` in its dependency set and `math:ExpressionRendering` is realized as a subclass of
`lang:Rendering` — the rendering seam registered in the grounding contract
([`docs/GROUNDING.md`](../../../../docs/GROUNDING.md)) — with `lang:renderedContent`,
`lang:renderingConvention`, and `lang:renderingPreservation` read directly off the math record; no
duplicated display vocabulary survives, per the greenfield principle.

The hard rules:

1. **A rendering names its content.** A form floating free of what it renders is just a form; a
   rendering record without `lang:renderedContent` is ill-formed. Identity never transfers: the
   rendering is evidence about the content, not the content.
2. **A rendering names its convention.** "Rendered as MathML," "rendered as Turtle under the
   GMEOW prefix map," "rendered as the French docs template" — the convention individual is what
   makes two renderings comparable and re-runnable.
3. **A rendering declares preservation.** `lang:renderingPreservation` carries a `logic:`
   preservation judgment: an emitted Turtle document is `ExactPreservation` of its graph; a prose
   gloss of an axiom is not, and says so.

## Transliteration — surface maps between orthographies

Transliteration maps **surface forms** across orthographies of one sign system (or between
scripts) without touching form identity — the `LANG-FORMS.md` identity rule that re-transliterating
widens the `lang:realizes` fan-out but creates no new form. A `lang:TransliterationMap` names its
source and target orthographies and its scheme — grafting the existing
`gmeow:TransliterationScheme` — and each application is recordable as a rendering whose kind is
transliteration. Round-trippable schemes (ISO 9) declare `ExactPreservation`; lossy ones (most
romanizations) declare their loss like every other map in the project.

## Translation — correspondence between sign systems

Core classes: `lang:Translation`, `lang:TranslationUnit`.
Core properties: `lang:translationSource`, `lang:translationTarget`, `lang:translationOf` (the
unit-level pairing), `lang:translationPreservation`, `lang:translationMethod`.

A translation is **a correspondence, not an equivalence**. The design lowers `lang:Translation`
onto the existing correspondence calculus — a `logic:Correspondence` between form structures in
two sign systems — so translation inherits, rather than re-invents, the machinery the project
already built for cross-ontology alignment: directional maps, composition, preservation
judgments, and the section/retraction laws that make "faithful" checkable. The parallel is exact,
and it is the charter's central insight made operational:

> Translating French↔English **is** ontology alignment, performed on sign systems instead of
> vocabularies. Both are structure-preserving-up-to-declared-loss maps; both need the loss
> recorded; both compose; and GMEOW already has the calculus.

The hard rules:

1. **Translation relates forms, through meaning.** A `lang:TranslationUnit` pairs a source form
   with a target form; what the pairing is supposed to preserve is *sense and denotation*
   ([`LANG-MEANING.md`](LANG-MEANING.md)), and the preservation judgment is about exactly that.
   String-level alignment with no form analysis is lawful only as an *unanalyzed* translation
   unit — recorded status, weakest judgment, never a silent default.
2. **Translation declares direction and method.** Human, machine (which engine, which run —
   provenance through `gmeow:Activity`), or compositional-through-the-abstract-tree (the
   Grammatical Framework mode, where both surfaces are linearizations of one analyzed form and
   the judgment can be strong by construction).
3. **Preservation is judged per unit and rolled up.** A document-level translation aggregates
   unit judgments; the roll-up is computed, never asserted over the units' heads — the
   count-the-real-artifacts discipline.
4. **Untranslatability is data.** A source unit with no adequate target — culture-bound senses,
   grammaticalized categories the target lacks (evidentiality, clusivity) — is held as a unit
   with a declared gap and residue, exactly as the correspondence calculus holds partial
   alignments. Dropping the unit or padding a gloss without marking it fabricates equivalence.

### The first live corpus — GMEOW's own docs trees

The generated multilingual documentation trees are the layer's first corpus and flagship 3 of the
manifesto: every non-English docs page is a rendering of term content *and* stands in translation
units with its English peer, so "what does the French tree lose against the English canon?"
becomes a competency query over translation preservation records instead of a diff nobody reads.
This is dogfooding with teeth — the docs pipeline already produces the pairs; the slice gives the
pairs their honest type.

### The GMN dialect crossings ride the same rail

The GMN dialect ladder ([`LANG-GMN.md`](LANG-GMN.md)) is this layer's formal-language corpus: each
level crossing — GMN-0 to its GTS and N-Quads encodings, GMN-0 to the GMN-1 model notation, GMN-1
to the GMN-2 compacted variety — is a `lang:TranslationUnit` carrying its law-spine on exactly one
`logic:Correspondence` through `lang:translationCorrespondence`, and each inter-version migration
(`gmeow:gmnMigratesFrom` / `gmeow:gmnMigratesTo`) is a judged crossing on the identical rail. The
verbalizer is a `lang:translationCorrespondence` between `gmeow:gmnModelNotation` and controlled
natural language, carried on the same rail — authored by the GMN projection sibling under this
charter's rules, no special case anywhere.

## Paraphrase — same system, declared sameness

A `lang:Paraphrase` pairs forms within one sign system under a declared sameness claim: its
`lang:paraphraseOf` role names the source and its `lang:paraphraseForm` role names the produced
restatement, while `logic:mediates` makes both ends explicit in the foundation calculus. The
declared sameness is same denotation (strong), same sense (stronger), or same communicative force
and content (strongest).
Paraphrase is what label alternatives, definition rewrites, and plain-language summaries actually
are, and the declared-sameness kind is what keeps a summary from silently claiming to be the
definition. A paraphrase claim is vantage-held like any observation — two editors may disagree
about whether a rewrite preserved the sense, and the model holds both.

## What this layer refuses to model

- **No translation-quality scores as bare numbers.** A quality judgment is a vantage-held
  observation over a translation unit with method and scale — the `math:`/`observations`
  discipline — never an unframed float on the unit.
- **No canonical interlingua.** The abstract-form mode is available where analysis reaches; the
  layer never *requires* a universal meaning representation as a precondition for holding a
  translation, because most real translations arrive as surface pairs and their epistemic shape
  must be preserved as such.
- **No silent normalization of variants.** Locale variants (en-US/en-GB) are varieties with
  renderings, not noise to collapse; collapsing is a declared map like any other.
