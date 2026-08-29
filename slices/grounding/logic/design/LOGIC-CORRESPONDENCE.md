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

The mapping DSL is the ergonomic frontend, not a competing semantic layer. A frontend cell may
state its correspondence class, morphism kind, and preservation kind; the compiler folds those
judgments into the typed IR, assigns content-addressed identity, derives the executable legs, and
governs the result through the loss ledger. The up-projection was formerly an independent
SSSOM-reading heuristic authored separately from the down CONSTRUCT; it is now the derived `put` leg
executed natively by `crates/pipeline/src/put_executor.rs`.

The realized grounding instance is
[`slices/grounding/logic/mappings/grounding-bridges.ttl`](../mappings/grounding-bridges.ttl).
Its `logic:GroundingCorrespondence` frontend marker requires explicit class/kind/preservation
judgments and compiles to a `logic:Correspondence` that retains the marker plus named
`logic:sourceEndpoint` and `logic:targetEndpoint`. Those records ship in the
`graph/correspondence-laws` named graph of `gmeow.gts`; SSSOM remains only a generated lowering.

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
- a set of **law constraints** — the laws the correspondence claims (`logic:GetPut`, `logic:PutGet`,
  `logic:PutPut`, `logic:SectionLaw`), each a `logic:LawClaim` whose **status reuses the foundation's
  executable discharge vocabulary** ([`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md), § Typed
  formalization governance): a `logic:DischargeVerdict` (`logic:ObligationDischarged` /
  `logic:ObligationUnknown` / `logic:ObligationViolated`) and, when applicable, the
  `logic:DischargeCondition` under which it was checked. A law proved in a certified fragment is
  `ObligationDischarged` under `logic:DischargeCertifiedFragment`; a refuted law is
  `ObligationViolated` with its countermodel; an authored-but-unchecked or inconclusive law is
  `ObligationUnknown`, carried forward, never silently passed.

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
`get : S → V` and `put : V × S → S`. This executable `get`/`put` core is the **`logic:Lens`** a
`logic:Correspondence` wraps; the correspondence adds the relation, the quantitative axes, the law
claims and the standpoint envelope around it. **GMEOW is the source `S`; the external vocabulary is the
view `V`.** Therefore down-projection (GMEOW → external) is `get`, and up-projection (external → GMEOW)
is `put`. The view is the smaller, derived thing; `put` folds a (possibly fresh) view back into the rich
source. The ingest-with-no-prior-state case (`S` empty) is exactly where the view alone is insufficient
and a **witness must travel in the view** — the mnemomorphism, below.

This orientation is also an ownership rule: grounding catalogs live in `lang:`, `math:`, or
`logic:` according to their semantic domain, and their external vocabulary is always the target.
For the formal grounding catalog, gUFO, BFO, OBO/RO, SUMO, OWL/RDFS, and SHACL are therefore views
of `logic:`, never sources from which `logic:` is defined.

### The quantity boundary — a peer-owned source endpoint

A catalog is selected by its **external** surface, but a row's GMEOW-side endpoint is selected by
**which slice owns the concept**, and for one row the two land in different slices. SUMO is an
upper ontology, so its boundary is `logic:`-owned and its rows live in
[`mappings/grounding-bridges.ttl`](../mappings/grounding-bridges.ttl). SUMO nonetheless carries a
`Quantity` class, and the [`GROUNDING.md`](../../../../docs/GROUNDING.md) tier rule fixes
`math:Quantity` as the sole class authority for dimensioned magnitude: `logic:` may not mint a
rival. The honest row is therefore `math:Quantity skos:broadMatch sumo:Quantity` — a
`logic:BridgeView` / `logic:CommitmentShiftingBridge` at `logic:ValidationOnly`, because SUMO's
`Quantity` broadly admits numbers and quantifiable entities while `math:Quantity` requires exactly
one explicit `math:Dimension`. A `logic:Quantity → sumo:Quantity` row is the rejected alternative,
and the foundational-bridging conformance suite pins that rejection so it cannot creep back.

This is the single place `logic:` names a grounding peer's term structurally, and it is registered
as such: the **quantity-boundary seam** (`logic:` → `math:`, carrying exactly `math:Quantity`) in
the seam registry of [`../manifest.ttl`](../manifest.ttl). The seam carries one term on purpose —
it sanctions naming the peer-owned quantity authority as a bridge row's source endpoint, and
nothing else. Any further `logic:` → peer reference needs its own registration or, more usually,
belongs in the peer that owns the term.

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
| **Prism** | partial map `S → V + S` on a sum/optional | match/build on the in-focus variant only | applies on one variant, passes through otherwise |
| **Affine correspondence** | co-projection onto a shared component | laws on the shared component only | "similar but not quite"; vague-determinacy targets |
| **Bridge view** | commitment-shifting comorphism | *no* satisfaction-preservation claim | BFO / DOLCE / SUMO / YAMATO / Cyc (CycL) |

The two partial-alignment rungs are distinct: a **prism** focuses an optional/sum variant (match-or-pass-through), an **affine correspondence** focuses a sub-structure source and view share. Both sit below the lossy lens and above the bridge view.

Two cross-cutting qualifiers:

- **`logic:morphismKind ∈ {logic:InstitutionMorphism, logic:CommitmentShiftingBridge}`** — the
  institution-theoretic split between a satisfaction-preserving morphism and a commitment-shifting
  bridge (the value is named `logic:CommitmentShiftingBridge` to keep it distinct from the
  `logic:BridgeView` rung it typically accompanies). This is the distinction the foundation already
  draws between gUFO (a truth-preserving down-projection of UFO⁺) and BFO/DOLCE/SUMO/YAMATO (bridge
  views). The loss ledger **refuses** to emit `owl:equivalentClass` for a bridge.
- **`mnemomorphic? ∈ {yes, no}`** — whether the forward leg retains a source witness. Orthogonal to
  the rung; it is the property that lets a correspondence *climb* the spine, because a retained witness
  is what discharges `put∘get = id`.

**Cyc as a bridge-view stress test.** Importing CycL microtheory content is a textbook **bridge
view**: `logic:morphismKind logic:CommitmentShiftingBridge`, no satisfaction-preservation claim, and
— per the refusal already stated above — the loss ledger refuses to emit `owl:equivalentClass` for
it. Cyc is a good calculus stress test because its microtheories (`Mt`s) are not a flat bag of
contexts: CycL's `genlMt` relation orders microtheories by generality, so the set of microtheories
forms a **lattice** under `genlMt`, not merely a partition. The bridge from CycL microtheories into
`logic:` standpoint/context indexing is therefore sharper than a plain commitment-shifting
comorphism between two structureless context sets — it is a **monotone lattice comorphism**: the
map from CycL `Mt`s into `logic:` standpoints preserves the `genlMt` order (more general Mt ↦ more
general standpoint), but it is still *not* an institution morphism, because Cyc's own semantics does
not guarantee satisfaction-preservation across that order (a specialization can locally contradict
the generalization it specializes, which is exactly the "mutually-inconsistent assertions across
contexts" Cyc is prior art for — see [`LOGIC.md`](LOGIC.md) § Lineage and Supersession).

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

### Genuine recovery cases

`logic:RecoveryCase` is the executable evidence for source recovery.  A correspondence links zero or
more cases through `logic:recoveryCase`; each case owns exactly one `logic:recoveryTransform`, an
ordered `∀(source → view)` formula.  The native executor supports the positive-conjunctive binary
RDF-atom fragment: it deterministically instantiates every variable in the complete declared source
pattern, constructs the view, runs the candidate inverse, and compares the recovered RDF atom set to
the source.  Every attached case is conjunctive evidence: all must recover; the first missing or
fabricated atom yields `ObligationViolated` with a deterministic countermodel.

That formula does not replace the correspondence legs.  For every attached case the executor first
resolves the actual `logic:getLeg` and `logic:putLeg` transaction bodies, executes their normalized
`logic:LegPath` relations on the same complete source seed, requires the relations to agree under
inversion, and requires every variable-bound endpoint selected by the executable get to survive in
the formula-constructed view.  Constants and predicates may change under the declared transform;
variable bindings may not silently disappear.  A missing, malformed, empty, or unrelated leg body
therefore violates the obligation even when the unchanged recovery formula can invert itself.  The
formula and the resolved bodies are one cross-checked proof object; neither is an independent semantic
source.

A recovery case is deliberately **neutral**.  Strong correspondences carry cases that discharge;
lossy correspondences may carry a refuting case, so changing only the rung or the
`logic:mnemomorphic` boolean cannot manufacture a proof.  The `gmeow:WritingSystem → lang:Script`
case, for example, includes `gmeow:writingSystemType` and `gmeow:textDirection` in the source while
the view omits them, and therefore reds if promoted to section/retraction.

This is a bounded query-class discharge, not a theorem over every possible RDF graph.  The authored
source pattern states the scope and must contain the distinctions on which injectivity depends;
`logic:DischargeBoundedCorpus` records that honesty boundary.  An atomic one-triple path has a
complete synthesized case.  A composite path has hidden intermediate structure and therefore stays
`ObligationUnknown` without an authored complete case — `put = get.invert()` is only candidate
construction, never evidence.

At the **process layer** the in-band witness is realized by a pair of back-references an executed
occurrence carries: `logic:instantiatesSchema` (occurrence → `logic:ActionSchema`, the reusable type)
and `logic:instantiatesPlan` (occurrence → the `logic:Plan` it was executed under, the whole planned
skeleton). Together they are the in-band complement that lets a plan↔execution-record correspondence's
`put` leg recover the planned portion of a run rather than synthesize a plausible plan — the openEHR
Instruction-State-Machine linkage (Instruction → Activity → Action) made canonical. Where the witness
is present the planned portion round-trips; the off-plan reality is an honest loss-ledger entry, never
a failure.

## The quantitative and contextual axes

A correspondence carries each axis **separately**, because they answer different questions and the
single `gmeow:confidence` of the old DSL destroys the distinctions:

- `logic:confidence` — the curator's epistemic confidence that the alignment is correct;
- `logic:evidenceStrength` — provenance-derived warrant (manual / lexical / structural / LLM);
- `logic:weight` — solver ranking, when competing correspondences exist for one source;
- `logic:probability` — only under a declared dependency model; most carry none;
- `logic:Determinacy` — whether the *target relationship* is ontically crisp or vague. "Similar but
  not quite" is `determinacy = vague` + `class = affine`, **not** low-confidence equivalence.

Every correspondence is **standpoint-indexed** (`gmeow:accordingTo`, the typed context algebra of
[`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md)). An unindexed correspondence holds in
`gmeow:unspecifiedStandpoint` — **unspecified, not universal** — which kills the silent-universality bug where
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

## A unifying lattice-graded reading

*Design note only — names a vocabulary and a direction; builds nothing here.* Three structures
already present in the calculus, and one from a sibling slice, are coordinates of a single
**lattice-graded correspondence** shape, not three unrelated mechanisms:

- the ordered preservation **law-spine** above (§ The ordered law-spine) — a poset a correspondence's
  claimable laws sit on, weakened monotonically by composition;
- the **confidence** (and sibling quantitative) axis (§ The quantitative and contextual axes) —
  computed in its own algebra under composition (t-norm/min/solver-additive, § Composition and
  merge);
- the `lang:` GMN dialect's **security-ring flow lattice** (`gmeow:GmnSecurityRing`, a separate
  slice — named here by reference, not imported or restructured: see `slices/grounding/lang/`) —
  a level×compartment product order the GMN serialization boundary respects.

Each axis is already computed in its own algebra — a semiring, a t-norm, or a `min` — and each
already weakens monotonically down a poset as correspondences compose (the law-spine itself is that
poset for the class axis; § Composition and merge is the general mechanism). Denning's
information-flow lattice `⟨SC, ⊑, ⊕, ⊗⟩` (security classes, dominance order, join, meet) is
recognizable as the **serialization-boundary instance** of this same shape: a lattice-graded
quantity, monotone under composition, attached to a crossing.

**Forward direction (named, not built here).** The unifying reading points toward two further
extensions of the calculus:

1. a `logic:` **flow-label axis** on the correspondence, alongside `confidence`/`evidenceStrength`/
   `weight`/`probability`/`Determinacy`, so a crossing's information-flow classification composes by
   the same rules as its other quantitative axes;
2. a **parametric round-trip harness** generalizing the calculus's existing byte-teeth gates — the
   narrow-waist superset gate, the RDFC-1.0 round-trip, and the GMN-1 round-trip gate are three
   instances of one round-trip-over-a-crossing shape, and a single parametric harness could discharge
   all three from one implementation.

Naming this direction is the entire scope of this note. The engine itself — the flow-label
vocabulary, its RDF terms, and the parametric harness — is **not** implemented here: `logic:` is
canon, and expanding it with a new foundational axis or a generalized harness is a design decision
that belongs to its own dedicated treatment, not a side effect of naming the direction.

## Preservation is the lens-law framework (reuse, do not reinvent)

The existing preservation machinery *is* the lens-law / abstract-interpretation framework in entailment
dress, and the calculus reuses it verbatim:

| Lens / abstract-interpretation concept | Existing `logic:` machinery |
|---|---|
| `get∘put = id` on the preserved fragment | `ExactPreservation` + the round-trip faithfulness gate |
| under-approximation `α(γ(a)) ⊑ a` | `logic:SoundUnderApproximation` |
| over-approximation `c ≤ γ(α(c))` | `logic:CompleteOverApproximation` |
| law claimed but not machine-verified | `logic:LawClaim` + the `logic:DischargeVerdict` / `logic:DischargeCondition` vocabulary |
| polarities co-holding | "preservation polarities are not mutually exclusive" |
| round-trip is a decidable check | content-addressed canonical-IR identity (graph-iso) |

`logic:LawClaim` reuses these exact individuals via `logic:lawDischargeVerdict` /
`logic:lawDischargeCondition`. The **overclaim gate** fires for alignment: marking a caveated overlap as
`sssom exactMatch`, or a bridge view as `logic:InstitutionMorphism`, is a build failure — strictly
stronger than the old `projection_lint` warning. The three former cross-layer
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
| GMN (`gmeow:gmnModelNotation`, the `lang:` dialect charter) | the GMN-0 narrow-waist normal form | section/retraction, exact preservation, `mnemomorphic true`: aliases invert through the version-pinned codebook bijection (injectivity-gated), confidence and annotations ride by reference (never inlined, never lost); discharged by the executed GMN-1 round-trip gate, total over the grounding slices' GMN-0 now and gated toward full coverage by the GMN-1-coverage quality axis elsewhere; the rate–fidelity contract rides the codebook |

Every lowering is a **legalization** (see [`LOGIC-IR.md`](LOGIC-IR.md) § IR commitments): a total
function into `⟨ legal output ⊕ flagged residue ⟩`; the loss ledger is the residue set.

## The IR commitments

Three commitments are recorded in [`LOGIC-IR.md`](LOGIC-IR.md) and are load-bearing for this calculus:

1. **Lowering is legalization** (`logic:ConversionTarget`) — the target names the legal IR or
   dialect, and partial conversion leaves an illegal construct in place, flagged: the "unsupported
   carried and flagged, never dropped" rule. This is distinct from `logic:ProjectionTarget`, which
   requests an output presentation without defining the legality domain or changing entailments.
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
section/retraction execute complete recovery cases and reproduce the declared source atom set), the
**Mnemomorphism gate** (a recoverability claim must actually recover the source), and the
**Composition gate** (composing may only preserve or weaken claims).

## OpenEHR — the worked subsumption (six layers)

openEHR is the worked instance. It is a six-layer standard, and GMEOW subsumes each layer with the same
projection doctrine — the **data axis** (`DV_QUANTITY` ↔ frame-relative quantity, reaching
section/retraction via an in-band complement) and the **process axis** (openEHR PROC / Task-Planning ↔
`logic:Plan`, a lossy lens for execution). The process axis joins this calculus to the canonical
process model: openEHR Task Planning is one more by-reference projection
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
