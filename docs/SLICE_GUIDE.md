<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The Slice Guide — authoring the optimal, maximal, richest slice

> **What this is.** A guidepost, not a rulebook: the operational sequence for authoring a
> GMEOW slice — or, more often, **re-authoring an existing slice** onto the grounding
> triad. Every rule here is stated once in a normative source and *cited*, never
> restated as new doctrine; where this document and a normative source disagree, the
> source wins and this guide has a bug. Normative sources, in precedence order:
> [`.goals`](../.goals) and [`CONSTITUTION.md`](../CONSTITUTION.md); the grounding design
> sets ([`slices/grounding/logic/design/`](../slices/grounding/logic/design/LOGIC.md),
> [`slices/grounding/math/design/`](../slices/grounding/math/design/MATHEMATICS.md),
> [`slices/grounding/lang/design/`](../slices/grounding/lang/design/LANG.md)); the
> correspondence calculus
> ([`docs/APPLIED_CATEGORY_THEORY/take1.md`](APPLIED_CATEGORY_THEORY/take1.md)); and the
> pipeline spine ([`docs/PIPELINE_SPINE.md`](PIPELINE_SPINE.md)).
>
> **The living example.** The `slices/core/tags/` slice is this guide's worked instance —
> small, useful, and re-authored to this recipe. Where a section says *"see the exhibit"*,
> it names a real file in that slice. Read the guide once, then read the tags slice in the
> order §13 gives, and you have seen every motion performed on real data.

---

## 1. The one-paragraph theory of a slice

A slice is a **thin layer of domain identity over thick grounding references**. The
grounding triad carries the weight — `logic:` grounds reasoning (truth, inference, proof,
plans, shapes), `math:` grounds quantity and structure (framed values, expressions,
distributions), `lang:` grounds meaning and expression (sign systems, forms, denotation,
translation) (Principle 19). What remains for a domain slice is what only it can say: which
kinds exist in its domain, which relators bind them, which facets they bear, and which
external vocabularies it corresponds to. Everything a slice emits — OWL, SHACL, ShEx,
SSSOM, EDOAL, FnO, SPARQL, docs — is a **generated lossy projection** of the canonical
authored source, each carrying a preservation judgment in the loss ledger (Principles 4,
17; take1 §11). You author the canon once; the projections are not your problem, and
hand-editing one is a drift failure (Principle 7).

**Richness is depth per term, never term count.** The optimal slice usually has *fewer*
terms after re-authoring than before — each one grounded, annotated, aligned, exemplified,
queried, and tested.

## 2. The five questions — answer before authoring anything

1. **Who consumes it?** Name the consumer — a product, a worked example, a dataset, a real
   corpus (Principle 15). "It would be modelled beautifully" is not a consumer. The answer
   goes in the manifest's `gmeow:sliceConsumer`, machine-checked.
2. **Where does it live?** One directory, `slices/<group>/<name>/`; the **manifest is the
   sole source of tier** — the path segment carries no semantics
   (`slices/vocabulary.ttl`; Principle 16). Core is for what the flagship products load
   plus what GMEOW refuses to make optional; everything else is an extension bundle.
3. **What does the triad already own?** For every concept you are about to model, run the
   grounding test (§4). Most of what feels like "your" vocabulary is a reference into
   `logic:`, `math:`, or `lang:` — and minting a domain twin of a grounding-layer term is
   the first anti-pattern (§12).
4. **What does the world already call it?** Survey the external vocabularies before
   minting (the design sets' references appendices are the pattern). You will align to
   them by reference, at an honest rung (§7) — never import, never rewrite (Principle 5).
5. **What is the data's epistemic shape?** Contested? Vague? Vantage-relative?
   Time-scoped? Suppressible? The shape must survive into the model (§8) — flattening it
   is fabrication, and imposing structure the source does not license is the same sin in
   the other direction.

## 3. The authoring loop — competency first

The single highest-leverage habit: **write the questions before the terms.** The grounding
design sets define their depth by flagship scenarios, not adjectives; a slice does the
same at domain scale.

1. **Write the competency questions** your consumer needs answered, as prose. *"What did
   this agent tag, under which scheme, during Q1?" "Which tags apply right now, honoring
   suppression?" "Which broader topics does this tag roll up to?"*
2. **Draft each as a SPARQL query** in `queries/competency/*.rq`. If you cannot phrase the
   question as a query, you do not yet know what you are modelling.
3. **Mint the minimum vocabulary** that makes every query answerable (§5), grounding each
   concept per §4.
4. **Annotate completely** (§6.2) — every term human-readable and machine-discoverable by
   default (Principle 8).
5. **Align honestly** (§7) and **exemplify richly** (§6.5).
6. **Pin the questions as tests**: each query becomes a `gmeow:CompetencyQuestion` cell in
   `tests/competency.ttl` with an expected shape or ASK verdict, executed by the native
   slice-test harness. A competency failure is a coverage gap, not a data violation.
7. **Run the gates and own every drift** (§11).

The loop is re-entrant: re-authoring an existing slice starts at step 1 against the
*existing* module and usually discovers that the vocabulary is adequate but unqueried,
unaligned, or epistemically flat — which is exactly what steps 2, 5, and 6 repair.
*Exhibit: the tags slice gained its `queries/competency/` and `tests/competency.ttl` in
its re-authoring pass; the module itself barely changed.*

## 4. The grounding test — one question per concept, three layers

For each concept, ask **what kind of weight it carries**, and ground that weight in the
layer that owns it. A domain slice *references* the triad; it never re-derives it.

| The concept involves… | It grounds in | As | Never as |
| --- | --- | --- | --- |
| inference, rules, constraints, plans, roles/kinds/phases | `logic:` | axioms, derivation rules, validation shapes, `logic:Plan`, foundation stereotypes (`logic:Kind`, `logic:Role`, `logic:Relator`, …) | bespoke boolean flags, out-of-band lint, prose-only rules |
| a number, count, measurement, probability, statistic, formula | `math:` | a framed quantity, an expression AST, a parameterized distribution, an estimate with its inference run | a bare literal; a probability that is secretly a confidence |
| a label, name, string, notation, translation, meaning link | `lang:` | a form in a sign system, a denotation record, a rendering, an audited translation | a tag-string treated as identity; a language as a bare code |

Three consequences worth internalizing:

- **Stereotype every class** with its foundation category (`logic:Kind`,
  `logic:SubKind`, `logic:Role`, `logic:Phase`, `logic:Relator`, `logic:Mode`, …) — the
  identity discipline is axioms, not lint (Principle 17). *Exhibit: `gmeow:Tag` is a
  `logic:Kind`; `gmeow:Tagging` is a `logic:Relator` mediating its three roles
  (`slices/core/tags/module.ttl`).*
- **A value without its frame is ill-formed** (Principle 11). Anything measured, counted,
  or scored routes through the observation spine and the `math:` layer — value, unit,
  frame, determinacy, uncertainty, provenance.
- **Meaning is never string equality** (`LANG-MEANING.md`). One surface form with two
  meanings is two denotations, not one term; two forms with one meaning is coreference in
  data, never `owl:sameAs` collapse (Principle 5). *Exhibit: the tags slice models
  homonyms as distinct Tag IRIs sharing a label — one form, two denotations — in
  `examples/contested-tagging.ttl`.*

## 5. Minting discipline — maximal depth, minimal surface

- **Individuals, not subclass explosions.** A named thing is an individual of a small
  class; a new OWL class exists only for a genuine class of individuals (the grounding
  design sets fix this pattern). Open value spaces are kind-individuals
  (`lang:signSystemKind`-style), not class ladders.
- **Flat-first, reify-on-demand** (CONSTITUTION, recurring patterns). Pair the 80% flat
  shortcut with a relator for when period, provenance, confidence, or standpoint must be
  first-class — and declare the pairing (`gmeow:pairsWith`) so the promotion path is
  machine-readable. *Exhibit: `gmeow:hasTag` ⟷ `gmeow:Tagging`.*
- **Orthogonal axes stay orthogonal** (Principle 9). Never inferred from one another,
  never ranked: no `primary*`, no `preferred*`. If two properties can be confused, make
  them property-disjoint and say why in both terms' `gmeow:avoidWhen`. *Exhibit: the
  typing / aboutness / tagging trichotomy — `rdf:type` vs `gmeow:isAbout` vs
  `gmeow:hasTag`, property-disjoint, tested.*
- **The Profile pattern** for open-but-structured facets: closed descriptor schema + open
  values + self-description, so extensibility is by construction.
- **Derived facts are never asserted in source.** If the reasoner can conclude it, do not
  author it — asserting a derived triple fails validation, and inventing a convenience
  copy of a computed value bloats the canon (Principle 12).

## 6. The anatomy — file by file, with the richness bar

```text
slices/<group>/<name>/
├── manifest.ttl      # identity, tier, dependencies, consumer — the slice's contract
├── module.ttl        # the vocabulary: classes, properties, individuals
├── design/           # (larger slices) the design charter(s), written before the module
├── mappings/         # correspondence frontend records → SSSOM/EDOAL/FnO/OWL lowerings
├── queries/
│   ├── competency/   # the questions the slice exists to answer
│   └── verify/       # (where needed) invariant queries beyond shapes
├── examples/         # worked scenes, mixing namespaces on purpose
├── tests/
│   ├── structural.ttl  # MUST/MUST-NOT cells over the module graph
│   └── competency.ttl  # the pinned competency questions
├── shapes.ttl        # (where needed) closed-world data shapes — see §9
└── docs.md           # the narrative: why the slice exists and how to think in it
```

### 6.1 `manifest.ttl` — the contract

Tier (`gmeow:sliceTier` — the sole tier truth), profile, and `gmeow:sliceDependsOn` —
which is **gate-verified against the computed cross-slice reference graph**: declare
exactly what your terms reference, no more, no stale edges. The `gmeow:sliceConsumer`
field carries the Principle 15 answer as real prose naming real consumers.
*Exhibit: `slices/core/affect/manifest.ttl` — the consumer field names three consumers
and the growth rule.*

### 6.2 `module.ttl` — every term carries its full coat

The annotation-completeness gate requires `rdfs:label`, `skos:definition`, and
`rdfs:isDefinedBy` on every term (Principle 8). The richness bar is higher:

- `skos:definition` that states the concept *and its boundaries* — what it is NOT;
- `skos:example` with a one-line worked triple;
- `gmeow:useWhen`, `gmeow:avoidWhen`, `gmeow:howToUse` — the three-part usage coat that
  makes the docs pages teach;
- `gmeow:graphBoxRole` (TBox/RBox/ABox placement);
- foundation stereotype (§4) and, on relators, `logic:mediates` naming the roles;
- OWL characteristics stated where true (functional, transitive, symmetric, inverse,
  property-disjointness) — these are the `logic:` axioms the projections lower from.

*Exhibit: any term in `slices/core/tags/module.ttl` — each carries all of the above; the
definition of `gmeow:Tag` states three NOTs.*

A coat must **distinguish** its term: two distinct terms may not carry a byte-identical
(normalized) `skos:definition` / `useWhen` / `avoidWhen` / `howToUse`, and a translation
may not collapse a distinction its English source makes. This is a hard structural gate
(N = 2, no calibration) — see the distinctiveness guard in
[`SLICE_QA.md`](./SLICE_QA.md). Reword a near-duplicate to be term-specific; do not
suppress it.

### 6.3 `design/` — for slices with a thesis

A slice whose domain carries real doctrine writes the charter first (the affect,
inhabitation, and grounding design sets are the pattern): manifesto voice, declarative
present tense as normative, hard rules each mapped to a gate. Small utility slices skip
this; their `docs.md` carries the thesis instead.

### 6.4 `mappings/` — the correspondence frontend (§7)

### 6.5 `examples/` — scenes, not snippets

An example is a small **scene** that exercises the slice's distinctive semantics — and the
best ones deliberately mix namespaces, because the grounding-layer composition is the
point: a domain relator carrying a `logic:`-stereotyped role, holding a framed value,
about a labelled form. Every example must include the epistemically *hard* case: the
contested claim, the suppressed value, the vague relationship — because examples are what
downstream authors copy, and if they only ever see the easy case they will flatten the
hard one. *Exhibit: `examples/contested-tagging.ttl` — two taggers disagree about one
resource; both taggings coexist with confidences; a third is suppressed
(`gmeow:displayable false`), never deleted (Principles 9, 10).*

### 6.6 `tests/` — assertions with rationales

- **Structural cells** (`tests/structural.ttl`): each `gmeow:StructuralAssertion` carries
  polarity (MUST / MUST-NOT), scope, the ASK pattern, and — non-negotiable — a
  `gmeow:saRationale` that explains *why the invariant matters*, not what the query does.
  A test whose reason has been forgotten is a test that gets deleted in refactoring.
- **Competency cells** (`tests/competency.ttl`): pin each query with the right
  cardinality mode — `logic:RowsContains` (witness mode) when the merged graph legitimately
  contains more than your witnesses; `RowsExact` only when the slice owns the whole answer;
  a bare ASK with a `gmeow:cqDataFile` overlay when the question is "does this whole scene
  hold together?". Overlays keep example data out of the module and let one scene serve
  many questions. *Exhibit: `slices/core/tags/tests/competency.ttl` — all three modes.*

### 6.7 `docs.md` — the narrative

Not generated, not a term listing (the docs pipeline builds those). It answers: what rots
without this slice, what the load-bearing distinctions are, and how to think in it. One
memorable thesis sentence beats a section of restatement. *Exhibit:
`slices/core/tags/docs.md` — "tagging systems rot when three different things get smeared
into one."*

### 6.8 The maturity ladder — FULL and MAXIMAL are exactly the rendered surfaces

The richness bar is not a mood; it is a **lattice of structural coverage dimensions** that
the docs generator detects, the coverage projection folds into the
`gmeow:graph/documentation` named graph, and the maturity gate scores. A slice claims a tier
with `gmeow:sliceDocMaturity`; the projection computes the tier its coverage actually
**earns** (`gmeow:docEarnedMaturity`, the largest anchor whose required dimensions are all
covered); the headline gate reds the build when a slice claims more than it earns
(`asserted ⊄ earned`). The vocabulary is `slices/core/documentation/module.ttl` — the
`gmeow:DocMaturity` anchors and the `gmeow:DocCoverageDimension` individuals — and its Rust
twin is `crates/docs/src/maturity.rs`.

Four anchors, nested by intent (`Minimal ⊆ Basic ⊆ Full ⊆ Maximal`):

- **Minimal** — the term is named and defined (`dimDefinition`, `dimLabel`).
- **Basic** — the six-dimension core coat: Minimal plus `dimUsageAdvice`, `dimExample`,
  `dimScopeNote`, `dimAlignment`. This is the § 6.2 annotation-completeness bar.
- **Full** — the core coat plus proof-carrying evidence and honest realized state.
- **Maximal** — Full plus every remaining structural dimension, including the Principle-17
  judgment-valued loss refinement.

Every dimension is a **deterministic structural predicate** — a present/absent fact of the
model, never a corpus-tuned threshold or a subjective grade — so the maturity axes stay
objective. The two tiers a re-authored slice aims for are defined here as **exactly** the
surfaces an author must provide. The first whitespace-delimited token on each line of the
blocks below is the canonical `gmeow:DocCoverageDimension` local name; the rest of the line
is the surface that satisfies it.

**FULL** requires all twelve of these:

<!-- doctrine-intent:full -->

```text
dimDefinition          a non-empty skos:definition stating the term AND its boundaries (what it is not)
dimLabel               a non-empty rdfs:label
dimUsageAdvice         at least one of gmeow:useWhen / gmeow:avoidWhen / gmeow:howToUse
dimExample             at least one skos:example — a one-line worked triple
dimScopeNote           at least one skos:scopeNote — an explicit boundary note
dimAlignment           the term is the subject of at least one external correspondence in mappings/
dimFixturePair         a conformance fixture AND a counter-example referencing the term (a rule with no negative fixture is not enforced)
dimCompetencyRationale a competency question exercising the term, carrying a non-blank rationale
dimWorkedInstance      a worked instance under examples/ demonstrating the term in a scene
dimLossLedgerRow       a projection-loss ledger row recorded for the term (its lossy projections)
dimLinkageCoverage     the term is a member of at least one compiled mapping set's coverage
dimRealizedState       every artifact in the slice's docs.md design-set table carries a design-only / partial / built marker
```

**MAXIMAL** requires everything in FULL, plus all seven of these:

<!-- doctrine-intent:maximal -->

```text
dimAnnotationCoat      the full advice coat present together: useWhen AND avoidWhen AND howToUse AND graphBoxRole
dimThesisSentence      the owning slice's docs.md opens with a non-empty thesis sentence
dimTranslationCoverage the term's carrier strings are present in every supported language (en / fr / cmn)
dimTestReach           the term is reached by at least one structural assertion or competency question
dimProvenanceHonesty   the rationale names no test artifact (a name-membership check, so a rationale is not silently a test reference)
dimProseQuality        the structural conjunction: a three-NOTs boundary AND a worked-triple example AND a usage coat distinct from the definition AND a rationale distinct from the label
dimLossJudgmentSound   every preservation judgment on the term's loss rows is sound-or-stronger in the logic:PreservationKind ordering
```

`dimRealizedState` sits at the FULL floor deliberately: a `≥ FULL` slice whose `docs.md`
design-set table lists an artifact with **no** realized-state marker misses the dimension,
drops below its asserted tier, and the gate bites. The marker is a gated completeness fact,
not authorial vigilance — see § 6.7's design-set table and
[`GROUNDING.md`](GROUNDING.md) § coverage duty.

**Alignment, linkage, and loss are required WHERE APPLICABLE.** Four dimensions —
`dimAlignment`, `dimLinkageCoverage`, `dimLossLedgerRow`, and `dimLossJudgmentSound` — are
**applicability-conditioned**, because GMEOW is a *superset* ontology that guarantees novel
terms with no external equivalent and native terms that are lossy projections of nothing.
A term COVERS such a dimension when `!applicable ∨ present`: the external-correspondence pair
(`dimAlignment` / `dimLinkageCoverage`) applies only to a term that **declares** an external
correspondence — a non-empty `gmeow:adoptionTarget`, or a term already carrying an alignment
/ mapping-set linkage — and the loss pair (`dimLossLedgerRow` / `dimLossJudgmentSound`)
applies only to a term that **is** a lossy-projection source (it appears in the projection-loss
ledger). A superset-native term with **no** external correspondence, or a native, non-projected
term, satisfies these by non-applicability and is **never** penalized — external linkage is an
*encouraged bonus* (more is better), never a per-term obligation. A term that DECLARES an
external correspondence but ships no documented mapping, or a lossy projection whose judgment
is weaker than sound, is `applicable ∧ ¬present` → still MISSING, a real defect the gate keeps
catching. (This changes only *when* the four dimensions count, never the FULL / MAXIMAL
intents below — they still list all four.)

**Per-term quality (∀) vs slice demonstration (∃).** When the coverage projection rolls a
per-term dimension up to the SLICE, it uses one of two quantifiers. Most dimensions are
**per-term qualities** every documented term must individually carry, so the slice covers
them **universally** (∀): it covers the dimension iff *every* applicable term covers it
(`definition`, `label`, `usageAdvice`, `example`, `scopeNote`, `alignment`,
`linkageCoverage`, `annotationCoat`, `translationCoverage`, `testReach`,
`provenanceHonesty`, `proseQuality`, `lossLedgerRow`, `lossJudgmentSound`). Three are
**slice-demonstration** dimensions — testing / documentation *practices* the slice
demonstrates, not per-term obligations — so the slice covers them **existentially** (∃):
it covers the dimension iff *at least one* applicable term demonstrates it (vacuously
covered when no term is applicable):

- **`dimFixturePair`** — "a rule with no negative fixture is not enforced" is about the
  slice demonstrating fixture discipline, not every term shipping its own pair.
- **`dimCompetencyRationale`** — a competency question documents the slice's vocabulary,
  not one term, so one rationale-carrying CQ demonstrates the practice for the slice.
- **`dimWorkedInstance`** — a worked scene under `examples/` documents the slice's
  vocabulary; the slice demonstrates it when one applicable term appears in a scene.

The per-TERM `gmeow:docCoversDimension` / `gmeow:docMissesDimension` incidence is
**unchanged** by this split — it still records every term's individual status as the
diagnostic; only the per-slice roll-up of these three flips from ∀ to ∃. The
`gmeow:DocCoverageDimension` vocabulary and the two doctrine blocks above are untouched.

> **Doctrine == vocabulary (binding contract).** The two `<!-- doctrine-intent:… -->`
> blocks above are the single prose definition of FULL and MAXIMAL, and they are pinned to
> the minted vocabulary by `crates/docs/tests/doctrine_matches_vocabulary.rs`. That test
> parses the first token of each fenced line here and asserts the FULL set and the
> FULL-plus-MAXIMAL set equal the `gmeow:maturityRequiresDimension` intents of
> `gmeow:docMaturityFull` / `gmeow:docMaturityMaximal` (via the `maturity::anchor_table()`
> twin). Editing either block without the matching change to
> `slices/core/documentation/module.ttl` (and its Rust twin) reds the build — the doctrine
> can never silently diverge from the ontology.

## 7. Alignment — correspondences at the honest rung

Alignment is authored **once, in the slice, as ergonomic frontend records** in
`mappings/equivalences.ttl`; the compiler lowers them into `logic:Correspondence` — the
ninth IR node kind — and from there generates every dialect: SSSOM, EDOAL, FnO, OWL
alignment axioms, SPARQL CONSTRUCT, and the up-lift (take1 §3, §11, §16). You never
hand-author a dialect, and you never hand-write raw IR (take1 §3.2). What you *do* own is
the honesty of each record:

- **Pick the rung you can discharge** (take1 §5). The law-spine caps what a correspondence
  may claim: isomorphism → section/retraction ("perfect subsumption") → well-behaved lens
  → lossy lens → prism/affine → bridge view. An `exactMatch` is a round-trip claim CI will
  check; if the semantics differ *at all*, say so and step down a rung — the **overclaim
  gate reds the build** on a bridge emitting equivalence or a caveated overlap emitting
  `exactMatch` (take1 §9, §15).
- **Write the caveat into the record.** The comment field is where the rung choice is
  justified — future curators inherit your reasoning or repeat your analysis. *Exhibit:
  `eqTags003` — `broaderTag` is transitive, `skos:broader` is the direct link, therefore
  `closeMatch`, therefore stated.*
- **Keep the five axes apart** (take1 §7): confidence (are *you* sure?) ≠ determinacy (is
  the *relationship* crisp?) ≠ evidence strength (what warrants it?) ≠ weight (solver
  ranking) ≠ probability (only under a declared model). Conflating "the relationship is
  fuzzy" with "I am unsure of the relationship" is the single most common alignment error.
- **Standpoint-index where curations can differ**; unindexed means *unspecified, not
  universal* (take1 §7).
- **The honest rung is often not equivalence.** When two external terms co-project onto
  one facet of your term, that is an affine correspondence onto a shared apex — the
  canonical worked case is the FOAF/schema.org contact triangle (take1 §14), and forcing
  it to `equiv` is exactly what the calculus exists to forbid.
- **Authority links are alignments, not identity** (Principle 5): Wikidata QIDs and their
  kin name correspondences; the GMEOW term stays the definition. Verify external IDs
  against the live source before committing them.

## 8. Epistemic shape — model the knowing, not just the known

Every value in GMEOW is an attributed, dated, confidence-weighted, vantage-relative
observation/claim, never ground truth (Principle 9); indeterminacy is ontic and distinct
from confidence; contested facts are coexisting standpoint-indexed claims, never
adjudicated by rank. For the slice author this means:

- **Disagreement is data.** Two agents assert conflicting things → two coexisting claims
  with vantages, both queryable. Design your relators so this is representable *before*
  someone needs it.
- **Suppression, never erasure** (Principle 10): retraction is `gmeow:displayable false`
  on the claim, enforced through projection; deleting is destroying the audit.
- **Ambiguity is co-resident readings** (`LANG-MEANING.md`): silently picking a winner
  fabricates certainty, and a projection that flattens the multiplicity must record the
  loss.
- **Time-scope with the temporal machinery** (intervals on relators, RDF-star
  `validFrom`/`validUntil` on flat shortcuts), and let the flat/reified pairing (§5)
  decide where the scope lives.

*Exhibit: `examples/contested-tagging.ttl` exercises all four — coexisting contested
taggings with confidences, a suppressed tagging, interval-scoped acts, and a homonym held
as two denotations.*

## 9. Validation and computation — author in the canon, project the surface

- **Data shapes are authored as `logic:` validation shapes** and *lowered* to the SHACL
  Core and ShEx surfaces (`LOGIC-VALIDATION.md`); derivation/aggregation is authored as
  `logic:` rules and projected to the SHACL-AF surface (`LOGIC-SHACL-AF.md`). A
  hand-authored computational SHACL construct without a `logic:` source fails the
  projection-purity gate (Principle 17's enforcement set).
- **Grounding a shape — the migration you do while touching any slice.** A legacy
  hand-authored `sh:NodeShape` in a slice's `shapes.ttl` is a second source of truth. When
  you touch a slice, migrate its authored shapes into the canon so the projector reproduces
  them, then delete the block. `gmeow-dev slice-quality slices/<g>/<s>` names every un-backed
  shape (the `slice-quality.projection.ungrounded-shape` finding of the **Shape Migration**
  axis); `gmeow-dev shape-equivalence --path slices/<g>/<s>` proves each is reproduced
  (`EQUIV`) before you delete it. Author the obligation in `module.ttl` with a **reasoner-safe**
  antecedent — never `owl:cardinality`/`owl:minCardinality`/`owl:maxCardinality`, which are
  out of the EL fragment and red `make reason-verify`:
  - at-most-one → declare the property `a owl:FunctionalProperty` (projects `sh:maxCount 1`);
  - existence → `owl:someValuesFrom <Class>` (projects `sh:class`; the `sh:minCount` is the
    design's deliberate ValidationOnly under-approximation, credited by the oracle);
  - class / datatype → `owl:allValuesFrom`; type-disjunction → `owl:unionOf`; faceted range
    (`[0,1]`) → `owl:onDatatype` + `owl:withRestrictions`; cross-node checks → a `logic:` FOL
    assertion projected through `crates/pipeline/src/stages/constraint_shapes.rs`.
  - A genuine ValidationOnly **residue** the fragment cannot express (exactly-N cardinality,
    node-level `sh:or`/`sh:and`, a bespoke cross-node `sh:sparql`) is *not* deleted: it carries
    a `logic:formalizes` back-reference naming its canonical `logic:` source, which is what the
    blanket projection-purity gate legalizes.
  - **The Shape Migration ladder and its floor.** The axis scores the *fraction* of a slice's
    authored `sh:NodeShape` / `sh:PropertyShape` blocks that carry `logic:formalizes`, and grades
    four tiers by that fraction: Grounded `0.60`, Linked `0.75`, Exemplified `0.85`, Maximal
    `0.95`. A slice with no `shapes.ttl` has nothing to migrate and scores a vacuous `1.0`. Its
    committed floor lives, like every axis floor, as a `gmeow:AxisFloorCommitment` individual
    (`gmeow:floorSlice` + `gmeow:floorAxis gmeow:axisShapeMigration` + `gmeow:floorValue`) authored
    in the `slices/core/slice-quality-rubric/module.ttl` canon — the read-only
    `generated/governance/slice-quality-axis-floors.tsv` is a generated projection of those
    individuals, never hand-edited. To commit a fresh floor at the live measured score, run
    `gmeow-dev slice-quality-seed-floors --axis axisShapeMigration` (or `--all-axes`); it emits the
    `gmeow:AxisFloorCommitment` TTL to paste into `module.ttl`, refuses to lower an already-committed
    floor, and is one-shot per axis. Raise a committed floor only by a deliberate hand-edit of the
    individual, in the same slice-local PR as the uplift that earned it, raise-only (§10). The
    **floor-coherence** invariant binds this axis to the slice's roll-up: its committed floor must
    grade to a tier at or above the slice's committed `gmeow:SliceTierFloor` (backing), and on a
    slice floored across every axis the tier floor equals the meet of the axis-implied tiers
    (tightness) — the gate reds on either breach.
  - **Why the shape-floor commitments span the whole corpus.** Of the 20 slices that ship a
    hand-authored `shapes.ttl`, only `slices/core/diagnostics` is partially grounded (≈ 1/3 of its
    blocks carry `logic:formalizes`); the other 19 score `0.0` (authored, wholly ungrounded), and the
    remaining ~60 shapeless slices score the vacuous `1.0`. Because the floor is committed for every
    slice — a `1.0` floor on a shapeless slice is a real, load-bearing ratchet that reds the moment
    an ungrounded shape is added — the shape-floor commitments blanket the corpus, not just the 20
    slices with migration debt today.
- **Converging a slice's GMN-1 coverage.** `gmeow:gmnCorrNormalToGmn`'s `logic:mnemomorphic
  true` claim is declared over ALL of GMN-0: the grounding slices are total NOW (the executed
  GMN-1 codec + round-trip gate, `crates/lang-bridge/src/gmn1_codec.rs` +
  `crates/pipeline/src/stages/gmn1_gate.rs`), and every other slice's coverage is the measured
  **GMN-1 Coverage** slice-quality axis (`axisGmn1Coverage`), gated at a committed floor with
  monotonic non-regression (never forced ascent) — the floor is a `gmeow:AxisFloorCommitment`
  individual in the `slices/core/slice-quality-rubric/module.ttl` canon, of which
  `generated/governance/slice-quality-axis-floors.tsv` is a read-only generated projection —
  and grounding is additionally hard-gated at floor `1.0`. `gmeow-dev slice-quality
  slices/<g>/<s>` names every uncovered GMN-0 quad (`slice-quality.gmn1-coverage.uncovered`);
  extend the codec's covered fragment to raise the score, then raise the slice's committed
  floor in a separate, deliberate commit once the uplift has genuinely landed.
- **Every hard rule maps to exactly one primary gate** with a named, typed failure class —
  the conformance charters (`LOGIC-CONFORMANCE.md` and its `math:`/`lang:` peers) fix the
  gate taxonomy: OWL axiom → SHACL Core → SHACL-SPARQL → source-lint → Rust validator →
  competency query → projection test, cheapest gate that owns the failure.
- **Heavy computation stays outside the logic** (Principle 12): transforms, interpolation,
  probabilistic updates are computed by engines the ontology points to by reference, never
  materialized as asserted triples.

## 10. Continuous uplift — the slice-quality lane

A slice is never *finished*; it is *held and lifted*. Beyond §9's pass/fail gates, every
slice carries a **per-axis quality profile** — one earned tier per rubric axis, a point in
`[0,1]^n` over an open axis vocabulary, each tier a `logic:QualityValue` along one quality
dimension (`slices/core/slice-quality-rubric/`). Lifting it is the standing, issue-less
**slice-quality uplift lane**: this section is its doctrine; the
[`gmeow-slice-uplift`](../.agents/skills/gmeow-slice-uplift/SKILL.md) skill is its
procedure, and where the two differ the skill and the rubric win. Keep the lane distinct
from §9's hard gates — it *measures and holds*, it never asserts a bit right or wrong
([`docs/SLICE_QA.md`](SLICE_QA.md)).

For the curation method that turns an advisor finding into a connected semantic
packet — canonical model, worked example, layered falsification, production-path
proof, natural translation, regeneration, and earned ratchet — use
[`docs/SLICE-UPGRADE.md`](SLICE-UPGRADE.md). In particular, do not treat the
advisor's term list as the specification of the uplift.

- **The ascent is a lattice meet, not a score.** The roll-up tier is the lossy meet
  projection of the profile: the UNWEIGHTED lattice meet — `min` over the per-axis tier
  ranks (`crates/slice-quality/src/prioritize.rs`). `axisWeight` orders ties only; it never
  moves a rank or a score, because that would corrupt the unweighted meet. The meet's
  witness is the **min-rank antichain** (the axes tied at the minimum rank), and the
  **capping axis** is that antichain's heaviest-leverage member — least rank, ties broken by
  weight then axis IRI. Fixing it is a NECESSARY start, never one-step-sufficient under
  ties: the roll-up rises only once EVERY tied axis clears its next threshold, so the lane
  may legitimately re-pick a slice with a fresh capping axis until the last tied axis lifts.
  Per-slice **termination is the advisory fixpoint** — `advise(slice) = ∅`, the observable
  `advice=0` column.

- **The workflow is one capping axis, one slice-local PR.** Consume the repo-wide
  prioritization (`make slice-quality`), uplift the named capping axis in the slice's
  canonical sources, ship one **slice-local** PR, ratchet the floor. The procedure — reading
  the worklist columns, applying the ranked `gmeow:axisAdviceTemplate` ahead of its per-site
  findings, landing under the bundle discipline — is the skill's contract; this guide states
  only *why* that order is forced.

- **The floors are a raise-only ratchet — now a hard gate.** The committed floors are the
  monotonicity certificate of the ascent, enforced not promised, and they are
  **ontology-resident** in the slice-quality-rubric slice's `module.ttl`: the per-slice tier
  ratchet is a `gmeow:SliceTierFloor` individual (`gmeow:floorSlice` + `gmeow:floorTier`) and each
  per-axis score ratchet is a `gmeow:AxisFloorCommitment` individual (`gmeow:floorSlice` +
  `gmeow:floorAxis` + `gmeow:floorValue`). Those individuals are the canonical source; the
  read-only TSVs `generated/governance/slice-quality-floors.tsv` and
  `generated/governance/slice-quality-axis-floors.tsv` (materialized locally by `make sync`)
  are **generated lossy projections** of them (Principle 17), for viewing only — never
  hand-edited. Both levels are strictly **raise-only** — monotonic non-regression,
  never forced ascent: a committed floor may be raised as a slice earns it (edit the individual,
  or seed a fresh one at the live score with `gmeow-dev slice-quality-seed-floors`) and is never
  bumped ahead of a real measured uplift. **LOWERING a committed floor is a hard gate failure.**
  There is no in-repo re-anchor, permit, or signal — re-baselining a floor downward is a
  **maintainer-only decision**, exercised out-of-band by authorizing the merge past the red. The gate
  (`crates/slice-quality/src/gate.rs`) reads every committed floor from the ontology and reds with
  three named verdicts — `MeasuredBelowDeclared` (the slice no longer holds the tier its manifest
  declares), `DeclaredBelowFloor` (the declaration was lowered beneath the committed floor), and
  `MeasuredBelowFloor` (a per-axis measured score fell below its committed floor, enforced for
  **every** committed axis, not just GMN-1) — while the floor-monotonicity check reds on a LOWERED
  floor value/tier and on the deletion of a still-live floor individual, and an
  axis floor whose implied tier falls below the slice's committed tier floor (or a loose tier floor
  on a fully-floored slice) reds the floor-coherence check. Land the raised floor in the SAME PR as
  the uplift that earned it; grounding is hard-gated at the `axisGmn1Coverage` floor of `1.0` (§9).

- **The contention rule: yield to the issue lanes.** The lane is a background citizen. Two
  shards are machine-enforced, one is doctrine CI cannot see. *Enforced:* floor
  monotonicity — a LOWERED floor line or the deletion of a still-live floor line reds
  `make slice-quality-gate`, and thus `make check`. *Doctrine (unenforced):* never touch a slice an in-flight branch or
  an active issue lane owns; keep every PR **slice-local**; `generated/dist/gmeow.gts` is a
  git-ignored local product regenerated by `make sync` — land bundle-touching PRs one at a
  time and re-sync after integrating main, since nothing in the gate knows which branch
  claimed which slice.

- **A sweep is never an issue.** A cross-cutting quality demand — "every slice needs X" — is
  never filed as an issue and never lands as a mega-PR. Re-scope the **sweep** into the three
  durable artifacts the lane already runs on: a quality axis that measures the deficiency
  objectively, curation docs that teach the remediation (this guide,
  [`docs/SLICE_QA.md`](SLICE_QA.md), or the axis's `gmeow:axisAdviceTemplate`), and the
  uplift skill if the loop's shape changed. Then the sweep discharges itself, one
  slice-local PR at a time, through the ordinary prioritization.

- **Every axis is an objective `[0,1]` measure** where `1.0` is *definitional* — the
  property fully holds — scored by a deterministic Rust primitive, never expert judgement and
  never calibrated toward a target (the quality-metrics doctrine). Use bounded fractions
  only: an unbounded ratio (a raw density or count) is BANNED from a tier ladder, because a
  ladder rung must be a bounded `[0,1]` fraction for the meet lattice to be well-defined.
  For example, `axisTranslationCoverage` measures the fraction of **every localizable literal**
  the slice authors — each `(term, predicate)` over the localizable predicates (labels,
  comments, definitions, scope notes, examples, pref/alt labels, notes, titles, descriptions,
  names) — that carries a non-empty translation accepted by the deterministic
  translation-integrity guard, averaged over English (authored),
  French, and Mandarin (`cmn`), reaching `1.0` iff every localizable literal is fully
  translated in both fr and cmn. Tiers rise **only by genuine uplift**; the floors above exist
  precisely so a score cannot quietly slide back after a ladder claims it — and lowering one is a
  hard gate failure only the maintainer may authorize out-of-band.

## 11. Gates, drift, and landing

- Work in a worktree (`.worktrees/<slug>/`), never the top-level checkout; build the
  native extensions before regenerating; regenerate with `make sync`, never the bare
  command (`CLAUDE.md` § working discipline).
- **Slice edits drift generated artifacts by design** — that is the pipeline working, not
  a problem. Expect: docs-model goldens on any term or annotation change; result-shapes
  and docs goldens on competency additions; SSSOM/mappings artifacts on alignment changes;
  the SPARQL corpus goldens on term additions. Re-bless deliberately, verify counts
  against real artifacts (never trust an auto-merged number), and land the regeneration in
  the same change as its cause.
- **Verify with the full `make check`**, merged into current `main` — partial gates lie by
  omission, and CI builds the merge result.
- **Own every red.** Any failure on your branch is yours to fix now, regardless of where
  it started.

## 12. Anti-patterns — named, so reviews can cite them

| Anti-pattern | Why it is wrong | What catches it |
| --- | --- | --- |
| Domain twin of a grounding-layer term | re-derives what the triad owns; splits reasoning | design review; the dependency gate |
| Subclass explosion for named values | TBox bloat; individuals were the right call | design review (§5) |
| `primary*` / `preferred*` anything | schema-enacted hierarchy | the no-preferred-rank lint (Principle 9) |
| Confidence used as probability | conflates epistemic state with measure | the axis-separation gates (take1 §7) |
| Vagueness recorded as low confidence | ontic/epistemic conflation | determinacy vocabulary (Principle 9) |
| Forced `exactMatch` / equivalence on a partial overlap | overclaim; breaks round-trip law | the overclaim gate (take1 §15) |
| String identity standing in for meaning | homonyms collapse, synonyms split | the Frege discipline (`LANG-MEANING.md`) |
| Silent disambiguation of ambiguous input | fabricates certainty | co-resident readings (§8) |
| Deleting a superseded value | destroys audit | suppression machinery (Principle 10) |
| Asserting derived or computed facts in source | second source of truth in the canon | validation; the solver boundary (P12) |
| Hand-editing a generated artifact | drift; competing source of truth | `make sync SYNC_MODE=check SYNC_OUTPUTS=generated` (Principle 7) |
| A value without its frame | uninterpretable scalar | frame shapes (Principle 11) |
| Structural test without a rationale | unmaintainable invariant | review practice (§6.6) |
| Module without a consumer | a monument, not a product | the manifest consumer field (Principle 15) |

## 13. The worked instance — reading order for the tags slice

The tags slice is small enough to read in one sitting and re-authored to this guide.
Read in this order, mapping each file to the section it demonstrates:

1. `docs.md` — the thesis and the trichotomy (§6.7, §5).
2. `manifest.ttl` — tier, dependencies, the named consumer (§6.1).
3. `module.ttl` — stereotyped classes, the flat/reified pairing, the full annotation coat
   (§4, §5, §6.2).
4. `mappings/equivalences.ttl` — honest rungs with reasoned caveats across SKOS,
   schema.org, Web Annotation, MOAT, and the authority links (§7).
5. `examples/folksonomy.ttl` then `examples/contested-tagging.ttl` — the easy scene, then
   the epistemically hard one (§6.5, §8).
6. `queries/competency/*.rq` and `tests/competency.ttl` — the questions, pinned (§3,
   §6.6).
7. `tests/structural.ttl` — the invariants, each with its why (§6.6).

## 14. The condensed checklist

Before opening the PR, every box:

- [ ] Consumer named in the manifest; dependencies exactly the computed set (P15/16)
- [ ] Every concept passed the grounding test; no domain twins of triad terms (P19)
- [ ] Every class stereotyped; relators declare `logic:mediates` (P17)
- [ ] Individuals over subclasses; flat-first with declared `pairsWith` promotion (§5)
- [ ] Every term: label, definition-with-boundaries, example, useWhen/avoidWhen/howToUse,
      isDefinedBy, graphBoxRole (P8, §6.2)
- [ ] Every quantity framed; every probability framed or absent (P11; `math:` doctrine)
- [ ] Every alignment at a dischargeable rung, caveat written, axes separate, external IDs
      verified live (take1; P5)
- [ ] Contested / suppressed / ambiguous / time-scoped cases representable and exemplified
      (P9/10, §8)
- [ ] Competency questions authored as queries and pinned as cells; structural cells carry
      rationales (§3, §6.6)
- [ ] No derived facts asserted; no hand-authored projection surfaces (P4/7/12/17)
- [ ] The tier claimed in `gmeow:sliceDocMaturity` is one the coverage genuinely earns —
      every dimension in its intent covered, realized-state markers present, the
      `asserted ⊄ earned` gate empty (§6.8)
- [ ] `make sync` landed with the change; drifted goldens re-blessed deliberately;
      full `make check` green merged into current `main` (§11)
- [ ] Capping axis uplifted where the lane names one; the raised floor individual
      (`gmeow:AxisFloorCommitment` / `gmeow:SliceTierFloor` in the rubric slice) landed in the
      same slice-local PR as the uplift, **raise-only** — lowering a floor is a hard gate
      failure only the maintainer authorizes out-of-band (§10)
