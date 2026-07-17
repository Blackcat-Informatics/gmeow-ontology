<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — Analysis, Topology, and Geometry

> The **analysis-and-geometry charter** of the GMEOW Mathematics design set: calculus and analysis
> (limits, derivatives, integrals, series, special functions), topology, and differential geometry
> (manifolds, charts, tensor fields, metrics — including the Lorentzian metric the relativity case
> needs). It builds on the expression AST and the measure charter
> ([`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md),
> [`MATHEMATICS-MEASURE-AND-DIMENSION.md`](MATHEMATICS-MEASURE-AND-DIMENSION.md)); it holds the
> **math side** of the physical-reference-frame flagship-adjacent case, leaving spacetime and SR/GR to
> a downstream physics slice. Anchors (OpenMath calculus CDs, DLMF, mathlib) are in
> [`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md); gates in
> [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's canonical `module.ttl` axioms and `logic:Constraint` records, competency queries, and the
> projection loss ledger.

## Purpose

Calculus, topology, and geometry share one discipline in GMEOW: **the objects of the continuous are
structured, not evocative.** A derivative is an operator application, not the string "dy/dx"; a
manifold declares its dimension and structure kind, not merely the label "manifold"; a complement
names its ambient space and its complement-semantics. Ambiguity in the continuous is where silent
error hides, so this charter hard-fails on it.

## Calculus and analysis

Core classes: `math:Limit`, `math:LimitResult`, `math:LimitOutcome`, `math:Derivative`,
`math:PartialDerivative`, `math:DifferentialOperator`, `math:Integral` (from the measure charter),
`math:Series`, `math:Sequence`, `math:Convergence`, and `math:SpecialFunction`.

Core properties: `math:limitOf`, `math:limitPoint`, `math:hasLimitResult`, `math:limitOutcome`,
`math:limitResultValue`, `math:derivativeOf`, `math:withRespectToVariable`, `math:derivativeOrder`,
`math:seriesTerm`, `math:convergesTo`, and `math:convergenceMode`.

Every operator here is a **binder over the expression AST**: `d/dx`, `∂`, `∫`, `∑`, and `lim` are
`math:BindingExpression`s binding the variable of differentiation, integration, or summation
([`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md)). A `math:Derivative` names what it
differentiates, with respect to which variable, at which order; a `math:Limit` names its expression,
its limit point, and — where it matters — the direction/mode; a `math:Series` and `math:Sequence`
carry `math:Convergence` with its mode (pointwise, uniform, in-measure, …). Special functions align
to **DLMF** by equation ID and to OpenMath `calculus1`/`transc1`.

Where a limit's **evaluated result** matters — not just the limit expression, but *what it runs to* —
a `math:Limit` carries a structured `math:LimitResult` through `math:hasLimitResult`. The result is a
first-class object, never a bare literal: it names its `math:limitOutcome` (one of the four
`math:LimitOutcome` individuals `math:convergesFinitely`, `math:divergesToPositiveInfinity`,
`math:divergesToNegativeInfinity`, `math:divergesWithoutLimit`) and, where the outcome has one, its
`math:limitResultValue`. The value **agrees with the outcome** by construction: a convergent result
carries a finite literal; a result diverging to a pole carries `math:PositiveInfinity` or
`math:NegativeInfinity` (the same signed extended-real points a `math:totalMass` or an
`math:extendedRealValue` ranges over, which is why `math:limitResultValue` is an honest `rdf:Property`
spanning literal and pole); a result diverging without a limit — an oscillating `sin(1/x)` at `0` —
carries no value at all, because there is no point of `ℝ̄` to name. The result is **optional** on a
limit (a limit may still carry only its expression, point, and mode), but once stated it says which of
convergence or the three modes of divergence the limit is, so `lim = ∞` is never a silent overload of a
finite value. A result missing its outcome, or carrying a value that disagrees with it, is ill-formed
(`math:UnderspecifiedLimitResult`). This is where a grounding layer
becomes analysis: derivatives are maps of function spaces
([`MATHEMATICS-NUMBERS-AND-SETS.md`](MATHEMATICS-NUMBERS-AND-SETS.md)), and integration is the measure
charter's operator.

The **qualitative analytic properties** a function or a `math:FunctionPiece`
([`MATHEMATICS-NUMBERS-AND-SETS.md`](MATHEMATICS-NUMBERS-AND-SETS.md)) carries — its monotonicity,
non-affinity, convexity, and boundedness — are each backed by a **real first-order `logic:Formula`
defining law**, exactly as continuity and connectedness are, and never by a bare flag. Monotonicity is an
open vocabulary of `math:MonotonicityKind` individuals (`math:strictlyIncreasing`, `math:strictlyDecreasing`,
`math:nonIncreasing`, `math:nonDecreasing`, `math:constant`), each resolving through `math:definingLaw` to a
law over the reified real/value signature: `math:strictMonotonicityLaw` states `x < y ⇒ f(x) > f(y)`,
`math:strictIncreaseLaw` its order-preserving mirror, and the weak/constant laws their `≥`/`≤`/`=`
counterparts. The remaining analytic properties are an open `math:AnalyticProperty` vocabulary a function
declares through `math:hasAnalyticProperty`: `math:nonAffinity` is backed by `math:nonAffinityLaw`, which
**existentially witnesses** a collinearity failure (three graph points, one over an interior point of a
chord, off the line) using `logic:exists`/`logic:not` exactly as `math:connectednessLaw` does;
`math:boundedness` is backed by `math:boundednessLaw` — the near-zero plateau's defining **value** property
`|f(x)| ≤ ε` over a declared `math:boundOnInterval`, distinct from measure dominance over an integral.
Convexity is the one property whose textbook statement exceeds the first-order fragment: the full
λ-chord inequality `f(λx+(1−λ)y) ≤ λf(x)+(1−λ)f(y)` quantifies over the **continuum** of weights `λ` and
needs scalar arithmetic on the values. Rather than invent arithmetic function symbols to fake
first-orderness, `math:convexityLaw` is authored in the **honestly-expressible midpoint form**
`f((x+y)/2) ≤ (f(x)+f(y))/2`, reifying the midpoint as the uninterpreted relation `math:midpointRel`
(as `math:preimageRel` reifies the preimage); the λ-general residue is disclosed, never silently faked.
Every relation atom these laws predicate over — ordering, function value, midpoint, affine combination,
collinearity, absolute bound — is declared as a `logic:Type` reflection individual with an `⟺` gloss,
exactly as `math:openSetRel` is for continuity.

**Smoothness** (C^∞ / real-analyticity — "derivatives of every order") is the lone analytic property that
is **genuinely second-order**: it quantifies over the infinite family of *all* derivatives `{f, f′, f″, …}`,
which is not first-order axiomatizable. So, exactly like compactness in the topology section, it is not
faked as a formula but carried as an honest loss-ledger boundary `math:smoothnessBoundary`
(`logic:expressivenessBoundary logic:SecondOrder`, `logic:preservationKind logic:Unsupported`), referenced
by the `math:smoothness` marker through `math:definingLaw` and `rdfs:seeAlso`. The discriminant is the same:
quantification over individual points and values is first-order over the reified signature; quantification
over an infinite family of functions is second-order. Every `math:MonotonicityKind` and every
`math:AnalyticProperty` individual therefore resolves through `math:definingLaw` to a real law or a recorded
boundary — never a bare token (`math:UnbackedAnalyticProperty`).

## Topology

Core classes: `math:TopologicalSpace` (from the object layer), `math:OpenSet`, `math:ClosedSet`,
`math:Neighbourhood`, `math:ContinuousMap`, `math:Homeomorphism`, `math:CompactSpace`,
`math:ConnectedSpace`, `math:Homotopy`, and `math:HomologyGroup`.

Core properties: `math:hasOpenSet`, `math:isContinuous`, `math:separationAxiom`, `math:isCompact`,
`math:isConnected`, and `math:homotopyEquivalentTo`.

A `math:TopologicalSpace` is given by its open sets (or a basis); continuity is a declared property of
a map (preimages of open sets are open), not an assumption; connectedness and the T0–T4 separation
axioms are first-class and each backed by a real first-order `logic:Formula` defining law
(`math:connectednessLaw`, `math:t0SeparationLaw` … `math:normalitySeparationLaw`), mirroring
`math:continuityLaw`. Compactness is the lone second-order property — a finite subcover over an
arbitrary family is not first-order axiomatizable — so rather than a faked formula it is carried as an
honest loss-ledger boundary (`math:compactnessBoundary`, `logic:expressivenessBoundary logic:SecondOrder`).
Homotopy and homology give the algebraic-topology bridge (a
`math:HomologyGroup` is a `math:AbelianGroup`, [`MATHEMATICS-ALGEBRA.md`](MATHEMATICS-ALGEBRA.md)).
Little external ontology exists — the content is in prover libraries (mathlib, Isabelle/AFP), so the
depth is **authored** and **cited**.

### Graded cells and incidence

Beneath every complex, filtration, and cellular sheaf sits the graded cell. A `math:Cell` is one
building block of a `math:CellComplex` — a 0-cell vertex, a 1-cell edge, a 2-cell face — and it carries
its grade *n* through the single canonical `math:cellDimension` (a `math:NaturalNumber`; "grade" is a
label synonym, never a second `math:grade` property). A cell whose dimension is left implicit is
ill-formed (`math:IncompleteCell`), because an undimensioned cell cannot sit in the graded boundary
chain. Cells relate through two distinct surfaces. `math:incidentTo` is the **unsigned** codimension-1
face relation (Cell → Cell) that the boundary operator runs over; it is the load-bearing relation for the
boundary-square-zero law (∂∘∂ = 0), which a downstream constraint predicates over it. The
**signed** boundary coefficient is reified as a `math:CellIncidence`, which names its coface
(`math:incidenceCoface`), its codimension-1 face (`math:incidenceFace`), and the exact orientation sign
±1 (`math:incidenceSign`, an `xsd:integer`, never a decimal) — one inspectable entry of the signed
incidence matrix. A `math:CellIncidence` missing any of the three is ill-formed
(`math:UnorientedIncidence`). Orientation itself is the open `math:CellOrientation` vocabulary
(`math:positivelyOriented`, `math:negativelyOriented`), attached to a cell through `math:orientation`
exactly as `math:MonotonicityKind` attaches to a function — an open `owl:NamedIndividual` set, never an
`owl:oneOf` enumeration; the cell's orientation is qualitative and distinct from the numeric
`math:incidenceSign`. This surface covers the **presence** of the boundary signs; their **coherence**
— that composing incidences makes the twice-applied boundary vanish — is a separate law, not this frame.
The worked triangle in `examples/graded-cell-complex.ttl` demonstrates the whole surface as the oriented
2-simplex whose boundary is ∂(faceABC) = +edgeAB + edgeBC − edgeAC.

### Chain and cochain complexes — the square-zero laws and their adjunction

The signed incidence surface carries the boundary *data*; on top of it sit the boundary *elements* and
the ∂∘∂ = 0 / δ∘δ = 0 *coherence* laws. A `math:Chain` is a graded signed combination of same-dimension
cells (naming its `math:ChainComplex` through `math:chainOf`); a `math:Cycle` (⊑ `math:Chain`) is a chain
in the kernel of ∂, a `math:Boundary` (⊑ `math:Chain`) a chain in its image. Because ∂∘∂ = 0 every
boundary is a cycle, and the `math:HomologyGroup` is the quotient **H = Z/B** — the cycles modulo the
boundaries. The dual, degree-raising side mints `math:Cochain` / `math:Cocycle` / `math:Coboundary` and a
`math:CochainComplex` (naming its `math:cochainCoboundary`, else `math:IncompleteCochainComplex`).

**The square-zero law is enforced in two genuinely distinct layers, and the split is an honest
mathematical distinction, not a scope dodge.**

- **The regular/simplicial reasoning-form law.** `math:boundarySquareZeroLaw` is a first-order
  `logic:Formula` (a sibling of `math:filtrationMonotonicityLaw`) stating the **combinatorial diamond with
  a fixed-point-free involution**: for every codimension-2 pair (coface *c*, far-face *a*) reached through
  two intermediate faces *b*, *b′* whose incidence-sign products are opposite (`math:oppositeIncidenceProductRel`),
  those faces are a **cancelling pair** (`math:cancellingPairRel`). This is a *theorem only on
  regular/simplicial complexes*, where a codimension-2 pair has **exactly two** intermediate faces (the
  diamond lemma); the law is scoped there deliberately, and `math:coboundarySquareZeroLaw` is its
  degree-raising transpose (over `math:cofaceRel`, deriving `math:coCancellingPairRel`). Both are
  Horn-derivable, so the native `gmeow:reasoningLogic` competency lane *fires* them over the worked triangle
  in `examples/chain-complex.ttl` and reads back the derived cancelling pair — the law is a live entailment
  consumer, not prose. The regular/simplicial enforcement is the cross-node ∀→∃ constraint
  `math:BoundarySquareZeroConstraint` (scoped to `math:SimplicialComplex`) /
  `math:CoboundarySquareZeroConstraint` (scoped to `math:CochainComplex`): every codim-2 composition path
  must have a distinct partner face, else `math:BrokenBoundarySquareZero` / `math:BrokenCoboundarySquareZero`.

- **The general-CW per-instance conformance constraint.** On a *general* CW complex the two-intermediate
  diamond is **not** a theorem — the exact statement is the finite signed sum Σ_b [c:b]·[b:a] = 0 over all
  intermediate cells, and a per-element "has an opposite partner" test is genuinely unsound (the multiset
  {+1, +1, −1} passes it yet sums to +1). So the correct general conformance check is
  `math:GeneralBoundarySquareZeroConstraint`, authored as the sanctioned `logic:JoinAggregateConstraint`
  (P9): a two-leg `math:CellIncidence` join (c → b → a) whose two integer `math:incidenceSign` leaves are
  multiplied, summed with SUM **grouped by (coface, far-face)**, and required = 0. It aggregates
  `xsd:integer` signs exactly (never a decimal) and lowers to a `SELECT $this ?far … GROUP BY $this ?far
  HAVING(SUM ≠ 0)` `sh:SPARQLConstraint`. This is the executable form that covers general CW complexes.

- **What stays a boundary.** `math:cellBoundarySumBoundary` records — as an honest
  `logic:expressivenessBoundary` (`logic:SecondOrder`, `logic:preservationKind logic:Unsupported`) — that
  the **universal class law** "Σ_b [c:b]·[b:a] = 0 for *every* codim-2 pair of *every* complex" is
  genuinely higher-order (it quantifies over the unbounded family of complexes and aggregates a group SUM
  the first-order fragment cannot express). This boundary covers **only** the universal class law; the
  regular/simplicial diamond law *is* first-order, and the general per-instance sum *is* executable — so the
  boundary is the honest residue, not a scope dodge. The homology quotient is treated the same way: the
  homologous-cycle equivalence (z₁ ~ z₂ ⟺ z₁ − z₂ ∈ B) is the first-order `math:homologyQuotientLaw`,
  while the quotient-**group** construction (forming Z/B with well-defined coset operations) is the honest
  `math:homologyQuotientBoundary`.

**The adjunction.** `math:boundaryCoboundaryAdjunctionLaw` states δ = ∂\* — the coboundary matrix is the
transpose of the boundary matrix, so a boundary `math:CellIncidence` and its `math:adjointIncidence`
transpose carry **equal** signs; `math:BoundaryCoboundaryAdjunctionConstraint` fires
`math:BrokenBoundaryCoboundaryAdjunction` when they disagree. This adjunction is exactly what the
already-grounded `math:HodgeLaplacian` (L = δδ\* + δ\*δ) and `math:HodgeDecomposition` depend on — nothing in
the Hodge/Laplacian surface is re-minted here. Finally, `math:constantSheafRecoversHomologyLaw` ties the
sheaf machinery back to ordinary homology: a `math:ConstantSheaf` (constant-field stalks, identity
restriction maps) has sheaf cohomology equal to the base complex's cellular homology
(`math:recoversHomology`), so the `math:CellularSheaf` / `math:SheafLaplacian` surface is a conservative
extension of plain homology, not a replacement. The worked triangle in
`tests/conformance-fixtures/chain-complex-square-zero-complete.ttl` demonstrates H₁ = Z/B (the 1-cycle
that bounds the filled face, so its class is zero) and the constant sheaf recovering it.

### Persistent homology — filtrations, lifetimes, and stability

Core classes: `math:CellComplex`, `math:SimplicialComplex`, `math:VietorisRipsComplex`,
`math:CechComplex`, `math:AlphaComplex`, `math:Filtration`, `math:FiltrationStage`,
`math:PersistentHomology`, `math:PersistenceDiagram`, `math:PersistenceBarcode`,
`math:PersistenceLandscape`, `math:BettiSummary`, `math:MapperConstruction`,
`math:MultiparameterPersistence`, `math:ZigzagPersistence`, and `math:PersistenceLifetime`. The
persistence **objects** (distinct from the activities above) are `math:MultiparameterFiltration`,
`math:PersistenceModule`, `math:PersistenceMorphism`, `math:ZigzagDiagram`, and the open
`math:ArrowDirection` vocabulary.

Core properties: `math:hasFiltrationStage`, `math:filtrationThreshold`, `math:stageStructure`,
`math:filtrationIndexKind`, `math:filtrationAmbient`, `math:overFiltration`, `math:persistenceFeature`,
`math:bornAt`, and `math:diesAt`; for the persistence objects, `math:filtrationIndexPoset`,
`math:multiIndex`, `math:moduleIndex`, `math:structureMap`, `math:morphismSource`,
`math:morphismTarget`, `math:zigzagArrow`, and `math:arrowDirection`.

A `math:Filtration` is a monotone family of substructures of an ambient object indexed by a real-valued
threshold ε: each `math:FiltrationStage` pairs a `math:filtrationThreshold` (a `math:Quantity`) with
the `math:stageStructure` present at it (a `math:TopologicalSpace`), and for thresholds ε₁ ≤ ε₂ the
stage at ε₁ is contained in the stage at ε₂. The containment is the **existing** transitive
`math:subsetOf`, not a new order relation; the nesting law that ε₁ ≤ ε₂ *entails* containment is
authored as the first-order `logic:Formula` `math:filtrationMonotonicityLaw`, a sibling of
`math:continuityLaw` and `math:connectednessLaw` over the reified stage/threshold/structure signature.
The SHACL gate enforces only structural presence (a filtration has stages; each stage has a threshold
and a structure) — monotonicity is a law, not a shape, exactly as for the separation axioms.

A `math:PersistenceLifetime` is the birth–death interval of one topological feature — a
`math:HomologyGroup` generator, a hole of some dimension — across a filtration: it names the filtration
(`math:overFiltration`), the feature (`math:persistenceFeature`), the threshold the feature appears at
(`math:bornAt`, a `math:Quantity`), and the threshold it disappears at (`math:diesAt`). Its persistence
is death − birth: a long-lived feature is signal, a short-lived one is noise. An **essential** feature
never dies within the filtration, so its `math:diesAt` is the individual `math:PositiveInfinity` rather
than a finite threshold — the same extended-real range `math:totalMass` carries, which is why
`math:diesAt` is a bare `rdf:Property` (its range spans a finite quantity individual and
`math:PositiveInfinity`, and neither DL property kind admits both).

The bedrock stability result is the **bottleneck stability theorem** (`math:bottleneckStabilityTheorem`,
a `math:Theorem` held under `math:computationalTopologyTheory` and warranted by
`math:cohenSteinerEdelsbrunnerHarer2007`): for tame functions *f*, *g* on a common space, the bottleneck
distance between their persistence diagrams satisfies d\_B(Dgm(*f*), Dgm(*g*)) ≤ ‖*f* − *g*‖\_∞ — the
persistence-diagram map is 1-Lipschitz. Its kept corollary (correct by the triangle inequality on the
two independently-bounded endpoints, |Δbirth| ≤ δ and |Δdeath| ≤ δ ⇒ |Δ(death − birth)| ≤ 2δ) is that a
feature's persistence value can shift by at most 2‖*f* − *g*‖\_∞ under such a perturbation. No
Wasserstein-distance bound is asserted: it needs extra hypotheses this statement does not verify. This
theorem is what turns the persistence of a feature into a *warranted* credence
([`MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md`](MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md)), the
persistence-calibration surface that lands as `logic:confidence` on a latent-meaning claim. Like the
rest of the topology depth, the content is authored and cited (mathlib/AFP; Edelsbrunner–Harer,
*Computational Topology*).

The analysis process itself is `math:PersistentHomology`, not the filtration and not a project-local
TDA umbrella. It names its input, exactly one filtration, and one or more persistence-diagram
outputs. Barcodes, landscapes, Betti summaries, Mapper constructions, and multi-parameter or zigzag
specializations remain distinct mathematical result or method classes, so a consumer can state
exactly which summary it calculated.

#### Multiparameter and zigzag persistence — the OBJECTS, distinct from the ACTIVITIES

`math:MultiparameterPersistence` and `math:ZigzagPersistence` are analysis **activities**
(`⊑ math:PersistentHomology ⊑ gmeow:Activity`) — they are *methods*, not the things they range over.
The mathematical **objects** those methods operate on are minted separately, distinctly named, and
cross-linked to their activities through `rdfs:seeAlso`:

- **`math:MultiparameterFiltration`** (`⊑ math:MathematicalObject`) — a filtration indexed by a
  **poset of dimension ≥ 2** (`math:filtrationIndexPoset`), not a single real line. It reuses the
  existing `math:FiltrationStage` / `math:hasFiltrationStage`, but each stage carries a
  `math:multiIndex` — a `math:Vector` of coordinates in the parameter poset — **rather than** the
  single real `math:filtrationThreshold` a one-parameter stage carries.
- **`math:PersistenceModule`** (`⊑ math:MathematicalObject , math:Functor`) — the **algebraic**
  object: the functor *P*: (index poset as a category) → Vect, naming its index through
  `math:moduleIndex` and its comparable-pair transition maps through `math:structureMap`. It reuses
  the shipped `math:Functor` grounding (**L1 reuse** — a persistence module *is* a functor on its
  index poset), so no parallel category-theory structure is minted. It is held
  `owl:disjointWith math:PersistenceLifetime`: the whole functor versus one feature's decorated bar.
- **`math:PersistenceMorphism`** (`⊑ math:Morphism`, U1-light) — a map of persistence modules,
  naming both endpoints through `math:morphismSource` / `math:morphismTarget`.
- **`math:ZigzagDiagram`** (`⊑ math:MathematicalObject`) — a sequence whose structure maps
  (`math:zigzagArrow`) may point forward or backward, each declaring its `math:arrowDirection` over
  the open `math:ArrowDirection` vocabulary (`math:forwardArrow` / `math:backwardArrow`), minted like
  `math:CellOrientation`.

**Non-collapse is the whole point, and it is executable.** A genuinely multi-parameter or
bidirectional construction must never silently degrade to the one-parameter, forward-only case. Three
structural guards make this a hard fail rather than a hope: `math:CollapsedMultiparameterFiltration`
(stages carrying only a real `math:filtrationThreshold` and no `math:multiIndex` — a cross-node
existence `logic:Constraint`), `math:DiagonalDegenerateFiltration` (a 2-index whose coordinates are
functionally dependent, the diagonal (*t*, *t*) — a cross-node equality `logic:Constraint`), and
`math:DegenerateZigzagDiagram` (a diagram declaring no `math:backwardArrow` — a guarded-existence
`logic:Constraint`). Each lowers to a `sh:SPARQLConstraint`. The missing-frame cases
(`math:IncompleteMultiparameterFiltration`, `math:IncompletePersistenceModule`,
`math:IncompletePersistenceMorphism`) are declarative `owl:minQualifiedCardinality` frames, and the
module/lifetime conflation (`math:PersistenceModuleLifetimeConflation`) is the direct
`owl:disjointWith` axiom's derived `sh:not`, wired exactly like `math:ProbabilityScaleConflation`.

**The principled reason, stronger than the guards.** Carlsson–Zomorodian (*The Theory of
Multidimensional Persistence*, Discrete & Computational Geometry 42(1), 2009) proved that
multiparameter persistence admits **no complete discrete invariant**: unlike the one-parameter case,
whose barcode is a complete discrete invariant, no finite discrete summary classifies a multiparameter
module up to isomorphism. This is *why* a multiparameter filtration cannot be reduced to a family of
one-parameter runs, and it is genuinely second-order (it ∀-quantifies over the family of all candidate
discrete invariants and asserts none is complete). It is recorded honestly as
`math:multiparameterInvariantBoundary` (`logic:expressivenessBoundary logic:SecondOrder`,
`logic:preservationKind logic:Unsupported`), mirroring `math:compactnessBoundary` and
`math:smoothnessBoundary` — never faked as a first-order `logic:Formula`.

### Cellular sheaves and Hodge structure

A `math:CellularSheaf` declares its base `math:CellComplex`, its `math:SheafStalk`s, and the
`math:SheafRestrictionMap`s transporting data along incidences; sections, cohomology, sheaf
Laplacians, and Hodge decomposition are separate reusable structures. `math:HodgeDecomposition`
names harmonic, exact, and coexact components rather than treating those readings as intrinsic
labels on a vector. A sheaf without a base, stalk, or restriction map is structurally incomplete.

#### Global versus local sections — the H¹ gluing obstruction

Sections carry a scope. A `math:GlobalSection` is a section over the **entire** base
(`math:sectionRegion` = the sheaf's `math:sheafBaseComplex`) whose stalk values restrict
**consistently** along every `math:SheafRestrictionMap` — it is an element of the degree-zero
sheaf cohomology H⁰(X; F). A `math:LocalSection` is a section over a **proper subcomplex** only;
it is the local datum whose compatibility with its neighbours gluing must reconcile. Both name
their carrier through `math:overSheaf` and their region through `math:sectionRegion`, so
global-versus-local scope is inspectable rather than implied.

The core sheaf statement — **a computed feature is never automatically global truth** — is exactly
the cohomological reading H¹ = obstruction-to-gluing. A family of local sections lifts to one global
section precisely when their pairwise disagreements vanish; the residual disagreement is the
first-cohomology class H¹(X; F), carried here as `math:GluingObstruction` (`rdfs:seeAlso`
`math:SheafCohomology`, anchored to its sheaf through `math:obstructionOf`). A non-vanishing
obstruction certifies that consistent-looking local data does **not** assemble into a global
feature. This makes the obstruction **executable** rather than declarative:

- `math:MisscopedSectionConstraint` (a cross-node equality, lowered to a SHACL `sh:sparql`
  constraint) forbids conflating the two scopes — a section typed `math:GlobalSection` whose
  `math:sectionRegion` is a proper subcomplex rather than the whole base is misscoped
  (`math:MisscopedSection`).
- `math:SectionGluingConsistencyConstraint` (a **per-restriction-map** equality in the style of
  `math:BoundaryCoboundaryAdjunctionConstraint`, never an aggregation) makes the H¹ obstruction
  bite: along each `math:SheafRestrictionMap` the exact integer `math:stalkValue` at the map's
  `math:sourceObject` and `math:targetObject` stalks must agree (the identity/constant-transport
  gluing condition). A single map whose declared endpoint values disagree witnesses that the local
  data fail to glue (`math:SectionGluingInconsistency`) — presence of a section is not global truth.
  A section declaring no stalk values glues vacuously.

The frame gates keep the surface honest: a global section must name its `math:overSheaf` and
`math:sectionRegion` (`math:IncompleteGlobalSection`), a local section must anchor to a sheaf that
declares actual stalk/restriction semantics (`math:IncompleteSheafSection`) so bare "the sections
disagree" claims are unauthorable, and an obstruction must name the sheaf it obstructs
(`math:UnanchoredGluingObstruction`). This global/local surface is the enforcement mechanism for
**WS-B Req 7** (a computed feature is never automatically global truth): a downstream extension slice
that computes a feature over a cover states it as local data, and only a discharged
`math:GluingObstruction` promotes it to a global claim.

### Connections and holonomy

The **primary reading of a connection here is DISCRETE**: the `math:SheafRestrictionMap`s of a
`math:CellularSheaf` **are** a connection. A `math:Connection` names what it is a connection **on**
through `math:connectionOn`; for the discrete reading that is the sheaf itself
(`math:connectionOfSheaf`, a refinement of `math:connectionOn`), whose restriction maps transport
stalk data along each incidence. `math:ParallelTransport` is the **composition** of those restriction
maps along a declared path — it names its rule through `math:transportConnection` and its path
through `math:transportAlong`, reusing the object-layer `math:Morphism` parent rather than minting a
fresh map algebra. `math:Holonomy` is that transport composite around a **closed loop**: it names the
loop through `math:holonomyLoop` and the connection through `math:holonomyOf`, and its deviation from
the identity is the curvature witness.

**Flatness is closedness.** A `math:FlatConnection` is a `math:Connection` whose `math:transportCochain`
is a `math:Cocycle` — flat ⇔ closed. This is an **EL-safe, checkable** subclass restriction (a min-1
qualified + `allValuesFrom` restriction onto `math:Cocycle`), tying the connection surface directly to
the landed chain/cochain machinery: the transport cochain lives in the same cochain complex whose
`math:SheafLaplacian` and `math:HodgeDecomposition` measure incompatibility of local data, so a flat
connection is exactly one whose transport class is a δ-kernel element (a cocycle from
[the square-zero section](#chain-and-cochain-complexes--the-square-zero-laws-and-their-adjunction)).

The **secondary geometric reading** is the manifold one: a `math:Atlas` of `math:Chart`s with
`math:transitionMap`s, and a `math:Connection` whose `math:connectionOn` is that atlas (or a bundle),
with a `math:Holonomy` around a closed coordinate loop. Both readings share one term surface; the
frame gates keep them honest — a connection must name what it is **on** (`math:IncompleteConnection`),
a parallel transport must name **rule and path** (`math:IncompleteParallelTransport`), and a holonomy
must name **loop and connection** (`math:IncompleteHolonomy`).

This is deliberately a **minimal realization, not a full curvature calculus**. The universal
path-ordered curvature F = dA + A∧A — the infinitesimal holonomy accumulated around **every** loop —
is genuinely second-order (it quantifies over the unbounded family of all loops and, in the continuum,
over a path-ordered exponential), so it is **not** faked as a first-order law but recorded as the
honest higher-order gap `math:holonomyCurvatureBoundary` (`logic:expressivenessBoundary`
`logic:SecondOrder`, `logic:preservationKind` `logic:Unsupported`), referenced by `math:Connection`
through `math:definingLaw` and mirroring `math:smoothnessBoundary` and `math:compactnessBoundary`. The
discrete surface stays first-order and checkable — flat ⇔ closed is a restriction, and holonomy around
a **named** loop is a concrete composite of restriction maps — while the universal statement over all
loops is disclosed in the loss ledger rather than as silent prose.

### Hamiltonian systems

A `math:HamiltonianSystem` is framed by exactly one smooth state space, symplectic form,
Hamiltonian function, and generated flow. These roles are explicit because a scalar field or flow
alone does not determine the symplectic dynamical system. The frame is mathematical; a physical
interpretation of the Hamiltonian remains a downstream, vantage-bearing claim.

## Differential geometry and manifolds

Core classes: `math:Manifold`, `math:SmoothManifold`, `math:ComplexManifold`,
`math:RiemannianManifold`, `math:LorentzianManifold`, `math:Chart`, `math:Atlas`,
`math:CoordinateMap`, `math:TangentSpace`, `math:TensorField`, `math:MetricTensor`, and
`math:Complement`.

Core properties: `math:manifoldDimension`, `math:manifoldStructureKind`, `math:hasChart`,
`math:chartDomain`, `math:coordinateMap`, `math:targetCoordinateSpace`, `math:metricTensor`,
`math:tensorFieldOn`, `math:ambientSpace`, and `math:complementSemantics`.

A `math:Manifold` declares its **dimension** and its **structure kind** (topological, smooth, complex,
Riemannian, Lorentzian) — a manifold without both is ill-formed. A `math:Chart` names its domain,
coordinate map, and target coordinate space; a `math:Atlas` is a covering family of charts. A
`math:MetricTensor` (Riemannian or **Lorentzian**) is a `math:TensorField`; the Lorentzian metric is
the math object a physics slice needs for spacetime, and it stays here on the **math** side of the
boundary.

> **Hard rule — named complements.** A `math:Complement` names its `math:ambientSpace` and its
> `math:complementSemantics` (set-theoretic, orthogonal, complex-linear, topological, or
> quotient/cokernel). "The complement of X" without an ambient space and a complement-semantics is
> ill-formed — the flagship-adjacent KG example's "complex complement" is exactly this hazard, and
> the gate forbids it (`math:UnqualifiedComplement`,
> [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md)). The orthogonal-complement case is
> realized in [`MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md`](MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md).

### Conformal geometry and compactification

Core classes: `math:Compactification`, `math:ConformalCompactification`, and `math:BoundaryAtInfinity`.

Core properties: `math:originalSpace`, `math:compactifyingMap`, `math:compactifiedSpace`,
`math:boundaryAtInfinity`, and `math:conformalFactor`.

A `math:Compactification` embeds an **unbounded** space in a **bounded** (compact) one as a dense
subset, adjoining the points the original space runs off toward as a boundary at infinity. It is not
tied to any single construction — not the one-point (Alexandroff) versus Stone–Čech distinction *per
se* — but the general **structured record** every such construction instantiates, and it names **four
distinct roles**: the original unbounded space (`math:originalSpace`, a `math:TopologicalSpace` or
`math:Manifold`), the embedding (`math:compactifyingMap`, a `math:Function`/`math:CoordinateMap`), the
resulting bounded space (`math:compactifiedSpace`), and the boundary added at infinity
(`math:boundaryAtInfinity`, a `math:BoundaryAtInfinity`). A compactification missing any of the four is
ill-formed (`math:UnderspecifiedCompactification`) — a compactification that does not say what it
embeds, by which map, into what, and with which boundary is not a compactification.

A `math:BoundaryAtInfinity` is the **ideal points** — the conformal boundary — the original space
approaches but never reaches, made a first-class object rather than an informal "edge": for the radial
half-line *r* ∈ [0, ∞) it is the single ideal point *r* = +∞.

A `math:ConformalCompactification` is the **conformal (Penrose-style)** specialization: its
`math:compactifyingMap` is angle-preserving, carrying a `math:conformalFactor` Ω that rescales the
metric so the points at infinity sit at a **finite** conformal boundary. A subclass of
`math:Compactification`, it additionally names its `math:conformalFactor` (a positive
function/quantity — an individual, never a bare literal, so the property is an `owl:ObjectProperty`),
and one that omits it is ill-formed. This is the correct general home for embedding a
`math:LorentzianManifold`'s **radial chart's infinity as a finite boundary**: the conformal factor
sends the unbounded metric distance to a finite rescaled one while preserving the causal (angle)
structure the Lorentzian metric carries. The metric and its rescaling stay **math-side** — a
spacetime's Penrose diagram is a downstream physics reading of this math object, exactly as the
Lorentzian metric above is the math object a physics slice builds spacetime frames on.

> **Hard rule — the four compactification roles.** A `math:Compactification` names its
> `math:originalSpace`, its `math:compactifyingMap`, its `math:compactifiedSpace`, and its
> `math:boundaryAtInfinity`; a `math:ConformalCompactification` additionally names its
> `math:conformalFactor`. "The compactification of X" without all four roles (or a conformal one
> without its rescaling factor) is ill-formed — the gate forbids it
> (`math:UnderspecifiedCompactification`,
> [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md)), mirroring `math:UnderspecifiedManifold`
> and `math:UnderspecifiedChart`.

## The math/physics boundary

Manifolds, charts, tensor fields, and Lorentzian metrics are **mathematics** and stay in this slice.
**Spacetime, physical reference frames, worldlines, proper time, and the SR/GR regimes** are
**physics** — a downstream physics slice authors them, consuming this charter's manifolds and metrics
(the only referenceable frame standard is IVOA STC, an align-by-reference target,
[`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md)). This keeps the boundary as clean as the
`logic:`/`observations` boundaries the manifesto draws.

## A worked example — a chart on a Lorentzian manifold

```ttl
ex:minkowski
    a math:LorentzianManifold ;
    math:manifoldDimension 4 ;
    math:manifoldStructureKind math:lorentzianStructure ;
    math:hasChart ex:inertialChart ;
    math:metricTensor ex:minkowskiMetric .

ex:inertialChart
    a math:Chart ;
    math:chartDomain ex:minkowski ;
    math:coordinateMap ex:inertialCoordinateFn ;
    math:targetCoordinateSpace ex:R4 .
```

The manifold declares dimension and structure kind; the chart names its coordinate map and target;
the Lorentzian metric is a tensor field — all math-side, ready for a physics slice to build spacetime
frames on top.

## The flagship demonstrator — a signed radial field on a compactified Lorentzian 2-plane

Two co-equal worked scenes (`examples/signed-radial-field-qualitative.ttl` and
`examples/signed-radial-field-closed-form.ttl`) model the *same* target: a signed extended-real field
on a Lorentzian (1,1) 2-plane whose radial half-line is conformally compactified onto the open chart
x ∈ (0, 1). The field runs to +∞ at the central singularity (x → 0⁺, the ideal value R′), declines
strictly outward across four ring bands, spends a broad near-zero plateau on Ring 2 that *dominates*
the spatial measure, and runs to −∞ at compactified infinity (x → 1⁻). The **qualitative** scene
proves the whole target is expressible from the existing term surface with **no equation** — four
`math:FunctionPiece` bands over `math:Interval`s, strict outward `math:hasMonotonicity`, a Ring-2
`math:hasBound` plateau, two structured divergent `math:LimitResult`s on the poles, and four
comparable `math:MeasureEvaluation`s whose masses make Ring 2 strictly the largest. The
**closed-form** scene gives the exact curve T(x) = A·x⁻ᵖ − B·(1−x)⁻q + C as a structured
`math:definingExpression` AST (a `math:ClosedFormFunction` that is also a `math:PiecewiseFunction`),
every variable leaf resolving to the formal argument x or a declared parameter, and re-derives the
same rings, limits, and dominant-plateau measure. Together they demonstrate that a downstream
consumer slice can carry such a field without private substitutes — via qualitative structure or via
the exact form, interchangeably. The reading is left purely mathematical here: a downstream consumer
may interpret the central singularity, the outward decline, the flat far field, and the −∞ barrier
physically (a gravitational well, a potential, a far field, a repulsive edge), but that reading lives
entirely downstream and is never asserted math-side — the slice states only a signed extended-real
field over a compactified line.

## Shape and lint gates

Catalogued in [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md): a derivative/limit/series
names what it is of and its variable/mode; continuity/compactness/connectedness are declared, not
assumed; a manifold declares dimension and structure kind; a chart names domain, coordinate map, and
target space; and a complement names its ambient space and complement-semantics.

## Competency questions

1. What does this derivative differentiate, with respect to which variable and at what order?
2. In what mode does this series/sequence converge, and to what?
3. What are the open sets (or basis) of this space, and is the map continuous/compact/connected?
4. What is this manifold's dimension and structure kind, and what charts cover it?
5. For this complement, what is the ambient space and the complement-semantics?
6. For this compactification, what are its original space, compactifying map, compactified space,
   boundary at infinity, and (if conformal) conformal factor?
