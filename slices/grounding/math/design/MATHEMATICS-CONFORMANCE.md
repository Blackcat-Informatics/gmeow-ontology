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
- **SHACL Core** — node/property shapes derived from EL-safe OWL/RDFS axioms in `module.ttl`
  (cardinality, datatype, class, and value constraints), under the canonical validation doctrine
  (`slices/grounding/logic/design/LOGIC-VALIDATION.md`).
- **SHACL-SPARQL** — constraints that need a query (uniqueness across a set, cross-node
  cardinality-by-role, contiguity), authored as `logic:` rules and projected to
  `sh:SPARQLConstraint`.
- **source-lint** — a Rust source-level check over the slice TTL before folding (the discipline that
  catches string-only computable expressions and dangling references early).
- **Rust validator** — a native check for obligations that genuinely need specialized execution
  (normalization identity and kernel totality); slot contiguity is already a canonical
  `logic:Constraint` projected to SHACL-SPARQL.
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
| A computable expression is not represented only by a string literal | source-lint + SHACL-SPARQL | `math:StringOnlyComputableExpression` |
| An `ApplicationExpression` has exactly one operator | SHACL Core | `math:ApplicationOperatorCardinality` |
| Each `ArgumentSlot` has exactly one index and one expression | SHACL Core | `math:MalformedArgumentSlot` |
| Slot indexes are unique per application | SHACL-SPARQL (`math:ArgumentSlotIndexUniquenessConstraint`) | `math:DuplicateArgumentSlotIndex` |
| Slot indexes are non-negative, zero-based, and contiguous | canonical `logic:Constraint` → SHACL-SPARQL (`math:ArgumentSlotContiguityConstraint`), composed with the non-negative and uniqueness gates | `math:NonContiguousArgumentSlots` |
| Every variable occurrence is bound or explicitly declared free | SHACL-SPARQL | `math:UnscopedVariableOccurrence` |
| The `math:argumentSlot` / `math:slotExpression` graph of an application or binding expression contains no cycle back through a node already being lowered | Rust validator (native graph-traversal cycle guard; cycle detection over an authored graph is not a flat relational join, so there is no SHACL/Datalog derivation) | `math:CyclicExpressionGraph` |
| Lowering an application or binding expression does not recurse past the native lowering's maximum supported expression-graph depth | Rust validator (native recursion-depth guard; the SAME "no flat relational join" reasoning as the cycle guard above) | `math:ExpressionDepthExceeded` |
| A free variable declares type/domain context | SHACL Core | `math:UntypedFreeVariable` |
| A `SymbolReference` resolves to exactly one local `math:MathematicalSymbol`; external identifiers align from that symbol | SHACL Core derived from the exact-one OWL restriction | `math:UnresolvedSymbolReference` |
| A truth-valued expression lowered to `logic:` declares denotation kind and lowering preservation | SHACL-SPARQL (`math:LogicLoweringDeclaredConstraint`, a guarded-implication requiring `math:denotationKind` and `math:logicLoweringPreservation`) | `math:UndeclaredLogicLowering` |
| A theorem/lemma/… role is asserted under a theory context (not as unconditional truth) | SHACL-SPARQL | `math:UnscopedStatementRole` |
| A `FormalVerificationResult` is grounded as an observation with a vantage | SHACL-SPARQL (`math:FormalVerificationResultVantageGroundingConstraint`, a conditional-existence rule over the grounding observation and its vantage) | `math:UngroundedVerificationResult` |
| A `math:Theorem` (or a statement carrying `math:statementRole math:roleTheorem`) carries a theory context (`math:roleInTheory`) and is warranted by an in-graph `math:Proof` (`math:provesStatement`) or a declared `math:externalWarrant` — theorem-hood is a role held under a named, versioned theory with a proof or external warrant, never a fact read off the type | SHACL-SPARQL (`math:TheoremWarrantConstraint`, a disjunctive existence over the proof and warrant arms) | `math:UngroundedTheoremClaim` |
| Every `math:ArithmeticOperation` carries its signature — a `math:operatorDomain` and a `math:operatorCodomain`, each a `math:NumberSystem` (the required-exactly-one restriction targets the class, so all eight operators are framed) | SHACL Core (OWL-axiom tier — paired `owl:maxQualifiedCardinality`/`owl:minQualifiedCardinality` exact-one restrictions on `math:ArithmeticOperation` in `module.ttl`) | `math:UnframedOperator` |
| A `math:ClosedFormFunction` names both its body (`math:definingExpression`) and its formal argument (`math:formalArgument`); its `math:functionParameter`s are unconstrained (0..n), and its `math:domain`/`math:codomain` come from the inherited `math:Function` gate | SHACL Core (OWL-axiom tier — paired `owl:maxQualifiedCardinality`/`owl:minQualifiedCardinality` exact-one restrictions on `math:ClosedFormFunction` in `module.ttl`) | `math:UnboundClosedForm` |

### Normalization identity rules

`math:normalForm` stays a plain edge from an expression to its declared normal form; the attributed
reason FOR that edge is the mediating `math:NormalizationDeclaration`, naming the source
(`math:normalizes`), the target (`math:normalizesTo` — the SAME object the `math:normalForm` edge
names), the `math:NormalizationStrength` the claim is held at, and, for a claim coarser than
structural, the `math:NormalizationProcedure` that licenses it. "These formulas are the same" is an
inferential act, not a lexical fact, so the gates below enforce that a normal-form claim is always
declared, that every declaration commits to a named strength (rather than silently discharging the
whole contract for free), that a structural-strength claim is checked directly against the computed
`math:structuralKey` digests, and that a coarser claim is attributed to a vantage and a procedure.
The structural-key digest itself is a computed projection of an expression's AST — never an
independently authored value — so it may drift from its own recomputation, leak surface-stratum
material into an identity computation that must stay structural, or be claimed for an expression the
grammar rejects — or, before any of that is even decided, the authored `math:structuralKey` usage
itself might not be a well-formed singleton literal in the first place; the four Rust-validator rows
below are the SAME architectural shape as `math:MalformedDimension` / `math:NonPositiveDefiniteNorm` /
`math:AsymmetricGramMatrix` (a plain Rust computation over the frozen reasoned graph, never a
divergent second source), so none of them carries a backing `logic:Constraint`.

| Rule | Primary gate | Failure class |
|---|---|---|
| Every `math:normalForm` edge from a source expression to a target expression has a `math:NormalizationDeclaration` naming that same pair (`math:normalizes` the source, `math:normalizesTo` the target) — a normal-form claim is never a bare edge | SHACL-SPARQL (`math:UndeclaredNormalFormConstraint`, a closed-world conditional-existence rule) | `math:UndeclaredNormalForm` |
| A `math:NormalizationDeclaration` names exactly one `math:normalizationStrength` — a strength-less declaration engages neither the structural guard below nor the coarser-than-structural guard below (both are gated on an ASSERTED strength before their existential body ever runs), so without this restriction it would silently satisfy the whole normal-form contract for free | SHACL Core (OWL-axiom tier — min-qualified-cardinality-1 restriction on `math:NormalizationDeclaration`'s `math:normalizationStrength` in `module.ttl`) | `math:UndeclaredNormalizationStrength` |
| A `math:NormalizationDeclaration` held at `math:structuralNormalization` has a `math:normalizes` source and a `math:normalizesTo` target whose `math:structuralKey` digests agree (and neither is missing) — structural identity is decided directly against the computed digests, never asserted on faith | SHACL-SPARQL (`math:FalseStructuralNormalizationClaimConstraint`, a closed-world digest-equality rule) | `math:FalseStructuralNormalizationClaim` |
| A `math:NormalizationDeclaration` held at any strength coarser than `math:structuralNormalization` carries a `math:normalizationProcedure`, is named by a `gmeow:Observation` (via `gmeow:observationResult`) that itself carries a `gmeow:vantage`, and carries a `logic:preservationKind` — a coarser-than-structural equivalence is a claim held by a vantage, attributed to a procedure, and preservation-judged, never asserted for free | SHACL-SPARQL (`math:UnattributedNormalizationConstraint`, a closed-world conditional-existence rule) | `math:UnattributedNormalization` |
| A `math:MathematicalExpression`'s `math:structuralKey` usage, when asserted, is a well-formed singleton literal — never two or more asserted values (of any kind), nor a single non-literal value, either of which can never be safely read as "the first value found" without silently masking a contradictory or ill-typed second value | Rust validator | `math:MalformedStructuralKey` |
| A `math:MathematicalExpression`'s authored `math:structuralKey` equals the digest recomputed from its own structure by the `math:` expression lowering — the key is a computed projection of the expression's α-equivalence class identity, never an independently authored value | Rust validator | `math:StructuralKeyDrift` |
| A `math:NormalizationDeclaration`'s structural-identity computation is not contaminated by surface-stratum material — the declaration itself, its `math:normalizes` source, or its `math:normalizesTo` target carries no `math:rendersAs` edge crossing into the computation | Rust validator | `math:SurfaceLeakInNormalForm` |
| A `math:MathematicalExpression` carries an authored `math:structuralKey` only when the `math:` expression lowering accepts its own AST as well-formed — a structural-identity claim cannot be made for an expression the grammar itself rejects | Rust validator | `math:StructuralKeyOnRejectedExpression` |

### Numbers-and-sets rules

| Rule | Primary gate | Failure class |
|---|---|---|
| A `math:Number` declares the number system it belongs to | SHACL Core | `math:UnsituatedNumber` |
| A `math:ApproximateValue` names the exact number it approximates and its error | SHACL Core | `math:ExactApproximateConflation` |
| A named constant is an exact individual, not a decimal literal | SHACL Core | `math:ConstantAsDecimalLiteral` |
| A signed-extended-real slot holds a finite number (either sign), `math:PositiveInfinity`, or `math:NegativeInfinity` | SHACL-SPARQL (`math:ExtendedRealValueConstraint`, a literal-or-one-of-two-poles disjunction) | `math:MalformedExtendedReal` |
| An intensional set's member condition denotes a `logic:` formula, not a string | SHACL-SPARQL (`math:SetBuilderMemberConditionNodeKindConstraint`, the `logic:` node-kind gate) | `math:StringOnlyMemberCondition` |
| A complement names its ambient space and its complement-semantics | SHACL Core | `math:UnqualifiedComplement` |
| A set is extensional or intensional, not silently both | SHACL-SPARQL | `math:AmbiguousSetExtent` |
| A `math:Interval` names both endpoints and both endpoint inclusions (inclusion is never silently omitted) | SHACL Core (paired `owl:maxQualifiedCardinality`/`owl:minQualifiedCardinality` exact-one restrictions on all four properties in `module.ttl`) | `math:UnderspecifiedInterval` |
| A `math:Function` declares its domain and codomain | SHACL Core | `math:UnframedFunction` |
| A `math:PiecewiseFunction` declares at least one `math:hasPiece`, and every `math:FunctionPiece` names exactly one `math:pieceDomain` (a `math:Interval`) | SHACL Core (`owl:minQualifiedCardinality` 1 on `math:hasPiece`; paired exact-one restrictions on `math:FunctionPiece`'s `math:pieceDomain` in `module.ttl`) | `math:UnderspecifiedPiecewiseFunction` |

### Algebra rules

| Rule | Primary gate | Failure class |
|---|---|---|
| An algebraic structure declares its underlying set, operation, and axioms | SHACL Core | `math:IncompleteAlgebraicStructure` |
| A ring declares the distributivity law tying its two operations together | SHACL Core | `math:NonDistributiveRing` |
| A homomorphism declares its preserved operation and preservation law | SHACL Core | `math:UnderspecifiedHomomorphism` |
| A preservation law denotes a `logic:` formula, not a string | SHACL-SPARQL (`math:PreservationLawNodeKindConstraint`, a path node-kind rule projected as a SPARQL-AF constraint) | `math:StringOnlyPreservationLaw` |
| A Lie group declares its root system | SHACL Core | `math:IncompleteLieStructure` |
| A root system declares its Cartan matrix, Weyl group, and rank | SHACL Core | `math:IncompleteLieStructure` |
| An automorphism group is anchored to the structure it is the symmetry of | SHACL Core | `math:UnanchoredAutomorphismGroup` |
| A homomorphic-encryption scheme declares its homomorphic operation, hardness assumption, and noise model | SHACL Core | `math:UnderspecifiedEncryptionScheme` |
| The E8 root-system invariants (240 roots, rank 8, Weyl order 696,729,600) are the pinned answer | competency query | a mistyped invariant fails the exact-match competency gate |
| A root system claiming the E8 fingerprint (240 roots, rank 8) declares the true Weyl order 696,729,600 | SHACL-SPARQL | `math:WrongE8WeylOrder` |
| A `math:CliffordAlgebra` declares its scalar field, signature, basis, grading, exact dimension, pseudoscalar square, involution, basis blade, carrier, product, and axioms | SHACL Core derived from OWL restrictions | `math:IncompleteCliffordAlgebra` |
| A `math:CliffordExtension` names its base algebra, extended algebra, and extension generator | SHACL Core derived from OWL restrictions | `math:IncompleteCliffordExtension` |
| A `math:CliffordModuleDecomposition` names both summands and carries a true exact split/join witness | SHACL Core derived from OWL restrictions + native producer test | `math:IncompleteCliffordExtension` |

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
| A `math:MeasureEvaluation` names all three of its evaluated measure, measured subset, and result (so μ(A) is comparable, not a display string) | SHACL Core (paired `owl:maxQualifiedCardinality`/`owl:minQualifiedCardinality` exact-one restrictions on all three roles in `module.ttl`) | `math:UnderspecifiedMeasureEvaluation` |
| A `math:MeasureEvaluation`'s `math:measureResult` is non-negative — a finite non-negative number or `math:PositiveInfinity`, never `math:NegativeInfinity` (a measure is non-negative) | SHACL-SPARQL (`math:MeasureResultNonNegativeConstraint`, the `logic:` forbidden-value gate) | `math:UnderspecifiedMeasureEvaluation` |
| A `math:Integral` names its integrand, domain, and the measure it integrates against | SHACL Core | `math:IncompleteIntegral` |
| Every `math:Quantity` carries a `math:Dimension` | SHACL Core | `math:UndimensionedQuantity` |
| A `math:DerivedDimension` declares a non-empty exponent structure, each cell raising a `math:BaseDimension` to an exact-rational power, and a `math:DimensionalExpression` combines at least two operands | SHACL Core | `math:MalformedDimension` |
| An expression is dimensionally homogeneous — a `math:DimensionalExpression`'s operands share one dimension, and a `math:Integral`'s declared result dimension equals its integrand's dimension combined with its measure's — computed from the exact-rational ℚ⁷ exponent vectors | Rust validator (the exact executable lowering of the `logic:` laws `math:dimensionalHomogeneityLaw` / `math:integralDimensionCompositionLaw`) | `math:DimensionalInhomogeneity` |
| An authored `math:dimensionVector` string matches the canonical render of the structured exponents (a computed projection, never a divergent second source) | Rust validator | `math:MalformedDimension` |

The dimensional-homogeneity checks are the charter's distinguished **reasoned gate**: rather than
trusting an asserted dimension label, the check computes each dimension's exponent vector in the
ℚ-vector space over the seven SI base dimensions and derives homogeneity by exact rational
arithmetic (a product of dimensions adds exponent vectors; commensurability is vector equality).
This is what a units vocabulary that records conversions as data cannot express, and it is why these
rows are Rust-validator gates rather than plain SHACL.

The gate is not a bare Rust side-channel, however: the invariant it enforces is authored as two
real `logic:Formula` first-order ASTs in `module.ttl` — `math:dimensionalHomogeneityLaw`
(∀ operands of a `math:DimensionalExpression`, their dimensions are equal) and
`math:integralDimensionCompositionLaw` (a `math:Integral`'s result dimension equals its integrand's
composed with its measure's) — exactly as the algebra preservation laws are authored (`math:`
expresses the law, `logic:` owns reasoning over it; the relation atoms are reified as `logic:Type`
individuals per the HiLog reflection, carrying `rdfs:seeAlso` back to the first-class `math:`
property they reflect so no near-synonym is minted). `math:DimensionalInhomogeneity` is decided
directly by the reasoner, not by a standalone Rust sweep reading the same conclusions off the side:
the two `logic:Constraint`s that formalize these laws (`math:DimensionalHomogeneityConstraint`,
`math:IntegralDimensionCompositionConstraint`) are compiled into violation-emitting forward
`EvalRule`s (`crates/logic/src/reason/math_gate.rs`) and driven through a native forward semi-naive
chase over the reasoned closure `verify()` checks, so the marker is **reasoner-derived from the
authored laws**, never a Rust side-channel decision — `crates/logic/src/math_dimension.rs` (which
still separately decides `math:MalformedDimension`, `math:AsymmetricGramMatrix`, and
`math:NonPositiveDefiniteNorm` by its own exact-rational sweep, since those three are genuine
computations no relational join can express) explicitly documents that it no longer computes
dimensional homogeneity for exactly this reason, so the law has one committed source of truth rather
than a Rust sweep and a reasoner path agreeing by construction. The row above still reads "Rust
validator" — the exact-rational ℚ⁷ arithmetic the compiled `EvalRule`s execute is native code, not a
SHACL/Datalog-expressible join — but the "Rust validator" here names the reasoner's own compiled
lowering of the authored `logic:Formula` law, not a standalone side-channel sweep. A violation raises
`math:DimensionalInhomogeneity`.

The same reasoned-graph sweep (`crates/logic/src/math_dimension.rs`) that decides
`math:MalformedDimension`'s zero-denominator-exponent case also certifies every authored
`math:GramMatrix`, since neither symmetry nor an exact LDLᵀ positive-definiteness factorization is a
flat relational join SHACL/Datalog can express:

| Rule | Primary gate | Failure class |
|---|---|---|
| A `math:RationalValue` declares a non-zero `math:denominator` — `p/0` is not a rational value | SHACL-SPARQL (`math:RationalValueDenominatorNonZeroConstraint`, the `logic:` forbidden-value gate) | `math:ZeroDenominator` |
| A `math:GramMatrix` is symmetric — every `math:MatrixEntry` at (row, column) has a transpose entry at (column, row) carrying the same `math:entryValue` | Rust validator (the exact-rational transpose-equality sweep; declarative twin `math:GramMatrixSymmetryConstraint` → SHACL-SPARQL) | `math:AsymmetricGramMatrix` |
| A `math:Norm` induced by a symmetric bilinear form, or a `math:GramMatrix` authored `math:definiteness math:positiveDefinite`, is genuinely positive-definite — certified by the exact-rational LDLᵀ factorization (all pivots `> 0` by Sylvester's criterion), the sole positive-definiteness enforcement point the runtime distance builtin trusts | Rust validator (declarative twin `math:NormPositiveDefiniteConstraint` / `math:GramPositiveDefiniteConstraint` → SHACL Core / SHACL-SPARQL) | `math:NonPositiveDefiniteNorm` |

### Analysis-and-geometry rules

The analysis-and-geometry charter lands the subset of the mathematical-core binder
AST its operators consume (`math:BindingExpression` over the indexed argument-slot
AST, the declaration/occurrence split, and the `math:denotationKind` lowering seam),
so the binder-AST rules below are enforced here. Every rule is SHACL Core or
SHACL-SPARQL — none needs a native validator, so `native_contract_hash` is
untouched. Every declared topological property is backed by its real defining law,
not prose: continuity (`math:continuityLaw`), connectedness (`math:connectednessLaw`),
and the T0–T4 separation axioms (`math:t0SeparationLaw` … `math:normalitySeparationLaw`)
are each authored as a first-order `logic:Formula`. Compactness is the lone
second-order property — "every open cover has a finite subcover" quantifies over
families and appeals to finiteness, which is not first-order axiomatizable — so it is
not faked as a formula but carried as an honest loss-ledger boundary
(`math:compactnessBoundary`, `logic:expressivenessBoundary logic:SecondOrder`,
`logic:preservationKind logic:Unsupported`). The discriminant: quantification over
individual points/open-sets/closed-sets is first-order over the reified signature;
quantification over families or an appeal to finiteness is second-order. These gates
enforce declaration discipline (the property is declared, not assumed), not
satisfaction of the law against a model.

The qualitative **analytic** properties of a function follow the same discipline.
Monotonicity, non-affinity, convexity, and boundedness are each a real first-order
`logic:Formula` (`math:strictMonotonicityLaw` … `math:constantMapLaw`,
`math:nonAffinityLaw`, `math:convexityLaw` — the honestly-expressible midpoint form,
its λ-general residue disclosed rather than faked — and `math:boundednessLaw`), so a
`math:MonotonicityKind` or a `math:AnalyticProperty` is a quantified statement over
the reified real/value signature, never a bare token (`math:UnbackedAnalyticProperty`).
**Smoothness** (C^∞ / real-analyticity) is the analytic charter's second-order
property — it quantifies over the infinite family of *all* derivatives, which is not
first-order axiomatizable — so, exactly as compactness is, it is not faked as a
formula but carried as an honest loss-ledger boundary (`math:smoothnessBoundary`,
`logic:expressivenessBoundary logic:SecondOrder`, `logic:preservationKind logic:Unsupported`),
reusing the existing preservation vocabulary verbatim (Principle 17). This is the
explicit-expressiveness-boundary arm: the residue is disclosed in the ledger, never
silent prose.

| Rule | Primary gate | Failure class |
|---|---|---|
| Each `math:ArgumentSlot` has exactly one index and one expression | SHACL Core | `math:MalformedArgumentSlot` |
| Slot indexes are unique within one application/binder | SHACL-SPARQL | `math:DuplicateArgumentSlotIndex` |
| Slot indexes form the strict zero-based contiguous sequence 0..n−1 | canonical `logic:Constraint` → SHACL-SPARQL | `math:NonContiguousArgumentSlots` |
| Each `math:SymbolReference` resolves to exactly one local `math:MathematicalSymbol` | SHACL Core | `math:UnresolvedSymbolReference` |
| A `math:VariableOccurrence` resolves to a declaration (bound or explicitly free) | SHACL Core | `math:UnscopedVariableOccurrence` |
| A binder binds a variable over a body | SHACL Core | `math:MalformedBindingExpression` |
| A truth-valued expression lowered into `logic:` (`math:compilesToLogicFormula`) declares its denotation kind and preservation | SHACL-SPARQL (`math:LogicLoweringDeclaredConstraint`, a guarded-implication requiring `math:denotationKind` and `math:logicLoweringPreservation`) | `math:UndeclaredLogicLowering` |
| A `math:Derivative` names what it differentiates, its variable, and its order | SHACL Core | `math:UnderspecifiedDerivative` |
| A `math:Limit` names its expression and its limit point (mode optional) | SHACL Core | `math:UnderspecifiedLimit` |
| A `math:Series`/`math:Sequence` carries a `math:Convergence` naming what it converges to and the mode | SHACL Core | `math:UnderspecifiedConvergence` |
| A `math:LimitResult` names its `math:limitOutcome`, and its `math:limitResultValue` agrees with that outcome (a finite value for `math:convergesFinitely`; `math:PositiveInfinity`/`math:NegativeInfinity` for the divergent poles; none for `math:divergesWithoutLimit`) | SHACL Core (missing outcome — paired exact-one restrictions on `math:limitOutcome` in `module.ttl`); SHACL-SPARQL (`math:LimitResultOutcomeValueConstraint`, the outcome↔value agreement) | `math:UnderspecifiedLimitResult` |
| Continuity/connectedness/separation(T0–T4) are declared, not assumed — each backed by a first-order `logic:Formula` law; compactness backed by a `logic:SecondOrder` boundary record | SHACL Core (backed by `math:continuityLaw`/`math:connectednessLaw`/the separation laws; `math:compactnessBoundary`) | `math:UndeclaredTopologicalProperty` |
| Every `math:AnalyticProperty` resolves through `math:definingLaw` to a real first-order `logic:Formula` (`math:nonAffinityLaw`, `math:convexityLaw`, `math:boundednessLaw`) or an honest `logic:SecondOrder` boundary record (`math:smoothnessBoundary`) — a monotonicity/analytic claim is never a bare flag | SHACL-SPARQL (`math:AnalyticPropertyBackedConstraint`, a class-guarded existence of `math:definingLaw`) | `math:UnbackedAnalyticProperty` |
| A `math:Manifold` declares its dimension and its structure kind | SHACL Core | `math:UnderspecifiedManifold` |
| A `math:Chart` names its domain, coordinate map, and target coordinate space | SHACL Core | `math:UnderspecifiedChart` |
| A chart's target space (and a tangent space) has the same dimension as its manifold | SHACL-SPARQL | `math:DimensionMismatch` |
| A `math:MetricSignature`'s `p + q` equals the manifold's dimension, and its `(p, q)` split agrees with the structure kind (Riemannian ⇒ `q = 0`; Lorentzian ⇒ exactly one timelike) | SHACL-SPARQL | `math:DimensionMismatch` |
| A `math:Compactification` names all four roles (original space, compactifying map, compactified space, boundary at infinity); a `math:ConformalCompactification` additionally names its conformal factor | SHACL Core (paired `owl:maxQualifiedCardinality`/`owl:minQualifiedCardinality` exact-one restrictions on all four roles, and on the conformal factor, in `module.ttl`) | `math:UnderspecifiedCompactification` |
| **A `math:Complement` names its ambient space and its complement-semantics** | SHACL Core | `math:UnqualifiedComplement` |
| A `math:PersistentHomology` activity names its input, one filtration, and a persistence-diagram output | SHACL Core derived from OWL restrictions | `math:IncompletePersistentHomologyAnalysis` |
| A `math:HamiltonianSystem` names its smooth state space, symplectic form, Hamiltonian function, and generated flow | SHACL Core derived from OWL restrictions | `math:IncompleteHamiltonianSystem` |
| A `math:CellularSheaf` names its base complex, at least one stalk, and at least one restriction map | SHACL Core derived from OWL restrictions | `math:IncompleteCellularSheaf` |
| A `math:Connection` names what it is a connection ON — a `math:CellularSheaf` (`math:connectionOfSheaf`) or an `math:Atlas`/bundle (`math:connectionOn`) | SHACL Core derived from OWL restrictions | `math:IncompleteConnection` |
| A `math:ParallelTransport` names both its `math:transportConnection` (the rule) and its `math:transportAlong` (the path) | SHACL Core derived from OWL restrictions | `math:IncompleteParallelTransport` |
| A `math:Holonomy` names both its `math:holonomyLoop` (the closed loop) and its `math:holonomyOf` (the connection whose transport it composes) | SHACL Core derived from OWL restrictions | `math:IncompleteHolonomy` |
| A `math:Cell` declares its `math:cellDimension` — an undimensioned cell cannot sit in a `math:CellComplex`'s graded boundary chain | SHACL Core | `math:IncompleteCell` |
| A `math:CellIncidence` names its `math:incidenceCoface`, `math:incidenceFace`, and `math:incidenceSign` — all three are constitutive of a signed boundary coefficient | SHACL Core | `math:UnorientedIncidence` |
| The twice-applied boundary vanishes (∂∘∂ = 0) — every codimension-2 composition path has its cancelling partner face (simplicial) or signed sum (general CW) | SHACL-SPARQL (`math:BoundarySquareZeroConstraint` / `math:GeneralBoundarySquareZeroConstraint`) | `math:BrokenBoundarySquareZero` |
| The twice-applied coboundary vanishes (δ∘δ = 0) — every codimension-2 co-composition path has its cancelling partner coface | SHACL-SPARQL (`math:CoboundarySquareZeroConstraint`) | `math:BrokenCoboundarySquareZero` |
| The boundary/coboundary adjunction δ = ∂* holds — a boundary `math:CellIncidence` and its `math:adjointIncidence` transpose carry agreeing ±1 signs | SHACL-SPARQL (`math:BoundaryCoboundaryAdjunctionConstraint`) | `math:BrokenBoundaryCoboundaryAdjunction` |
| A `math:CochainComplex` names its `math:cochainCoboundary` — the `math:CoboundaryOperator` whose degree-plus-one maps assemble it | SHACL Core | `math:IncompleteCochainComplex` |
| A `math:Coboundary` names its `math:coboundaryOf` — the `math:CoboundaryOperator` it is an image under (c = δd) | SHACL Core | `math:IncompleteCoboundary` |
| A `math:Chain` names its `math:chainOf` — the `math:ChainComplex` it is a graded element of | SHACL Core | `math:UngroundedChain` |
| A `math:Cycle` names its `math:cycleOf` — the `math:ChainComplex` whose kernel of ∂ it lies in | SHACL Core | `math:UngroundedCycle` |
| A `math:GlobalSection` names its frame — its `math:overSheaf` carrier and its `math:sectionRegion` | SHACL Core derived from OWL restrictions | `math:IncompleteGlobalSection` |
| A `math:GlobalSection`'s `math:sectionRegion` equals the whole `math:sheafBaseComplex` of its `math:overSheaf`, never a proper subcomplex — a section typed global is not silently scoped local | SHACL-SPARQL (`math:MisscopedSectionConstraint`, a cross-node equality rule) | `math:MisscopedSection` |
| A declared `math:GlobalSection` restricts consistently along each `math:SheafRestrictionMap` — the `math:RestrictionImage` transporting the source stalk's value agrees with the target stalk's `math:stalkValue` | SHACL-SPARQL (`math:SectionGluingConsistencyConstraint`, a cross-node equality rule) | `math:SectionGluingInconsistency` |
| A `math:RestrictionImage` names both its `math:imageSourceValue` and its `math:imageTargetValue` — the pair the map realizes for one declared source value | SHACL Core | `math:IncompleteRestrictionImage` |
| A `math:LocalSection` anchors to genuine stalk-and-restriction semantics — its `math:overSheaf` carrier and its `math:sectionRegion` | SHACL Core derived from OWL restrictions | `math:IncompleteSheafSection` |
| A `math:GluingObstruction` names its `math:obstructionOf` sheaf — an H¹ obstruction is defined only relative to the sheaf whose local-to-global lifting it obstructs | SHACL Core | `math:UnanchoredGluingObstruction` |
| A Hodge decomposition names its signal, its exact/coexact/harmonic components, its boundary operator, its carrier sheaf, its exact reconstruction residual, and its three pairwise-orthogonality inner products | SHACL Core derived from OWL restrictions | `math:IncompleteHodgeDecomposition` |
| A `math:CombinatorialLaplacian` names its `math:CellComplex`, its `math:laplacianDegree`, and both its `math:upperBoundaryOperator` and `math:lowerBoundaryOperator` | SHACL Core derived from OWL restrictions | `math:IncompleteCombinatorialLaplacian` |
| A Mapper construction names its source metric space, filter (lens) function, cover of the filter codomain, per-element clustering rule, and output nerve complex | SHACL Core derived from OWL restrictions | `math:IncompleteMapperConstruction` |

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
| A `math:DimensionalReduction` names its input, exact non-negative target dimension, and embedding output | SHACL Core derived from OWL restrictions | `math:IncompleteDimensionalityReductionAnalysis` |
| A `math:TensorComputationGraph` declares its computation nodes, which are `math:ApplicationExpression`s reusing the argument-slot AST (so the inherited slot-uniqueness/well-formedness gates bite) | SHACL Core (`math:TensorComputationGraphShape`) + inherited SHACL-SPARQL (`math:SlotIndexUniquenessShape`) | `math:MalformedTensorComputationGraph` / `math:DuplicateArgumentSlotIndex` |
| A `math:WeightTensor` names the `math:ParameterSpace` it lives in | SHACL Core | `math:UnframedWeightTensor` |
| A `math:Filtration` declares at least one `math:FiltrationStage`, and every stage names its `math:filtrationThreshold` and `math:stageStructure` (structural presence only — monotonicity ε₁ ≤ ε₂ ⇒ containment is the first-order law `math:filtrationMonotonicityLaw`, not a shape) | SHACL Core | `math:UnderspecifiedFiltration` |
| A `math:PersistenceLifetime` names its `math:overFiltration`, its `math:persistenceFeature`, its `math:bornAt`, and its `math:diesAt` — a finite `math:Quantity` or `math:PositiveInfinity` for an essential feature, never omitted | SHACL Core | `math:UnderspecifiedPersistenceLifetime` |
| A `math:StabilityCalibrationRecord` names its `math:calibrationEvidence`, its `math:credenceDerivationKind`, and its `math:stabilityGuarantee` — the persistence-derived credence is warranted, not a heuristic | SHACL Core | `math:UngroundedStabilityCalibration` |
| A vector-symbolic operation names the `math:VectorSpace` it composes in, the `math:Basis` fixing its coordinates, its operand vectors, its capacity descriptor, and its recovery-loss (fidelity) contract | SHACL Core derived from OWL restrictions | `math:IncompleteVectorSymbolicOperation` |
| A `math:MultiparameterFiltration` names its `math:filtrationIndexPoset` and at least one `math:hasFiltrationStage` | SHACL Core derived from OWL restrictions | `math:IncompleteMultiparameterFiltration` |
| A `math:MultiparameterFiltration`'s stages carry a genuine `math:multiIndex`, not only a single real `math:filtrationThreshold` — a nominally multi-parameter filtration must not silently degrade to the one-parameter case | SHACL-SPARQL (`math:CollapsedMultiparameterFiltrationConstraint`) | `math:CollapsedMultiparameterFiltration` |
| A `math:MultiparameterFiltration`'s coordinates are not functionally dependent — no per-stage diagonal `math:multiIndex` and, across stages, a genuine coordinate-independence witness | SHACL-SPARQL (`math:DiagonalDegenerateFiltrationConstraint` / `math:MultiparameterFunctionalDependenceConstraint`) | `math:DiagonalDegenerateFiltration` |
| A `math:PersistenceModule` names its `math:moduleIndex` (the index poset) and at least one `math:structureMap` (a comparable-pair transition map) | SHACL Core derived from OWL restrictions | `math:IncompletePersistenceModule` |
| A `math:PersistenceMorphism` names both its `math:morphismSource` and its `math:morphismTarget` | SHACL Core derived from OWL restrictions | `math:IncompletePersistenceMorphism` |
| A `math:ZigzagDiagram` declares at least one `math:backwardArrow` — a zigzag with only forward arrows has collapsed to ordinary forward-only persistence | SHACL-SPARQL (`math:DegenerateZigzagDiagramConstraint`) | `math:DegenerateZigzagDiagram` |
| A `math:PersistenceModule` and a `math:PersistenceLifetime` are never the same individual — the whole algebraic functor is not the birth–death interval of a single feature | SHACL-SPARQL (paired shape over the directly-asserted `owl:disjointWith`, `math:PersistenceModuleLifetimeConflationShape`) | `math:PersistenceModuleLifetimeConflation` |

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
| Odds/log-odds are modeled as scale transforms, not as `ProbabilityValue` | OWL axiom (disjointness) projected to SHACL Core (`math:ProbabilityScaleConflationShape`, a directly-asserted disjointness — `math:OddsValue`/`math:LogOddsValue` and `math:ProbabilityValue` share no disjoint ancestor, so the disjointness is asserted directly on the pair and the paired shape surfaces it as `sh:not [ sh:class ]`) | `math:ProbabilityScaleConflation` |
| A probability value names its probability frame/model | SHACL-SPARQL (`math:ProbabilityValueFramedConstraint`, an at-least-one choice-group over `gmeow:hasReferenceFrame`/`logic:probabilityModel`) | `math:UnframedProbabilityValue` |
| A probability value is not inferred from confidence without a declared mapping | source-lint + SHACL-SPARQL | `math:ConfidenceAsProbability` |
| A `ProbabilitySpace` has sample-space, σ-algebra, and measure objects (possibly symbolic) | SHACL Core | `math:IncompleteProbabilitySpace` |
| A `RandomVariable` has domain/codomain and a space or distribution context | SHACL Core | `math:UnscopedRandomVariable` |
| A `Distribution` has a family and a parameterization | SHACL Core | `math:MissingDistributionParameterization` |
| Each required parameter role is supplied exactly once | SHACL-SPARQL | `math:DistributionParameterRoleCardinality` |
| Parameter quantities satisfy the role's dimensional/positivity constraints | Rust validator | `math:DistributionParameterConstraint` |
| A conditional/conditional-independence assertion names its conditioning set | SHACL Core | `math:UnconditionedAssertion` |
| A probability-model lowering into `logic:` is declared for a reasoning-facing model | Rust validator | `math:MissingProbabilityModelLowering` |
| A dependency-model declaration is structurally complete (DAG, CPT/factor totality) | Rust validator | `math:IncompleteDependencyModel` |
| A `math:ProbabilityEvent` (a measurable set of outcomes) is never also typed a `logic:Event` (an occurrent) — the measure-theoretic set-of-outcomes and the occurrence are distinct | SHACL Core (paired shape over the directly-asserted `owl:disjointWith`) | `math:ProbabilityEventOccurrentConflation` |
| An information measure (an entropy, divergence, Fisher information, or surprisal) declares its required frame component — its probability distribution, its logarithm base, its information unit, a divergence's reference distribution, a Fisher information's likelihood model or score parameter, or a surprisal's outcome | SHACL Core derived from OWL restrictions | `math:IncompleteInformationMeasure` |

### Statistics rules

| Rule | Primary gate | Failure class |
|---|---|---|
| An `Estimate` references its estimand and/or estimated parameter, and its estimator | SHACL Core | `math:UnderspecifiedEstimate` |
| A `FittedModel` references data and a model specification | SHACL Core | `math:UnfittedModel` |
| A `PValue` references a test, null hypothesis, test statistic, null distribution, and tail/sidedness | SHACL-SPARQL | `math:IllFramedPValue` |
| A `ConfidenceInterval` has lower/upper bounds and a confidence level | SHACL Core | `math:IncompleteConfidenceInterval` |
| A `CredibleInterval` has a posterior context and a credible mass | SHACL Core | `math:IncompleteCredibleInterval` |
| Confidence and credible intervals are not interchanged | OWL axiom (disjointness) projected to SHACL-SPARQL (`math:IntervalKindConflationShape` — the disjointness is ENTAILED through the disjoint `math:FrequentistResult`/`math:BayesianResult` paradigm parents rather than asserted directly on the pair, so the paired shape needs the reasoned closure and is a SPARQL-AF constraint) | `math:IntervalKindConflation` |
| An `EffectSize` identifies its contrast, scale, and frame | SHACL Core | `math:UnframedEffectSize` |
| A `ModelDiagnostic` identifies the fitted model and the diagnostic method | SHACL Core | `math:UnanchoredDiagnostic` |
| A missingness mechanism is explicit where an analysis depends on it | SHACL-SPARQL | `math:ImplicitMissingness` |
| A `math:CalibrationDiagnostic` (the statistical sense: predicted probabilities against observed frequencies) is never also a `math:StabilityCalibrationRecord` (the topological sense: a credence calibrated against a bottleneck stability bound) — they share only the word "calibration" | SHACL Core (paired shape over the directly-asserted `owl:disjointWith`) | `math:CalibrationSenseConflation` |

### Process/result/claim separation

| Rule | Primary gate | Failure class |
|---|---|---|
| An inference/analysis *process* is a `gmeow:Activity`, not typed as an `Observation` | OWL axiom (disjointness) | `math:ProcessObservationConflation` |
| A held statistical/probabilistic *result claim* is an `Observation` with a vantage | Rust validator (a cross-node obligation over `gmeow:Observation`/`gmeow:observationResult`/`gmeow:vantage`, none of which is `math:`-specific, so it carries no `generated/`-dependent SHACL twin) | `math:UngroundedResultClaim` |
| The structured *result object* (estimate, p-value, posterior) is neither the process nor the claim | OWL axiom | `math:ResultRoleConflation` |

### Projection rules

| Rule | Primary gate | Failure class |
|---|---|---|
| Every projection declares its unsupported constructs | projection test (`crates/pipeline/tests/support/math_projection_producer.rs`, all three producers) | `math:UndeclaredUnsupportedConstruct` |
| Every projection declares a `logic:preservationKind` | projection test (`crates/pipeline/tests/support/math_projection_producer.rs`, all three producers) | `math:MissingPreservationKind` |
| No projection silently converts confidence to probability | projection test (`crates/pipeline/tests/support/math_projection_producer.rs` `produce_confidence_probability_projection`) | `math:ProjectionConfidenceAsProbability` |
| No projection silently drops distribution parameterization | projection test (`crates/pipeline/tests/support/math_projection_producer.rs` `produce_distribution_scipy_projection`) | `math:ProjectionDroppedParameterization` |
| No projection flattens an expression AST to a string without recording loss | projection test (`crates/pipeline/tests/support/math_projection_producer.rs` `produce_expression_annotation_projection`) | `math:UnrecordedProjectionLoss` |
| A declared-exact `math:JointProbabilityTable`/`math:MarkovKernel`/`math:BayesianNetwork`/`math:FactorGraph` actually has the outcome mass / completeness its declared `logic:ExactPreservation` claims | Rust validator (`check_math_probability_invariants`; arithmetic outcome-mass summation and dependency-graph completeness over the probability-model families, not a `math:ProjectionRecord` join) | `math:ExactPreservationViolated` |

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
| A proof QED result object is grounded *by* an observation with a vantage (result ≠ claim) | SHACL-SPARQL (`math:FormalVerificationResultVantageGroundingConstraint`, a conditional-existence rule over the grounding observation and its vantage) | `math:UngroundedVerificationResult` |
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

### Flagship acceptance-manifest rules

The layer's five flagship acceptance scenarios (the depth bar of [`MATHEMATICS.md`](MATHEMATICS.md))
are themselves a typed, gated object, not prose. Each is reified as a `gmeow:FlagshipScenario`
(authored in `examples/flagship-acceptance.ttl`) binding the scenario to the five artifacts that
realize and enforce it: its worked example (`gmeow:demonstratedByExample`), its competency question
(`gmeow:demonstratedByCompetency`), the native producer that emits it (`gmeow:demonstratedByProducer`),
its guarding counter-example (`gmeow:guardedByCounterExample`), and the named failure class its gate
raises (the shared `gmeow:enforcesFailureClass`). The acceptance bar is thereby a contract — a scenario
that does not wire all five to a real conformance failure is `math:UnwiredFlagshipScenario`.

| Rule | Primary gate | Failure class |
|---|---|---|
| A `gmeow:FlagshipScenario` binds its example, competency, producer, counter-example, and a failure class that IS a `math:MathConformanceFailure` subclass | shared `gmeow:FlagshipScenarioShape` (SHACL Core) + thin `math:FlagshipScenarioShape` (SHACL-SPARQL) | `math:UnwiredFlagshipScenario` |
| The five canonical flagship scenarios are all present and fully wired | structural (`ex:saFlagshipCoverage`) | (structural assertion) |
| Each flagship's competency reference resolves to a registered green (`cqExpectRow`) competency question with an existing query file, and its example/counter-example files exist | Rust cross-check (`crates/slicetest` `flagship_manifest`) | (native test) |
| Each counter-example raises exactly its failure class, each worked example is clean, and each named producer runs to its pinned output | execution-discharge harness (`crates/pipeline/tests/math_flagship_discharge.rs`) | (native test) |

The competency cross-check is a **native** gate for the same dataset-split reason the unliftable-ingest
rule is: the `gmeow:CompetencyQuestion` individuals live in `tests/competency.ttl`, which the
module/examples-scoped SHACL and structural validators never load, so the reference from a
`gmeow:FlagshipScenario` into that dataset can only be resolved — and its `cqExpectRow` greenness
confirmed — by a validator that unions the two. The three surfaces together make the epic's depth bar
regression-proof: drop a scenario, unwire a link, or point at an unregistered competency question, and
one of the three gates fails.

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
