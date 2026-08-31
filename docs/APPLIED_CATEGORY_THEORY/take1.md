<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# A Correspondence Calculus for GMEOW — `take1`

> **Genre.** A design charter and SME-breakout brief, synthesizing research
> The declarative present tense is intended
> normatively — "X is" means "a conforming realization implements X" — but nothing here is
> claimed already-built; the conformance corpus (§14) is what would establish it.
>
> **Audience.** Readers fluent in lens/optic theory, abstract interpretation, institution
> theory, and data-integration theory. GMEOW is the worked instance, not a system the theory
> is bent to fit.
>
> **Scope of `take1`.** Fix the canonical object, the law-spine, the keystone construct, the
> compiler shape, and the conformance contract. Two axes are deliberately left **open** for
> the breakout (the merge/colimit direction, §8.2; and empirical OpenEHR complement
> transparency, §13.4). Concrete real-data use cases that ground the spec follow this
> document.

---

## 1. Thesis

> **SSSOM, EDOAL, FnO, SPARQL CONSTRUCT, OWL alignment axioms, and the up-projection lift
> map are not alignment *sources*. They are *target dialects* generated from a single
> canonical object: `logic:Correspondence`, a first-class, RDF 1.2-native, law-bearing node
> kind in the `logic:` typed IR. A correspondence carries typed source/target theories,
> executable `get`/`put` legs, an algebraic kind on an ordered law-spine, its claimed laws
> with discharge status, five separated quantitative/contextual axes, FOL/SOL caveats, and a
> preservation ledger. "GMEOW perfectly subsumes vocabulary `V`" becomes a CI-checkable
> section/retraction law (`u ∘ d = id`), not a slogan — and the construct that makes lossy
> directions recoverable is the *mnemomorphism*: a correspondence whose forward map carries a
> witness of its source, so the inverse is recovery, not reconstruction.**

This is not a departure from GMEOW's doctrine; it is the completion of it. Principle 4 (one
canonical source; everything else a generated lossy projection) and Principle 17 (the logic
itself is canonical; OWL/Datalog/SHACL/gUFO are projections) already did this move for facts
and for axioms. `take1` applies the identical move to *alignment*.

---

## 2. The two inversions (the problem)

### 2.1 The first inversion is done — and stops one level too shallow

GMEOW already authors each mapping once in `dsl/mappings/` and renders SSSOM, EDOAL, FnO, and
SPARQL CONSTRUCT from it (`docs/projections.md`). That is Principle 4 applied to the mapping
layer, and it killed the four-way hand-authoring drift it was built to kill.

It stops shallow because that DSL is, by its own front-matter, *"a spec layer never reasoned
over"* — structurally a portable subset of SPARQL (a property-path algebra plus a closed
expression algebra plus a per-profile fan-out). Three defects follow, each real:

- **Morphism anonymity.** The DSL cannot say *what kind* of correspondence a mapping is. It
  models relationships as flat strings (`"="`, `"<="`, `">="`) and free IRIs. Perfect
  structural equivalence, partial overlap, and commitment-shifting bridge are indistinguishable.
- **Dimensional collapse.** It forces orthogonal axes into one `gmeow:confidence`, which
  renders as *both* the SSSOM `confidence` column *and* the EDOAL `edoal:measure`. The logic
  layer is explicit that `logic:confidence`, `logic:probability`, `logic:weight`, and
  `logic:evidenceStrength` are not interchangeable — and *determinacy* (is the target crisp or
  vague?) is a fifth, orthogonal concern the single number cannot express.
- **Decoupled asymmetry.** Down-projection is the CONSTRUCT; up-projection *was* an independent
  heuristic that re-read SSSOM and *re-derived* an inverse, audited post hoc at ~81% liftability.
  Down and up were authored apart and free to drift — the very defect the first inversion targeted,
  re-appearing one level up. (The calculus below retires that heuristic: the up-projection is now the
  derived `put` leg of the correspondence, executed natively as a projection of the same IR.)

### 2.2 Why external standards cannot be the source of truth

Each is excellent as a *dialect* and structurally unfit as the *canonical center*:

- **SSSOM** is row/cell-oriented (`subject_id`, `predicate_id`, `object_id`,
  `mapping_justification`). Perfect for curated 1:1 term links and review; it cannot natively
  hold multi-step graph transforms, parametric traversal, conditional logic, invertibility
  laws, or a proof-carrying loss ledger.
- **FnO** separates abstract function declarations from concrete implementations. A good
  *function registry*; it has no in-band mechanism for invertibility laws, type/effect rules,
  or preservation guarantees in the logical core.
- **EDOAL** expresses class expressions, property compositions, restrictions, and paths — the
  richest of the three — but is bound to a legacy RDF/XML-centric API lineage with partial
  rendering support; a powerful backend, not a compiler IR.

RDF 1.2 is the better substrate: triple terms denote propositions and reifiers denote
statements/claims/acts about them. A correspondence is exactly a *claim about structured
propositions*, made by an agent or tool, under a context, with caveats, confidence, evidence,
transforms, and a preservation contract. That is an RDF-1.2-native object, not a TSV row.

### 2.3 The second inversion

Fold correspondence into `logic:` as a node kind. The DSL's pattern/expression algebra is
*kept* — it is exactly the portable graph-query fragment we need, and `logic:PathShape`
already extends it with parametric, bounded-depth, by-name traversal SPARQL §9 lacks — but it
becomes the **body of a correspondence**, not a standalone spec. The moment correspondence is
IR, three things are free: it is **reasoned over** (composition, conflict detection, transitive
closure of `exactMatch` become inferences, not scripts); it is **content-addressed** (so it
dedups and rides compactly in `gmeow.gts`); and it is **governed by the loss ledger** (so every
lowering carries an honest preservation judgment and the overclaim gate can red the build).

---

## 3. The canonical object: `logic:Correspondence` (the ninth IR node kind)

`LOGIC-IR.md` enumerates eight node kinds (object-formula, meta-formula, constraint,
derivation-rule, query, transaction-program, action-schema, validation-shape). `take1` adds a
ninth, `logic:Correspondence`, defined *in terms of* the existing kinds:

```text
                         logic:Correspondence
                                  │
        ┌─────────────────────────┼─────────────────────────┐
        ▼                         ▼                         ▼
  Meta-formula envelope    Executable legs           Law constraints
  (relation + caveats,     (get / put as            (GetPut, PutGet,
   standpoint, axes;        transaction-programs;     section law, …;
   stays meta — never       ins/del; suppression-     status = NonEntailment
   leaks to object level)   not-erasure)              discharge state)
```

- **Meta-formula envelope.** The assertion "`V₁`-term relates to GMEOW-term thus, with this
  confidence/determinacy/standpoint/caveat" is a meta-formula — a statement about a
  relationship between propositions. SOL-flavoured caveats use the existing HiLog reification
  (quantify over reified predicate/type objects, staying first-order). The envelope **stays
  meta**: it must not leak into object-level closure (the current "never reasoned over"
  invariant survives as the IR's stratification rule for any `holds`/truth predicate).
- **Executable legs.** `get` (down) and `put` (up) are transaction-programs over Transaction
  Logic path semantics (`ins`/`del`, hypothetical execution, supersession-not-erasure).
- **Law constraints.** Claimed laws are constraint nodes whose *status* reuses the
  `NonEntailmentObligation` discharge vocabulary: `proved-in-certified-fragment` /
  `declared-unverified` / `refuted-with-witness` / `unknown-not-discharged`.

**Why a new top-level kind, not a meta-formula with an attached transaction-program.** The
content-addressed identity and preservation judgment must attach to the correspondence *as a
unit*: the loss ledger needs one preservation row per correspondence, hashing the relation,
the axes, the caveat, *and both legs* together, so it can attribute a dropped construct to the
leg that dropped it. The new kind is the join point the ledger requires — the same reasoning
that keeps `constraint` distinct from `derivation-rule`.

### 3.1 The relation lattice (not flat strings)

The `skos:*Match` match predicate / `gmeow:relation` become a typed `logic:CorrespondenceRelation` lattice:

```text
        equiv
       /     \
 subsumes   subsumedBy
       \     /
       overlaps
          │
     relatedMatch
          │
       disjoint   (negative pole)
```

The lattice lets the compiler *derive* the SSSOM predicate, the EDOAL relation symbol, and the
OWL-alignment axiom strength from one authored relation — instead of the author repeating it
three ways. "FOAF Person and schema ContactPoint both co-project onto `gmeow:contact`" becomes
two `overlaps` legs into a shared apex, not a forced `equiv` (see §13).

### 3.2 Frontend syntax versus canonical form

Correspondences *compile into* `logic:` IR and *may carry* FOL/SOL caveats and laws, but
**authors do not hand-write raw FOL.** The frontend stays ergonomic and slice-local (the
existing `gmeow:ProjectionMapping` ergonomics are a fine starting frontend); the canonical,
reasoned, law-bearing form is what the *compiler produces*. This is the standard
language/IR separation, and it is what keeps "maximal expressivity at the center" from
becoming "unusable surface" (the Ithkuil failure mode the projection doctrine exists to avoid).

### 3.3 Vocabulary (indicative)

```text
logic:Correspondence          logic:CorrespondenceLeg       logic:CorrespondenceLaw
logic:LawClaim                 logic:LawStatus               logic:Mnemomorphism
logic:CorrespondenceWitness    logic:Complement             logic:CorrespondenceCaveat
logic:CorrespondenceRelation   logic:Determinacy            logic:morphismKind
logic:Isomorphism  logic:SectionRetraction  logic:WellBehavedLens
logic:LossyLens    logic:Prism  logic:AffineCorrespondence  logic:BridgeView
logic:InstitutionMorphism      logic:ProjectionLowering     logic:PreservationClaim
logic:loadBearing              logic:flaggedResidue         logic:ConversionTarget
logic:RelationalCore           logic:PhysicalPlan           logic:semiringAnnotation
```

The umbrella term is **`Correspondence`**, deliberately not `mapping` (contaminated by SSSOM
row semantics), `morphism` (too broad; implies truth-preservation everywhere), or `lens` (one
rung, not the genus).

---

## 4. Orientation convention (settled)

A classical asymmetric lens is between a rich **source** `S` and a derived **view** `V`, with
`get : S → V` and `put : V × S → S`. **GMEOW is the source `S`; the external vocabulary is the
view `V`.** Therefore:

| Operation | Lens role | Direction |
|---|---|---|
| **down-projection** (GMEOW → external) | `get : S → V` | extract the smaller derived view |
| **up-projection** (external → GMEOW) | `put : V × S → S` | fold a (possibly fresh) view back into the rich source |

Rationale: the view should be the *smaller, derived* thing, and `put`'s job — "fold a modified
view back into the rich source" — is the only reading under which the lens laws say what we
want. Relational framing alone leaves this orientation ambiguous; `take1` fixes it. Note the
`put : V × S → S` signature takes the *old source* as well as the new view; the
**ingest-with-no-prior-state** case (`S` empty) is precisely where the view alone is
insufficient and a *witness must travel in the view* — which is the in-band complement (§13)
and the motivation for mnemomorphism (§6).

---

## 5. The ordered law-spine

Classify correspondences by **how much invertibility they can lawfully claim**, on a single
ordered spine; **each rung caps the laws a correspondence may assert** (the optic lattice fused
with the categorical notion of subobject):

| Rung | Categorical structure | Laws claimable | GMEOW reading |
|---|---|---|---|
| **Isomorphism** | iso (`get∘put = id`, `put∘get = id`) | full round-trip, both directions | `gmeow:Person ≡ schema:Person`, conf 1.0 |
| **Section / retraction** | split mono (`put∘get = id_S`; `get∘put` idempotent on `V⊕complement`) | source embeds losslessly; augmentation = `S ∖ im(get)`, carried in the complement | **"perfect subsumption" — the OpenEHR target** |
| **Well-behaved lens** | asymmetric lens | GetPut + PutGet (PutPut optional) | structured→flat downcasts with sound update |
| **Lossy lens** | lens with non-injective `get` | one direction faithful; inverse needs witness/claim/defaults | most schema.org/FOAF downcasts |
| **Prism / affine** | partial map `S → V + S` | match/build on the in-focus case only | "similar but not quite"; co-projection onto a shared component |
| **Bridge view** | comorphism that shifts commitments | *no* satisfaction-preservation claim | BFO / DOLCE / SUMO |

Two cross-cutting qualifiers:

- **`morphismKind ∈ {institutionMorphism, bridgeView}`** — the institution-theoretic split
  (Goguen–Burstall) between a satisfaction-preserving morphism and a commitment-shifting
  bridge. Already latent in the charter's gUFO (truth-preserving down-projection) vs.
  BFO/DOLCE/SUMO (bridge views) distinction; naming it lets the loss ledger *refuse* to emit
  `owl:equivalentClass` for a bridge.
- **`mnemomorphic? ∈ {yes, no}`** — whether the forward leg retains a source witness (§6).
  Orthogonal to the rung: it is the property that lets a correspondence *climb* the spine,
  because a retained witness is what discharges `put∘get = id`.

Composition can only weaken the rung, never strengthen it (§8.1).

---

## 6. Mnemomorphism — the keystone

**Definition.** A correspondence is a *mnemomorphism* (μνήμη, *memory*, + -morphism) when its
forward map `get` factors through the **graph of the correspondence** — the source-witness — so
that `put` is obtained by *projecting along the retained witness* rather than synthesizing a
plausible source. Equivalently: `get` carries, in its output, enough of its input that
`put∘get = id` holds *because nothing the retraction needs was discarded*.

**Lineage** (so the construct can be placed precisely):

- **Recursion schemes.** A *paramorphism* is primitive recursion (a fold seeing the original
  substructure); a *histomorphism* generalizes it to course-of-value recursion via a cofree
  comonad. A mnemomorphism is the alignment analogue — the "history" is the source graph,
  carried as a cofree-style annotation. *(This is the correct home of the project's original
  "paramorphism" reach; the recursion-scheme name is dropped, the idea is kept and renamed.)*
- **Bidirectional transformation.** A state-based lens upgraded with a **trace** (the seam
  where state-based BX meets delta/edit lenses); the trace is the witness.
- **Databases.** The witness is **provenance** (semiring provenance); `put` is view-update
  resolved by provenance, not guesswork — which is Principle 14 (every output an attributed,
  evidenced claim) turned into a lens law.
- **MDE.** The witness is the TGG **correspondence graph** — the middle node that makes
  forward/backward derivable from one rule set.

**Why it is the keystone.** It dissolves the status-quo defect (§2.1) that `put` is a separate
heuristic: *if `get` is mnemomorphic, `put` is not authored — it is the projection along the
witness, derived and law-bearing by construction.* Correspondences that are **not**
mnemomorphic (amnesic) fall to the lossy-lens/prism rungs and need a co-authored
put-with-claim with explicit "minted-with-claim" provenance. So mnemomorphism is the precise
dividing line between *subsumption* (recoverable) and *approximation* (reconstructed), and the
loss ledger reports it per correspondence — that single bit predicts the rung.

### 6.1 Settled: backwards-execution is a candidate preimage, not a lawful `put`

A relational predicate `lens(S, V)` run backward — querying `lens(?S, knownView)` — yields a
**candidate preimage**: *a* source consistent with the view, not *the* lawful `put`. It carries
no GetPut/PutGet guarantee, no loss tracking, no provenance, and silently fails on non-injective
`get`. It is exactly the *amnesic* case. **Naive backward-execution is therefore a named
anti-pattern, not the runtime architecture.** Lawful `put` comes from exactly two sources:

1. a **mnemomorphic witness** (derived, law-bearing — §6); or
2. a **co-authored put-with-claim** under *explicit* mode declarations, tabling/cycle policy,
   minting policy, and a declared law status.

A relational backward-execution *capability* — unification with SLD-style resolution and
backtracking, whether served by GMEOW's own logic engine or delegated to an embedded
Prolog-class engine only where needed — is a fine *execution substrate*; it is not a
*correctness story*. The Turing-complete substrate's termination hazard (a `put` that updates
the source, re-triggering view recomputation, cascading) is contained by mandatory tabling or
explicit cycle detection in the generated IR — a runtime obligation, never an optional
convenience.

---

## 7. Five axes, kept apart (+ standpoint)

A correspondence carries each separately, because they answer different questions and the
single `gmeow:confidence` destroys the distinctions:

- `logic:confidence` — the curator's epistemic confidence that the alignment is *correct*.
- `logic:evidenceStrength` — provenance-derived warrant (manual curation / lexical overlap /
  structural match / LLM suggestion).
- `logic:weight` — solver ranking, used when *competing* correspondences exist for one source.
- `logic:probability` — used **only** under a declared dependency model; most cells carry none.
- `logic:Determinacy` — whether the *target relationship* is ontically crisp or vague.
  "`foaf:Person` is *similar to* `schema:ContactPoint` but not quite" is `determinacy = vague`
  - `class = affine`, **not** a low-confidence equivalence. Conflating "the relationship is
  fuzzy" with "I am unsure of the relationship" is the single most common alignment error.

Every correspondence is **standpoint-indexed** (`gmeow:accordingTo`). The same pair may be
`iso` under one standpoint and `affine` under another, coexisting and contested. An *unindexed*
correspondence holds in `gmeow:unspecifiedStandpoint` — **unspecified, not universal** — which kills
the silent-universality bug where a curated alignment is applied where it was never validated.

---

## 8. Composition and merge

### 8.1 Composition (sequential: `C₁ ∘ C₂`, e.g. FOAF → GMEOW → schema)

Each axis composes in its own algebra — which is *why* they must be kept apart:

- **Class** — optic-lattice join, monotone-downward (prism ∘ lens = affine; iso ∘ X = X's
  class). Composition only weakens the rung.
- **Laws** — well-behavedness is preserved by lens composition, **but** `unknown` is absorbing:
  `proved` ∘ `declared` = `declared`. Weakest status dominates.
- **`confidence`** — a t-norm, default product (independence made *explicit and declared*,
  never assumed); correlated chains declare min or Łukasiewicz as data.
- **`evidenceStrength`** — weakest-link / min.
- **`weight`** — solver-additive (log-weights).
- **`probability`** — composes only under a declared cross-chain dependency model; else the
  composite carries none and the field is `not-evaluated`.
- **Loss** — Galois-connection composition: under∘under = under, over∘over = over, under∘over =
  incomparable / `unsupported` for exact-answer classes; the `unsupported` construct-sets union.

All computed *by `logic:` rules over correspondence nodes* — dogfooded and conformance-checked,
not buried in compiler arithmetic.

### 8.2 Merge (the colimit/pushout direction — **open axis, carried forward**)

Composition is sequential. *Merging* is different: combining incoming data from FOAF **and**
schema (and OpenEHR…) into GMEOW simultaneously is a **colimit/pushout in the category of
theories** — gluing multiple sources along the shared GMEOW apex without collapsing their
distinct, possibly contested, contexts. None of the six commentaries developed this beyond a
mention; `take1` names it as a first-class axis and leaves its formalization to the breakout.
The constraint it must respect: a pushout that would force `owl:sameAs`-style collapse of
standpoint-indexed claims is *ill-formed* (Principle 5/9). The merge is a colimit in a category
whose objects carry standpoint indices, so the glue preserves contested coexistence.

---

## 9. Reuse: the preservation machinery *is* the lens-law framework

The highest-leverage claim in the design: **do not build a parallel loss system.** GMEOW's
existing preservation machinery is the lens-law / abstract-interpretation framework in
entailment dress.

| Lens / abstract-interpretation concept | Existing `logic:` machinery |
|---|---|
| `get∘put = id` on the preserved fragment | `ExactPreservation` + the Common-Logic round-trip faithfulness gate |
| under-approximation `α(γ(a)) ⊑ a` | `logic:SoundUnderApproximation` |
| over-approximation `c ≤ γ(α(c))` | `logic:CompleteOverApproximation` |
| law claimed but not machine-verified | `NonEntailmentObligation` discharge status |
| polarities co-holding (lossy *and* inconsistency-faithful) | "preservation polarities are not mutually exclusive" |
| law-status indexed by query class / contract / constructs | preservation claim indexed by query class, contract, construct set |
| round-trip is a decidable check, not equivalence search | content-addressed canonical-IR identity (graph-iso check) |

`logic:LawClaim.status` *reuses these exact individuals.* A correspondence proves PutGet the
way a projection proves `ExactPreservation`: a conformance case round-trips a witness through
`get` then `put` and checks canonical-IR identity. The conformance corpus gains a
`Correspondence` category that generalizes the Common-Logic round-trip gate from "CL
emit/re-ingest" to "any correspondence's get/put." The **overclaim gate** now fires for
alignment too: marking a caveated overlap as `sssom exactMatch`, or a bridge view as
`institutionMorphism`, becomes a *build failure* — strictly stronger than today's
`projection_lint` warning. "Perfectly subsume" becomes a claim CI can refuse.

The three current cross-layer invariants collapse into this: `fno-type` becomes the FnO
back-end's soundness check; `spec-drift` *disappears* because EDOAL and SPARQL now lower from
the *same* lens leg and cannot drift, exactly as OWL-DL and OWL-EL cannot drift from one
`LogicProgram`.

---

## 10. Compiler architecture

The calculus is a compiler, structured MLIR-style (dialects, typed ops, verifier passes,
canonicalization, progressive lowering):

```text
frontend syntax (slice-local, ergonomic)
  → logic:Correspondence IR (typed; content-addressed; alpha/graph-normalized)
  → verifier passes (type/effect checks; rung/law consistency; stratification)
  → e-graph normalizer + optimizer (equality saturation)
  → lowering passes
  → target emitters
```

- **Verifier passes** check source/target class & property compatibility, cardinality,
  open/closed-world reading, total vs. partial, injective/collapsing behaviour, datatype and
  unit/frame conversion safety, disclosure/suppression effects, the four-axes separation, and
  the resource/termination profile — *before* any target is emitted.
- **E-graph / equality saturation** (the `egg` lineage) normalizes equivalent path
  expressions and *selects the cheapest plan that provably preserves the declared semantics*
  (e.g. `schema:contactPoint/telephone` vs. `inverse(contactFor)/telephoneLiteral`). When
  equivalence *cannot* be proven, it forces a preservation downgrade rather than silently
  emitting a lossy target ("unsupported carried and flagged").
- **Theory.** The `get`/`put` derivation draws on **functorial data migration** (the Δ/Σ/Π
  adjoint triple for schema mappings) and **GLAV data integration + the chase** (GAV = `get`;
  LAV = `put` by query rewriting; GLAV = the bidirectional correspondence; chase = the
  universal-solution lift, with weak/joint-acyclicity giving termination certification reused
  from the existing chase machinery). **Certain answers** give the honest semantics of querying
  *through* a lossy correspondence (skeptical vs. credulous), mapping onto the five-valued
  reasoning result.
- **Engine of record (engine-agnostic by design).** The executor is GMEOW's own logic engine
  (Rust-native; `crates/logic`, `crates/pipeline`, `crates/validate`). It *subsumes* a bundle
  of capabilities — unification, SLD-style backward resolution, backtracking, fixpoint/chase,
  tabling — entirely in the native physical core. Nothing in the correspondence calculus is
  foundational to any external engine, and the engine may grow without touching the calculus.
  Python remains a surface binding, not the
  executor of record; no new Python is added for this work (per `.goals`).

### 10.1 Compiler-IR lineage — MLIR architecture, LLVM mechanisms (adopt / skip)

The IR shape is the one decision that constrains every later option, so its lineage is fixed
explicitly. The architecture is **MLIR's, not LLVM IR's**: borrow MLIR's multi-level scaffolding
(dialects, per-op verifiers, progressive lowering); never LLVM IR's substrate. **Lowering
`logic:` to LLVM IR is a category error** — LLVM IR is an imperative SSA IR for scalar+memory
computation and cannot represent open-world entailment, paraconsistency, or modal scope. Patterns
and tooling cross over; the substrate does not. Three mechanisms are adopted as **IR commitments**
— each is cheap to honour now and expensive to retrofit:

1. **Lowering *is* legalization (MLIR dialect conversion).** A lowering to a target dialect is a
   legalization against a `ConversionTarget` declaring what the target can express — statically,
   or *dynamically legal* iff a construct falls in the target's certified fragment. MLIR's
   **partial conversion** (leave the illegal construct in place, flagged) *is* the "unsupported
   carried and flagged, never dropped" rule (§3). **Commitment:** every lowering is a total
   function into `⟨ legal output ⊕ flagged residue ⟩`; the loss ledger (§11) *is* the residue set.
   This is the enforcement engine the lowerings table assumes.

2. **Every annotation is typed *load-bearing* or *droppable* (operand bundles vs `!metadata`).**
   LLVM separates *droppable* `!metadata` (correctness must never depend on it; dropping only
   pessimizes) from *load-bearing* **operand bundles** (the optimizer must preserve them). GMEOW
   must draw the same line in the IR: a display hint / `scopeNote` is **droppable**; the in-band
   complement (§13.2) and the four axes are **load-bearing** — `u` needs them for `u∘d = id`.
   **Commitment:** each annotation / axis / complement node carries a `logic:loadBearing` bit; a
   lowering may drop a droppable annotation silently, but must either preserve a load-bearing one
   or record its loss. *Without this bit the section/retraction rung cannot be verified* — which is
   why it must be in the IR from the start.

3. **Validate transforms, don't trust them (`debugify` / Alive2).** LLVM does not trust its
   passes: `debugify` tests that debug info (the source witness) survives each pass, and Alive2
   SMT-proves each optimization is a **refinement** (target more-defined-or-equal — *not*
   equality). GMEOW's conformance gates (§15) are the same methodology: the Round-trip and
   Mnemomorphism gates are GMEOW's `debugify` (the witness survives the lowering); the Overclaim
   gate is GMEOW's Alive2 (the lowering matched its *declared* preservation polarity — a refinement
   claim, not equality). **Commitment:** the IR is content-addressed and round-trippable by
   construction (§3), so these gates are decidable graph-iso checks, not semantic-equivalence
   search.

Two further patterns adopted at no IR-shaping cost: **analysis vs transform passes with explicit
invalidation** (the preservation-analyzer / law-checker are *analyses*; lowerings are
*transforms*; the build-pipeline executor's artifact-level incremental rebuild *is* analysis invalidation), and
**declarative op / rewrite description** (TableGen / PDL-DRR → the closed operator algebra and
rewrite rules authored as data — already GMEOW's dogfooded-generator shape).

Deliberately **not** borrowed: LLVM IR's instruction set, its CFG/SSA/φ form, and above all its
*exactness aspiration*. LLVM has no loss ledger because a lossy compile is a **bug**; GMEOW
*declares* loss as a first-class typed result. The one LLVM idea in that spirit is the poison/UB
**refinement** relation — a one-directional preservation, useful as intuition for
under/over-approximation, not a model to import.

### 10.2 Execution & optimization — the physical engine

Routing every query to external whole-program substrates was a **bootstrap, not an architecture**: two
black-box whole-program engines cannot be planned across, specialized, parallelized, or made
incremental, and each boundary pays re-serialization. The design extends §10.1's progressive lowering *downward* — past the
projection lowerings (§11) — to a **single native physical engine** in Rust, with external engines
demoted to conformance evidence while fragments are promoted
(§10). The substrate is subsumed **fragment-by-fragment, oracle-gated** by the differential ledger
— the same retirement discipline already applied to the Python oracle.

That subsumption is now **realized**: native decides the production bundle. Per-fragment
`native ⊒ corpus` evidence is enforced by the conformance harness and divergence ledger, keyed to
`native_contract_hash()` so captured results are tied to the exact native core they certify. The
external execution substrates and their runtime parity lanes are deleted.

The execution lowering stack (the physical continuation of §10):

```text
logic: IR (full-FOL, facets)
  → ReasoningContract / fragment analysis   — route to the WEAKEST sufficient strategy
  → relational-core dialect (Datalog± + stratified ¬ + aggregation + existentials)
  → physical plan (join order · WCOJ · index selection · magic-sets · semiring annotation)
  → native core: semi-naive + alternating fixpoint, tabling, incremental, compiled
```

Seven levers, prioritized — we *lower onto* battle-tested Rust dataflow cores, not rebuild
everything:

1. **One relational core, not two engines.** Datalog *is* relational algebra + fixpoint (Soufflé,
   RecStep). Implement the evaluation primitives natively over a shared columnar/indexed store;
   forward (Datalog) and backward (tabled) both reduce to it.
2. **Magic-sets / demand transformation** — the move that unifies forward and backward demand: a
   goal-directed query rewrites to a bottom-up program computing only demand-relevant facts, so one
   bottom-up core serves both directions (SLG / subsumptive tabling is the dual).
3. **Incremental maintenance — the biggest long-term lever.** Differential Dataflow / DBSP give
   incremental *recursive* evaluation proportional to the *change*; Rust-native and proven
   (`differential-dataflow`/`timely`, **DDlog**, **DBSP/Feldera**). This is the foundation for
   "re-reason after an edit" and for the build-pipeline executor's artifact-level incrementality.
4. **Worst-case-optimal joins** (Leapfrog Triejoin / free-join) for cyclic graph patterns
   (triangles, paths) where binary joins are asymptotically bad — i.e. exactly ontology reasoning.
5. **Provenance semirings — the GMEOW unification.** Annotate tuples with a semiring element and
   evaluate *once*, getting each answer tagged by world/standpoint and the four axes, instead of N
   passes over a context cross-product. **The semiring *is* the §8.1 axis algebra** — execution and
   axis-composition become one computation.
6. **Compile, don't interpret** (Soufflé): lower contract-specialized queries to specialized Rust /
   Cranelift JIT over purpose-built indices, keyed by the content-addressed IR hash so plans *and*
   derived relations cache and share. `egglog` (e-graph + Datalog, Rust) unifies the §10 plan
   e-graph with the evaluation core.
7. **Fragment-routing = decidability-as-projection, for performance.** Statically detect the
   weakest fragment that suffices and run the fast path; only the genuine residue pays for the heavy
   machinery. **The `ReasoningContract` is the physical-plan selector**, not just the soundness
   selector — one typed object, two roles.

These fit GMEOW by construction: the **semiring = the four-axis algebra** (§7–8); the
**content-addressed IR = the cache / incremental key** (§3); the **correspondence calculus rides
the same engine** (a `get` is a join plan; a `put`/chase is semi-naive fixpoint with existentials;
a lens-law check is run-get-then-put + hash-compare); and the **contract drives both correctness
and the plan**.

**IR commitment (add now — the spine of all later execution):** a **relational-core dialect** — a
first-class Datalog±-with-stratified-negation sub-language inside the IR — as the lowering waist
between `logic:` and the physical engine. It is cheap to define now and impossible to retrofit
cleanly later: every execution strategy, the incremental layer, and the semiring annotation all
target *it*. The other two prerequisites already hold — the four axes are semiring-annotatable
first-class structure (§7), and the IR is content-addressed (§3).

**Staging & honest risk.** Native subsumption is fragment-by-fragment and evidence-gated. Both
external substrates are retired. The forward-chase OWL profiles (EL, RL, DL Horn) are native and
in production; the remaining hard parts stay ahead of them. Name them now,
do not promise them early:
**well-founded / stable-model semantics *incrementally*** (non-monotonic + differential is
research-frontier — monotone recursion and stratified negation are tractable, full WFS-incremental
is not); **existential-rule chase with termination *and* incrementality together**; and the
**paraconsistent / modal facets**, which stay heavy-path fallbacks longest and must be flagged
non-incremental in the perf ledger.

---

## 11. The lowerings (target dialects)

Each existing artifact becomes a registered lowering with its own preservation claim, in the
*same* `projection-report.ttl` loss ledger that governs OWL/Datalog/gUFO:

| Target | Lowers from | Typical preservation |
|---|---|---|
| **SSSOM** | the meta-formula's 1:1 lattice band (equiv/closeMatch) | exact for the equiv band; else under-approx; drops caveat/laws/legs |
| **EDOAL** | the `get` leg + relation lattice + measure (from confidence) | under-approx; drops SOL caveats, the `put` leg, world/standpoint scope |
| **FnO** | transform functions referenced by `get` | exact for signatures; validation-only for entailment |
| **SPARQL CONSTRUCT** | `get` compiled to the closed algebra | the faithful executable down-projection; profile losses are explicit `actual_drops` |
| **up-lift** (replaces the heuristic) | the `put` leg — *derived* for mnemomorphic cells | complete-over for invertible cells; validation-only (mint-with-claim) otherwise; `unsupported` where `get` is non-injective and no witness exists |
| **OWL alignment axioms** *(new)* | the meta-formula relation, DL-expressible band | under-approx; `unsupported` for caveated overlaps and bridges |
| **OAEI / Alignment-API XML** *(new)* | the whole correspondence set | under-approx; carries `align:measure` where SSSOM/OWL drop confidence |

---

## 12. Riding along in `gmeow.gts`

The compiled correspondence capsule rides in the existing **`gmeow:graph/alignments`** named
graph — but its *content* changes: from generated SSSOM/EDOAL metadata to the **canonical,
content-addressed `logic:Correspondence` IR** (the authored source). SSSOM/EDOAL/FnO/CONSTRUCT
become emitted-on-demand views that no longer need to ship inside the bundle. This is a net
*shrink*: the authored correspondences plus their once-stored, content-addressed `get`/`put`
legs (shared transforms deduped, not copied four ways) replace dozens of serialized artifacts.

A repo-free consumer loads `gmeow:graph/alignments`, then:

```sh
gmeow ingest  --from foaf     input.ttl          # put: external → GMEOW (lift via witness/claim)
gmeow project --from gmeow --to schema.org data  # get: GMEOW → external view
gmeow project --from gmeow --to openehr    data  # get → OpenEHR-valid file + in-band complement
gmeow emit    --target sssom | edoal | fno | oaei  # render a dialect on demand
gmeow explain-projection --correspondence <hash> # the loss ledger + law statuses for one cell
```

`gmeow project` returns a *typed result* (input validity, evaluation status, completeness,
preservation, information state, loss-ledger pointer, artifact hashes), not merely files —
reusing the `ReasoningResult` structure. The up-projection is now the `put` leg of the same
correspondence, not a separate SSSOM heuristic; the old "81% liftable" number becomes a derived
statistic of the loss ledger (`count(PutGet holds) / count(correspondences)`).

---

## 13. Perfect subsumption and the OpenEHR replacement target

"Subsume, extend, enhance" has, for OpenEHR, an exact categorical meaning and an exact test.
And OpenEHR is not one model but a **six-layer standard**; GMEOW subsumes each layer with the
*same* projection doctrine — verified against real GECCO data (the `Genkidata` corpus:
`blood_pressure.json` + `Blutdruck.opt`) and the openEHR PROC 1.6.0 process examples:

| OpenEHR layer | Artifact | GMEOW canonical core | Mechanism |
|---|---|---|---|
| Data (RM) | `DV_QUANTITY` in `blood_pressure.json` | `logic:` foundation; frame-relative quantity (P11) | correspondence / lens (§13.3) |
| Constraints (AM/ADL) | `C_DV_QUANTITY` in `Blutdruck.opt` | `logic:` validation-shapes (full-FOL ⊃ ADL) | ADL → FOL lowering |
| Process (PROC/TP-VML) | Task Plan / Work Plan | `logic:Plan` | correspondence; DAG-profile by-reference target (§13.5) |
| Decision logic (DLM) | input / tracked-state / rules | `logic:` derivation rules + observation-conditioned policies | rule projection |
| Query (AQL) | DLM-variable ↔ EHR-path bindings | `logic:` query / SPARQL projection | query lowering |
| Terminology | `DV_CODED_TEXT` (SNOMED/LOINC) | identity-by-reference (P5) | nested correspondence |

Layers 1–2 are the **data axis** (§13.1–13.4); layer 3 the **process axis** (§13.5); layers
4–6 are recorded as further lowerings/nested correspondences. The foundational refinements that
make these round-trips precise are adopted by reference from **YAMATO** (Mizoguchi 2010) — a
*bridge view* (P5), never imported, authored canonically in `logic:` (P17) — and noted inline.

### 13.1 The replacement requirement

Down-projecting GMEOW produces an artifact `d(g) = ⟨ openEHR_file ⊕ gmeow_additions ⟩` such
that the `openEHR_file` slice validates **and** the artifact back-transforms losslessly. This
is a **section–retraction pair with an in-band complement**, with three laws:

1. **Validation.** `π_openEHR(d(g)) ⊨ RM ∧ AM` — the OpenEHR slice validates standalone against
   the Reference Model and the relevant archetypes/Operational Templates (Archie, EHRbase, the
   reference ITS), unmodified.
2. **Lossless subsumption.** `u ∘ d = id_GMEOW` — GMEOW is the section; `u` recovers it exactly.
3. **Store-replacement duality.** For a faithful OpenEHR instance `o`, `d(u(o)) ≅ o` under
   OpenEHR canonical equality — GMEOW ingests and re-emits indistinguishably.

### 13.2 The in-band complement

The `gmeow_additions` are the **complement object of a symmetric lens (Hofmann–Pierce–Wagner)
— materialized in-band** rather than in a side channel. That is mnemomorphism at the
serialization layer: the witness travels inside the artifact, which is exactly what makes the
ingest-with-no-prior-state `put` lawful (§4). The validation law forces the complement to
occupy **validation-transparent** carriers — `ARCHETYPED.archetype_details`, `FEEDER_AUDIT`,
`ITEM.*` tags, `other_details`, `DV_PARSABLE`, `LINK` — or a content-hash-bound RDF sidecar the
validator never sees but `u` always does. The complement carries `S ∖ im(get)`: standpoint
indices, the four axes, RDF-1.2 reifier identities, multi-vantage claims.

### 13.3 Structural alignments (why subsumption is honest, not asserted)

- **`DV_QUANTITY` ↔ frame-relative quantity** (Principle 11: value+unit; a value without its
  frame is ill-formed) — mnemomorphic, section rung. Sharpened by the **YAMATO unit-independent
  true-quantity** refinement: the *pressure* exists independent of `mm[Hg]`; the unit belongs to
  the *measurement*, not the quantity. So `DV_QUANTITY {magnitude, units}` decomposes into a
  frame-independent magnitude + a `value+unit+frame` measurement — exactly the seam P11 names.
- **`OBSERVATION`/`HISTORY` of dated values ↔ a persistent `logic:Quality`.** YAMATO's
  most-evaluated refinement: "the patient's systolic pressure" is **one** enduring quality whose
  dated results change; openEHR's `data.events[]` HISTORY is literally a time-series of results of
  that one quality. GMEOW reifies the persistent `logic:Quality` (inhering in the patient) with
  the dated `gmeow:Observation`s attached — so "how did this value change over time?" is a
  first-class query, and the HISTORY round-trips as the quality's result-series.
- **OPT `property` ↔ the YAMATO generic-quality→quality-role ladder.** `Blutdruck.opt`'s
  `C_DV_QUANTITY` declares `property = openehr::pressure` (the **generic quality**), while the
  archetype node `at0004`/"Systolisch" is the **quality-role** that generic quality plays in the
  arterial-blood-pressure bearer-context. This ladder *is* Principle 11 in role terms
  (`logic:Role`), and is exactly how the leaf value acquires meaning from its path (§6).
- **`DV_CODED_TEXT` ↔ coded-value-by-reference** (Principle 5: SNOMED/LOINC by `skos:exactMatch`
  - authority anchor; no identity collapse).
- **`ENTRY`/`OBSERVATION`/`COMPOSITION` ↔ the attributed-claim spine** (Principles 9, 14:
  committer/time/audit ↔ `ai-package` Source→Chunk→EvidenceSpan→Claim).
- **ADL ↔ FOL validation-shapes.** Archetypes constrain the RM (two-level modelling); `logic:`
  does multi-level modelling natively (HiLog, `Kind`/`Category`, powertype instantiation), and
  full-FOL strictly exceeds ADL constraint expressivity — so archetype rules become first-class
  axioms, not out-of-band text checks. *This is the "augment": the part outside `im(get)`.*

### 13.4 Genuine open problems (breakout)

1. **In-band complement transparency** — which extension points carry the complement without
   perturbing validation across the validator zoo; embedded vs. sidecar. *Falsifiable: either a
   validation-transparent complement exists for every field in `S ∖ im(get)`, or the exact
   field that forces a validation-vs-losslessness choice is the nameable boundary of GMEOW's
   subsumption.*
2. **Retraction-to-ADL fidelity** — is regenerating idiomatic ADL2/OPT in scope for "replace,"
   or is instance-data losslessness the bar?
3. **Nested terminology binding** — SNOMED/LOINC value-set bindings are correspondences inside
   the OpenEHR correspondence (correspondence-valued caveats); needs a clean recursion story.
4. **Witness-cost** — quantify the complement's footprint; confirm content-addressed sharing
   keeps `gmeow.gts` compact enough to "ride along."

### 13.5 The process axis: openEHR PROC ↔ `logic:Plan`

OpenEHR's Task-Planning (PROC 1.6.0) is a *process* model, and it is a subsumption target for
the canonical process model — the full `logic:Plan` Transaction-Logic superset, its certified
acyclic DAG profile, and the dogfooded build-pipeline executor. The process model and the
correspondence calculus are the *same* projection doctrine applied to two cores (alignment and
process); they meet here, because the by-reference DAG projection *already* generates workflow
surfaces "Airflow/CWL/WDL/Temporal added by reference (SSSOM/EDOAL/FnO)". **The correspondence
calculus is the DAG profile's by-reference projection mechanism, and openEHR Task Planning is
one more by-reference projection target of `logic:Plan`.** The construct map is near-1:1:

| openEHR PROC | `logic:` canonical |
|---|---|
| Task Plan / Work Plan | `logic:Plan` (serial `⊗`, guards, branching, concurrency, loops, fallback) |
| hand-off / system-request / manual-notification Task | `logic:ActionSchema` (precondition / effect / resource / capability / observation / compensation) |
| decision / branch point | guarded branching |
| subplan | sub-`logic:Plan` |
| DLM `currency` / `time_window` | the typed context algebra (valid / asserted time) |
| DLM terminology `definitions` | nested terminology correspondences (§13.4-Q3) |
| precondition / trigger | action-schema precondition + observation |

**The prescriptive↔descriptive seam is a lens with a declared loss — and YAMATO names its
poles, now formalized in `logic:`.** PROC is explicit that the plan *guides but does not dictate*
reality: the EHR records outcomes and "manual notifications close gaps." This is exactly the
**action(open, on-going) vs event(closed, unitary)** distinction — *arrive* (the prescriptive
plan, an on-going action) vs *arrival* (the descriptive record, a closed unitary event) — typed
canonically as a value (Principle 9) via `logic:occurrentBoundary` over `logic:Open` /
`logic:Closed`, and it is the
**plan ⟂ execution** de-conflation already specified in the canonical process model ("path vs. intention vs. causation:
connected, never identified") and in the canonical `logic:` plan spine. As a correspondence it is a
**lossy lens**: the descriptive record is a reality-perturbed realization of the prescriptive
plan, and it is **not** mnemomorphic in general — events occur outside the engine, so the plan is
not fully recoverable from the record. Whether a given record *is* recoverable is the
mnemomorphism question: does it carry enough witness (plan reference + action-schema typing,
openEHR's Instruction-State-Machine linkage) to recover the plan? Where it does not, that gap is
an honest loss-ledger entry, never a failure.

Two further YAMATO process refinements — now **formalized canonically in `logic:`** (Principle 17;
by-reference bridge to YAMATO terms) — sharpen the plan model: **causal parts vs temporal parts**
(`logic:causalPartOf` ⊆ `logic:temporalPartOf`, transitive) — a plan's hand-off/callback edges are
*causal* dependencies, carried at the domain level by `gmeow:causalPartOf` distinct from the
temporal nesting of `gmeow:hasSubEvent`, which is what the build-pipeline executor's typed `DataFlow` capture realizes; and
**process ≠ event** (a plan prescribes over dissective, changeable processes; the record is
unitary, immutable events), the change-asymmetry — enforced by `logic:OccurrentChangeAsymmetry`
over `logic:Closed` + `logic:Fluent` — that keeps a revisable plan distinct from a completed
occurrence.

---

## 14. Worked example: the FOAF / schema.org / `gmeow:contact` affine triangle

`foaf:Person` (an *agent*) and `schema:ContactPoint` (a *contact channel*) are **not** peers,
**not** subsets, **not** equivalent. They **co-project onto the contact-bearing facet** of
`gmeow:contact`. The honest canonical object is an affine correspondence, not a forced equality:

```turtle
:gmeowContactCorrespondence a logic:Correspondence ;
    logic:correspondenceClass logic:AffineCorrespondence ;
    logic:morphismKind        logic:institutionMorphism ;
    logic:mediatingTheory     gmeow: ;
    logic:sourceTheory        foaf: ;          # see orientation note below
    logic:targetTheory        schema: ;
    logic:relation            logic:overlaps ;
    logic:determinacy         logic:Vague ;
    logic:confidence          "0.72"^^xsd:decimal ;
    logic:evidenceStrength    :manualSemanticReview ;
    logic:caveat [ a logic:CorrespondenceCaveat ;
        skos:definition "foaf:Person denotes an agent/person; schema:ContactPoint denotes a
        contact channel/role. Both project through the contact-bearing facet of gmeow:contact;
        they are not equivalent and neither subsumes the other."@en ] ;
    logic:hasLeg :foafPersonToGmeowContactFacet ;     # an affine optic onto the apex
    logic:hasLeg :schemaContactPointToGmeowContactFacet .
```

Generated views: SSSOM → `skos:relatedMatch` (with the caveat as `comment`), **not**
`exactMatch`; EDOAL → a structural cell over the specific contact-bearing paths; FnO → only the
value transforms actually needed; SPARQL/native → executable projection with suppression/context
guards; docs → a human-readable warning that these are not entity-equivalent. The overclaim
gate *forbids* emitting `owl:equivalentClass` here. (Orientation note: the canonical
correspondences are GMEOW-as-source; this triangle is stated source/target-relative to the two
external vocabularies, mediated by GMEOW — the apex `gmeow:contact` is `S` for both legs.)

---

## 15. Conformance

A new conformance category, `conformance/correspondence/`, with cases declaring an input source
graph, an input target graph/expected view, the `logic:Correspondence` under test, expected
lowerings, expected law statuses, expected preservation claims, expected loss-ledger rows, and
expected gap report. The decisive gates:

1. **Law gate** — a correspondence may not claim a law it fails.
2. **Overclaim gate** — a bridge view cannot emit equivalence; a caveated overlap cannot emit
   `exactMatch`; a claimed rung must be satisfiable by the lowered legs.
3. **Round-trip gate** — `iso` and `section/retraction` claims execute their complete declared
   query-class recovery cases and reproduce the source atom set.
4. **Mnemomorphism gate** — if a correspondence claims recoverability, the witness/complement
   must *actually* recover the source.
5. **Composition gate** — composing correspondences may only preserve or weaken claims.
6. **Exit-gate loss ledger** — every lowering declares its preservation polarity (exact /
   under / over / validation-only / inconsistency-preserving / inconsistency-reflecting /
   unsupported); none is silent.

Cases run in isolated test graphs, in parallel, with no external runtime — preserving the
Docker-free authoritative path.

---

## 16. Migration (equivalence-before-deletion)

Per Principle 6 (greenfield, no backwards-compat) tempered by Principle 7 (verified by
construction): the existing `dsl/mappings/` becomes a **frontend syntax** into
`logic:Correspondence` first. A one-shot transpiler compiles each native alignment cell
(a reified `skos:*Match` statement) / `gmeow:ProjectionMapping` cell into a correspondence;
the new pipeline must regenerate the
committed SSSOM/EDOAL/FnO/CONSTRUCT **byte- or graph-isomorphically** (the existing
the strict `sync` mappings golden set is the oracle); only then are the old DSL, emitters, and
`projection_lint`/`alignment_lint` deleted. Real files touched: `slices/grounding/logic/module.ttl`
(or a new `slices/core/correspondence/` slice — see open question below);
`crates/logic-compile/src/{ir.rs, projections/mod.rs, report.rs}`;
`crates/pipeline/src/{put_executor.rs, stages/mappings.rs}`; `crates/slice/src/{edoal_emit,
fno_emit, sparql_emit, mapping_emit}.rs` (rendering logic *moves* under the new back-ends).

**Open placement question:** a dedicated `slices/core/correspondence/` slice (its own
vocabulary, conformance, examples) versus a chapter under `slices/grounding/logic/`. Lean: a
dedicated slice, because alignment is important enough to own its surface while still compiling
into `logic:` IR.

---

## 17. Open axes and SME breakout

**Settled in `take1`:** the canonical object (§3); orientation GMEOW-as-source (§4); the
law-spine (§5); mnemomorphism as keystone and backwards-execution as a candidate preimage
only (§6); the five axes + standpoint (§7); reuse of the preservation machinery (§9); the
compiler shape (§10).

**Left open (carried forward):** the **merge/colimit** direction (§8.2); the **empirical
OpenEHR complement-transparency** boundary (§13.4); and the **slice placement** (§16).

Breakout tracks:

1. **Formal semantics, optic laws, caveat stratification** — prove a meta-level caveat cannot
   leak to object-level closure; formalize lattice joins and Galois soundness under composition;
   model correspondence-valued caveats (nested terminology).
2. **Compiler IR, equality saturation, progressive lowering** — e-graph rewrite rules without
   match explosion; verifier-pass boundaries per lowering; compact bytecode for the GTS capsule.
3. **Runtime safety** — tabling vs. cycle-detection for `put` cascades; meta-interpretation cost
   and the threshold to compile to a static sub-graph; lossy-`put` default-resolution rules.
4. **Target interoperability & OpenEHR subsumption** — in-band complement transparency across
   the validator zoo; retraction-to-ADL fidelity; the field-by-field boundary test; the
   **process axis** (PROC ↔ `logic:Plan`, §13.5) and the prescriptive↔descriptive lens; the
   YAMATO foundational refinements (persistent `Quality`, action/event open-closed, causal parts).
5. **Multi-axis modelling** — static rejection of confidence/determinacy conflation;
   evidence-strength composition; unindexed-standpoint isolation.
6. **Conformance & the overclaim gate** — the equivalence-before-deletion harness; SARIF
   diagnostics for overclaim; isolated-graph round-trip in parallel.

**The single sharpest question for the breakout:** can the in-band complement be made
validation-transparent across real OpenEHR tooling for *every* field in `S ∖ im(get)`? If yes,
"perfectly replace OpenEHR" is a theorem with a test. If even one field forces a
validation-vs-losslessness tradeoff, that field is the exact, nameable boundary of GMEOW's
subsumption — and naming it honestly is worth more than papering over it.

---

## 18. Summary commitments

- One new IR node kind, `logic:Correspondence`, with `logic:Lens` legs; the existing
  pattern/expression algebra retained as the leg body; `dsl/mappings/` demoted to a frontend
  and retired after equivalence-before-deletion.
- The ordered law-spine as the taxonomy, with **section/retraction = perfect subsumption** as
  its load-bearing rung and `u∘d = id` as a CI-checkable claim under the overclaim gate.
- **Mnemomorphism** as the keystone that makes lossy directions recoverable and the section
  rung reachable, reported per correspondence; backwards-execution is a candidate preimage only.
- Reuse of the preservation/discharge machinery verbatim as the lens-law framework; a new
  `Correspondence` conformance category generalizing the round-trip gate.
- Five axes kept apart; standpoint-indexed (unindexed ≠ universal); composition by dogfooded
  `logic:` rules; the **merge/colimit** direction named as an open first-class axis.
- An MLIR-style lowering pipeline with an equality-saturation optimizer, grounded in functorial
  migration + GLAV/chase; Rust-first, Python-surface, no new Python.
- OpenEHR treated as a **six-layer** subsumption (data, constraints, process, decision-logic,
  query, terminology), grounded in real GECCO data: the **data axis** (`DV_QUANTITY` section/
  retraction via the in-band complement) and the **process axis** (PROC ↔ `logic:Plan`, the
  prescriptive↔descriptive lossy lens, joined to the canonical process model — the `logic:Plan` superset, its acyclic DAG profile, and the build-pipeline executor), with the four
  genuine unknowns flagged rather than smoothed over.
- **YAMATO** (Mizoguchi 2010) adopted by-reference as a bridge view (P5), canonical in `logic:`
  (P17): the quality stratification (persistent `Quality` identity, generic-quality→role ladder,
  unit-independent true quantity) grounds the data axis; the event refinements (process ≠ event,
  action-open/event-closed, causal-vs-temporal parts) ground the process axis.
- **Execution** (§10.2): the long-term engine is a single native physical core the IR lowers to
  (relational-core dialect → physical plan → semi-naive/incremental/compiled), with the external
  execution substrates retired. Seven levers
  (one core, magic-sets, incremental differential/DBSP, WCOJ, provenance-semirings = the axis
  algebra, compile-don't-interpret, fragment-routing). One IR commitment to lock in now: a
  first-class **relational-core dialect** as the logical↔physical lowering waist.
