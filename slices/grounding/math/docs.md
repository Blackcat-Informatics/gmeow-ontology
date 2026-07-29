<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Mathematics — the `math:` grounding layer

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/math` · **tier: core**
> The grounding vocabulary for mathematical references, expression ASTs, category-theoretic
> structure, quantities/dimensions, numbers, sets, functions, probability, and statistics. A peer of
> the `logic:` reasoning layer and `lang:` realization layer.

This slice is the home of **GMEOW Mathematics (`math:`)**. Its discipline is that mathematical
identity and structure are explicit: a local concept is distinct from its symbol, notation,
rendering, and external identifier; a computable expression is a typed AST with strict ordered
slots; category-theoretic objects are first-class; a dimensioned quantity and its numeric value have
one canonical authority; and numeric exactness is never implied by an approximate literal.

The grounding surface realized here includes:

- **References, expressions, and ACT** — concepts, symbols, notations, theories, contexts, and
  definitions; literal/symbol/variable/application/binding AST nodes; strict zero-based contiguous
  argument slots; and first-class categories, morphisms, functors, and natural transformations.
- **Quantities and dimensions** — the sole `math:Quantity` class and `math:quantityValue` property,
  exact-rational dimension vectors, homogeneity laws, and observation qualifiers applied from their
  owning slices without quantity aliases.

- **Number systems and exactness** — the containment tower ℕ ⊂ ℤ ⊂ ℚ ⊂ ℝ ⊂ ℂ
  (`math:NaturalNumber` … `math:ComplexNumber`), the algebraic/transcendental cross-cut, named
  constants (`math:MathematicalConstant`, with `math:pi`, `math:eulerNumber`,
  `math:eulerMascheroni`), and the exact/approximate seam (`math:ApproximateValue`,
  `math:approximates`, `math:approximationError`, `math:numericDatatype` subsuming IEEE-754
  `xsd:double`/`xsd:float`).
- **Arithmetic** — `math:ArithmeticOperation` and the operator individuals `math:Addition`,
  `math:Multiplication`, `math:Exponentiation` and their inverses, applied inside a
  `math:ApplicationExpression`.
- **Sets** — `math:Set` and its extensional (`math:FiniteSet`, `math:hasElement`) and intensional
  (`math:SetBuilderExpression`, `math:memberCondition`) forms, set operations
  (`math:SetOperation`, `math:Complement`, `math:PowerSet`, `math:CartesianProduct`), and cardinality
  (`math:Cardinality`, `math:cardinalityFinite`, `math:alephNull`, `math:continuum`).
- **Relations and functions** — `math:Relation`, `math:Function` with `math:domain`/`math:codomain`/
  `math:image`, the declared boolean bearers `math:isInjective`/`math:isSurjective`/`math:isBijective`
  (of which `math:InjectiveFunction`/`math:SurjectiveFunction`/`math:BijectiveFunction` are convenience
  projections), and `math:PartialFunction`, `math:FunctionSpace`, `math:functionComposition`,
  `math:inverseFunction`.

The object-layer parents and the reference, expression, proof/theory, probability, statistics, and
applied-category-theory surfaces are all authored in this grounding slice; the design files below
factor their contracts without creating separate semantic owners.

Every well-formedness violation is a typed, queryable object: `math:MathConformanceFailure` and its
subclasses, each wired from a canonical EL-safe axiom or `logic:Constraint` through
`gmeow:enforcesFailureClass` and projected to the applicable validation surface. The
preservation-polarity vocabulary is reused verbatim from the `logic:` loss ledger, and each named
constant and number system is anchored to Wikidata (with OEIS locators for the constants) as a
native alignment cell.

## The design set

The normative design is a set of charters under [`design/`](./design/):

| Document | Genre | Realized state | Contents |
| --- | --- | --- | --- |
| [`design/MATHEMATICS.md`](./design/MATHEMATICS.md) | manifesto | realized | vision, doctrine, the grounding-layer posture |
| [`design/MATHEMATICS-EXTERNAL-CORPUS-CROSSWALK.md`](./design/MATHEMATICS-EXTERNAL-CORPUS-CROSSWALK.md) | coverage audit | realized | anonymized, mechanically gated dispositions for 95 topics from a private comparison corpus |
| [`design/MATHEMATICS-NUMBERS-AND-SETS.md`](./design/MATHEMATICS-NUMBERS-AND-SETS.md) | charter | realized | the bedrock: number systems and exactness, arithmetic, sets, relations and functions |
| [`design/MATHEMATICS-EXPRESSIONS.md`](./design/MATHEMATICS-EXPRESSIONS.md) | charter | realized (reference layer, typed AST, strict slot contiguity, ACT core; content-addressed interning realized on the ingest path via the shared term arena) | the mathematical core: the reference, expression-AST, object, ACT, and statement/proof layers |
| [`design/MATHEMATICS-MEASURE-AND-DIMENSION.md`](./design/MATHEMATICS-MEASURE-AND-DIMENSION.md) | charter | realized (incl. the native ℚ⁷ homogeneity gate) | measurable spaces, measures, integration, dimensional analysis, units |
| [`design/MATHEMATICS-ALGEBRA.md`](./design/MATHEMATICS-ALGEBRA.md) | charter | realized | the structure hierarchy, homomorphism laws, E8, exact Clifford algebras and extensions, homomorphic encryption, secret sharing |
| [`design/MATHEMATICS-ANALYSIS-AND-GEOMETRY.md`](./design/MATHEMATICS-ANALYSIS-AND-GEOMETRY.md) | charter | realized | calculus binders, computational topology, cellular sheaves and Hodge structure, Hamiltonian systems, manifolds |
| [`design/MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md`](./design/MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md) | charter | realized (exact-rational numeric core; general heavy computation design-only) | inner products, reduction/reconstruction, information measures, vector-symbolic operations, PCA, embeddings, tensor graphs, residual meaning |
| [`design/MATHEMATICS-PROBABILITY.md`](./design/MATHEMATICS-PROBABILITY.md) | charter | realized | probability spaces, random variables, distributions with mandatory parameterization, dependency models, the `logic:probabilityModel` seam |
| [`design/MATHEMATICS-STATISTICS.md`](./design/MATHEMATICS-STATISTICS.md) | charter | realized (external computational engines remain projections) | statistical models, estimation, inference, p-values, interval paradigms, diagnostics, and the process/result/claim split |
| [`design/MATHEMATICS-BRIDGES.md`](./design/MATHEMATICS-BRIDGES.md) | charter | realized — the ingest-run spine, the native unliftable gate, and all three executable front-ends (`gmeow math lift-r` / `lift-onnx` / `lift-proof`, also folded into the bundle as producers) | executable-artifact ingestion: R, ONNX, and proof-assistant lifts |
| [`design/MATHEMATICS-PROJECTIONS.md`](./design/MATHEMATICS-PROJECTIONS.md) | contract | partially realized — shipped grounding correspondences and projection-failure records are live; full document/codec emitters remain design | outbound lossy lowerings (MathML, OpenMath, Data Cube, STATO, QUDT) and inbound refusal contracts |
| [`design/MATHEMATICS-RUNTIME.md`](./design/MATHEMATICS-RUNTIME.md) | runtime | partially realized — exact Clifford kernel and producer shipped, and the R/ONNX/proof ingestion front-ends now lift real artifacts with content-addressed interning; solver profiles remain design-only | ingestion as projection run backwards, expression interning, exact bounded calculation, the solver-profile handoff, acceptance gates |
| [`design/MATHEMATICS-CONFORMANCE.md`](./design/MATHEMATICS-CONFORMANCE.md) | enforcement | realized across the canonical structural, probability, statistics, and grounding-correspondence surfaces; codec-only rows remain pending | the gate matrix — each hard rule, its gate kind, and the `math:` failure class it raises |
| [`design/MATHEMATICS-REFERENCES.md`](./design/MATHEMATICS-REFERENCES.md) | references | realized (alignment lanes authored in `mappings/equivalences.ttl`) | the external-authority landscape (OpenMath, Wikidata, OEIS, DLMF, QUDT, xsd) and the anchoring posture |

## The gates

Each bedrock hard rule is authored as an EL-safe axiom or `logic:Constraint` in
[`module.ttl`](./module.ttl), projected to the applicable validation surface, and demonstrated by a
positive fixture and a negative counter-example under the native slicetest harness:

| Rule | Shape | Failure class |
| --- | --- | --- |
| A number declares its number system | `math:NumberSituatedShape` | `math:UnsituatedNumber` |
| An approximation names what it approximates and its error | `math:ApproximateValueShape` | `math:ExactApproximateConflation` |
| A named constant is an exact individual, not a decimal | `math:MathematicalConstantShape` | `math:ConstantAsDecimalLiteral` |
| An intensional condition denotes a `logic:` formula | `math:IntensionalSetShape` | `math:StringOnlyMemberCondition` |
| A complement names its ambient set | `math:ComplementShape` | `math:UnqualifiedComplement` |
| A set is extensional or intensional, not both | `math:SetExtentShape` | `math:AmbiguousSetExtent` |
| A function declares its domain and codomain | `math:FunctionFramingShape` | `math:UnframedFunction` |

The bedrock competency questions in [`queries/competency/`](./queries/competency/) demonstrate that
the term surface answers, over the fixtures, what number system a number is in and whether it is
exact, whether a set is extensional or intensional and its condition, a function's
domain/codomain/image and its injectivity/surjectivity/bijectivity, a set's cardinality, and which
named constants appear with their external anchors. Further questions in the same registry exercise
every realized charter, including the cross-corpus coverage frames described below.

## Algebra — structures, symmetry, and homomorphisms

The algebra charter ([`design/MATHEMATICS-ALGEBRA.md`](./design/MATHEMATICS-ALGEBRA.md)) deepens the
object layer with four regions, on one discipline: **a structure declares its operations and laws, and
a map between structures declares what it preserves.** `math:` expresses; `logic:` owns reasoning —
every axiom and preservation law is a real `logic:Formula` first-order AST, not an opaque string. The
canonical `logic:` layer carries these formulas exactly (`logic:ExactPreservation`) and the reasoner
consumes the full quantifier tree; their lowering to the evaluable Datalog/relational engine is
recorded as `logic:SoundUnderApproximation` in the logic projection report
(`generated/logic/projection-report.ttl`), because the flagship laws are n-ary (the ternary
`op(x,y,z)` predication) and the binary evaluable core carries the extra arity as flagged unsupported
residue. The laws are thus preserved and reasoned over faithfully in the canonical layer; entailment
over their n-ary predication is a relational-core capability, not an algebra-slice concern.

- **The algebraic-structure hierarchy** — `math:AlgebraicStructure` ⊐ `math:Magma` ⊐ `math:Semigroup`
  ⊐ `math:Monoid` ⊐ `math:Group` ⊐ `math:AbelianGroup`; `math:Ring` ⊐ `math:CommutativeRing` ⊐
  `math:Field`; `math:Module` ⊐ `math:VectorSpace`; `math:PolynomialRing`, `math:Ideal` — each naming
  its `math:underlyingSet`, `math:structureOperation`, and the axioms it `math:satisfiesAxiom`. The
  reusable axiom library ([`examples/algebra-axioms.ttl`](./examples/algebra-axioms.ttl)) authors
  associativity, commutativity, identity, inverse, and distributivity as `logic:Formula` ASTs.
- **Structure-preserving maps** — `math:Homomorphism` (⊑ `math:Morphism`) with `math:GroupHomomorphism`,
  `math:RingHomomorphism`, `math:Isomorphism`, `math:Automorphism`, and `math:AutomorphismGroup` (⊑
  `math:Group`), carrying `math:preservedOperation`, `math:kernel`, and a `math:preservationLaw`; the
  first-isomorphism triple (`math:Quotient`, `math:normalSubgroupOf`). The determinant example authors
  `det(A·B) = det(A)·det(B)` as a `logic:Formula`, with `GL₂/SL₂ ≅ ℝ*`.
- **Lie theory and the E8 flagship** — `math:LieGroup`, `math:LieAlgebra`, `math:RootSystem`,
  `math:CartanMatrix`, `math:DynkinDiagram`, `math:WeylGroup` (⊑ `math:AutomorphismGroup`), `math:Lattice`,
  `math:GroupRepresentation`, and `math:GroupAction`/`math:actsOn`. E8 is authored with 240 roots, rank
  8, and a Weyl group of order 696,729,600 modelled as the automorphism group acting on the roots. The
  Weyl group is anchored to the roots through `math:automorphismGroupOf` (every automorphism group is
  the symmetry OF some structure), and E8's numbers are pinned two ways: an exact-match competency
  question, and `math:E8WeylOrderShape` — a by-value SHACL gate that rejects a root system claiming the
  E8 fingerprint (240 roots, rank 8) whose Weyl order is not 696,729,600.
- **Homomorphic encryption (flagship 2) and secret sharing** — `math:HomomorphicEncryptionScheme` (a
  `math:RingHomomorphism`) with `math:homomorphicOver`, `math:securityAssumption` (LWE/RLWE), and
  `math:noiseModel`; encrypt/evaluate/decrypt as `gmeow:Activity` processes. `Dec(E(a) ⊗ E(b)) = a ⊕ b`
  is a `logic:Formula` over the declared ring structure — the purest exercise of the one-way
  `math:` → `logic:` bridge. Shamir secret sharing reuses the field / polynomial-ring machinery.
- **Clifford structure and exact extension calculation** — `math:QuadraticSpace`,
  `math:QuadraticForm`, `math:GradedAlgebra`, `math:ExteriorAlgebra`, and
  `math:CliffordAlgebra`; blades, multivectors, geometric/exterior/contraction products and the
  standard involutions; and `math:CliffordExtension` with an explicit module decomposition and
  split/join witness. The native exact kernel and eighth producer calculate `Cl(12,0)`, `Cl(6,6)`,
  `Cl(13,0)`, and `Cl(7,6)`. Equal dimensions create no E8 relationship: any such relationship must
  arrive through a declared faithful representation and equivariant map.

Algebra also **dogfoods** GMEOW: `math:Lattice math:formalizes gmeow:tuningSystemJustIntonation` (a
prime-limit just-intonation tuning is a free abelian lattice), the one-way `math:formalizes` bridge
(Principle 19) making algebra grounding for the ontology. The algebra classes align to Wikidata
(`skos:exactMatch`, QIDs curl-validated) and to Lean **mathlib** by reference (a new
`gmeow-mathlib.sssom.tsv` lane, `skos:relatedMatch`, URLs curl-validated).

The algebra gates:

| Rule | Shape | Failure class |
| --- | --- | --- |
| A structure declares its carrier, operation, and axioms | `math:AlgebraicStructureShape` | `math:IncompleteAlgebraicStructure` |
| A homomorphism declares its preserved operation and law | `math:HomomorphismShape` | `math:UnderspecifiedHomomorphism` |
| A preservation law denotes a `logic:` formula, not a string | `math:PreservationLawShape` | `math:StringOnlyPreservationLaw` |
| A Lie group declares its root system | `math:LieStructureShape` | `math:IncompleteLieStructure` |
| A root system declares its Cartan matrix, Weyl group, and rank | `math:RootSystemShape` | `math:IncompleteLieStructure` |
| An automorphism group is anchored to the structure it is the symmetry of | `math:AutomorphismGroupShape` | `math:UnanchoredAutomorphismGroup` |
| A root system claiming the E8 fingerprint declares the true Weyl order | `math:E8WeylOrderShape` (SHACL-SPARQL) | `math:WrongE8WeylOrder` |
| An HE scheme declares its homomorphic operation, hardness, and noise | `math:HomomorphicEncryptionSchemeShape` | `math:UnderspecifiedEncryptionScheme` |
| A ring declares the distributivity law tying its two operations together | `math:RingDistributivityShape` | `math:NonDistributiveRing` |

## Analysis, topology & geometry — the continuous

The analysis-and-geometry charter ([`design/MATHEMATICS-ANALYSIS-AND-GEOMETRY.md`](./design/MATHEMATICS-ANALYSIS-AND-GEOMETRY.md))
brings the objects of the continuous under the same discipline as the rest of `math:`: **the continuous
is structured, not evocative.** A derivative is a binder over the expression AST, not the string "dy/dx";
a manifold declares its dimension and structure kind, not merely the label "manifold"; a complement
names its ambient space and its complement-semantics. It also lands the subset of the mathematical-core
binder AST its operators consume, using the canonical reserved names (no near-synonyms).

- **The binder AST** — `math:BindingExpression` over the indexed argument-slot AST (`math:ArgumentSlot`,
  `math:slotIndex`, `math:slotExpression`, `math:operator`), the declaration/occurrence split
  (`math:VariableDeclaration`, `math:VariableOccurrence`, `math:boundVariable`, `math:bindsOccurrence`,
  `math:declaredVariable`) that makes nested and shadowed binders checkable, and the denotation seam
  (`math:denotationKind` + `math:compilesToLogicTerm`/`Formula`/`Type`) that lowers an expression into
  `logic:` with a recorded preservation — so the mathematical surface never silently becomes a logical
  assertion. `math:Integral` (from the measure charter) becomes a binding expression, unifying ∫ with
  d/dx, ∂, ∑, and lim under one binding form.
- **Calculus and analysis** — `math:Derivative`, `math:PartialDerivative`, `math:DifferentialOperator`,
  `math:Limit`, `math:Series`, `math:Sequence`, `math:Convergence`, and `math:SpecialFunction` (Γ, ζ,
  Bessel, erf as first-class individuals aligned to DLMF and OpenMath). A derivative names what it
  differentiates, its variable, and its order; a series carries a `math:Convergence` with its mode
  (pointwise, uniform, absolute, in-measure, …) — "it converges" is meaningless without the mode.
- **Topology** — `math:TopologicalSpace`, `math:OpenSet`, `math:ContinuousMap` (⊑ `math:Morphism`),
  `math:Homeomorphism`, `math:CompactSpace`, `math:ConnectedSpace`, `math:Homotopy`, and
  `math:HomologyGroup` (⊑ `math:AbelianGroup`, so it carries the full algebra frame). Continuity,
  compactness, and connectedness are declared, not assumed; the defining law that the preimage of an
  open set is open is authored as a first-order `logic:Formula` (`math:continuityLaw`).
- **Computational topology** — cell, simplicial, and chain complexes with boundary/coboundary
  operators; Vietoris–Rips, Čech, and alpha complexes; persistent, zigzag, and multiparameter
  homology; diagrams, barcodes, landscapes, Betti summaries, and Mapper. A
  `math:PersistentHomology` activity must name its analysis input, filtration, and result.
- **Cellular sheaves and Hodge structure** — stalks, restriction maps, sections, cohomology,
  sheaf/Hodge Laplacians, and exact/coexact/harmonic decomposition are explicit objects over a
  declared base complex. The sheaf and its local-to-global structure cannot be reduced to labels.
- **Hamiltonian dynamics** — scalar/vector fields, flows, symplectic forms, Hamiltonian functions,
  flows, and systems. A `math:HamiltonianSystem` declares its state space, symplectic form,
  Hamiltonian function, and generated flow; application-specific landscape readings remain
  evidence-bearing observations.
- **Differential geometry** — the manifold tower `math:Manifold` ⊐ `math:SmoothManifold` ⊐
  `math:ComplexManifold`/`math:RiemannianManifold`/`math:LorentzianManifold`, with `math:Chart`,
  `math:Atlas` (carrying transition maps φᵢ∘φⱼ⁻¹ through the landed function composition),
  `math:CoordinateMap`, `math:TangentSpace`, `math:TensorField`, and `math:MetricTensor` with a
  structured `math:MetricSignature` (p, q). The **Lorentzian** metric is the math object a physics slice
  needs for spacetime, and it stays here on the math side of the boundary — spacetime, worldlines, and
  the SR/GR regimes are physics.
- **Conformal geometry and compactification** — `math:Compactification` (the structured record naming
  four roles: `math:originalSpace`, `math:compactifyingMap`, `math:compactifiedSpace`, and
  `math:boundaryAtInfinity`), its conformal (Penrose-style) specialization
  `math:ConformalCompactification` (additionally carrying a `math:conformalFactor` Ω), and
  `math:BoundaryAtInfinity` (the ideal points at infinity). The conformal case is the general home for
  embedding a `math:LorentzianManifold`'s radial infinity as a finite boundary; the metric and its
  rescaling stay math-side.

The distinguished hard rule is the **named complement**: a `math:Complement` names its `math:ambientSpace`
and its `math:complementSemantics` (set-theoretic, orthogonal, complex-linear, topological, or
quotient/cokernel), generalizing the bedrock set-theoretic complement — an unqualified complement is
ill-formed. A chart's target space and a tangent space must share the manifold's dimension, a
SHACL-SPARQL gate (`math:DimensionMismatch`) that is the analysis-geometry analogue of dimensional
homogeneity.

| Rule | Shape | Failure class |
|---|---|---|
| A derivative names what it differentiates, its variable, and order | `math:DerivativeShape` | `math:UnderspecifiedDerivative` |
| A limit names its expression and its limit point | `math:LimitShape` | `math:UnderspecifiedLimit` |
| A series carries a convergence naming its mode | `math:SeriesShape` / `math:ConvergenceShape` | `math:UnderspecifiedConvergence` |
| Continuity/compactness/connectedness are declared, not assumed | `math:ContinuousMapShape` / `math:CompactSpaceShape` / `math:ConnectedSpaceShape` | `math:UndeclaredTopologicalProperty` |
| A manifold declares its dimension and structure kind | `math:ManifoldShape` | `math:UnderspecifiedManifold` |
| A chart names its domain, coordinate map, and target space | `math:ChartShape` | `math:UnderspecifiedChart` |
| A chart's/tangent space's dimension matches its manifold | `math:ChartDimensionShape` / `math:TangentSpaceDimensionShape` (SHACL-SPARQL) | `math:DimensionMismatch` |
| A compactification names all four roles (+ a conformal one its conformal factor) | `math:CompactificationShape` / `math:ConformalCompactificationShape` | `math:UnderspecifiedCompactification` |
| A complement names its ambient space and complement-semantics | `math:ComplementShape` | `math:UnqualifiedComplement` |
| An argument slot has exactly one index and expression; indexes are unique, non-negative, zero-based, and contiguous | derived `math:ArgumentSlotShape` / `math:SlotIndexUniquenessShape` / `math:ArgumentSlotContiguityConstraint` | `math:MalformedArgumentSlot` / `math:NonContiguousArgumentSlots` |
| A symbol-reference AST leaf resolves to exactly one local mathematical symbol | derived exact-one SymbolReference shape | `math:UnresolvedSymbolReference` |
| A variable occurrence resolves to a declaration | `math:VariableOccurrenceShape` | `math:UnscopedVariableOccurrence` |

## Linear algebra, learning & representation — the operational objects of AI

The linear-algebra-and-learning charter ([`design/MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md`](./design/MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md))
turns the algebraic and geometric primitives into the operational objects of data and AI: **a
decomposition or embedding declares its inputs, its policy, and its outputs — and any *meaning* read
off a residual or a latent dimension is a claim from a vantage, never a property of the vector.** It is
the most author-heavy charter (the survey found almost no external ontology for decompositions, latent
spaces, or representation geometry) and it carries two flagships.

- **Inner-product spaces and decompositions** — `math:InnerProductSpace`, `math:HermitianInnerProduct`,
  `math:Subspace`, `math:OrthogonalComplement` (⊑ `math:Complement`, so it realizes the geometry
  charter's complement contract — ambient space, orthogonal semantics, and now `math:definedByInnerProduct`),
  `math:Basis`, `math:LinearMap`, `math:Rank`, `math:Eigendecomposition`, `math:SingularValueDecomposition`,
  and `math:CovarianceOperator`. Decompositions are first-class objects with declared operands and outputs.
- **PCA and the KG-projection flagship** — `math:PCAAnalysis` (a `gmeow:Activity`), `math:PrincipalComponent`,
  `math:LoadingVector`, `math:ScoreVector`, `math:ExplainedVariance`, `math:ProjectionResidual`, and
  `math:ResidualInterpretationClaim`. A PCA names its input, centering/scaling policy, covariance operator,
  eigensolver, and its component/loading/score/variance/residual outputs. The flagship — embed a knowledge
  graph, take the orthogonal complement of the embedded subspace, run PCA on the residuals, interpret the
  residual — composes cleanly, and the *meaning* of a component is a `math:ResidualInterpretationClaim`: a
  `gmeow:Observation` with a `gmeow:vantage`, never a property (no direct meaning property is minted, so the
  property-form is unauthorable).
- **Learning, embeddings, and latent spaces** — `math:LearnedModel`, `math:LossFunction`,
  `math:OptimizationProblem`, `math:Embedding`, `math:KnowledgeGraphEmbedding`, `math:LatentSpace`, and
  `math:EmbeddingDimension`. An embedding names its source, target space, function, and model; latent-space
  and dimension semantics are GMEOW-original.
- **Reduction, information, and vector-symbolic structure** — `math:DimensionalReduction` with
  UMAP/Isomap, spectral initialization, ordered reduction paths, reconstruction maps, and target
  dimensions; entropy, cross-entropy, mutual information, KL divergence, Fisher information, and
  surprisal; and framed vector binding, bundling, and unbinding. These are mathematical structures
  and analyses, while implementation policy and semantic interpretation remain profiles or claims.
- **An AI describing its own structure** — `math:TensorComputationGraph`, `math:NeuralLayer`,
  `math:WeightTensor`, `math:ActivationFunction`, `math:AttentionOperation`, and `math:ParameterSpace`. A
  neural network's forward pass **is** a `math:TensorComputationGraph` — a `math:ApplicationExpression` over
  tensor operators reusing the expression AST wholesale, whose weights are tensors in a declared
  `math:ParameterSpace`. Its reflection ("these dimensions encode X") is carried at the `logic:` metalevel
  (`logic:MetaLevelFormula`), self-reference without paradox across the grounding layers — the dogfooding apex.

The distinguished hard rule is **residual meaning as an observation, not a property**: semantic meaning read
off geometry is inference from a standpoint (Principle 9). Every rule is SHACL Core (or the inherited
argument-slot uniqueness gate the tensor graph rides), so `native_contract_hash` is untouched.

| Rule | Shape | Failure class |
|---|---|---|
| An orthogonal complement names its defining inner product | `math:OrthogonalComplementShape` | `math:UnqualifiedOrthogonalComplement` |
| A PCA names its inputs, policy, and outputs | `math:PCAAnalysisShape` | `math:IncompletePCAAnalysis` |
| A residual/latent meaning is a vantage-held observation, not a property | `math:ResidualInterpretationClaimShape` | `math:ResidualMeaningAsProperty` |
| An embedding names its source, target, function, and model | `math:EmbeddingShape` | `math:UnderspecifiedEmbedding` |
| A tensor computation graph declares its (AST-reusing) computation nodes | `math:TensorComputationGraphShape` / `math:SlotIndexUniquenessShape` | `math:MalformedTensorComputationGraph` / `math:MalformedArgumentSlot` |
| A weight tensor names its parameter space | `math:WeightTensorShape` | `math:UnframedWeightTensor` |

## Anonymized external-corpus coverage

[`design/MATHEMATICS-EXTERNAL-CORPUS-CROSSWALK.md`](./design/MATHEMATICS-EXTERNAL-CORPUS-CROSSWALK.md)
is the public, anonymized audit of a private comparison snapshot. Its 95 unique topics each receive
exactly one reviewed disposition: `REUSE`, `COMPOSE`, `EXTEND`, `MINT`, or `PROFILE`. The ledger is
pinned to the GMEOW base revision, names only reusable public mathematics, and deliberately excludes
the source project's identity, paths, revision, and local vocabulary.

Five positive fixtures and six minimal counterexamples exercise the newly shared frames for
persistent homology, dimensional reduction, Hamiltonian systems, cellular sheaves, and Clifford
algebras/extensions. Their competency queries prove the required structure is retrievable. The exact
Clifford producer is additional runtime evidence, not a sixth flagship: the repository's five
flagship acceptance scenarios remain unchanged.

The crosswalk is a coverage decision ledger, not an imported ontology or dependency. Rows marked
`PROFILE` stay downstream because they select workflows, storage, security, agent behavior, or
domain interpretations. Mathematical coincidences, topological readings, and representation claims
remain framed, provenance-bearing claims; they never become intrinsic ontology axioms by appearing
in the comparison corpus.

## The flagship acceptance manifest — the depth bar as a typed contract

The layer's depth is defined by five flagship acceptance scenarios — the symmetry groups of E8, how
homomorphic encryption works, complex proofs as process, a universal R → `math:` bridge, and an AI
describing its own structure. Each has a worked example, a competency question that pins it, and a
counter-example that proves its gate bites. The acceptance bar *itself* is reified as a typed,
queryable object rather than left as prose: a `gmeow:FlagshipScenario` (authored in
[`examples/flagship-acceptance.ttl`](./examples/flagship-acceptance.ttl)) binds each scenario to the
five artifacts that realize and enforce it —

- `gmeow:demonstratedByExample` → the worked, self-contained fixture under `tests/conformance-fixtures/`,
- `gmeow:demonstratedByCompetency` → the `gmeow:CompetencyQuestion` that pins it,
- `gmeow:demonstratedByProducer` → the native producer entrypoint (`math::producers::*`) that emits it,
- `gmeow:guardedByCounterExample` → the minimal violation under `tests/counter-examples/`,
- `gmeow:enforcesFailureClass` → the `math:MathConformanceFailure` subclass its gate raises.

The wiring is gated on three static surfaces (the module/examples vs. `tests/` dataset split forces
the split) **plus execution**: the shared `gmeow:FlagshipScenarioShape` (SHACL cardinality, producer
now required) and the thin slice `math:FlagshipScenarioShape` (failure-range) with `ex:saFlagshipCoverage`
(structural) prove the five are present and fully linked to a real failure class; a native cross-check
in `crates/slicetest` resolves each competency reference into `tests/competency.ttl` and confirms it is
a registered, green (`cqExpectRow`) question with an existing query file; and the execution-discharge
harness (`crates/pipeline/tests/math_flagship_discharge.rs`) RUNS each counter-example (asserting it
raises exactly its declared failure class), each worked example (asserting nothing fires), and each
native producer (asserting its pinned output). A scenario that is not fully wired is the typed failure
`math:UnwiredFlagshipScenario` — the depth bar cannot silently regress.

| Rule | Gate | Failure class |
| --- | --- | --- |
| A flagship scenario binds all five artifacts (incl. a native producer) to a real conformance-failure subclass | `gmeow:FlagshipScenarioShape` (shared SHACL) | `gmeow:UnwiredFlagshipScenario` |
| The five canonical scenarios are all present and fully wired | `ex:saFlagshipCoverage` (structural) | — |
| Each competency reference is a registered, green question with an existing query file | `flagship_manifest` (native cross-check) | — |
| Each counter-example raises exactly its class, each worked example is clean, each producer runs to its pinned output | `math_flagship_discharge` (execution) | — |
