<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Language — Conformance and the Gate Matrix

> The **conformance charter** of the GMEOW Language design set: it turns every "hard rule" stated
> in the sibling charters into a specific, traceable gate with a named failure class, and it fixes
> the preservation vocabulary the projection layer uses so semiotic loss is queryable alongside
> every other GMEOW lowering. It is the language peer of
> `slices/grounding/logic/design/LOGIC-CONFORMANCE.md` and
> `slices/grounding/math/design/MATHEMATICS-CONFORMANCE.md`. Where a sibling charter says
> "established by shapes, competency queries, and the loss ledger", this document says *by which
> shape, which query, which validator, and what failure is raised*.
>
> **Reading this charter.** The declarative present tense is normative: "X is enforced by G" means
> a conforming realization enforces X through gate G, and a violation raises the named failure
> class.

## The gate taxonomy

The taxonomy is inherited verbatim from the sibling conformance charters — every hard rule has
exactly one *primary* gate that owns the failure, of one of these kinds, ordered from cheapest and
most declarative to most procedural: **OWL axiom**, **SHACL Core**, **SHACL-SPARQL**,
**source-lint**, **Rust validator**, **competency query**, **projection test**.

Failure classes are IRIs in the `lang:` failure vocabulary (`lang:LangConformanceFailure`
subclasses), so a violation is itself a typed, queryable object, not a log line.

## The gate matrix

### Sign-system and form rules

| Rule | Primary gate | Failure class |
|---|---|---|
| A form that denotes, is grammar-governed, or is translated is structured, not string-only | source-lint + SHACL Core | `lang:StringOnlyMeaningBearingForm` |
| Every `lang:SurfaceForm` realizes a form or is typed `lang:UnanalyzedProse` (exactly one of the two) | SHACL-SPARQL | `lang:UndeclaredAnalysisStatus` |
| Every `lang:Form` names exactly one sign system | SHACL Core | `lang:FormWithoutSignSystem` |
| A sign system is an individual; a bare tag used as a system is ill-formed | source-lint | `lang:TagAsSignSystem` |
| Form-slot indexes are unique per composed form | SHACL-SPARQL | `lang:DuplicateFormSlotIndex` |
| Slot indexes are zero-based and contiguous (unconditional) | Rust validator | `lang:NonContiguousSlots` |
| Morphological content is typed feature pairs, never an unparsed feature string | SHACL Core | `lang:UntypedMorphology` |
| A hashing-facing surface form declares script, encoding, and normalization | SHACL Core | `lang:UnhashableSurface` |
| A `lang:WordForm` names its lexeme | SHACL Core | `lang:OrphanWordForm` |
| A grammar names the sign system it licenses; a rule names its grammar | SHACL Core | `lang:UnanchoredGrammar` |
| Form content keys are computed over structure only (no surface/encoding/rendering input) | Rust validator | `lang:SurfaceLeakInContentKey` |

### Meaning rules

| Rule | Primary gate | Failure class |
|---|---|---|
| Form, sense, and denotation-record kinds are disjoint | OWL axiom (disjointness) | `lang:FregeConflation` |
| A denotation names its form, target, kind, and context | SHACL Core | `lang:UnderspecifiedDenotation` |
| A denotation attaches to a form, never to a surface form | SHACL Core | `lang:SurfaceLevelDenotation` |
| `lang:viaSense` is present where the head lexeme has multiple recorded senses | SHACL-SPARQL | `lang:UnroutedAmbiguousDenotation` |
| A denotation kind matches its target's type (formula-kind → `logic:` formula, …) | SHACL-SPARQL | `lang:DenotationKindMismatch` |
| An interpretation *act* is a `gmeow:Activity`, never typed as an `Observation` | OWL axiom (disjointness) | `lang:ActObservationConflation` |
| A reading-correctness claim is an `Observation` with a vantage | SHACL Core | `lang:UngroundedReadingClaim` |
| Co-resident readings are never structurally collapsed by any stage | Rust validator + projection test | `lang:SilentDisambiguation` |
| An indexical denotation names its `lang:IndexicalAnchor` | SHACL-SPARQL | `lang:UnanchoredIndexical` |
| Reading preference weights are confidences unless a `math:` probability frame is declared | source-lint + SHACL-SPARQL | `lang:ConfidenceAsProbability` (reused discipline) |
| A compositional lowering into `logic:` declares per-stage preservation | Rust validator | `lang:UndeclaredLoweringStage` |

### Rendering, transliteration, translation, and paraphrase rules

| Rule | Primary gate | Failure class |
|---|---|---|
| A rendering names its content, its convention, and its preservation | SHACL Core | `lang:UnderspecifiedRendering` |
| A rendering never substitutes for its content's identity | OWL axiom + source-lint | `lang:RenderingAsIdentity` |
| A translation unit names source form, target form, and preservation | SHACL Core | `lang:UnderspecifiedTranslationUnit` |
| String-pair units without form analysis are typed unanalyzed with the weakest judgment | SHACL-SPARQL | `lang:UnmarkedSurfaceTranslation` |
| A translation names direction and method (with activity provenance for machine output) | SHACL Core | `lang:UnattributedTranslation` |
| Document-level judgments are computed from unit judgments, never asserted over them | Rust validator | `lang:AssertedRollup` |
| An untranslatable unit declares its gap/residue rather than being dropped or padded | SHACL-SPARQL + projection test | `lang:FabricatedEquivalence` |
| A paraphrase declares its sameness kind (denotation/sense/force+content) | SHACL Core | `lang:UndeclaredParaphraseKind` |
| A transliteration names source and target orthographies and its scheme | SHACL Core | `lang:UnanchoredTransliteration` |

### Projection rules

| Rule | Primary gate | Failure class |
|---|---|---|
| Every projection declares its unsupported constructs | projection test | `lang:UndeclaredUnsupportedConstruct` |
| Every projection declares a `logic:preservationKind` | projection test | `lang:MissingPreservationKind` |
| CoNLL-U emission is per reading; no silently chosen winner | projection test | `lang:ProjectionSilentDisambiguation` |
| Lemon emission enumerates dropped epistemic structure | projection test | `lang:UnrecordedEpistemicLoss` |
| A declared-exact projection round-trips (section/retraction) on the corpus — EBNF/ABNF, GTS/Turtle grammar surfaces | projection test | `lang:ExactPreservationViolated` |
| No hand edit to a generated surface | drift gate (existing) | repository-standard drift failure |

### Ingestion rules

| Rule | Primary gate | Failure class |
|---|---|---|
| An ingester lifts fully or hard-fails with a typed diagnostic naming the construct | Rust validator | `lang:SilentIngestDrop` |
| Engine output enters as vantage-held readings, never unattributed structure | SHACL-SPARQL | `lang:UnattributedEngineClaim` |
| Promotion from engine reading to slice assertion is an explicit provenance-carrying act | SHACL-SPARQL | `lang:SilentPromotion` |
| Document-scale surfaces hold blob references, never inline payload bytes | Rust validator | `lang:InlineBlobPayload` |

### GMN dialect rules

The hard rules of the GMN dialect charter ([`LANG-GMN.md`](LANG-GMN.md)). SHACL-tier rows are
enforced by the `lang:Gmn*Shape` gates in `shapes.ttl` (each naming its class through
`gmeow:enforcesFailureClass`) and cite their `tests/counter-examples/` fixture; Rust-validator rows
are the GMN parser/writer's table-driven tier and cite the charter normative-example block their
machine fixtures are seeded from.

| Rule | Primary gate | Failure class |
|---|---|---|
| Every glyph of the GMN script carries a canonical "U+XXXX"-style codepoint spelling | SHACL Core + SHACL-SPARQL (`tests/counter-examples/gmn-noncanonical-codepoint.ttl`) | `lang:GmnNonCanonicalCodepoint` |
| No co-resident confusable pair in the glyph inventory (UTS #39 skeleton rule) | SHACL-SPARQL (`tests/counter-examples/gmn-confusable-glyph.ttl`) | `lang:GmnConfusableGlyph` |
| One codepoint sequence, one glyph identity per script, unless both readings are sigil-scoped | SHACL-SPARQL (`tests/counter-examples/gmn-glyph-collision.ttl`) | `lang:GmnGlyphCollision` |
| Every envelope carries its full eight-field contract | SHACL Core (`tests/counter-examples/gmn-envelope-missing-field.ttl`) | `lang:GmnMissingEnvelopeField` |
| Every `gmeow:GmnSecurityRing` carries exactly one `gmeow:gmnRingLevel` and only declared `gmeow:GmnCompartment` values (the authored coordinates the derived `gmeow:gmnRingWithin` lattice reads from) | SHACL Core (`tests/counter-examples/gmn-ring-lattice-malformed.ttl`) | `lang:GmnRingLatticeMalformed` |
| No mnemomorphic migration over a stronger-than-additive bump; no accept window beyond 1 | SHACL-SPARQL (`tests/counter-examples/gmn-version-overclaim.ttl`) | `lang:GmnVersionOverclaim` |
| Every compaction names its sources and its holding vantage | SHACL Core (`tests/counter-examples/gmn-compaction-without-provenance.ttl`) | `lang:GmnCompactionWithoutProvenance` |
| No compaction correspondence stronger than `ValidationOnly` | SHACL-SPARQL (`tests/counter-examples/gmn-compaction-overclaim.ttl`) | `lang:GmnCompactionOverclaim` |
| Every document token resolves through the pinned dictionary or a named-key ruling | Rust validator (`LANG-GMN.md`, the invalid-uncovered-term block) | `lang:GmnUncoveredTerm` |
| Records in content-sorted order; keys in generation order (`s p o v q st ev`) | Rust validator (`LANG-GMN.md`, the invalid-key-order block) | `lang:GmnNonCanonicalOrder` |
| Confidences at two fractional digits; no scientific notation; one spelling per value | Rust validator (`LANG-GMN.md`, the invalid-number block) | `lang:GmnMalformedNumber` |
| No record before the `@gmn` header pins the dialect coordinates | Rust validator (`LANG-GMN.md`, the invalid-missing-header block) | `lang:GmnUndeclaredDialectVersion` |
| The declared `LL(1)` determinism class survives parse-table construction | Rust validator (the `grammars/gmn.ebnf` exact round-trip lift) | `lang:GmnNonDecodableGrammar` |
| A compaction run never silently collapses co-resident readings (`gmeow:GmnCompaction` inputs included) | Rust validator (`tests/counter-examples/gmn-compaction-silent-disambiguation.ttl`, native lint) | `lang:SilentDisambiguation` (reused discipline) |
| Every emitted morphological feature value is dispositioned — glyphed, alias-planed, or named-key-ruled — with no silent gap | SHACL-SPARQL (`tests/counter-examples/gmn-undispositioned-feature-value.ttl`) | `lang:GmnUndispositionedTerm` |
| Every imported alias plane carries both its citation and its version | SHACL Core (`tests/counter-examples/gmn-plane-missing-version.ttl`) | `lang:GmnUnattributedPlane` |
| Every GMN-script glyph carries its measured per-glyph token-cost feed — no silent gap in the glyph plane | SHACL-SPARQL (`tests/counter-examples/gmn-uncosted-script-glyph.ttl`) | `lang:GmnUncostedScriptGlyph` |
| The `@λ` lang-AST tabular batch reuses the CoNLL-U column order verbatim (`ID FORM LEMMA UPOS XPOS FEATS HEAD DEPREL DEPS MISC`), never a rival scheme | Projection test (`GMN_LANG_AST_COLUMNS` pinned to the `ConlluToken` serializer order) | — (drift is a build failure) |

The coverage row is the graph-side reading of the charter's coverage gate: `lang:GmnUncoveredTerm`
is the writer-tier failure of a document token the pinned dictionary cannot resolve at emit time,
while `lang:GmnUndispositionedTerm` is the authoring-time completeness a SHACL shape sees in the
graph — every feature value the surface emits is bound to a disposition, the denominator derived by
type over the `lang:FeatureValue` population rather than a hand-listed set, so a value added without
a disposition reds the gate. The coverage report is green exactly when that gate finds nothing. The
glyph-plane sibling `lang:GmnUncostedScriptGlyph` closes the same silent gap over the glyph
inventory: every `lang:Grapheme` in `gmeow:gmnScript` — the IPA graphemes and the `*` operator glyph
— must carry its measured `gmeow:gmnGlyphTokenCost`, the denominator derived by the script
repertoire, so a glyph admitted without its cost feed reds `lang:GmnScriptGlyphCoverageShape`.

## Preservation vocabulary — reuse, do not re-mint

Projection, rendering, and translation preservation use the **existing** `logic:` loss-ledger
vocabulary verbatim, so semiotic loss — including human-language translation loss — is queryable
in the same ledger as OWL, Datalog, SHACL, correspondence, and `math:` lowerings. The language
slice mints **no** near-synonyms; where translation practice wants finer distinctions (sense
preserved but register lost), the distinction is carried as enumerated residue on the unit, not as
a new preservation kind.

## The fixture corpus

Conformance is demonstrated on a positive/negative fixture corpus in the established
bless-discipline (`GMEOW_CONFORMANCE_BLESS` + `_BLESS_INIT` for new cases), sized to the flagship
scenarios of the manifesto:

- **Positive**: the worked examples of each charter (the *cats chase mice* form and its lowering;
  the *saw her duck* co-resident readings; a French/English docs-tree translation pair with unit
  judgments; the Turtle grammar round-trip; a code-switched composed form; a transliteration
  pair with declared loss).
- **Negative**: one fixture per failure class above, each triggering exactly its named class —
  the same one-rule-one-failure discipline the sibling charters enforce.
- **Budget**: fixtures respect the per-test budget; corpus-scale sweeps (full treebank lifts,
  whole-docs-tree typing) live in off-gate `maint-` lanes.
