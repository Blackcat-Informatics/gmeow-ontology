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

| Document | Genre | Realized state | Contents |
|---|---|---|---|
| [`LANG.md`](design/LANG.md) | manifesto | realized | the third grounding layer, doctrine, and lineage |
| [`LANG-FORMS.md`](design/LANG-FORMS.md) | charter | realized (**this slice** — canonical `module.ttl` + projected validation surfaces) | the sign-system reference layer and the typed form AST |
| [`LANG-MEANING.md`](design/LANG-MEANING.md) | charter | realized | sense, reference, the denotation bridge into `logic:` |
| [`LANG-TRANSLATION.md`](design/LANG-TRANSLATION.md) | charter | realized (`lang:Rendering` plus canonical closure/constraint authorities) | rendering, translation, and paraphrase |
| [`LANG-GMN.md`](design/LANG-GMN.md) | charter | realized — graph-derived glyph/dictionary registries, a candidate audit closed over every executable target, scoped reader/writer resolution, Unicode security, measured cost, a single sentinel-to-generated grammar seam, and round-trip gates are executable | GMN — the token-compact model-notation dialect ladder and its contracts |
| [`LANG-PROJECTIONS.md`](design/LANG-PROJECTIONS.md) | contract | realized (`crates/lang-bridge/src/{ontolex,nif,conllu}.rs`, loss-ledger counter-examples) | the generated lossy lowerings + loss ledger |
| [`LANG-RUNTIME.md`](design/LANG-RUNTIME.md) | runtime | realized (`crates/lang-form/src/intern.rs`, `crates/lang-bridge/src/{conllu,plain_text,engine}.rs`) | ingestion, content-addressed interning, engine handoff |
| [`LANG-CONFORMANCE.md`](design/LANG-CONFORMANCE.md) | contract | realized (41 conformance fixtures, 56 counter-examples) | the gate matrix and failure classes |
| [`LANG-REFERENCES.md`](design/LANG-REFERENCES.md) | appendix | realized (`mappings/equivalences.ttl`) | the classified external survey |

## Hard rules → gates → failure classes

Every hard rule of the form AST has one canonical OWL/`logic:` authority in `module.ttl` and maps
to a typed `lang:LangConformanceFailure` subclass through `gmeow:enforcesFailureClass`, so a
violation is a queryable object rather than a log line. SHACL is a generated validation view. The
small authored residue retained during equivalence-before-deletion carries resolvable
`logic:formalizes` links back to those authorities; it is not a second source of truth.

| Rule | Gate | Failure class |
|---|---|---|
| Every form names exactly one sign system | `lang:formInSignSystemClosure` + qualified maximum | `lang:UnsituatedForm` |
| Analyzed or explicitly unanalyzed; structure matches level | `lang:surfaceAnalysisLevelClosure` + `lang:SurfaceAnalysisStructureConstraint` | `lang:MisdeclaredAnalysis` |
| Slot indexes unique per composed form | `lang:FormSlotIndexUniquenessConstraint` | `lang:DuplicateSlotIndex` |
| Slot indexes zero-based and contiguous | `lang:SlotContiguityConstraint` + native validator | `lang:NonContiguousSlots` |
| Dependency/token edges stay in their analysis, acyclic | `lang:DependencyLeavesAnalysisConstraint` + `lang:FormSlotAcyclicDependencyConstraint` | `lang:DanglingDependency` |
| Morphology typed from the feature inventory | class-scoped feature-key/value closure + OWL value typing | `lang:UntypedMorphology` |
| Surfaces declare their material identity | `lang:SurfaceMaterialChoiceConstraint` + class-scoped field closure | `lang:UnhashableSurface` |
| Anchors declare source, span, and offset space | class-scoped anchor closure + OWL value typing | `lang:UnanchoredOffset` |
| Structural positions hold forms, not literals | `lang:SlotFormNodeKindConstraint`, `lang:RealizesNodeKindConstraint`, `lang:FormHeadNodeKindConstraint` | `lang:StringOnlyForm` |
| Graphemes grounded in their script's repertoire | `lang:GraphemeRepertoireConstraint` + `lang:graphemeScriptClosure` | `lang:UngroundedGrapheme` |

## The meaning stratum

Over the form AST sits the semantic layer specified in [`LANG-MEANING.md`](design/LANG-MEANING.md):
the **Frege triangle**, the **reified denotation record**, the **one-way bridge into `logic:`**, and
**interpretation as a vantage-held act**.

- **Form ≠ sense ≠ reference.** `lang:Form`, `lang:Sense`, and `lang:Denotation` are disjoint kinds
  (the layer's signature hard rule): string identity never implies form identity, form identity never
  implies sense identity, and sense identity never implies co-reference. A `lang:Sense` attaches to a
  lexeme or form through `lang:senseOf` and evokes a `lang:LexicalConcept` (a synset) through
  `lang:evokes`, so synonymy is derived, not asserted flat. Sense-to-sense relations (hypernymy,
  meronymy) are **reasoned in `logic:`** over the `logic:Type` a sense denotes — GMEOW's own engine,
  a single source of truth — never a second is-a graph forked inside `lang:`. Where a taxonomic
  relation is worth recording explicitly, `lang:SenseRelation` (its kind named by
  `lang:senseRelationKind`, only `lang:hypernymy`/`lang:hyponymy` minted) reifies it as
  **correspondence-only provenance** — sourced and vantage-holdable — while the subsumption itself
  still recovers as `logic:` over the denoted `logic:Type`; no gate, query, or reasoner reads
  `lang:senseRelationKind` as the subsumption source.
- **The denotation record.** A `lang:Denotation` is a reified record (the Peircean triad made
  structural), never a bare edge: it names its form (`lang:denotedForm`, above the byte level), its
  kind (`lang:denotationKind`), its target (`lang:denotationTarget`), and its context
  (`lang:denotationContext`), routing through `lang:viaSense` where the head lexeme is ambiguous.
- **The one-way bridge into `logic:`.** A declarative sentence denotes a `logic:Formula`
  (`lang:denotesLogicFormula`), a referring expression a `logic:Individual` (`lang:denotesLogicTerm`),
  a common noun or predicate a `logic:Type` (`lang:denotesLogicType`), and an interrogative's content
  a query (`lang:denotesQuery`). Lowering is compositional where analysis reaches — a composed
  denotation names its `lang:CompositionRule` (`lang:composedBy`) and its constituent denotations
  (`lang:composedFrom`), each carrying a `logic:preservationKind`. The bridge runs `lang:` → `logic:`
  and never reverses (Principle 19 acyclic grounding).
- **Interpretation, ambiguity, deixis, force.** A `lang:InterpretationAct` (a `gmeow:Activity`, never
  an observation) produces `lang:Reading` results; the claim that a reading is correct is a
  `gmeow:Observation` held from a `gmeow:vantage` with `logic:confidence` (never a probability absent a
  declared frame). Ambiguity is co-resident readings — *I saw her duck* keeps both, and *every man
  loves a woman* keeps both scopings as distinct `logic:Formula`s. Deixis is anchored, not resolved
  away (`lang:IndexicalAnchor` + `lang:anchorKind`); communicative force (`lang:CommunicativeForce`) is
  not content, and only assertive force lowers to an asserted formula. Because the whole spine is
  reified, GMEOW reasons *about* readings and claims to arbitrary order.

| Rule | Gate | Failure class |
|---|---|---|
| Form, sense, and denotation kinds are disjoint | `lang:FregeDisjointnessAxiom` | `lang:FregeConflation` |
| A denotation names its form, kind, target, and context | class-scoped denotation closure + OWL value typing | `lang:UnderspecifiedDenotation` |
| A denotation attaches to a form, never a surface | `lang:DenotationNonSurfaceConstraint` | `lang:SurfaceLevelDenotation` |
| Ambiguous denotations route through a sense | `lang:AmbiguousDenotationRoutingConstraint` | `lang:UnroutedAmbiguousDenotation` |
| A denotation kind matches its target's type | `lang:DenotationKindMatchConstraint` | `lang:DenotationKindMismatch` |
| An interpretation act is never an observation | `lang:ActObservationDisjointnessAxiom` | `lang:ActObservationConflation` |
| A reading-correctness claim has a vantage | `lang:ReadingClaimVantageConstraint` | `lang:UngroundedReadingClaim` |
| Co-resident readings are never silently collapsed | native Rust validator | `lang:SilentDisambiguation` |
| An indexical denotation names its anchor | `lang:IndexicalAnchoredConstraint` | `lang:UnanchoredIndexical` |
| Preference weights are confidences, not probabilities | `lang:ConfidenceModelConstraint` | `lang:ConfidenceAsProbability` |
| A compositional lowering declares its preservation | native Rust validator | `lang:UndeclaredLoweringStage` |

Two rows above are enforced directly by a **native Rust validator** rather than by the generated
SHACL view:
`lang:SilentDisambiguation` and `lang:UndeclaredLoweringStage`. Both require reasoning SHACL
cannot express cleanly — the first is a whole-dataset check that a resolved reading is backed
somewhere by a vantage-held observation, the second a kind-derived stage-coverage check over the
compositional program — so they run bundle-wide inside `structural_lint_dataset`
(`crates/validate/src/lint.rs`) alongside the one-way-bridge acyclicity check. Their negative
fixtures accordingly live inline in that crate's Rust tests, not as `example-conformance.ttl`
cells; the projected validation rules keep their fixture cells. The split is intentional: each
gate lives where the invariant it enforces can actually be stated.

## Competency

The slice's competency questions (in `tests/competency.ttl`, backed by `queries/competency/*.rq`)
demonstrate that the layer answers real questions: which surfaces are explicitly unanalyzed, the
constituents of a composed form in index order, a word form's lexeme and morph features, sign systems
by kind, and the surfaces that realize one form as its encoding fan-out widens.
