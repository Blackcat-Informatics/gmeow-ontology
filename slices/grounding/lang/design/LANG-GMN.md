<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Language — GMN, the Model Notation Dialect

> The **dialect charter** of the GMEOW Language design set: GMN — the GMEOW Model Notation — as an
> engineered, token-compact dialect ladder over the one canonical model, specified with the same
> rigor the set applies to every other sign system. The sign-system machinery GMN instantiates is
> in [`LANG-FORMS.md`](LANG-FORMS.md); the level crossings it is judged by are
> [`LANG-TRANSLATION.md`](LANG-TRANSLATION.md) correspondences; its emission seam is
> [`LANG-PROJECTIONS.md`](LANG-PROJECTIONS.md); its ingestion lane is
> [`LANG-RUNTIME.md`](LANG-RUNTIME.md); its gates are rows of
> [`LANG-CONFORMANCE.md`](LANG-CONFORMANCE.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's `shapes.ttl`, competency queries, the GMN
> parser/writer's validator tier, and the projection loss ledger — not a claim that any
> implementation already realizes X except as those gates demonstrate.

## Purpose

GMN exists because the highest-traffic consumer of the canonical model is now a language model
reading and writing over a metered token channel. GMN treats that channel the way the project
treats every other surface: as a **judged projection** of the canon — engineered for token
economy, specified by a first-class grammar object, versioned relationally, and honest about every
bit it drops.

## The dialect ladder

A **dialect** is an abstraction level with a preservation contract; a **codec** is a medium. The
ladder has three levels, and only the crossings between levels carry judgments:

- **GMN-0** (`gmeow:gmnNormalForm`) **is the existing narrow-waist normal form** — the RDFC-1.0
  canonically blank-node-labeled, content-sorted term-table normal form of the bundle
  (`docs/gts-narrow-waist.md`). It mints **no** new canonical object. GTS (`lang:gts`) and
  canonical N-Quads (`lang:nquads`) are correspondence-carrying **encodings** of this one normal
  form, never rival canons: each crossing (`gmeow:gmnCorrNormalToGts`,
  `gmeow:gmnCorrNormalToNQuads`) is an isomorphism with `logic:mnemomorphic true`, and the claim
  is discharged by the executed byte-reconstruction gates — the narrow-waist superset gate and the
  RDFC-1.0 canonicalization round-trip — not declared on faith.
- **GMN-1** (`gmeow:gmnModelNotation`) is the token-compact model surface: a **section/retraction**
  over GMN-0 with exact preservation and a retained mnemomorphic witness
  (`gmeow:gmnCorrNormalToGmn` — `logic:ExactPreservation`, `logic:SectionRetraction`,
  `logic:mnemomorphic true`) — `put ∘ get = id_S` on the normal form, one rung below
  `logic:Isomorphism` because GMN-1 need not recover the exact GMN-0 *byte* serialization, only
  the underlying model. The prior drop list is **eliminated, not narrowed**: full IRIs invert
  through the codebook's version-pinned alias bijection (`gmeow:GmnDictionary` /
  `gmeow:gmnDictionaryVersion`, gated for injectivity); confidence and every annotation ride **by
  reference** (an alias or record identifier, never inlined digits or prose — see the rate–fidelity
  contract below); the only non-image is an uncovered term (`lang:GmnUncoveredTerm`), a hard fail,
  never a silent drop. The declared domain is the ENTIRE GMN-0 normal form, realized as a
  convergent coverage contract: the codec + round-trip gate are total over the grounding slices'
  GMN-0 (logic, lang, math) now, and coverage of every other slice's GMN-0 is tracked by the
  GMN-1-coverage slice-quality axis at a committed, monotonically non-regressing floor — an
  uncovered non-grounding term is a measured quality deficit, never an assumed drop. The claim is
  discharged by the executed GMN-1 round-trip gate, exactly as `gmeow:gmnCorrNormalToGts` is
  discharged by the narrow-waist superset gate.
- **GMN-2** (`gmeow:gmnCompacted`) is lossy cognitive compaction — a `lang:LanguageVariety` of
  GMN-1, not a third sign system. A compaction is a **new provenance-linked claim about older
  claims** (`gmeow:GmnCompaction`), and its crossing (`gmeow:gmnCorrGmnToCompacted`) is
  get-leg-only: `logic:ValidationOnly`, `logic:BridgeView`, no round-trip obligation. The razor:
  a zstd-compressed claim is the same claim (a codec); a compaction is not (a dialect level).

## The rate–fidelity contract

GMN is a **source code over the LLM token channel**, and it is specified as coding theory
specifies a code — codebook, rate, fidelity:

- **Codebook** — `gmeow:GmnCodebook` (`gmeow:gmnCodebookCurrent`): the GMN script plus the ten
  sigil roles (linked through `gmeow:references`) plus the alias-table version
  (`gmeow:gmnDictionaryVersion`) and glyph-table version (`gmeow:gmnGlyphTableVersion`). The alias
  table itself is a first-class `gmeow:GmnDictionary` (e.g. `gmeow:gmnDictV2`) whose
  `gmeow:GmnDictionaryEntry` members each bind one full GMEOW term to one compact alias string — a
  gated **bijection** over its covered term set (injectivity, `lang:GmnDictionaryAliasCollision`).
  A reader that holds the codebook holds everything needed to decode a conforming document.
- **Declared rate** — `gmeow:gmnDeclaredRate` points the codebook at
  `gmeow:gmnRateTokensPerStatement`, a dimensionless `math:Quantity` (16 tokens per statement).
  The declaration is the contract; the token metrics of the machine-compression sibling **measure**
  the realized encoding against it — the metric is the check, and a sustained divergence is a
  design finding about the codebook, never a reason to silently edit the declaration.
- **Fidelity** — the preservation judgment on each dialect crossing, carried on exactly one
  `logic:Correspondence` per crossing and nowhere else.

Confidence now rides **by reference** across the GMN-0 → GMN-1 crossing — a GMN-1 record carries an
alias or record identifier pointing at the canonical confidence-bearing statement, never the digits
themselves — so number-policy rounding is **no longer a ledgered crossing drop at all**. The
two-fractional-digit rule the grammar's fraction production enforces (below, "Number policy") is
the canonical **assertion/serialization** precision for *asserted* confidences — a policy about how
a curator writes a confidence down, not a claim that the confidence *algebra* is 2-digit-closed:
the product t-norm of two 2-digit confidences is 4-digit (0.95 × 0.95 = 0.9025), and re-quantizing
a derived value breaks associativity, so resting `logic:ExactPreservation` on quantization alone
would be unsound. Derived/computed confidences keep their full internal precision and ride by
reference across the crossing like any other referenced value; only a freshly *asserted* confidence
is written at two digits.

## The sigil table

Every GMN record opens with a sigil, and sigils are **record-initial** only: a sigil character
anywhere but the first position of a record is ordinary content. The ten roles are
`gmeow:GmnSigilRole` individuals, each naming its concrete string through `gmeow:gmnSigilGlyph`:

| Sigil | Role individual | Record carries |
|---|---|---|
| `@c` | `gmeow:gmnSigilClaim` | an asserted claim |
| `@e` | `gmeow:gmnSigilEvidence` | evidence for or against a claim |
| `@s` | `gmeow:gmnSigilStandpoint` | a standpoint scoping claims to a vantage |
| `@p` | `gmeow:gmnSigilProcess` | an activity or process description |
| `@π` | `gmeow:gmnSigilProof` | a proof or derivation |
| `@d` | `gmeow:gmnSigilDefeater` | a defeater of another record's claim |
| `@m` | `gmeow:gmnSigilModal` | a modal qualification |
| `@μ` | `gmeow:gmnSigilMath` | mathematical content |
| `@λ` | `gmeow:gmnSigilLangAst` | a `lang:` form-AST fragment |
| `@ℒ` | `gmeow:gmnSigilLogic` | `logic:` content — a formula, a rule, a type |

The **μ-collision ruling** follows from record-initiality by construction: μ opens a math record
only at position zero; inside any record μ is an ordinary symbol (a measure, a mean) whose scoped
reading is pinned through `gmeow:gmnSigilScope`. The same discipline lets a proof record mention
π the constant and a lang-AST record mention λ the binder.

## Record form, tabular form, and canonical order

The surface grammar is rendered in `grammars/gmn.ebnf` — a rendering of `gmeow:gmnGrammar`, never
its identity. The fenced blocks below are **normative example blocks**: they follow the EBNF
exactly and are the contract surface the GMN projection sibling seeds machine fixtures from.

A conforming document opens with the in-band header — one `@gmn` line pinning the schema major
and the alias-table reference (resolved, with `gmeow:gmnDictionaryVersion`, to the version
individual of the dialect lineage):

```text
@gmn{v: 1, aliases: dict-v2, glyphs: 2}
```

A **record** is a sigil followed by a braced key–value list. **Key order is generation order**,
and the canonical key order is exactly `s p o v q st ev m ek` (subject, predicate, object,
literal value, confidence, standpoint, evidence, modality, evidentiality-kind) — decide-first
fields first: the model commits to the subject before the confidence, and the epistemic
qualifiers (modality, evidentiality-kind) come last because they refine a claim already fully
identified by everything before them. Keys that are absent are simply omitted; keys that are
present appear in exactly this order. A valid record:

```text
@gmn{v: 1, aliases: dict-v2, glyphs: 2}
@c{s: gate1, p: hasState, o: doorGate1, v: open, q: 0.95, st: sensorCrew, ev: e12}
```

### The factored qualifier slots (`m`, `ek`, and the `@p`-only `bd` / `it`)

Ithkuil's orthogonal-factorization lesson (`LOGIC.md`, "Design influences beyond formal
logics — Ithkuil") is applied here directly, with its "unusable symbolic orthography" failure
mode engineered out by construction: every factored slot below is a **named key**, never a
private glyph, and it dealiases to a term that already exists in the canonical vocabulary —
no GMN-local shadow axis is minted (Principle 4). The slots are a **closed factored set** in
**fixed positions**: they extend the canonical key order, they are single-token markers, and a
record that omits one simply carries no assertion on that axis (the same discipline as `st` and
`ev`).

- **`m` (modality)** — a single-slot marker for a claim's alethic **modal force**, dealiasing to
  one individual of `gmeow:ModalForce` via the standpoint slice's `gmeow:claimModalForce`
  (`gmeow:modalForceNecessary`, `gmeow:modalForceActual`, `gmeow:modalForcePossible`,
  `gmeow:modalForceCounterfactual` — the □/◊ register a `gmeow:StandpointClaim` already carries).
  `m` is a compact, functional shorthand for the common single-force case inline on any
  claim-bearing record; it is distinct from the `@m` sigil (`gmeow:gmnSigilModal`), which opens a
  full record for a modal formula too structured for one key (a sigil is record-initial only, so
  the two never collide positionally — the same discipline the μ-collision ruling already
  establishes). Deontic force (obligation/permission/prohibition) is deliberately **not** covered
  by `m`: its only closed vocabulary lives in the `norms` extension slice, which a grounding
  slice must not depend on (Principle 4/16 — extensions depend on core, never the reverse), so
  `m` honestly covers the alethic register logic: already formalizes and leaves deontic content to
  a full `@ℒ` record until a reachable canonical deontic vocabulary exists.
- **`ek` (evidentiality-kind)** — a single-slot marker for **how** a claim's evidence was
  produced, dealiasing to one individual of `gmeow:ObservationMethod` via the observations
  slice's `gmeow:observationMethod` (`gmeow:methodDirectObservation`,
  `gmeow:methodInstrumentalReading`, `gmeow:methodRemoteSensing`,
  `gmeow:methodComputationalModel`, `gmeow:methodExpertJudgement`, `gmeow:methodSurvey`,
  `gmeow:methodStreaming`). This is Ithkuil's *Validation* category (observed / instrumented /
  inferred / reported / …) realized obligatory-but-compact: `st` already names WHO holds a claim
  and `ev` already names WHICH evidence backs it (a record identifier, by reference); `ek` adds
  the missing third leg, HOW the evidence was obtained, without inlining prose. `gmeow:Observation`
  is the domain of `gmeow:observationMethod`, and `gmeow:StandpointClaim` is itself a
  `gmeow:Observation` subclass, so `ek` is domain-consistent on exactly the records `st`/`ev`
  already annotate.
- **`bd` (boundary, `@p` records only)** — a single-slot marker for a process record's
  **action/event boundary**, dealiasing to one individual of `logic:OccurrentBoundary` via
  `logic:occurrentBoundary` (`logic:Open` — an on-going, unfinished action; `logic:Closed` — a
  completed, closed-interval event). This is the "openEHR-style action open/closed" YAMATO/Galton
  distinction `logic:` already formalizes (`design/LOGIC.md`'s occurrent-boundary machinery); `bd`
  is its compact GMN-1 rendering, never a re-minted local boundary vocabulary.
- **`it` (iteration, `@p` records only)** — a single-slot marker naming the `gmeow:EventSeries`
  a process occurrence belongs to, dealiasing to `gmeow:occurrenceOfSeries` (events slice, an
  existing lang dependency). Absent means a one-off occurrence (the default, matching every other
  optional-key omission rule); present carries a record identifier for the series — by reference,
  exactly like `st` and `ev`, never an inlined recurrence rule.

**Investigated and declined: a separate `phase` marker.** The Ithkuil-inspired factored-aspect
brief for `@p` records named three candidate factors — phase, boundary, iteration. Genuine
investigation of `logic:` (the occurrent/process home) turned up no canonical, closed vocabulary
for a process-internal phase (inceptive / progressive / completive) distinct from the
action/event boundary above: `logic:Phase` already names something else entirely (the UFO
anti-rigid-sortal meta-type, e.g. "child"/"adult" — a false friend), and no other reachable term
carries a phase-of-occurrent axis. Rather than mint a new foundational `logic:` axis as a
byproduct of a GMN compaction slot — a new occurrent-semantics distinction deserves its own
design-reviewed treatment, not a silent side effect here — the `@p` factored-aspect set stays at
two genuinely-grounded factors, `bd` and `it`; "phase" is subsumed by `bd`'s two-valued boundary,
and a dedicated phase marker is declined.

A record exercising the new slots, alongside a `@p` process record exercising the `@p`-only pair:

```text
@gmn{v: 1, aliases: dict-v2, glyphs: 2}
@c{s: gate1, p: hasState, o: doorGate1, v: open, q: 0.95, st: sensorCrew, ev: e12, m: poss, ek: inst}
@p{s: gate1, p: cycling, o: doorGate1, st: sensorCrew, bd: open, it: cycleSeries1}
```

### The measured token-cost razor

A borrowing — any of the factored qualifier slots above, or a future one proposed the same way —
is admitted **only** if it clears at least one of two razor halves, and both halves are
**executable**, never a free-text design assertion (R8/F5):

- **(a) Measured cost reduction.** The compact marker's alias string, run through the pinned
  `cl100k_base` primitive (`crates/lang-bridge/src/gmn_symbology.rs`,
  `gmn_glyph_token_cost`), costs strictly fewer tokens than the full canonical IRI it dealiases —
  the cost the alternative of inlining or separately asserting the full term would pay. This is
  the same primitive and the same measured-not-declared discipline the `*` operator glyph and the
  `⟦·⟧`/`den` ruling already use; `crates/lang-bridge/tests/gmn_cost_feed.rs` asserts the
  inequality for every marker admitted under this half.
- **(b) Ambiguity-class elimination.** The marker retires a *named* `lang:Gmn*` failure class —
  discharged by an executable fixture pair (the class fires on the un-marked form, is absent on
  the marked form), run through the codec/validator gate. The codec and validator now execute this
  half, but no current factored qualifier is admitted by it: a graph-level check cannot see a
  *compaction*-only ambiguity when the underlying RDF graph is already fully disambiguated.
  Every current qualifier therefore earns admission under half (a), honestly. A future marker may
  qualify under half (b) only when its named failure and positive/negative codec fixtures land in
  the same change.
- **No symbolic orthography, ever.** Every slot above is a named ASCII key and every value is a
  named ASCII alias — no private glyph is minted for any of them, and none enters the shared GMN
  script (`gmeow:gmnScript`) repertoire, which stays reserved for the codepoint-explicit,
  confusable-checked glyph plane. This is the Ithkuil failure mode — total precision with no
  usable, typeable surface — engineered out by construction, not by convention.

A **schema-once tabular batch** declares its columns once in an `@claims[...]` header and then
streams bare rows — the token-economy form for homogeneous runs. A valid batch:

```text
@gmn{v: 1, aliases: dict-v2, glyphs: 2}
@claims[s p o q]
gate1 locatedIn yardNorth 0.95
gate2 locatedIn yardSouth 0.88
```

**The `@λ` lang-AST column ruling.** A `@λ` (lang-AST) tabular batch of morphosyntactic rows reuses
the **existing CoNLL-U column order verbatim** — `ID FORM LEMMA UPOS XPOS FEATS HEAD DEPREL DEPS
MISC`, the ten Universal-Dependencies columns of the slice's CoNLL-U projection — never a rival
column scheme. The ruling is machine-pinned, not prose: the `GMN_LANG_AST_COLUMNS` constant is
asserted equal to the `ConlluToken` serializer's field order, so a reordering of either fails the
build. `@λ` is the lang-AST record role opened by the sigil `@λ` (`gmeow:gmnSigilLangAst`).

**Escaping.** GMN-1 has no escape syntax because it has no free-string production: a value is an
identifier, a canonical number, or a list of values, and nothing else. Prose and arbitrary
literals ride **by reference** — a dictionary alias or a record identifier — never as raw text
inside a record, so there is no quoting convention to attack and no injection surface to defend.
A token that is not identifier-shaped, not a canonical number, and not covered by the pinned
dictionary is `lang:GmnUncoveredTerm`, never a guess.

**Determinism and canonical ordering.** A conforming writer is deterministic: records appear in
the content-sorted order inherited from the GMN-0 normal form, keys appear in generation order,
and every value has exactly one spelling. Two conforming serializations of one model are
byte-comparable — the property the digest discipline of the envelope contract depends on.

**Number policy.** Confidences are fixed at **two fractional digits**; scientific notation is
forbidden. The grammar's fraction production is exactly two digits, so the rule is enforced by
the parse table itself, not by convention.

The four validator-tier failure classes, each with its labeled INVALID block:

INVALID — `lang:GmnNonCanonicalOrder` (wrong key order: the confidence precedes the subject):

```text
@gmn{v: 1, aliases: dict-v2, glyphs: 2}
@c{q: 0.95, s: gate1, p: hasState, o: doorGate1}
```

INVALID — `lang:GmnMalformedNumber` (scientific notation; a three-fractional-digit confidence):

```text
@gmn{v: 1, aliases: dict-v2, glyphs: 2}
@c{s: gate1, p: hasState, o: doorGate1, q: 9.5e-1}
@c{s: gate2, p: hasState, o: doorGate2, q: 0.951}
```

INVALID — `lang:GmnUndeclaredDialectVersion` (no `@gmn` header before the first record):

```text
@c{s: gate1, p: hasState, o: doorGate1, q: 0.95}
```

INVALID — `lang:GmnUncoveredTerm` (grammar-valid tokens that the pinned dictionary `dict-v2`
does not mint and no named-key ruling covers — undecodable, never guessable):

```text
@gmn{v: 1, aliases: dict-v2, glyphs: 2}
@c{s: zx9, p: quuxes, o: gate1, q: 0.50}
```

## In-band repair — `@err`, `@patch`, `@retract`

Repair is in-dialect. A reader that rejects a record answers with an `@err` record naming the
failure class, so the failure is itself GMN — a typed record the emitting model can read and act
on without leaving the channel. Corrections are **deltas over stable record identifiers**:
`@patch` restates fields of an identified record, `@retract` withdraws one, and both are
claims-about-claims — new records with their own standpoint, never in-place mutation of history:

```text
@gmn{v: 1, aliases: dict-v2, glyphs: 2}
@err{id: c42, class: GmnMalformedNumber}
@patch{id: c42, q: 0.95}
@retract{id: c17}
```

## The primer card

The teaching surface of the dialect is a generated **primer card**: roughly five hundred tokens
carrying the sigil table, the live aliases, and three worked examples. The card is a **derived
projection over the codebook** — token-budget-gated by the deterministic estimator discipline and
regenerated whenever the codebook changes — never hand-authored: a hand-written primer is a second
codebook wearing documentation's clothes.

## Encoding policy

- **UTF-8, NFC, one canonical codepoint sequence per glyph.** Comparison is codepoint-literal —
  the `gmeow:gmnCodepoints` "U+2291"-style spelling is the comparison key, and a glyph without a
  canonical spelling is `lang:GmnNonCanonicalCodepoint`.
- **Confusables hard-fail.** Two glyphs are confusable iff they share a Unicode UTS #39 skeleton;
  a co-resident confusable pair in the inventory is `lang:GmnConfusableGlyph`. The seeded
  `gmeow:gmnConfusableWith` pairs are data-level anchors; the skeleton computation is the
  validator's.
- **Per-plane uniqueness.** There is a **single shared glyph namespace keyed on (Script,
  codepoint-sequence)** — the three symbology planes (math, lang, logic) draw from one inventory,
  not three. Cross-plane reuse of a codepoint sequence is lawful **only** through a cited
  sigil-scoped exemption: each reading pins its record scope through `gmeow:gmnSigilScope`, and an
  unscoped collision is `lang:GmnGlyphCollision` — ambiguity in the glyph table itself.
- **Coverage gate.** Every term the writer emits is either glyphed or covered by an explicit
  named-key ruling; a term that is neither is `lang:GmnUncoveredTerm`. The independent glyph-
  optimality audit is closed in both directions: every `gmeow:GmnSymbolCandidate` is scored, and
  every target already reachable through an executable Denotation → Grapheme registry binding
  must have a candidate. Adding a working glyph without its candidate therefore creates
  `slice-quality.gmn-glyph-optimality.unaudited-executable-target` and enters the denominator; an
  executable sign can never grow silently outside the audited inventory.
- **Borrow real notation or use a named key.** A glyph is drawn from an established notation
  (DL's ⊑, measure theory's μ) or the term stays a named key — the dialect invents no private
  symbols. Symbol provenance is a **citation**: the citations vocabulary (`gmeow:cites`, promoted
  to a `gmeow:CitationAct` with a typed `gmeow:citationIntent`) on the glyph's source record,
  never a new provenance property.

## The linguistic symbology plane

The `lang:` plane of the shared glyph inventory. It is authored, not declared: every glyph is a
`lang:Denotation` fact carrying its codepoint-explicit spelling on the `lang:Grapheme` and its
**measured** token cost through `gmeow:gmnGlyphTokenCost` — a `math:Quantity` whose value is what
the crate-side token-cost primitive encodes for the glyph's rendering under the pinned codebook
vocabulary, cross-checked so an authored value can never drift from the measurement.

- **Disposition is read off the measurement and safety evidence.** A conventional Unicode sign may
  earn a glyph slot when it costs no more than its meaningful ASCII fallback; a cheaper named key
  wins, ambiguity moves the distinction into a structured constructor, and a semantic mismatch is
  rejected. The disposition is recorded, never inferred from typography: the crate-side benchmark
  and the named ambiguity/confusability/mismatch basis decide.
  - **`⊑`** (U+2291) costs three pinned tokens, no more than the explicit `is_subclass_of` fallback, so the
    established description-logic sign earns a scoped `@ℒ` glyph without a token penalty. Its
    denotation target is canonical `logic:subClassOf`; the grounding correspondence to
    `rdfs:subClassOf` owns the external spelling, preserving the grounding direction.
  - **`*`** (U+002A) is one token — it earns a glyph for the ungrammatical form, its reading pinned
    to the lang-AST record scope through `gmeow:gmnSigilScope` because the codepoint is reused by
    mathematical multiplication in the shared inventory.
  - **⟦·⟧** (U+27E6 / U+27E7), the formal-semantics denotation brackets, each fragment to two
    tokens — the pair costs four against the one-token key `den`, so the denotation term stays the
    named key and **no ⟦/⟧ grapheme enters the script**. This is the executable form of the
    charter's "token-benchmark before adopting": the ruling is the measurement's output.
  - **⇝** (U+21DD), the translation-leg arrow, costs three tokens, so the translation-leg term
    stays the named key `xl`.
- **IPA is the phonological encoding.** `lang:Phone` content is notated in IPA. The IPA chart is an
  imported plane (see below); its symbols join the shared script as graphemes denoting their
  phones — or, for the primary-stress mark, the prosodic suprasegmental it notates. The worked
  form **/ˈkæt/** glyphs [k], [æ], [t] plus the IPA primary-stress mark `ˈ` (U+02C8): the three
  segmental glyphs each measure one token, while `ˈ` measures **two** and carries a two-token
  cost, so the feed spans distinct measured values rather than a uniform one-token band.
- **Leipzig glosses are an imported alias plane.** The Leipzig Glossing Rules abbreviations
  (`NOM`, `ACC`, `PL`, `SG`, `PST`, `PRS`, the person values, the `3SG` portmanteau, the morpheme
  boundary) are imported **verbatim by reference** as named-key aliases, each a `lang:Denotation`
  binding the gloss to the `lang:` feature value it abbreviates — never a re-minted parallel code.
  The morpheme boundary `-` stays a named-key alias, never a glyph, because it is confusable with
  U+2010 / U+2212 / U+00AD; it is authored as a full `lang:WordForm` + `lang:Denotation` on the
  Leipzig plane whose target is the `lang:MorphemeBoundary` class (the segmentation point between
  morphs), not a feature value, and it carries no glyph-token cost.
- **Imported planes are cited and versioned.** An external standard the plane imports — the IPA
  chart, the Leipzig Glossing Rules — is a first-class `lang:GmnImportedPlane` the codebook
  references, and it carries **both** its citation (`gmeow:cites`, typed by a `gmeow:CitationAct`)
  **and** its version (`dcterms:hasVersion`). A plane missing either is `lang:GmnUnattributedPlane`.
- **Coverage is total and derived.** Every morphological feature value the surface emits is bound
  to a disposition — a glyph, a named key, or an imported alias — with the population derived by
  type over `lang:FeatureValue`, so a value added without a disposition is
  `lang:GmnUndispositionedTerm`. This graph-side completeness is distinct from the writer-tier
  `lang:GmnUncoveredTerm`. **Glyph-cost coverage is total the same way:** every `lang:Grapheme`
  admitted to the GMN script (`lang:inScript gmeow:gmnScript`) — the IPA phonological graphemes,
  the `*` operator glyph — carries its measured per-glyph token-cost feed through
  `gmeow:gmnGlyphTokenCost`, with the population derived by the `gmeow:gmnScript` repertoire rather
  than a hand-listed set, so a script glyph admitted without its cost feed is
  `lang:GmnUncostedScriptGlyph`. The two coverage rules close both the feature-value plane and the
  glyph plane against silent gaps.

## Decodability is a property of the grammar object

`gmeow:gmnGrammar` carries `gmeow:gmnDeterminismClass "LL(1)"`: the grammar is prefix-stable and
newline-delimited, a reader always knows from the next token which production applies, and no
lookahead beyond one token is ever needed. The consequence is the dialect's deepest design bet:
**the parser and the constrained decoder are the same automaton** — the table that validates a
GMN stream is the table that constrains a model's generation of one. The determinism class is a
declared contract, never taken on faith: the parse-table construction over the grammar's rules is
the machine check, `lang:GmnNonDecodableGrammar` names its failure, and the graph-derived glyph
registry renders the EBNF token production used by the same reader/writer. The authored
`grammars/gmn.ebnf` is therefore a structural template, not a second codebook: it contains one
`glyphToken ::= GRAPH_DERIVED_GLYPH_TOKEN` replacement seam and no literal glyph list. Regeneration
must replace that one production from the registry before parsing or shipping it; zero or multiple
seams hard-fail. Registry removal, wrong-scope, all-sigil, and generated-projection tests make that
check executable end to end.

## Versioning

Versions are first-class individuals, relational and never IRI-suffixed: the dialect lineage is
the `gmeow:VersionSet` `gmeow:gmnDialectVersions`, its majors are version entities
(`gmeow:gmnVersionOne`), and canonical/latest status is carried by reified
`gmeow:VersionMembership` relators. Migration between majors is a judged crossing —
`gmeow:gmnMigratesFrom` / `gmeow:gmnMigratesTo` on a translation unit or correspondence — and a
migration correspondence is `logic:mnemomorphic` **iff the bump is additive-only**: a claim of
recoverability over a stronger-than-additive bump is `lang:GmnVersionOverclaim`. The current
version alone lives in the canonical core; prior versions are frozen generated artifacts plus
their migration correspondences. The **support policy** is a lineage-level integer,
`gmeow:gmnAcceptWindow` on `gmeow:gmnDialectVersions` (shipped value: 1): a conforming writer
emits the current major, and a conforming reader accepts the current major plus N−1 priors —
every accepted prior entering only through a judged migration crossing, never silent tolerance.

## The envelope contract

A `gmeow:GmnEnvelope` is the attested carrier of a GMN payload across a serialization boundary,
and its eight-field contract is total — a missing field is `lang:GmnMissingEnvelopeField`:

| Field | Vocabulary | Carries |
|---|---|---|
| schema version | `gmeow:gmnSchemaVersion` | the grammar/record-shape major |
| dictionary version | `gmeow:gmnDictionaryVersion` | the alias table the names resolve through |
| glyph-table version | `gmeow:gmnGlyphTableVersion` | the projected glyph/codepoint table |
| security ring | `gmeow:gmnSecurityRing` | the deontic serialization boundary — a point in a factored `(level, compartment)` information-flow lattice; the transitive `gmeow:gmnRingWithin` order between rings is DERIVED from those coordinates, never hand-chained (see [Ring-lattice model](#the-ring-lattice-model) below). Minted fresh after checking the rights and agreements vocabulary: `gmeow:RightsStatement` deontics regulate content licensing, not serialization boundaries |
| standpoint | `gmeow:accordingTo` | the asserting standpoint |
| generating activity | `gmeow:wasGeneratedBy` | the emission run's provenance |
| content digest | `gmeow:contentDigest` | byte-exact identity, blake3 |
| losslessness judgment | `gmeow:gmnEnvelopeCorrespondence` | the single `logic:Correspondence` judging the crossing back to the normal form — never a boolean flag |

**The digest domain is pinned.** `gmeow:contentDigest` is a blake3 digest computed over the
canonical GMN-0 normal-form bytes — pre-envelope, post-NFC — so equal models share a digest across
surface variants: two envelopes carrying different encodings of one model carry one digest.

## The ring-lattice model

`gmeow:GmnSecurityRing` is the full Denning information-flow lattice, factored into two orthogonal
coordinates rather than the fixed three-ring chain the dialect shipped with originally:

- **Classification level** — `gmeow:gmnRingLevel` (functional), an ordered `gmeow:GmnRingLevel`
  value. The three shipped levels form a linear ladder, `gmeow:gmnLevelCore` ≻
  `gmeow:gmnLevelTrusted` ≻ `gmeow:gmnLevelRestricted`, ordered by `gmeow:gmnRingLevelDominates`
  (reflexive-and-transitive, its full closure hand-asserted per level — the same idiom as
  `logic:levelAtOrAbove` in the `logic:` design set). The level axis is **lang-local**, not the
  kernel `gmeow:SensitivityLevel`: the kernel's only ordering property, `gmeow:coarserThan`, is
  `rdfs:domain`/`rdfs:range`-scoped to `gmeow:GranularityLevel`, so reusing it here would wrongly
  entail every ring level is also a granularity level — a genuine mismatch, not a style choice.
- **Compartment / caveat set** — `gmeow:gmnRingCompartment` (non-functional, zero or more), an
  open, unordered `gmeow:GmnCompartment` value vocabulary (a NATO-eyes-only scope, a named-partner
  scope, …). An empty set is well-formed: the three shipped default rings carry none.

**The order is derived, never authored.** `gmeow:gmnRingWithin(X, Y)` — X within Y — holds iff X's
level dominates Y's (`gmeow:gmnRingLevelDominates`) **and** X's compartment set contains Y's (⊇) —
the Denning product-order dominance test. This is computed by a `logic:Rule` pair
(`gmeow:ruleGmnRingWithinDerive` plus its `gmeow:ruleGmnRingCompartmentGap` negation guard for the
⊇ test), materialized by the native `logic:` reasoner over the authored coordinates — the same
mechanism `logic:ruleProjectIsAwareOf` uses to project the coarse knowledge ladder. The property
stays `owl:TransitiveProperty` (EL-clean transitivity alone is fine), but **acyclicity and the ⊇
containment test are never OWL characteristics** — no `owl:irreflexive` / `owl:AsymmetricProperty`
on the derived order, which would push the profile out of the EL fragment and hard-fail
`make reason-verify`. The relation is REFLEXIVE (a ring is trivially within itself, the standard
Denning ⊑ reading — the flow-check idiom already asks "ring equals the destination, OR is
`gmeow:gmnRingWithin` it", so equality is handled once); the derivation rule itself carries no inequality guard, since the order it computes wants to be
reflexive; the migration-equivalence witness below instead applies its own `FILTER(?x != ?y)` at the
SPARQL layer, where irreflexivity is actually load-bearing.
A structural gate (`tests/structural.ttl`) additionally asserts that ZERO
`gmeow:gmnRingWithin` triples are ever authored in the carrier — the predicate is populated only at
reason time, and a hand-authored edge (including a reintroduction of the retired chain) fails the
build even if the edge happens to match the computed order.

**The well-formedness gate bites the authored coordinates, not the derived relation** — gating the
derived `gmeow:gmnRingWithin` for partial-order-ness would be a tautology (a derivation from a
partial order is trivially one). `lang:GmnRingLatticeMalformed` (SHACL) instead requires every ring
to carry exactly one `gmeow:gmnRingLevel` and only declared `gmeow:GmnCompartment` values.

**Open set, shipped preset.** Rings are first-class individuals in an OPEN set — no `owl:oneOf`
closure. The three original rings are re-expressed with explicit coordinates as members of
`gmeow:gmnRingPresetDefault` (a `gmeow:GmnRingPreset`, the `gmeow:VersionSet` bundling idiom without
its full relator reification, since preset membership carries no competing-authority or interval
semantics). A **migration-equivalence witness** (a `gmeow:reasoningLogic` competency question)
computes the derived order over the three default-preset rings' coordinates and asserts it equals
the retired core-within-trusted-within-restricted chain exactly — proving the refactor is
behaviour-preserving rather than a mere renaming of the same asserted facts.

**The NATO case.** `gmeow:gmnRingNato` is a fourth ring at the SAME level as
`gmeow:gmnRingTrusted` but carrying a non-empty, distinct compartment set
(`gmeow:gmnCompartmentNato`, `gmeow:gmnCompartmentPartner`) — demonstrating that scope is a genuine
second axis: same level, different compartment, different (and asymmetrically related) ring. It is
deliberately not a member of the default preset.

**Typed admission/exclusion criteria.** `gmeow:gmnRingAdmits` / `gmeow:gmnRingExcludes` relate a
ring to a typed `gmeow:GmnRingCriterion` value it requires or forbids of admitted content —
mirroring the rights slice's `gmeow:RightsAction` discipline (`gmeow:Permission` /
`gmeow:Prohibition` point at a typed action, never a free-text reason) rather than inventing a bare
literal reason field.

## GMN-2 doctrine

A compaction is a reified **`gmeow:StandpointClaim` subclass** (`gmeow:GmnCompaction`) with its
own identity, standpoint, and confidence. Four rules hold:

- **Provenance is total.** Every compacted source is reachable through `gmeow:gmnCompacts`, and
  the holding standpoint is named through `gmeow:vantage`; a partial source list or a missing
  vantage is `lang:GmnCompactionWithoutProvenance`. Sources are never overwritten.
- **Honesty is `ValidationOnly`.** A compaction's correspondence never claims more than
  `logic:ValidationOnly` / `logic:BridgeView`; exactness, soundness, or a recoverability witness
  is `lang:GmnCompactionOverclaim` — the crossing's own law-spine denies the witness.
- **Silent disambiguation applies to compactions.** A compaction run that collapses co-resident
  readings without a vantage-held observation grounding the choice fires the existing bundle-wide
  `lang:SilentDisambiguation` discipline, reused verbatim.
- **A compaction is a dialect act, not a codec act.** The same claim compressed is the same
  claim; a compaction is a new one.

## Wiring

The [`LANG-PROJECTIONS.md`](LANG-PROJECTIONS.md) registry is the **sole emission seam** for GMN:
the writer registers as a projection target or it emits nothing, and its preservation is
**derived** from the executed crossings, never declared. Ingestion of LLM-emitted GMN is the
[`LANG-RUNTIME.md`](LANG-RUNTIME.md) lane — the projection run backwards. A parallel generic
transcode codec beside the registry is **ruled out**: one canonical seam, per the pipeline-spine
doctrine. Division of labor across the sibling work is explicit: the glyph inventories for the
three symbology planes are the glyph-enrichment children of the dialects epic; the parser/writer —
the table-driven automaton, its validator tier, and the verbalizer — is the GMN projection
sibling's; the token metrics that measure the declared rate are the machine-compression
sibling's. This charter is the contract they all implement against.

## Conformance

Every hard rule above is a row of the [`LANG-CONFORMANCE.md`](LANG-CONFORMANCE.md) gate matrix
("GMN dialect rules") with a named `lang:Gmn*` failure class — fourteen rules total, nine
enforced by the SHACL gates in `shapes.ttl` (each naming its class through
`gmeow:enforcesFailureClass`), five by the GMN parser/writer's validator tier against the
normative example blocks of this charter. A violation
is a typed, queryable object, not a log line.
