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
- **GMN-1** (`gmeow:gmnModelNotation`) is the token-compact model surface: a well-behaved lossy
  lens over GMN-0 (`gmeow:gmnCorrNormalToGmn` — `logic:SoundUnderApproximation`,
  `logic:LossyLens`, `logic:mnemomorphic false`) whose drop list is categorical and ledgered:
  full IRIs collapse to dictionary aliases, annotation baggage drops, and numeric confidences
  round. Everything GMN-1 states is derivable from the normal form; the reverse does not hold.
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
  (`gmeow:gmnDictionaryVersion`) and glyph-table version (`gmeow:gmnGlyphTableVersion`). A reader
  that holds the codebook holds everything needed to decode a conforming document.
- **Declared rate** — `gmeow:gmnDeclaredRate` points the codebook at
  `gmeow:gmnRateTokensPerStatement`, a dimensionless `math:Quantity` (16 tokens per statement).
  The declaration is the contract; the token metrics of the machine-compression sibling **measure**
  the realized encoding against it — the metric is the check, and a sustained divergence is a
  design finding about the codebook, never a reason to silently edit the declaration.
- **Fidelity** — the preservation judgment on each dialect crossing, carried on exactly one
  `logic:Correspondence` per crossing and nowhere else.

Number-policy rounding is enumerated as **distortion** in the GMN-0 → GMN-1 drop list: confidence
values round to two fractional digits, and the loss is part of the crossing's ledgered claim, not
an implementation detail.

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
@gmn{v: 1, aliases: dict-v1}
```

A **record** is a sigil followed by a braced key–value list. **Key order is generation order**,
and the canonical key order is exactly `s p o v q st ev` (subject, predicate, object, literal
value, confidence, standpoint, evidence) — decide-first fields first: the model commits to the
subject before the confidence. Keys that are absent are simply omitted; keys that are present
appear in exactly this order. A valid record:

```text
@gmn{v: 1, aliases: dict-v1}
@c{s: gate1, p: hasState, o: doorGate1, v: open, q: 0.95, st: sensorCrew, ev: e12}
```

A **schema-once tabular batch** declares its columns once in an `@claims[...]` header and then
streams bare rows — the token-economy form for homogeneous runs. A valid batch:

```text
@gmn{v: 1, aliases: dict-v1}
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
@gmn{v: 1, aliases: dict-v1}
@c{q: 0.95, s: gate1, p: hasState, o: doorGate1}
```

INVALID — `lang:GmnMalformedNumber` (scientific notation; a three-fractional-digit confidence):

```text
@gmn{v: 1, aliases: dict-v1}
@c{s: gate1, p: hasState, o: doorGate1, q: 9.5e-1}
@c{s: gate2, p: hasState, o: doorGate2, q: 0.951}
```

INVALID — `lang:GmnUndeclaredDialectVersion` (no `@gmn` header before the first record):

```text
@c{s: gate1, p: hasState, o: doorGate1, q: 0.95}
```

INVALID — `lang:GmnUncoveredTerm` (grammar-valid tokens that the pinned dictionary `dict-v1`
does not mint and no named-key ruling covers — undecodable, never guessable):

```text
@gmn{v: 1, aliases: dict-v1}
@c{s: zx9, p: quuxes, o: gate1, q: 0.50}
```

## In-band repair — `@err`, `@patch`, `@retract`

Repair is in-dialect. A reader that rejects a record answers with an `@err` record naming the
failure class, so the failure is itself GMN — a typed record the emitting model can read and act
on without leaving the channel. Corrections are **deltas over stable record identifiers**:
`@patch` restates fields of an identified record, `@retract` withdraws one, and both are
claims-about-claims — new records with their own standpoint, never in-place mutation of history:

```text
@gmn{v: 1, aliases: dict-v1}
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
  named-key ruling; a term that is neither is `lang:GmnUncoveredTerm`.
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

- **Disposition is read off the measurement.** A symbol earns a glyph slot **iff it costs a single
  token**; a dearer symbol is not paying its way against a one-token named key and stays a key.
  The disposition is computed, never pre-committed: the crate-side benchmark decides.
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
  imported plane (see below); its segmental symbols join the shared script as graphemes denoting
  their phones (the worked form /kæt/ glyphs [k], [æ], [t]).
- **Leipzig glosses are an imported alias plane.** The Leipzig Glossing Rules abbreviations
  (`NOM`, `ACC`, `PL`, `SG`, `PST`, `PRS`, the person values, the `3SG` portmanteau, the morpheme
  boundary) are imported **verbatim by reference** as named-key aliases, each a `lang:Denotation`
  binding the gloss to the `lang:` feature value it abbreviates — never a re-minted parallel code.
  The morpheme boundary `-` stays a named-key alias, never a glyph, because it is confusable with
  U+2010 / U+2212 / U+00AD.
- **Imported planes are cited and versioned.** An external standard the plane imports — the IPA
  chart, the Leipzig Glossing Rules — is a first-class `lang:GmnImportedPlane` the codebook
  references, and it carries **both** its citation (`gmeow:cites`, typed by a `gmeow:CitationAct`)
  **and** its version (`dcterms:hasVersion`). A plane missing either is `lang:GmnUnattributedPlane`.
- **Coverage is total and derived.** Every morphological feature value the surface emits is bound
  to a disposition — a glyph, a named key, or an imported alias — with the population derived by
  type over `lang:FeatureValue`, so a value added without a disposition is
  `lang:GmnUndispositionedTerm`. This graph-side completeness is distinct from the writer-tier
  `lang:GmnUncoveredTerm`.

## Decodability is a property of the grammar object

`gmeow:gmnGrammar` carries `gmeow:gmnDeterminismClass "LL(1)"`: the grammar is prefix-stable and
newline-delimited, a reader always knows from the next token which production applies, and no
lookahead beyond one token is ever needed. The consequence is the dialect's deepest design bet:
**the parser and the constrained decoder are the same automaton** — the table that validates a
GMN stream is the table that constrains a model's generation of one. The determinism class is a
declared contract, never taken on faith: the parse-table construction over the grammar's rules is
the machine check, `lang:GmnNonDecodableGrammar` names its failure, and the EBNF exact round-trip
lift (`grammars/gmn.ebnf` lifting to an isomorphic grammar object and back) is the partial
executable check already in force.

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
| security ring | `gmeow:gmnSecurityRing` | the deontic serialization boundary — a lattice-ordered admission class, ordered by the transitive `gmeow:gmnRingWithin` (core within trusted within restricted; content permitted in a ring flows wherever that ring is within). Minted fresh after checking the rights and agreements vocabulary: `gmeow:RightsStatement` deontics regulate content licensing, not serialization boundaries |
| standpoint | `gmeow:accordingTo` | the asserting standpoint |
| generating activity | `gmeow:wasGeneratedBy` | the emission run's provenance |
| content digest | `gmeow:contentDigest` | byte-exact identity, blake3 |
| losslessness judgment | `gmeow:gmnEnvelopeCorrespondence` | the single `logic:Correspondence` judging the crossing back to the normal form — never a boolean flag |

**The digest domain is pinned.** `gmeow:contentDigest` is a blake3 digest computed over the
canonical GMN-0 normal-form bytes — pre-envelope, post-NFC — so equal models share a digest across
surface variants: two envelopes carrying different encodings of one model carry one digest.

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
("GMN dialect rules") with a named `lang:Gmn*` failure class — seven enforced by the SHACL gates
in `shapes.ttl` (each naming its class through `lang:enforcesFailureClass`), five by the GMN
parser/writer's validator tier against the normative example blocks of this charter. A violation
is a typed, queryable object, not a log line.
