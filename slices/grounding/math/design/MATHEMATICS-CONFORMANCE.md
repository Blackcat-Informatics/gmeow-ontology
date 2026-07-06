<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — Conformance and the Gate Matrix

> The **conformance charter** of the GMEOW Mathematics design set: it turns every "hard rule" stated
> in the sibling charters into a specific, traceable gate with a named failure class, and it fixes
> the preservation vocabulary the projection layer uses so mathematics loss is queryable alongside
> every other GMEOW lowering. It is the mathematical peer of
> `slices/grounding/logic/design/LOGIC-CONFORMANCE.md`. Where a sibling charter says "established by
> shapes, competency queries, and the loss ledger", this document says *by which shape, which query,
> which validator, and what failure is raised*.
>
> **Reading this charter.** The declarative present tense is normative: "X is enforced by G" means a
> conforming realization enforces X through gate G, and a violation raises the named failure class.

## The gate taxonomy

Every hard rule in the design set is enforced by exactly one *primary* gate of one of these kinds
(a rule may have secondary gates, but one owns the failure). The kinds are ordered from cheapest and
most declarative to most procedural:

- **OWL axiom** — subsumption, disjointness, domain/range carried in `module.ttl` and checked by the
  native reasoner over the reasoned graph.
- **SHACL Core** — node/property shapes in `shapes.ttl` (cardinality, datatype, value range,
  `sh:in`) lowered from the canonical `logic:` validation-shape node kind
  (`slices/grounding/logic/design/LOGIC-VALIDATION.md`).
- **SHACL-SPARQL** — constraints that need a query (uniqueness across a set, cross-node
  cardinality-by-role, contiguity), authored as `logic:` rules and projected to
  `sh:SPARQLConstraint`.
- **source-lint** — a Rust source-level check over the slice TTL before folding (the discipline that
  catches string-only computable expressions and dangling references early).
- **Rust validator** — a check in the future native crates (`math-ast`, `stats-model`) that is not
  expressible as a shape (normalization identity, contiguity in strict mode, kernel totality).
- **competency query** — a `queries/competency/*.rq` that must return the expected answer on the
  slice examples; a competency failure is a coverage gap, not a data violation.
- **projection test** — an acceptance query over a generated lowering that fails if loss is
  unrecorded or preservation is mis-declared.

Failure classes are IRIs in the `math:` failure vocabulary (`math:MathConformanceFailure`
subclasses), so a violation is itself a typed, queryable object, not a log line.

## The gate matrix

### Expression and mathematical-core rules

| Rule | Primary gate | Failure class |
|---|---|---|
| A computable expression is not represented only by a string literal | source-lint + SHACL Core | `math:StringOnlyComputableExpression` |
| An `ApplicationExpression` has exactly one operator | SHACL Core | `math:ApplicationOperatorCardinality` |
| Each `ArgumentSlot` has exactly one index and one expression | SHACL Core | `math:MalformedArgumentSlot` |
| Slot indexes are unique per application | SHACL-SPARQL | `math:DuplicateArgumentSlotIndex` |
| Strict-mode slot indexes are zero-based and contiguous | Rust validator | `math:NonContiguousArgumentSlots` |
| Every variable occurrence is bound or explicitly declared free | SHACL-SPARQL | `math:UnscopedVariableOccurrence` |
| A free variable declares type/domain context | SHACL Core | `math:UntypedFreeVariable` |
| A `SymbolReference` resolves locally or to a declared external symbol | Rust validator + mapping check | `math:UnresolvedSymbolReference` |
| A truth-valued expression lowered to `logic:` declares denotation kind and lowering preservation | SHACL Core | `math:UndeclaredLogicLowering` |
| A theorem/lemma/… role is asserted under a theory context (not as unconditional truth) | SHACL-SPARQL | `math:UnscopedStatementRole` |
| A `FormalVerificationResult` is grounded as an observation with a vantage | SHACL Core | `math:UngroundedVerificationResult` |

### Numbers-and-sets rules

| Rule | Primary gate | Failure class |
|---|---|---|
| A `math:Number` declares the number system it belongs to | SHACL Core | `math:UnsituatedNumber` |
| A `math:ApproximateValue` names the exact number it approximates and its error | SHACL Core | `math:ExactApproximateConflation` |
| A named constant is an exact individual, not a decimal literal | SHACL Core | `math:ConstantAsDecimalLiteral` |
| An intensional set's member condition denotes a `logic:` formula, not a string | SHACL Core | `math:StringOnlyMemberCondition` |
| A complement names its ambient space and its complement-semantics | SHACL Core | `math:UnqualifiedComplement` |
| A set is extensional or intensional, not silently both | SHACL-SPARQL | `math:AmbiguousSetExtent` |
| A `math:Function` declares its domain and codomain | SHACL Core | `math:UnframedFunction` |

### Algebra rules

| Rule | Primary gate | Failure class |
|---|---|---|
| An algebraic structure declares its underlying set, operation, and axioms | SHACL Core | `math:IncompleteAlgebraicStructure` |
| A ring declares the distributivity law tying its two operations together | SHACL Core | `math:NonDistributiveRing` |
| A homomorphism declares its preserved operation and preservation law | SHACL Core | `math:UnderspecifiedHomomorphism` |
| A preservation law denotes a `logic:` formula, not a string | SHACL Core | `math:StringOnlyPreservationLaw` |
| A Lie group declares its root system | SHACL Core | `math:IncompleteLieStructure` |
| A root system declares its Cartan matrix, Weyl group, and rank | SHACL Core | `math:IncompleteLieStructure` |
| An automorphism group is anchored to the structure it is the symmetry of | SHACL Core | `math:UnanchoredAutomorphismGroup` |
| A homomorphic-encryption scheme declares its homomorphic operation, hardness assumption, and noise model | SHACL Core | `math:UnderspecifiedEncryptionScheme` |
| The E8 root-system invariants (240 roots, rank 8, Weyl order 696,729,600) are the pinned answer | competency query | a mistyped invariant fails the exact-match competency gate |
| A root system claiming the E8 fingerprint (240 roots, rank 8) declares the true Weyl order 696,729,600 | SHACL-SPARQL | `math:WrongE8WeylOrder` |

Every algebra axiom and preservation law is authored as a real `logic:Formula` first-order AST (atoms
over `logic:relation` predications, the logical connectives, and quantifiers) — `math:` expresses the law and `logic:` owns
reasoning over it. Because a `logic:Formula` round-trips losslessly in canonical RDF 1.2 and weaker
projections carry any formula they cannot express as flagged unsupported residue (the
`logic:FormulaShape` tags), the algebra laws ride the existing projection / loss-ledger path: their
lowering to OWL/Datalog is recorded as non-exact with named residue in the same `logic:` loss ledger as
every other GMEOW lowering, so no new preservation vocabulary is minted (Principle 17). Algebra also
dogfoods GMEOW through the one-way `math:formalizes` annotation, and aligns to Wikidata (identity) and
Lean mathlib (structural, by reference).

### Measure-and-dimension rules

| Rule | Primary gate | Failure class |
|---|---|---|
| A `math:Measure` declares its measurable space and total mass (a non-negative number or `math:PositiveInfinity`) | SHACL Core | `math:IncompleteMeasure` |
| A `math:ProbabilityMeasure` has total mass one | SHACL Core | `math:ProbabilityMeasureMassViolation` |
| A `math:Integral` names its integrand, domain, and the measure it integrates against | SHACL Core | `math:IncompleteIntegral` |
| Every `math:Quantity` carries a `math:Dimension` | SHACL Core | `math:UndimensionedQuantity` |
| A `math:DerivedDimension` declares a non-empty exponent structure, each cell raising a `math:BaseDimension` to an exact-rational power, and a `math:DimensionalExpression` combines at least two operands | SHACL Core | `math:MalformedDimension` |
| An expression is dimensionally homogeneous — a `math:DimensionalExpression`'s operands share one dimension, and a `math:Integral`'s declared result dimension equals its integrand's dimension combined with its measure's — computed from the exact-rational ℚ⁷ exponent vectors | Rust validator (the exact executable lowering of the `logic:` laws `math:dimensionalHomogeneityLaw` / `math:integralDimensionCompositionLaw`) | `math:DimensionalInhomogeneity` |
| An authored `math:dimensionVector` string matches the canonical render of the structured exponents (a computed projection, never a divergent second source) | Rust validator | `math:MalformedDimension` |

The dimensional-homogeneity checks are the charter's distinguished **reasoned gate**: rather than
trusting an asserted dimension label, the native validator computes each dimension's exponent
vector in the ℚ-vector space over the seven SI base dimensions and derives homogeneity by exact
rational arithmetic (a product of dimensions adds exponent vectors; commensurability is vector
equality). This is what a units vocabulary that records conversions as data cannot express, and it
is why these rows are Rust-validator gates rather than SHACL.

The gate is not a bare Rust side-channel, however: the invariant it enforces is authored as two
real `logic:Formula` first-order ASTs in `module.ttl` — `math:dimensionalHomogeneityLaw`
(∀ operands of a `math:DimensionalExpression`, their dimensions are equal) and
`math:integralDimensionCompositionLaw` (a `math:Integral`'s result dimension equals its integrand's
composed with its measure's) — exactly as the algebra preservation laws are authored (`math:`
expresses the law, `logic:` owns reasoning over it; the relation atoms are reified as `logic:Type`
individuals per the HiLog reflection, carrying `rdfs:seeAlso` back to the first-class `math:`
property they reflect so no near-synonym is minted). The native ℚ⁷ validator is then declared as the
**executable lowering** of these two laws through `math:dimensionalHomogeneityLowering`, a loss-ledger
record carrying `logic:preservationKind logic:ExactPreservation`: because the validator decides the
laws' `dimEqual` and `dimProduct` conclusions by exact rational arithmetic over the exponent vectors,
it neither misses a genuine inhomogeneity nor reports a spurious one — it yields exactly the answers
the canonical laws entail. So the row above reads "Rust validator" as *the declared exact lowering of
the `logic:` law*, a first-class queryable object in the same loss ledger every other GMEOW lowering
rides (Principle 17), never a mere side-channel. A violation raises `math:DimensionalInhomogeneity`.

### Analysis-and-geometry rules

The analysis-and-geometry charter lands the subset of the mathematical-core binder
AST its operators consume (`math:BindingExpression` over the indexed argument-slot
AST, the declaration/occurrence split, and the `math:denotationKind` lowering seam),
so the binder-AST rules below are enforced here. Every rule is SHACL Core or
SHACL-SPARQL — none needs a native validator, so `native_contract_hash` is
untouched. The defining law of continuity (the preimage of an open set is open) is
authored as a first-order `logic:Formula` (`math:continuityLaw`), the exemplar
witness that "declared, not assumed" predicates over real structure rather than a
bare boolean.

| Rule | Primary gate | Failure class |
|---|---|---|
| Each `math:ArgumentSlot` has exactly one index and one expression | SHACL Core | `math:MalformedArgumentSlot` |
| Slot indexes are unique within one application/binder | SHACL-SPARQL | `math:MalformedArgumentSlot` |
| A `math:VariableOccurrence` resolves to a declaration (bound or explicitly free) | SHACL Core | `math:UnscopedVariableOccurrence` |
| A binder binds a variable over a body | SHACL Core | `math:MalformedBindingExpression` |
| A truth-valued expression lowered into `logic:` (`math:compilesToLogicFormula`) declares its denotation kind and preservation | SHACL Core | `math:UndeclaredLogicLowering` |
| A `math:Derivative` names what it differentiates, its variable, and its order | SHACL Core | `math:UnderspecifiedDerivative` |
| A `math:Limit` names its expression and its limit point (mode optional) | SHACL Core | `math:UnderspecifiedLimit` |
| A `math:Series`/`math:Sequence` carries a `math:Convergence` naming what it converges to and the mode | SHACL Core | `math:UnderspecifiedConvergence` |
| Continuity/compactness/connectedness are declared, not assumed | SHACL Core (backed by `math:continuityLaw`) | `math:UndeclaredTopologicalProperty` |
| A `math:Manifold` declares its dimension and its structure kind | SHACL Core | `math:UnderspecifiedManifold` |
| A `math:Chart` names its domain, coordinate map, and target coordinate space | SHACL Core | `math:UnderspecifiedChart` |
| A chart's target space (and a tangent space) has the same dimension as its manifold | SHACL-SPARQL | `math:DimensionMismatch` |
| A `math:MetricSignature`'s `p + q` equals the manifold's dimension, and its `(p, q)` split agrees with the structure kind (Riemannian ⇒ `q = 0`; Lorentzian ⇒ exactly one timelike) | SHACL-SPARQL | `math:DimensionMismatch` |
| **A `math:Complement` names its ambient space and its complement-semantics** | SHACL Core | `math:UnqualifiedComplement` |

The named-complement rule is the charter's distinguished gate: it generalizes the
bedrock set-theoretic complement (`math:complementWithin`, replaced) to
`math:ambientSpace` + `math:complementSemantics` (set-theoretic, orthogonal,
complex-linear, topological, or quotient/cokernel) — "the complement of X" without
an ambient space and a named semantics is ill-formed. A `math:HomologyGroup` is a
`math:AbelianGroup`, so it inherits the algebra structure gate (`underlyingSet`,
`structureOperation`, `satisfiesAxiom`); a homology-group individual that is not a
fully-framed abelian group raises `math:IncompleteAlgebraicStructure`, not a
topology failure.

### Linear-algebra-and-learning rules

Every rule is SHACL Core (or a SHACL-SPARQL uniqueness constraint reused from the
mathematical-core AST) — none needs a native validator, so `native_contract_hash`
is untouched. The distinguished discipline is that a decomposition or embedding
declares its inputs, policy, and outputs, and any *meaning* read off a residual or a
latent dimension is a `gmeow:Observation` held from a `gmeow:vantage`, never a
property of the vector.

| Rule | Primary gate | Failure class |
|---|---|---|
| A `math:OrthogonalComplement` names its ambient space and complement-semantics (inherited from `math:Complement`) and additionally the inner product defining it (`math:definedByInnerProduct`) | SHACL Core | `math:UnqualifiedOrthogonalComplement` |
| A `math:PCAAnalysis` declares its input, centering and scaling policy, covariance operator, eigensolver, and its component / loading / score / explained-variance / residual outputs | SHACL Core | `math:IncompletePCAAnalysis` |
| **The meaning of a residual, component, or latent dimension is a `math:ResidualInterpretationClaim` — a `gmeow:Observation` with a `gmeow:vantage` and a result — never a property (no direct meaning property is minted)** | SHACL Core | `math:ResidualMeaningAsProperty` |
| A `math:Embedding` names its source, target space, function, and model | SHACL Core | `math:UnderspecifiedEmbedding` |
| A `math:TensorComputationGraph` declares its computation nodes, which are `math:ApplicationExpression`s reusing the argument-slot AST (so the inherited slot-uniqueness/well-formedness gates bite) | SHACL Core (+ inherited `math:SlotIndexUniquenessShape`) | `math:MalformedTensorComputationGraph` / `math:MalformedArgumentSlot` |
| A `math:WeightTensor` names the `math:ParameterSpace` it lives in | SHACL Core | `math:UnframedWeightTensor` |

The residual-meaning rule is the charter's distinguished gate: because no direct
"meaning" property is minted, the only way to state what a residual or latent
dimension means is a `math:ResidualInterpretationClaim`, which this gate then forces
to carry its `gmeow:vantage` and result — semantic meaning read off geometry is
inference from a standpoint (Principle 9), so the property-form of meaning is
unauthorable by construction. The tensor-computation-graph rule reuses the
expression AST wholesale rather than minting a parallel structure: a graph node *is*
a `math:ApplicationExpression`, so a node with duplicate argument-slot indexes trips
the same `math:SlotIndexUniquenessShape` that guards every application and binder.
`math:PCAAnalysis` is a `gmeow:Activity` (the analysis process), its components and
residuals are result objects, and the interpretation is the held claim — the
process / result / claim separation, realized across the `math:` and `gmeow:` layers.

### Probability rules

| Rule | Primary gate | Failure class |
|---|---|---|
| A `ProbabilityValue` lies in `[0, 1]` | SHACL Core (range) + Rust numeric check | `math:ProbabilityOutOfBounds` |
| Odds/log-odds are modeled as scale transforms, not as `ProbabilityValue` | OWL axiom (disjointness) | `math:ProbabilityScaleConflation` |
| A probability value names its probability frame/model | SHACL Core | `math:UnframedProbabilityValue` |
| A probability value is not inferred from confidence without a declared mapping | source-lint + SHACL-SPARQL | `math:ConfidenceAsProbability` |
| A `ProbabilitySpace` has sample-space, σ-algebra, and measure objects (possibly symbolic) | SHACL Core | `math:IncompleteProbabilitySpace` |
| A `RandomVariable` has domain/codomain and a space or distribution context | SHACL Core | `math:UnscopedRandomVariable` |
| A `Distribution` has a family and a parameterization | SHACL Core | `math:MissingDistributionParameterization` |
| Each required parameter role is supplied exactly once | SHACL-SPARQL | `math:DistributionParameterRoleCardinality` |
| Parameter quantities satisfy the role's dimensional/positivity constraints | Rust validator | `math:DistributionParameterConstraint` |
| A conditional/conditional-independence assertion names its conditioning set | SHACL Core | `math:UnconditionedAssertion` |
| A probability-model lowering into `logic:` is declared for a reasoning-facing model | Rust validator | `math:MissingProbabilityModelLowering` |
| A dependency-model declaration is structurally complete (DAG, CPT/factor totality) | Rust validator | `math:IncompleteDependencyModel` |

### Statistics rules

| Rule | Primary gate | Failure class |
|---|---|---|
| An `Estimate` references its estimand and/or estimated parameter, and its estimator | SHACL Core | `math:UnderspecifiedEstimate` |
| A `FittedModel` references data and a model specification | SHACL Core | `math:UnfittedModel` |
| A `PValue` references a test, null hypothesis, test statistic, null distribution, and tail/sidedness | SHACL-SPARQL | `math:IllFramedPValue` |
| A `ConfidenceInterval` has lower/upper bounds and a confidence level | SHACL Core | `math:IncompleteConfidenceInterval` |
| A `CredibleInterval` has a posterior context and a credible mass | SHACL Core | `math:IncompleteCredibleInterval` |
| Confidence and credible intervals are not interchanged | OWL axiom (disjointness) | `math:IntervalKindConflation` |
| An `EffectSize` identifies its contrast, scale, and frame | SHACL Core | `math:UnframedEffectSize` |
| A `ModelDiagnostic` identifies the fitted model and the diagnostic method | SHACL Core | `math:UnanchoredDiagnostic` |
| A missingness mechanism is explicit where an analysis depends on it | SHACL-SPARQL | `math:ImplicitMissingness` |

### Process/result/claim separation

| Rule | Primary gate | Failure class |
|---|---|---|
| An inference/analysis *process* is a `gmeow:Activity`, not typed as an `Observation` | OWL axiom (disjointness) | `math:ProcessObservationConflation` |
| A held statistical/probabilistic *result claim* is an `Observation` with a vantage | SHACL Core | `math:UngroundedResultClaim` |
| The structured *result object* (estimate, p-value, posterior) is neither the process nor the claim | OWL axiom | `math:ResultRoleConflation` |

### Projection rules

| Rule | Primary gate | Failure class |
|---|---|---|
| Every projection declares its unsupported constructs | projection test | `math:UndeclaredUnsupportedConstruct` |
| Every projection declares a `logic:preservationKind` | projection test | `math:MissingPreservationKind` |
| No projection silently converts confidence to probability | projection test | `math:ProjectionConfidenceAsProbability` |
| No projection silently drops distribution parameterization | projection test | `math:ProjectionDroppedParameterization` |
| No projection flattens an expression AST to a string without recording loss | projection test | `math:UnrecordedProjectionLoss` |
| A declared-exact projection round-trips (section/retraction) on the conformance corpus | projection test | `math:ExactPreservationViolated` |

### Bridges / ingestion rules

A bridge is the mnemomorphic `put` leg of a `logic:Correspondence` — GMEOW is the source `S`, the
external artifact the view `V`, and the lift is the up-projection `put`, never a `get` run backward
(the calculus's named anti-pattern). The three bridge runs (`math:RIngestRun`, `math:ONNXIngestRun`,
`math:ProofIngestRun`) share the additive unifier `math:IngestRun` (a `gmeow:Activity`), and the
rules below turn the shared bridge contract into gates.

| Rule | Primary gate | Failure class |
|---|---|---|
| A bridge run is a `gmeow:Activity` (the executed `put`-leg occurrence, not an Observation) | OWL axiom (subclass) + structural | (structural assertion) |
| A bridge run retains a `logic:loadBearing` `math:parseSource` witness and carries the process-layer in-band witness (`logic:instantiatesSchema` / `logic:instantiatesPlan`) | SHACL Core | `math:UngroundedIngestRun` |
| A bridge's lift is lawful — its residue is carried in the `logic:mnemomorphic` witness or enumerated `unsupported`; an unsupported or silently-partial drop hard-fails | Rust validator | `math:UnliftableIngest` |
| A proof QED result object is grounded *by* an observation with a vantage (result ≠ claim) | SHACL Core | `math:UngroundedVerificationResult` |
| A `math:FittedModel` references data (`math:fittedToData`) and a model specification (`math:modelFormula`) | SHACL Core | `math:UnfittedModel` |

The unliftable-ingest rule is the charter's distinguished **native validator** gate. It is not a bare
side-channel: it is the process-layer projection of the correspondence calculus's Overclaim and
Mnemomorphism verdict. Because a bridge is a `put` leg, an unliftable residue — content the forward
lift dropped with no witness to recover it and no `unsupported` enumeration to record it — is the
`logic:ObligationViolated` / `unsupported` outcome those gates decide, so the native lint reuses the
shared `logic:` discharge and loss-ledger vocabulary verbatim (Principle 17) rather than minting a
`math:` preservation shadow. A bridge hard-fails on the unliftable; it never emits a degraded or
string-valued placeholder, because "for *any* input" is a universality bar, not a best-effort
aspiration.

## Preservation vocabulary — reuse, do not re-mint

Projection preservation uses the **existing** `logic:` loss-ledger vocabulary verbatim, so
mathematics loss is queryable in the same ledger as OWL, Datalog, SHACL, and correspondence
lowerings. The mathematics slice mints **no** near-synonyms.

| Design-set prose | Canonical `logic:` term |
|---|---|
| "exact" | `logic:ExactPreservation` |
| "sound but incomplete" | `logic:SoundUnderApproximation` |
| "complete but unsound" | `logic:CompleteOverApproximation` |
| the polarity property | `logic:preservationKind` |
| "lossy with named drops" | a preservation record with `unsupportedConstruct` entries (not a distinct polarity) |

A mathematics projection is therefore a `logic:Correspondence` lowering carrying a
`logic:preservationKind` and, where lossy, an enumeration of unsupported constructs — the same shape
the correspondence calculus uses (`slices/grounding/logic/design/LOGIC-CORRESPONDENCE.md`). "Lossy with
named drops" is not a fourth polarity; it is a non-exact preservation with its drops enumerated.

## The conformance corpus

Acceptance is demonstrated by a fixture corpus under the slice, not asserted. Each hard rule above
has at least one **positive** fixture (a well-formed artifact that passes) and one **negative**
fixture (a minimal violation that raises exactly the named failure class). The competency queries in
`queries/competency/` run against the positive fixtures; the negative fixtures live under the
counter-example convention the logic slice already uses (`tests/counter-examples/`). A rule with no
negative fixture is not considered enforced, however green the positive path looks — the negative
fixture is what proves the gate actually bites.

## What conformance does not claim

Conformance establishes that the *ontology source and its projections* satisfy their gates. It does
**not** claim that any mathematical statement is *true*, that any distribution is *well-specified for
its domain*, or that any statistical result is *correct* — those are claims held from vantages under
theories and analysis plans, exactly as the manifesto requires. The gate matrix enforces
*well-formedness and preservation*, never mathematical or empirical truth.
