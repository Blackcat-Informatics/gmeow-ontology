<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The Slice Guide — authoring the optimal, maximal, richest slice

> **What this is.** A guidepost, not a rulebook: the operational sequence for authoring a
> GMEOW slice — or, more often, **re-authoring an existing slice** onto the grounding
> triad. Every rule here is stated once in a normative source and *cited*, never
> restated as new doctrine; where this document and a normative source disagree, the
> source wins and this guide has a bug. Normative sources, in precedence order:
> [`.goals`](../.goals) and [`CONSTITUTION.md`](../CONSTITUTION.md); the grounding design
> sets ([`slices/core/logic/design/`](../slices/core/logic/design/LOGIC.md),
> [`slices/grounding/math/design/`](../slices/grounding/math/design/MATHEMATICS.md),
> [`slices/grounding/lang/design/`](../slices/grounding/lang/design/LANG.md)); the
> correspondence calculus
> ([`docs/APPLIED_CATEGORY_THEORY/take1.md`](APPLIED_CATEGORY_THEORY/take1.md)); and the
> pipeline spine ([`docs/PIPELINE_SPINE.md`](PIPELINE_SPINE.md)).
>
> **The living example.** The `slices/core/tags/` slice is this guide's worked instance —
> small, useful, and re-authored to this recipe. Where a section says *"see the exhibit"*,
> it names a real file in that slice. Read the guide once, then read the tags slice in the
> order §12 gives, and you have seen every motion performed on real data.

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
   the first anti-pattern (§11).
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
7. **Run the gates and own every drift** (§10).

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
- **Every hard rule maps to exactly one primary gate** with a named, typed failure class —
  the conformance charters (`LOGIC-CONFORMANCE.md` and its `math:`/`lang:` peers) fix the
  gate taxonomy: OWL axiom → SHACL Core → SHACL-SPARQL → source-lint → Rust validator →
  competency query → projection test, cheapest gate that owns the failure.
- **Heavy computation stays outside the logic** (Principle 12): transforms, interpolation,
  probabilistic updates are computed by engines the ontology points to by reference, never
  materialized as asserted triples.

## 10. Gates, drift, and landing

- Work in a worktree (`.worktrees/<slug>/`), never the top-level checkout; build the
  native extensions before regenerating; regenerate with `make regenerate`, never the bare
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

## 11. Anti-patterns — named, so reviews can cite them

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
| Hand-editing a generated artifact | drift; competing source of truth | `make check-generated` (Principle 7) |
| A value without its frame | uninterpretable scalar | frame shapes (Principle 11) |
| Structural test without a rationale | unmaintainable invariant | review practice (§6.6) |
| Module without a consumer | a monument, not a product | the manifest consumer field (Principle 15) |

## 12. The worked instance — reading order for the tags slice

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

## 13. The condensed checklist

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
- [ ] `make regenerate` landed with the change; drifted goldens re-blessed deliberately;
      full `make check` green merged into current `main` (§10)
