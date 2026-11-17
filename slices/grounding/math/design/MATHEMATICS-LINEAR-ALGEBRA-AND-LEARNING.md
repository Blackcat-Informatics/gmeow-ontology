<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — Linear Algebra, Learning, and Representation

> The **linear-algebra-and-learning charter** of the GMEOW Mathematics design set: inner-product
> spaces and subspaces, matrix decompositions and PCA, statistical and representation learning,
> embeddings and latent spaces, and the tensor structure of AI systems. It carries two flagships —
> the **KG-projection / PCA-of-residuals** case and **an AI describing its own structure** — and is
> the most author-heavy charter, because the survey found almost no external ontology for
> decompositions, latent spaces, or representation geometry
> ([`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md)). It builds on the algebra and geometry
> charters and gates through [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's canonical `module.ttl` axioms and `logic:Constraint` records, competency queries, and the
> projection loss ledger.

## Purpose

This charter turns the algebraic and geometric primitives into the operational objects of data and
AI: matrices as linear maps, decompositions as named analyses, embeddings as maps into latent spaces,
and a neural network as a tensor computational graph. Its discipline is that **a decomposition or an
embedding declares its inputs, its policy, and its outputs — and any *meaning* read off a residual or
a latent dimension is a claim from a vantage, never a property of the vector.**

## Inner-product spaces, subspaces, and decompositions

Core classes: `math:InnerProductSpace`, `math:HermitianInnerProduct`, `math:Subspace`,
`math:OrthogonalComplement`, `math:Basis`, `math:LinearMap`, `math:Rank`, `math:Eigendecomposition`,
`math:SingularValueDecomposition`, `math:CovarianceOperator`, and `math:PrincipalComponent`.

Core properties: `math:innerProduct`, `math:subspaceOf`, `math:orthogonalComplementOf`,
`math:definedByInnerProduct`, `math:hasBasis`, `math:rankValue`, `math:eigenvalue`,
`math:eigenvector`, `math:singularValue`, and `math:ambientSpace`.

A `math:Matrix` (object layer) represents a `math:LinearMap`; an `math:InnerProductSpace` carries its
inner product (a `math:HermitianInnerProduct` for the complex case). The
`math:OrthogonalComplement` **realizes** the complement contract from the geometry charter: it names
its `math:ambientSpace` and is `math:definedByInnerProduct`, so "the orthogonal complement" is never
ambiguous ([`MATHEMATICS-ANALYSIS-AND-GEOMETRY.md`](MATHEMATICS-ANALYSIS-AND-GEOMETRY.md)).
Decompositions — `math:Eigendecomposition`, `math:SingularValueDecomposition` — are first-class
objects with declared operands and outputs. The registered eigen grounding in
[`mappings/equivalences.ttl`](../mappings/equivalences.ttl) is a `skos:closeMatch`
from `math:Eigendecomposition` to Wikidata `wd:Q190524`, eigenvectors and eigenvalues,
with `logic:AffineCorrespondence`, `logic:InstitutionMorphism` and
`logic:ValidationOnly` preservation. It relates the decomposition to the broader
eigenvalue/eigenvector concept; it asserts neither identity nor executable numerical
lowering. There is no authored OpenMath eigen correspondence in this mapping.

### Basis identity and coordinate order

`math:Basis` names an algebraic basis of a vector space over its scalar field.
Each vector has unique finitely supported coefficients indexed by the basis
elements. An ordered coordinate tuple additionally needs a correspondence from
positions to those elements. Preserve that correspondence with the source
representation; a basis label or IRI does not supply an ordering.

`math:hasBasis` records a basis choice for a space or algebra carrier and permits
several supplied choices. `math:inBasis` identifies the basis context of a vector
or Gram matrix. Keep the coordinate-independent vector or bilinear form separate
from its coordinate array, and retain `math:representsForm` as the separate form
relation. Changing the basis link does not transform the entries; a basis change
requires the actual transformation and its ordered input/output conventions.

The exact Gram producer names `p:covBasis` as `math:Basis` and gives `p:pcaGram`
that `math:inBasis` context. Its RDF does not enumerate the basis elements or their
order, so the link alone does not certify an unambiguous indexed matrix. The
Clifford producer separately names an algebra carrier's `BladeBasis` and indexed
grade-one generator records. In the finite real `Cl(p,q)` profile the carrier
dimension is `2^(p+q)`, while the generating quadratic space has `p+q` basis
vectors. The emitted `math:hasBasisBlade` generator records do not enumerate all
carrier basis elements or prove linear independence and spanning.

Orthonormality also requires an inner product. A self-adjoint operator on a
finite-dimensional real or complex inner-product space admits an orthonormal
eigenbasis; this does not make every basis an eigenbasis. Arbitrary
infinite-dimensional self-adjoint operators need not admit an eigenvector basis.
A truncated PCA component family need not span the full input space.

### Eigenvalue and eigenvector associations

An eigenvector is a nonzero vector `v` satisfying `Av = λv` for the supplied
operator on the declared vector space over its scalar field. The eigenvalue `λ`
may be zero. A real operator need not have real eigenvalues; an extension of
scalars is explicit, never inferred from a number or vector label.

In finite dimension, `A = QΛQ⁻¹` requires a complete eigenbasis. The columns of
invertible `Q` are the eigenvectors, and diagonal `Λ` lists their eigenvalues in
matching order. The operator `A` need not be invertible: in a supplied ordered
real basis, `A = diag(0,3)` has `Q = I`, `Λ = diag(0,3)` and the nonzero
eigenvector `(1,0)` for eigenvalue zero. General operators need not be
diagonalizable. Orthonormality or a unitary `Q` requires the corresponding
inner-product and operator hypotheses; the finite-dimensional formula does not
supply a discrete eigenvector basis for arbitrary infinite-dimensional operators.

`math:eigenvalue` and `math:eigenvector` are separate unordered output relations.
Preserve actual associations and ordering in the source representation. The
relations do not construct eigenpairs: neither equal output counts nor RDF order
identifies a pairing, and the Cartesian product of the outputs is not a set of
valid eigenpairs. Repeated identical RDF assertions carry no additional count.
Distinct number resources do not establish algebraic multiplicity, which counts
roots of the characteristic polynomial. Geometric multiplicity is the dimension
of an eigenspace, established by a basis of that eigenspace rather than resource count.
Nonzero scalar multiples share an eigendirection; normalization and representative
selection do not change either multiplicity. A partial output family establishes
no complete diagonalization.

The `exact_pca_residual` producer in
[`crates/math/src/producers.rs`](../../../../crates/math/src/producers.rs) computes
a dominant axis and LDL pivots for fixed Gram inputs and emits PCA role records
with `math:eigensolver p:exactLDLT`. That descriptor is typed as
`math:MathematicalObject`; the producer emits no `math:eigenvalue` or
`math:eigenvector` result triples. The ordinary example's `ex:randomizedSVD`
descriptor names a `math:SingularValueDecomposition`. Neither descriptor, a type
declaration nor the validation-only grounding certifies an executed
eigendecomposition or a complete eigenbasis.

### Singular-value factors, dimensions and multiplicities

`math:SingularValueDecomposition` describes a linear map between finite-dimensional
real or complex inner-product spaces. In supplied orthonormal coordinate bases,
`A = UΣV*`; `V*` is the conjugate transpose, or ordinary transpose over the reals.
The diagonal entries of `Σ` are real and nonnegative. Singular and rectangular
maps admit this factorization without an inverse or an eigenbasis of `A`.
Preserve both inner products: changing them may change the singular values. This
finite matrix statement supplies no SVD for an arbitrary infinite-dimensional
operator.

For `m×n` matrix `A`, `k = min(m,n)` and exact rank `r`, the following conventions
make factor dimensions and zero treatment explicit. Source names vary; preserve
the actual dimensions and retained occurrences rather than interpreting a name
alone.

| Form | U | Σ | V | Retained diagonal occurrences |
| --- | --- | --- | --- | --- |
| Full | m×m | m×n | n×n | k, including zeros |
| Thin | m×k | k×k | n×k | k, including zeros |
| Rank-compact | m×r | r×r | n×r | r, all strictly positive |

The columns of each `U` and `V` are orthonormal, and corresponding columns share
the diagonal order of `Σ`. For rank zero, the compact convention has empty
column families and an empty diagonal; it still represents the zero map. A
truncation that discards positive singular values approximates `A` rather than
factoring it exactly. Numeric rank based on a supplied tolerance is a separate
reported policy and result, not mathematical rank or an implicit zero test.

For real `A = diag(3,3,0)` with the standard Euclidean inner product and an
ordered orthonormal basis, `U = V = I` and `Σ = A` give the full exact SVD.
The singular-value sequence `3,3,0` has two positive occurrences and rank two,
although it has only one distinct positive value. An unordered
`math:singularValue` link does not encode these occurrences. Repeating an RDF
assertion adds no multiplicity; distinct number resources prove no multiplicity
either. Preserve supplied occurrence, factor association, order, dimensions and
completeness evidence in its source representation. Missing zero outputs do not
establish full rank, and a truncated output family does not disclose omitted
values. Singular values, their squares and explained-variance ratios retain
distinct mathematical roles even when particular numbers coincide.

The registered SVD grounding in
[`mappings/equivalences.ttl`](../mappings/equivalences.ttl) is `skos:exactMatch`
to Wikidata `wd:Q420904`, with `logic:WellBehavedLens`,
`logic:InstitutionMorphism` and `logic:SoundUnderApproximation` preservation.
This concept relation supplies no factor matrices, numerical algorithm or
execution certificate. The ordinary scene and PCA fixtures name SVD descriptors
without publishing factor or `math:singularValue` results. The exact Gram/PCA
producer emits its LDL descriptor and PCA role records, with no singular-value
output triples; those declarations do not establish an executed SVD.

### Linear maps and finite rank values

`math:LinearMap` is a total map `T: V → W` between vector spaces over the same
scalar field, with `T(av+bw) = aT(v)+bT(w)`. Neither finite dimension nor an
inner product is needed for linearity. Preserve the vector-space structures,
their scalar actions and their carriers separately: the inherited
`math:Function` source profile requires `math:domain` and `math:codomain` to
identify carrier sets, while `math:underlyingSet` relates the algebraic structures
to their carriers. Source and target roles may share a set without becoming the
same role. A class, frame, label or finite sample does not prove the universal
preservation law or execute the map.

A finite matrix representation needs ordered bases for both source and target.
Its column coordinates use the target basis, and column positions correspond to
the ordered source basis. Coordinate changes for the same map over the same
field preserve rank; they require actual transformations and their conventions.
A matrix-shaped dataset supplies no such map or basis evidence. A neural layer
may also contain an affine bias or a nonlinear activation; its weight matrix
does not establish linearity of the whole layer.

`math:Rank` is the dimension of the attained image over the declared field,
which can differ from either ambient dimension. A finite-dimensional source
obeys `dim(V) = rank(T) + dim(kernel(T))`. For the real projection
`T: R³ → R²`, `(x,y,z) ↦ (x,y)`, the rank is two and the kernel has dimension
one. The zero map has rank zero even between nonzero spaces. Infinite-dimensional
spaces can have finite-rank maps, while an infinite-dimensional image has
infinite rank; do not replace an unknown or infinite rank with a guessed finite
number. Preserve the actual scalar-field interpretation when comparing ranks.

`math:rankValue` records only a supplied finite nonnegative-integer value. Its
mathematical value contract has no fixed machine-width cap or infinity marker.
Omission is distinct from zero. Numerical tolerance-based rank and rank-truncated
approximations retain their policies and evidence separately. In the
finite-dimensional real or complex inner-product SVD setting, exact rank counts
positive singular-value occurrences with multiplicity, as the
[singular-value contract](#singular-value-factors-dimensions-and-multiplicities)
states. Distinct values or RDF resources do not supply that count. Tensor-axis
count and root-system rank are separate invariants.

Keep the `math:Rank` resource distinct from the map, its matrix and its numeric
value. These terms supply no dedicated rank-of relation; preserve the source
identification of the relevant map and field in its existing representation.
The finite-value annotation alone certifies neither a specialized lexical/range
or cardinality guard nor an executed generated/native validation or rank
calculation.

The registered LinearMap grounding in
[`mappings/equivalences.ttl`](../mappings/equivalences.ttl) is `skos:exactMatch`
to Wikidata `wd:Q207643`, with `logic:WellBehavedLens`,
`logic:InstitutionMorphism` and `logic:SoundUnderApproximation` preservation.
It relates the linear-map concept; it does not prove an individual map's laws,
calculate its image or rank, or certify an executable implementation.

## The exact numeric layer — bilinear forms, matrices, norms, and rational carriers

Core classes: `math:BilinearForm`, `math:SymmetricBilinearForm`, `math:Matrix`, `math:SymmetricMatrix`,
`math:GramMatrix`, `math:Vector`, `math:Norm`, `math:Distance`, `math:DefinitenessKind`,
`math:RationalValue`, `math:VectorComponent`, and `math:MatrixEntry`.

Core properties: `math:inducedByForm`, `math:inducedByNorm`, `math:representsForm`, `math:inBasis`,
`math:definiteness`, `math:hasComponent`, `math:componentValue`, `math:hasEntry`, `math:entryValue`,
`math:numerator`, `math:denominator`, `math:atIndex`, `math:atRow`, and `math:atColumn`.

This reusable layer turns the inner-product **frame** into carriers you can compute a length on. A
`math:SymmetricBilinearForm` is the symmetric case of a `math:BilinearForm`; a `math:MetricTensor`
**is** the tangent-space instance of that same idiom, so the geometry charter's metric and this layer
share one notion of a symmetric form (no parallel tower). Represented in a `math:Basis` a symmetric
form becomes a `math:GramMatrix` (`math:representsForm` + `math:inBasis`) — a `math:SymmetricMatrix`
(⊑ `math:Matrix`) whose entries are indexed `math:MatrixEntry` cells (`math:atRow` / `math:atColumn` /
`math:entryValue`). A `math:Vector` carries indexed `math:VectorComponent` coordinates in the same
style. When a symmetric form is positive-definite (`math:definiteness math:positiveDefinite`) it
induces a `math:Norm` (`math:inducedByForm`), ‖x‖ = √⟨x, x⟩ = √(xᵀGx), which in turn induces a metric
`math:Distance` (`math:inducedByNorm`). `math:DefinitenessKind` is an open value vocabulary
(`math:positiveDefinite`, `math:positiveSemidefinite`, `math:indefinite`, `math:negativeDefinite` —
individuals, never `owl:oneOf`), the object-layer counterpart of a `math:MetricSignature`.

> **Hard rule — exactness is explicit.** A `math:RationalValue` is an *exact* p/q carrier: a
> `math:numerator` / `math:denominator` integer pair (denominator ≠ 0), and it carries **no** decimal
> literal — 1/4 is the pair (1, 4), never 0.25 (the decimal belongs to a distinct
> `math:ApproximateValue`). Matrix entries and vector components are `math:RationalValue`, so a Gram
> matrix and the quadratic form xᵀGx — and hence the norm √(xᵀGx) an affect-intensity metric grounds
> on — are exact by construction. The specialized `math:CartanMatrix` and `math:DatasetMatrix` keep
> their own parents; they are not reparented onto `math:Matrix`.

## PCA and the KG-projection flagship

Core classes: `math:PCAAnalysis`, `math:PrincipalComponent`, `math:LoadingVector`,
`math:ScoreVector`, `math:ExplainedVariance`, `math:ProjectionResidual`, and
`math:ResidualInterpretationClaim`.

Core properties: `math:analysisInput`, `math:centeringPolicy`, `math:scalingPolicy`,
`math:covarianceOperator`, `math:eigensolver`, `math:principalComponent`, `math:explainedVarianceRatio`,
and `math:residualSubspace`.

A `math:PCAAnalysis` declares its input matrix/tensor, its centering and scaling policy, the
`math:CovarianceOperator` it decomposes, the `math:eigensolver` used, and its outputs — principal
components, loadings, scores, explained-variance ratios, and residuals. The KG flagship —
*embed a knowledge graph into a complex space, take the orthogonal complement of the embedded
subspace, run PCA on the residuals, interpret the residual meaning* — composes cleanly: the embedding
(below) produces a subspace, the geometry charter's `math:OrthogonalComplement` (defined by the
Hermitian inner product) names the residual subspace, and the PCA decomposes it.

> **Hard rule — residual meaning is a claim, not a property.** A `math:ResidualInterpretationClaim`
> (what a residual subspace or a latent dimension *means*) is a `gmeow:Observation` with a
> `gmeow:vantage` and evidence — never an intrinsic property of the residual or the vector. Semantic
> meaning read off geometry is inference from a standpoint (`math:ResidualMeaningAsProperty`,
> [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md)).

## Learning, embeddings, and latent spaces

Core classes: `math:LearnedModel`, `math:LossFunction`, `math:OptimizationProblem`,
`math:Embedding`, `math:KnowledgeGraphEmbedding`, `math:LatentSpace`, and `math:EmbeddingDimension`.

Core properties: `math:embeddingSource`, `math:targetSpace`, `math:embeddingFunction`,
`math:embeddingModel`, `math:objectiveFunction`, `math:constraint`, and `math:latentDimensionOf`.

A `math:Embedding` names its source object, target space, embedding function, and model; a
`math:KnowledgeGraphEmbedding` maps a graph into a `math:LatentSpace`. Training is a
`math:OptimizationProblem` (objective + constraints; optimization structure is largely authored,
[`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md)) minimizing a `math:LossFunction`; the
gradient machinery is the calculus charter's derivatives, and natural-gradient / Fisher-information
geometry is the authored information-geometry material. Latent-space and dimension *semantics* are
GMEOW-original — no external ontology exists.

The *credence* a latent direction means something is itself first-class, and it has two sibling
sources over the SAME `math:ResidualInterpretationClaim` surface. The status-quo signal is
**similarity-derived** (`math:similarityDerivedCredence`): confidence read off embedding-space
proximity — nearest-neighbour retrieval, cosine similarity, clustering density. Its topological sibling
is **persistence-derived** (`math:persistenceDerivedCredence`): confidence read off how long the
topological feature a direction realizes *survives* across a `math:Filtration` of the embedded image —
the `math:PersistenceLifetime`'s death − birth
([`MATHEMATICS-ANALYSIS-AND-GEOMETRY.md`](MATHEMATICS-ANALYSIS-AND-GEOMETRY.md)). The two are one axis,
`math:CredenceDerivationKind`, minted with both members rather than a lone new individual, so which
evidence a confidence rests on is queryable rather than implicit. A `math:StabilityCalibrationRecord`
ties the persistence evidence (`math:calibrationEvidence`), the derivation kind
(`math:credenceDerivationKind`), and the stability guarantee (`math:stabilityGuarantee`,
`math:bottleneckStabilityTheorem`) to the produced confidence, which lands as `logic:confidence` on the
held `math:ResidualInterpretationClaim` (never `logic:probability`, never `math:ProbabilityValue`). The
claim references the record through `gmeow:wasDerivedFrom`, so a consumer can trace a confidence back to
the filtration, the birth/death thresholds, and the bottleneck bound that calibrated it — the whole
point of grounding the credence rather than emitting a bare number.

> **Hard rule — a persistence-derived credence is warranted, not a heuristic.** A
> `math:StabilityCalibrationRecord` must name its persistence evidence, its derivation kind, and the
> stability theorem underwriting it, or it is ill-formed (`math:UngroundedStabilityCalibration`,
> [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md)); the filtration and the lifetime it rests
> on must themselves be structurally complete (`math:UnderspecifiedFiltration`,
> `math:UnderspecifiedPersistenceLifetime`). The stability theorem is why the calibration is sound: the
> bottleneck bound guarantees that a small perturbation of the underlying function moves the persistence
> — and hence the credence — by a bounded amount.

## Dimensional reduction, information measures, and vector-symbolic operations

`math:DimensionalReduction` generalizes PCA-adjacent projection without collapsing the method and
its output. Every run names its input, exact target dimension, and output `math:Embedding`;
`math:UMAPReduction`, `math:IsomapReduction`, initialization, reconstruction, intrinsic-dimension,
and path records specialize or qualify that frame. A lower-dimensional coordinate array is never
given semantic meaning by the reduction itself: interpretations remain observation claims with a
vantage and evidence.

The information family distinguishes `math:Entropy`, `math:MutualInformation`,
`math:KullbackLeiblerDivergence`, `math:CrossEntropy`, `math:FisherInformation`, and
`math:Surprisal` under `math:InformationMeasure`; their verified Wikidata links are identity anchors,
not substitutes for GMEOW's explicit probability and operand frames. `math:VectorBinding`,
`math:VectorBundling`, and `math:VectorUnbinding` name reusable vector-symbolic operations, while
recovery quality and semantic readings remain measured, framed results rather than properties of
the vectors.

## An AI describing its own structure — the self-structure flagship

Core classes: `math:TensorComputationGraph`, `math:NeuralLayer`, `math:WeightTensor`,
`math:ActivationFunction`, `math:AttentionOperation`, and `math:ParameterSpace`.

Core properties: `math:computationNode`, `math:tensorOperation`, `math:weightOf`,
`math:parameterSpaceOf`, and `math:architectureOf`.

A neural network's forward pass **is** a `math:TensorComputationGraph` — an expression AST whose
operators are tensor operations (matmul, contraction, `math:AttentionOperation` as a bilinear form),
whose leaves are `math:WeightTensor`s living in a `math:ParameterSpace`, and whose structure is the
architecture. This reuses the expression AST wholesale: the graph is `math:ApplicationExpression`s
over tensor operators. The AI's *reflection* on that structure — "these dimensions encode X" — is
carried at the `logic:` metalevel (statements-as-objects) and the metacognition slice, exactly the
residual-meaning discipline above. This is the **dogfooding apex**: the grounding layer represents the
structure of the system using it, lifted from any ONNX/model graph via the bridge
([`MATHEMATICS-BRIDGES.md`](MATHEMATICS-BRIDGES.md)).

> **Flagship — AI self-structure.** Answerable when a tensor computational graph is a `math:` object
> (the expression AST over tensors), weights are matrices in a parameter space, embeddings are latent
> geometry with residual meaning held as observations, and the reflection on it is `logic:`
> metalevel + metacognition — self-reference without paradox, across the `math:` and `logic:` grounding layers.

## A worked example — the KG residual PCA

```ttl
ex:kgEmbedding
    a math:KnowledgeGraphEmbedding ;
    math:embeddingSource ex:sourceGraph ;
    math:targetSpace ex:complexSpace4096 ;
    math:embeddingFunction ex:nodeRelFn ;
    math:embeddingModel ex:hyperbolicComplexModelV3 .

ex:residualSubspace
    a math:OrthogonalComplement ;
    math:orthogonalComplementOf ex:embeddedSubspace ;
    math:ambientSpace ex:complexSpace4096 ;
    math:complementSemantics math:orthogonalComplement ;
    math:definedByInnerProduct ex:hermitianInnerProduct .

ex:residualPCA
    a math:PCAAnalysis ;
    math:analysisInput ex:residualSubspace ;
    math:centeringPolicy math:meanCentered ;
    math:covarianceOperator ex:residualCovariance ;
    math:eigensolver ex:randomizedSVD ;
    math:principalComponent ex:pc1 , ex:pc2 , ex:pc3 .

ex:residualMeaning
    a math:ResidualInterpretationClaim , gmeow:Observation ;
    gmeow:vantage ex:analystModelCommit ;
    gmeow:observedFeature ex:pc1 ;
    gmeow:observationResult ex:latentThemeClaim .
```

Every step is named — the embedding, the inner-product-defined complement, the PCA policy and outputs
— and the *meaning* of `pc1` is an observation held by a vantage, not a fact stamped on the vector.

## Shape and lint gates

Catalogued in [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md): an orthogonal complement
names its ambient space and inner product; a PCA names input/centering/scaling/covariance/eigensolver
/components/loadings/scores/explained-variance/residuals; a residual-meaning claim is an observation,
not a property; an embedding names source/target/function/model; a tensor computation graph is an AST
over tensor operators with weights in a declared parameter space. The numeric layer
adds three gates: a `math:RationalValue` has a non-zero `math:denominator`, a `math:Norm` rests on a
positive-definite `math:SymmetricBilinearForm` (`math:inducedByForm`), and a `math:GramMatrix` is
symmetric (every `math:MatrixEntry` has a transpose entry of equal value).

## Competency questions

1. What are the subspace, its ambient space, and the inner product defining this orthogonal
   complement?
2. What input, centering/scaling policy, covariance operator, and eigensolver produced these
   principal components, and what variance do they explain?
3. What does this residual subspace or latent dimension *mean*, and which vantage claims it?
4. What source, target space, function, and model define this embedding?
5. What is this AI's tensor computation graph, its layers, and its parameter space — and where is its
   self-reflection carried?
6. Which symmetric bilinear forms are positive-definite, what Gram matrix represents each, and what
   norm do they induce (the exact numeric layer grounding √(xᵀGx))?

## Declared tensor-shape consistency

`TensorShape` is reusable metadata over the existing mathematical-object owner,
not a tensor payload or an alias for a rendered type string. `TensorAxis`
retains the separate indexed-axis identity already defined by the expression
model; it is not an `ArgumentSlot` requiring a slot expression. An unknown shape
is an omitted `tensorShape`; a declared rank-zero shape with no axes is a scalar.

Eight canonical laws and their class-local profiles state the source contract:
nonnegative rank, index and concrete extent; distinct-axis count equal to rank;
positive-rank axis presence; per-parent index bounds and numeric uniqueness;
and at-most-one concrete/symbolic extent choice. Rank and index are required
only on their declared shape and axis objects. Extent disclosure remains
optional, and shape disclosure remains optional on the existing tensor and
type descriptors.

For rank n, the profile gives n distinct axis nodes with unique nonnegative
integer indices strictly below n. Therefore the index set is exactly
0 through n minus one, with no inferred predecessor arithmetic. The rank-zero
case needs no axis; the separate positive-rank law closes the aggregate's empty
group without turning missing shape metadata into zero.

Integer-family unions preserve the source literal datatypes, while value-range
laws express nonnegativity. Numeric equality deliberately compares separately
bound index values, including alternate integer lexical spellings. The extent
choice concerns property presence; it neither resolves symbolic lengths nor
fills an unknown extent with a sentinel. A symbolic length points to a typed
free-variable declaration, preserving the existing declaration/value-domain
distinction rather than assigning a tensor's value space to its axis length.

The source uses existing logic validation and aggregate primitives. Their
validation projections are derived, never second authoring surfaces.
The [ordinary example](../examples/tensor-shapes.ttl) records unknown shape,
scalar rank zero, symbolic and concrete axes, an unstated extent and a disclosed
zero extent. It is source data, not an executed validation result. The
slice tests hold the controls: one conforming fixture that satisfies all eight
laws, one counter-example per law that breaks that law alone, and two
structural assertions over the laws' own parameters and comparisons. The
`expressionValueSpace` seam and ONNX construction behavior remain separate
existing owners; these laws claim no tensor payload/operator validation or
native, produced or installed-consumer completion.
