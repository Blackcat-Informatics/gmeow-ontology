<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — The Correspondence Calculus

> Member of the GMEOW Logic design set ([`LOGIC.md`](LOGIC.md)). This document is the **normative
> canon** for cross-ontology alignment as a first-class `logic:` construct. Its extended rationale
> — the research synthesis, the optic/Galois/institution/GLAV derivations, and the worked
> openEHR use cases — lives in [`docs/APPLIED_CATEGORY_THEORY/take1.md`](../../../../docs/APPLIED_CATEGORY_THEORY/take1.md)
> and the sibling `usecase_*.md` + `fixtures/` there. Where this document and `take1.md` differ,
> **this document governs**; `take1.md` is the cited rationale, not a second source of truth.
>
> **Reading this document.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the conformance corpus
> ([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md) § Correspondence). It is not a claim that any
> implementation already realizes X except as the corpus demonstrates.

## The document set

| Document | Role |
|---|---|
| [`LOGIC.md`](LOGIC.md) | manifesto, vision, lineage |
| [`LOGIC-IR.md`](LOGIC-IR.md) | the typed IR `logic:Correspondence` joins as the ninth node kind; the three IR commitments |
| [`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md) | the native execution engine the calculus lowers to |
| [`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md) | the Correspondence conformance category and its gates |
| `LOGIC-CORRESPONDENCE.md` (this) | the alignment calculus — node kind, law-spine, mnemomorphism, lowerings |

## The thesis

SSSOM, EDOAL, FnO, SPARQL CONSTRUCT, OWL alignment axioms, and the up-projection lift map are **not
alignment sources**. They are **generated target dialects** of one canonical object: a
`logic:Correspondence`, a first-class, RDF 1.2-native, law-bearing node kind in the typed IR. A
correspondence carries typed source/target theories, executable `get`/`put` legs, an algebraic kind
on an ordered law-spine, its claimed laws with discharge status, the separated quantitative/contextual
axes, FOL/SOL caveats, and a preservation judgment. "GMEOW perfectly subsumes vocabulary `V`" is a
**CI-checkable section/retraction law** (`u ∘ d = id`), not a slogan.

This is the project's own doctrine applied to alignment. Principle 4 (one canonical source; everything
else a generated lossy projection) and Principle 17 (the logic is canonical; OWL/Datalog/SHACL/gUFO
are projections) already performed this move for facts and for axioms. The correspondence calculus
performs the identical move for cross-ontology alignment: the alignment layer becomes a generated
projection of `logic:`, peer to the OWL/Datalog/gUFO projections, never a second source of truth.

The current mapping DSL (`dsl/mappings/`) is a useful first inversion (one source → four artifacts)
but stops one level too shallow: it is "a spec layer never reasoned over," so it cannot say *what kind*
of correspondence a mapping is, it collapses the distinct quantitative axes into one
`gmeow:confidence`, and it authors down- and up-projection apart (the up-projection is an independent
SSSOM-reading heuristic, `crates/pipeline/src/up_projection.rs`). Folding correspondence into the IR
makes it **reasoned over**, **content-addressed**, and **governed by the loss ledger**.

## The ninth node kind

[`LOGIC-IR.md`](LOGIC-IR.md) defines the typed IR as a sum of node kinds. `logic:Correspondence` is a
node kind, defined *in terms of* the existing kinds rather than as a specialization of any one:

- a **meta-formula envelope** — the relation between a source pattern and a GMEOW pattern, with its
  caveats, standpoint index, and quantitative axes. It is meta-level (a statement *about* the
  relationship between propositions) and **stays meta**: it must not leak into object-level closure.
  The IR's stratification rule for any `holds`/truth predicate enforces this — it is how the current
  "never reasoned over" invariant survives while the correspondence itself becomes reasoned data;
- two **executable legs** — `get` (down-projection) and `put` (up-projection), each a
  transaction-program over the path semantics of [`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md)
  (`ins`/`del`, hypothetical execution, supersession-not-erasure). The leg body reuses the existing
  pattern/expression algebra and `logic:PathShape` ([`LOGIC-PATHS.md`](LOGIC-PATHS.md)) — no new
  pattern language is introduced;
- a set of **law constraints** — the laws the correspondence claims (GetPut, PutGet, PutPut, the
  section law), each a constraint node whose **status reuses the `NonEntailmentObligation` discharge
  vocabulary** ([`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md)): `proved-in-certified-fragment` /
  `declared-unverified` / `refuted-with-witness` / `unknown-not-discharged`.

A correspondence is a *new top-level kind*, not a meta-formula with an attached transaction-program,
because content-addressed identity and the preservation judgment must attach to the correspondence **as
a unit**: the loss ledger needs one preservation row per correspondence, hashing the relation, the
axes, the caveat, *and both legs* together, so it can attribute a dropped construct to the leg that
dropped it. This is the same reasoning that keeps `constraint` distinct from `derivation-rule`.

### The relation lattice

The correspondence's relation is a typed `logic:CorrespondenceRelation` on an ordered lattice, not a
free string: `equiv` ⊐ {`subsumes`, `subsumedBy`} ⊐ `overlaps` ⊐ `relatedMatch`, with `disjoint` as
the negative pole. The lattice lets the compiler **derive** the SSSOM predicate, the EDOAL relation
symbol, and the OWL-alignment-axiom strength from one authored relation, so the artifacts cannot drift.

### Frontend versus canonical form

A correspondence *compiles into* the IR and *may carry* FOL/SOL caveats and laws, but authors do not
hand-write raw FOL. The frontend stays ergonomic and slice-local (the existing
`gmeow:ProjectionMapping` ergonomics are a starting frontend); the canonical, reasoned, law-bearing
form is what the compiler produces. This is the standard language/IR separation, and it is what keeps
maximal expressivity at the centre from becoming an unusable surface (the Ithkuil failure mode the
projection doctrine exists to avoid — see [`LOGIC.md`](LOGIC.md)).

## Orientation convention

A correspondence is an asymmetric lens between a rich **source** `S` and a derived **view** `V`, with
`get : S → V` and `put : V × S → S`. **GMEOW is the source `S`; the external vocabulary is the view
`V`.** Therefore down-projection (GMEOW → external) is `get`, and up-projection (external → GMEOW) is
`put`. The view is the smaller, derived thing; `put` folds a (possibly fresh) view back into the rich
source. The ingest-with-no-prior-state case (`S` empty) is exactly where the view alone is insufficient
and a **witness must travel in the view** — the mnemomorphism, below.

## The ordered law-spine

A correspondence is classified by **how much invertibility it can lawfully claim**, on one ordered
spine; **each rung caps the laws the correspondence may assert** (the profunctor-optic lattice fused
with the categorical subobject notion):

| Rung | Categorical structure | Laws claimable | Reading |
|---|---|---|---|
| **Isomorphism** | iso (`get∘put = id`, `put∘get = id`) | full round-trip both directions | conf-1.0 equivalence |
| **Section / retraction** | split mono (`put∘get = id_S`; `get∘put` idempotent on `V ⊕ complement`) | source embeds losslessly; augmentation = `S ∖ im(get)`, in the complement | **perfect subsumption** |
| **Well-behaved lens** | asymmetric lens | GetPut + PutGet (PutPut optional) | structured→flat downcast with sound update |
| **Lossy lens** | lens, non-injective `get` | one direction faithful; inverse needs witness/claim/defaults | most schema.org/FOAF downcasts |
| **Prism / affine** | partial map `S → V + S` | match/build on the in-focus case only | "similar but not quite"; co-projection onto a shared component |
| **Bridge view** | commitment-shifting comorphism | *no* satisfaction-preservation claim | BFO / DOLCE / SUMO / YAMATO |

Two cross-cutting qualifiers:

- **`morphismKind ∈ {institutionMorphism, bridgeView}`** — the institution-theoretic split between a
  satisfaction-preserving morphism and a commitment-shifting bridge. This is the distinction the
  foundation already draws between gUFO (a truth-preserving down-projection of UFO⁺) and
  BFO/DOLCE/SUMO/YAMATO (bridge views). The loss ledger **refuses** to emit `owl:equivalentClass` for
  a bridge.
- **`mnemomorphic? ∈ {yes, no}`** — whether the forward leg retains a source witness. Orthogonal to
  the rung; it is the property that lets a correspondence *climb* the spine, because a retained witness
  is what discharges `put∘get = id`.

Composition can only weaken the rung, never strengthen it (§ Composition).

## Mnemomorphism

A correspondence is a **mnemomorphism** (μνήμη, *memory*, + -morphism) when its forward map `get`
factors through the **graph of the correspondence** — the source-witness — so that `put` is obtained by
*projecting along the retained witness* rather than synthesizing a plausible source. Equivalently:
`get` carries, in its output, enough of its input that `put∘get = id` holds *because nothing the
retraction needs was discarded*.

The witness is the alignment analogue of a paramorphism's access to the original substructure
(generalized to a histomorphism's cofree-comonad annotation); of a delta/edit-lens trace; of database
provenance (semiring-annotated — see [`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md) § semiring); and of a TGG
correspondence graph. Mnemomorphism is the dividing line between **subsumption** (recoverable, reaches
the section rung) and **approximation** (reconstructed, falls to lossy-lens/prism and needs a
co-authored `put`-with-claim).

**Backwards-execution is a candidate preimage, not a lawful `put`.** Running a relational leg backward
yields *a* source consistent with the view, not *the* lawful `put`: no GetPut/PutGet guarantee, no loss
tracking, no provenance, and it fails silently on non-injective `get`. It is the amnesic case. Lawful
`put` comes only from (1) a mnemomorphic witness, or (2) a co-authored `put`-with-claim under explicit
mode/tabling/minting declarations and a declared law status. Naive backward-execution is a named
anti-pattern, never the architecture.

## The quantitative and contextual axes

A correspondence carries each axis **separately**, because they answer different questions and the
single `gmeow:confidence` of the old DSL destroys the distinctions:

- `logic:confidence` — the curator's epistemic confidence that the alignment is correct;
- `logic:evidenceStrength` — provenance-derived warrant (manual / lexical / structural / LLM);
- `logic:weight` — solver ranking, when competing correspondences exist for one source;
- `logic:probability` — only under a declared dependency model; most carry none;
- `logic:Determinacy` — whether the *target relationship* is ontically crisp or vague. "Similar but
  not quite" is `determinacy = vague` + `class = affine`, **not** low-confidence equivalence.

Every correspondence is **standpoint-indexed** (`logic:accordingTo`, the typed context algebra of
[`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md)). An unindexed correspondence holds in
`UnspecifiedStandpoint` — **unspecified, not universal** — which kills the silent-universality bug where
a curated alignment is applied where it was never validated.

## Composition and merge

**Composition (sequential, `C₁ ∘ C₂`)** computes each axis in its own algebra: class by optic-lattice
join (monotone-downward — composition only weakens the rung); laws with weakest-status-dominates
(`unknown` is absorbing); `confidence` by a declared t-norm (default product, independence made
explicit); `evidenceStrength` by weakest-link/min; `weight` solver-additive; `probability` only under a
declared cross-chain model (else `not-evaluated`); loss by Galois-connection composition with union of
the unsupported-construct sets. All computed **by `logic:` rules over correspondence nodes** — dogfooded
and conformance-checked, not buried in compiler arithmetic.

**Merge (the colimit/pushout direction)** — combining incoming data from several sources into GMEOW
simultaneously is a **colimit/pushout in the category of theories**, gluing along the shared GMEOW apex
without collapsing distinct, possibly contested, contexts. A pushout that would force `owl:sameAs`-style
collapse of standpoint-indexed claims is ill-formed (Principles 5/9): the merge is a colimit in a
category whose objects carry standpoint indices, so contested claims coexist. This axis is **declared,
not yet fully specified** — an open axis for design (it is left open in `take1.md` §8.2).

## Preservation is the lens-law framework (reuse, do not reinvent)

The existing preservation machinery *is* the lens-law / abstract-interpretation framework in entailment
dress, and the calculus reuses it verbatim:

| Lens / abstract-interpretation concept | Existing `logic:` machinery |
|---|---|
| `get∘put = id` on the preserved fragment | `ExactPreservation` + the round-trip faithfulness gate |
| under-approximation `α(γ(a)) ⊑ a` | `logic:SoundUnderApproximation` |
| over-approximation `c ≤ γ(α(c))` | `logic:CompleteOverApproximation` |
| law claimed but not machine-verified | `NonEntailmentObligation` discharge status |
| polarities co-holding | "preservation polarities are not mutually exclusive" |
| round-trip is a decidable check | content-addressed canonical-IR identity (graph-iso) |

`logic:LawClaim.status` reuses these exact individuals. The **overclaim gate** fires for alignment:
marking a caveated overlap as `sssom exactMatch`, or a bridge view as `institutionMorphism`, is a build
failure — strictly stronger than the old `projection_lint` warning. The three former cross-layer
invariants collapse into this: `fno-type` becomes the FnO back-end's soundness check; `spec-drift`
*disappears* because EDOAL and SPARQL now lower from the same `get` leg.

## The lowerings (target dialects)

Each former artifact is a registered lowering with its own preservation claim, in the *same*
`generated/logic/projection-report.ttl` loss ledger that governs OWL/Datalog/gUFO:

| Target | Lowers from | Typical preservation |
|---|---|---|
| SSSOM | the meta-formula's 1:1 lattice band | exact for `equiv`; else under-approx; drops caveat/laws/legs |
| EDOAL | the `get` leg + relation lattice + measure | under-approx; drops SOL caveats, the `put` leg, world/standpoint scope |
| FnO | transform functions referenced by `get` | exact for signatures; validation-only for entailment |
| SPARQL CONSTRUCT | `get` compiled to the closed algebra | the faithful executable down-projection; profile losses explicit |
| up-lift (replaces the heuristic) | the `put` leg — *derived* for mnemomorphic cells | complete-over for invertible; validation-only (mint-with-claim) otherwise; `unsupported` where `get` is non-injective and no witness exists |
| OWL alignment axioms | the relation, DL-expressible band | under-approx; `unsupported` for caveated overlaps and bridges |
| OAEI / Alignment-API XML | the whole correspondence set | under-approx; carries `align:measure` where SSSOM/OWL drop confidence |

Every lowering is a **legalization** (see [`LOGIC-IR.md`](LOGIC-IR.md) § IR commitments): a total
function into `⟨ legal output ⊕ flagged residue ⟩`; the loss ledger is the residue set.

## The IR commitments

Three commitments are recorded in [`LOGIC-IR.md`](LOGIC-IR.md) and are load-bearing for this calculus:

1. **Lowering is legalization** (`logic:ConversionTarget`) — partial conversion leaves an illegal
   construct in place, flagged: the "unsupported carried and flagged, never dropped" rule.
2. **Every annotation is typed `logic:loadBearing` or droppable** — a display hint is droppable; the
   in-band complement and the axes are load-bearing (`put` needs them for `u∘d = id`). *Without this
   bit the section/retraction rung cannot be verified*, so it is in the node type from the start.
3. **The `logic:RelationalCore` dialect** — the logical↔physical lowering waist between `logic:` and the
   native execution engine; every execution strategy targets it.

## Execution

The correspondence calculus rides the native execution engine of
[`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md): a `get` leg is a join plan; a `put`/chase is semi-naive fixpoint
with existentials; a lens-law check is run-get-then-put + content-hash compare. The quantitative axes
are **semiring annotations** computed in one evaluation pass, not N passes over a context cross-product
— the semiring *is* the axis algebra (§ Composition). The `ReasoningContract`
([`LOGIC-CONTRACT.md`](LOGIC-CONTRACT.md)) selects both correctness and the physical plan.

## Conformance

A new **Correspondence** conformance category ([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md))
generalizes the Common-Logic round-trip gate, with five decisive gates: the **Law gate** (a
correspondence may not claim a law it fails), the **Overclaim gate** (a bridge view cannot emit
equivalence; a claimed rung must be satisfiable by the lowered legs), the **Round-trip gate** (iso and
section/retraction pass canonical-identity checks), the **Mnemomorphism gate** (a recoverability claim
must actually recover the source), and the **Composition gate** (composing may only preserve or weaken
claims).

## OpenEHR — the worked subsumption (six layers)

openEHR is the worked instance. It is a six-layer standard, and GMEOW subsumes each layer with the same
projection doctrine — the **data axis** (`DV_QUANTITY` ↔ frame-relative quantity, reaching
section/retraction via an in-band complement) and the **process axis** (openEHR PROC / Task-Planning ↔
`logic:Plan`, a lossy lens for execution). The process axis joins this calculus to the canonical
process model (work-streams W1/W2/W3): openEHR Task Planning is one more by-reference projection
target of `logic:Plan`, and the correspondence calculus is its projection mechanism. The YAMATO
refinements that ground both axes (persistent `Quality`; action/event open-closed; causal-vs-temporal
parts) are adopted by-reference (Principle 5; see
[`foundational-bridging.md`](../../../../docs/foundational-bridging.md)). Worked end-to-end against real
GECCO data in [`usecase_openehr_bloodpressure.md`](../../../../docs/APPLIED_CATEGORY_THEORY/usecase_openehr_bloodpressure.md)
and [`usecase_openehr_taskplan_rchops21.md`](../../../../docs/APPLIED_CATEGORY_THEORY/usecase_openehr_taskplan_rchops21.md).

## Constitutional alignment

One canonical source; every surface a generated projection carrying an honest preservation judgment
(Principle 4). Maximal bridging by reference, never `owl:sameAs` collapse (Principle 5). The logic —
now including alignment — is canonical; SSSOM/EDOAL/FnO/SPARQL/OWL-alignment are lossy projections
(Principle 17). The correspondence calculus is the third consolidation under this doctrine, peer to the
process model and the typed compositional meta-semantics, all sharing the native execution engine.
