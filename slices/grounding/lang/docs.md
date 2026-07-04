<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Language — the `lang:` grounding layer

> Slice `<https://blackcatinformatics.ca/gmeow/slices/lang>` · tier **core** · namespace
> `lang:` = `https://blackcatinformatics.ca/lang/`. The third co-foundational grounding layer
> (Principle 19), peer to `logic:` (reasoning) and `math:` (quantity and structure). It grounds
> **meaning and expression** — here, specifically, *what language is made of* before any meaning is
> attached.

The form core makes two claims precise. First, **a language is an individual, never a tag**: a sign
system (English, Turtle, IPA, Braille) is a first-class `lang:SignSystem` distinguished by a
`lang:SignSystemKind` and a `lang:Modality`, with BCP-47 tags, ISO codes, and Wikidata QIDs recorded
as *alignments* rather than as identity. Second, **a string is a surface; the form is a tree**: a
form that carries inferential weight is a structured `lang:Form`, and its identity is computed over
structural content alone — sign system, stratum, typed morphology, and slot structure with
constituent keys — and is independent of encoding, script, casing, and rendering.

## The form AST

The layer is stratified the way a century of linguistics stratifies it, each stratum with a distinct
identity criterion:

- **Surface** — `lang:SurfaceForm` (text with declared script, encoding, Unicode normalization, and
  collation locale; optionally a stand-off `lang:SurfaceAnchor`). The only stratum where byte
  identity means anything.
- **Sub-morph** — `lang:Grapheme` (the members of a `lang:Script`'s repertoire) and `lang:Phone`.
- **Morphology** — `lang:Morpheme` (abstract) versus `lang:Morph` (its realization, with
  `lang:allomorphOf`), carrying typed `lang:MorphFeature` pairs drawn from a `lang:FeatureInventory`.
- **Words** — `lang:Lexeme` (the dictionary word) versus `lang:WordForm` (its inflection); the
  CoNLL-U token/word split as `lang:OrthographicWord` (spanning) versus `lang:SyntacticWord`.
- **Composition** — `lang:ComposedForm` over indexed `lang:FormSlot`s, typed by
  `lang:CompositionLevel`, with co-resident constituency (slot roles) and dependency edges
  (`lang:dependencyRelation`, `lang:dependsOn`), `lang:formHead`, and `lang:CovertForm` for
  structurally-present, surface-absent constituents.

Every surface declares how far it has been analyzed through a graded `lang:AnalysisLevel`
(raw → segmented → tokenized → morph-analyzed → parsed); `lang:UnanalyzedProse` is the honest `raw`
status. Competing analyses of one surface are held co-resident and non-collapsing through
`lang:Analysis`.

## The design set

| Charter | Contents |
|---|---|
| [`LANG.md`](design/LANG.md) | manifesto — the third grounding layer, doctrine, and lineage |
| [`LANG-FORMS.md`](design/LANG-FORMS.md) | **this slice** — the sign-system reference layer and the typed form AST |
| [`LANG-MEANING.md`](design/LANG-MEANING.md) | sense, reference, the denotation bridge into `logic:` |
| [`LANG-TRANSLATION.md`](design/LANG-TRANSLATION.md) | rendering, translation, and paraphrase |
| [`LANG-PROJECTIONS.md`](design/LANG-PROJECTIONS.md) | the generated lossy lowerings + loss ledger |
| [`LANG-RUNTIME.md`](design/LANG-RUNTIME.md) | ingestion, content-addressed interning, engine handoff |
| [`LANG-CONFORMANCE.md`](design/LANG-CONFORMANCE.md) | the gate matrix and failure classes |
| [`LANG-REFERENCES.md`](design/LANG-REFERENCES.md) | the classified external survey |

## Hard rules → gates → failure classes

Every hard rule of the form AST maps to a SHACL gate (in `shapes.ttl`) that points at a typed
`lang:LangConformanceFailure` subclass through `lang:enforcesFailureClass`, so a violation is a
queryable object rather than a log line.

| Rule | Gate | Failure class |
|---|---|---|
| Every form names exactly one sign system | `lang:FormSituatedShape` | `lang:UnsituatedForm` |
| Analyzed or explicitly unanalyzed; structure matches level | `lang:SurfaceAnalysisShape` | `lang:MisdeclaredAnalysis` |
| Slot indexes unique per composed form | `lang:SlotIndexUniqueShape` | `lang:DuplicateSlotIndex` |
| Slot indexes zero-based and contiguous | `lang:SlotContiguityShape` | `lang:NonContiguousSlots` |
| Dependency/token edges stay in their analysis, acyclic | `lang:DependencyIntegrityShape` | `lang:DanglingDependency` |
| Morphology typed from the feature inventory | `lang:TypedMorphologyShape` | `lang:UntypedMorphology` |
| Surfaces declare their material identity | `lang:SurfaceMaterialShape` | `lang:UnhashableSurface` |
| Anchors declare source, span, and offset space | `lang:AnchorCompleteShape` | `lang:UnanchoredOffset` |
| Structural positions hold forms, not literals | `lang:StructuralFormShape` | `lang:StringOnlyForm` |
| Graphemes grounded in their script's repertoire | `lang:GraphemeGroundedShape` | `lang:UngroundedGrapheme` |

## Competency

The slice's competency questions (in `tests/competency.ttl`, backed by `queries/competency/*.rq`)
demonstrate that the layer answers real questions: which surfaces are explicitly unanalyzed, the
constituents of a composed form in index order, a word form's lexeme and morph features, sign systems
by kind, and the surfaces that realize one form as its encoding fan-out widens.
