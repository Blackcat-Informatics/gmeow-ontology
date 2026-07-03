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
> realization implements X, established by the slice's `shapes.ttl`, competency queries, and the
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
objects with declared operands and outputs. External anchor: only OpenMath `linalg`/`linalgeig`
names eigen symbols; SVD/PCA are authored.

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
> metalevel + metacognition — self-reference without paradox, across the two grounding layers.

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
over tensor operators with weights in a declared parameter space.

## Competency questions

1. What are the subspace, its ambient space, and the inner product defining this orthogonal
   complement?
2. What input, centering/scaling policy, covariance operator, and eigensolver produced these
   principal components, and what variance do they explain?
3. What does this residual subspace or latent dimension *mean*, and which vantage claims it?
4. What source, target space, function, and model define this embedding?
5. What is this AI's tensor computation graph, its layers, and its parameter space — and where is its
   self-reflection carried?
