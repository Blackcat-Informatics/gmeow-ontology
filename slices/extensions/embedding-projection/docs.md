<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The embedding-projection extension — vectors as an auditable category

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/embedding-projection` · **tier: extension**
> The GMEOW meaning of PurRDF's `.purremb` container (the PurRDF PURREMB v1 container format):
> PurRDF owns the binary layout and access API; this slice owns what the projection MEANS.

Every embedding stack quietly collapses four different things into one blob of
floats: the exact source it was computed over, the lossy vector pack it produced,
the retrieval scores it hands back, and the search index it built to go fast.
Once they are one blob, you cannot say which vectors came from which bytes,
whether two scores are even comparable, or whether the index just missed a true
neighbour. This slice makes that **category boundary machine-checkable**: it
gives each of the four its own GMEOW class, and it makes the crossings between
them either honest correspondences or hard failures.

## The four categories, kept apart

The whole design is the refusal to conflate four things (this is the AC5
four-way separation, stated in prose):

1. **The canonical source** — `gmeow:InformationObject` (`ex:bookshelfPack`).
   The exact, content-addressed, classified RDF/GTS bytes. Its
   `gmeow:contentDigest` is byte identity, its `gmeow:gtsHeadId` is committed
   history, and its `gmeow:hasSensitivity` / `gmeow:hasDisclosurePolicy` are the
   classification everything downstream inherits. It is upstream, exact, and
   canonical.
2. **The generated lossy projection** — `gmeow:EmbeddingProjection`
   (`ex:bookshelfProjA`). A whole `.purremb` pack that AGGREGATES the core ai
   slice's per-object `gmeow:Embedding` rows (via `gmeow:aggregatesEmbedding`) under one
   `gmeow:VectorSpaceContract`. It is deliberately NOT a subclass of
   `gmeow:Embedding` — an embedding is one object's vector; a projection is the
   pack that gathers many — so the two constructs are non-duplicative. It is
   lossy: payloads live outside the graph by reference (Principle 12), and
   quantization and pooling discard information. Its own `gmeow:contentDigest`
   answers "which exact projection?".
3. **The retrieval-similarity observation** — `gmeow:SimilarityObservation`
   (`ex:crossObs`, `ex:inObs`). A standalone, attributable, pairwise proximity
   CLAIM over one effective space under one metric — `logic:Vague`, never
   equivalence, never entailment, never a truth judgment. It is `owl:disjointWith`
   the core ai slice's `gmeow:RetrievalEvent`: a weighable claim about two resources'
   vector proximity is not a per-triple score riding a retrieval activity.
4. **The derived rebuildable index** — `gmeow:DerivedVectorIndex`
   (`ex:annIndexA`). A non-authoritative accelerator over one exact projection
   and one exact effective space, whose approximation profile
   (`gmeow:approximationRecall`, `gmeow:indexNondeterminism`,
   `gmeow:indexLossContract`) stays explicit and whose rebuild is an append-only
   supersession, never an erasure. The authoritative vectors live in the
   projection; the index only speeds retrieval over them.

Keeping these four apart is what lets the slice's constraints say precisely
useful things: that a projection's recorded source digest must match its live
source (`gmeow:SourceDigestMatchConstraint`), that a certified source's history
must travel with it (`gmeow:GtsHeadIdPropagationConstraint`), that sensitivity
never drops without a reviewable `gmeow:DeclassificationAct`
(`gmeow:SecurityMonotonicityConstraint`), and that a container and its sidecar
manifest must agree on every shared referent
(`gmeow:ProfileSourceDigestAgreementConstraint` and its siblings).

## The fibration: why you cannot compare across spaces

The heart of the slice is a small piece of structure that turns "you cannot
compare vectors across two spaces" from an ad-hoc rule into a fact you can read
off the types.

A `gmeow:VectorSpaceContract` effective space is a **base object**. Over each
one sits a **fiber**: the metric space in which its vectors actually live, under
the `gmeow:distanceMetric` and normalization that space declares. A
`gmeow:EmbeddingFamily` groups the effective spaces that share one stored
matrix — the full family space (`gmeow:matryoshkaFixed`) and every declared
leading-prefix space over it (`gmeow:matryoshkaPrefix`) — so a family is the
bundle of base objects resolving to a single once-stored matrix.

What the slice **enforces** here is the comparability identity, not every contract
component individually. A space's `gmeow:contentDigest` is the enforced comparability
identity, and its mandatory core is individually required: the family anchor
(`gmeow:effectiveOfFamily`) plus the three space-level comparability axes
(`gmeow:embeddingDimensions`, `gmeow:distanceMetric`, `gmeow:normalizationKind`). A
family individually requires only its `gmeow:contentDigest` and the irreducible model
artifact (`gmeow:embeddingModel`). The remaining generation-contract components
(tokenizer, preprocessing, chunking, pooling, truncation, dtype, quantization, and
the rest) are **declared, not individually required** — they fold into the digest and
bear on comparability, but the digest is the comparability decision.

A `gmeow:similarityScore` is meaningful **only within a fiber**: it is a graded
proximity between two vectors of one effective space, which is exactly why
`gmeow:overVectorSpace` is functional and `gmeow:CrossSpaceComparisonConstraint`
guards it. Comparing a vector of one base object against a vector of another is
**transport along a base morphism** — and a base morphism is not free. It is a
justifying `logic:Correspondence` (in the worked scene, an
`logic:AffineCorrespondence`, `ex:crossCorr`) whose endpoints are exactly the two
`gmeow:VectorSpaceContract` individuals being crossed. So "you cannot compare
across spaces" is a **structural fact**: without a base morphism there is no map
between the fibers, and the constraint simply refuses a cross-space
`gmeow:SimilarityObservation` that has no correspondence to travel along.

This base-object structure is what several harder retrieval problems frame
cleanly. **Multi-model retrieval**, **model migration**, and **cross-corpus
search** are all transport along base morphisms — a chain of correspondences
A→B→C *describes* a path from A's fiber to C's. But the chain does not
**automatically** satisfy validation: `gmeow:CrossSpaceComparisonConstraint`
demands a DIRECT `logic:Correspondence` whose endpoints are exactly the two
`gmeow:VectorSpaceContract` individuals a `gmeow:SimilarityObservation` crosses,
and the slice does not compose A→B and B→C into an A→C bridge for you. To score
across A and C a producer MUST supply (materialize) the composed A→C
correspondence explicitly; only then does the guard pass. What this framing does
**not** hand you at all is model **fusion**: merging two spaces into one is a
colimit / merge, and that axis of the correspondence calculus is deliberately
OPEN. The slice does not overclaim it as solved; it gives you honest, explicitly
supplied composition, not free coproducts and not automatic chain closure.

A forward pointer for the mathematically inclined: a Lawvere `[0, ∞]`-enriched
metric-space reading of the fibers is deliberately NOT minted here. There is no
canonical source metric to enrich over — the metric is a per-space contract, not
a property of the source — so promoting the fibers to enriched categories would
invent structure the source does not carry. The fibration over
`gmeow:VectorSpaceContract` base objects is the honest floor.

## Minting the minimum

The slice mints only what carries genuinely new meaning, and reuses everything
already shipped:

- **ai** (core) — `gmeow:Embedding`, `gmeow:embeddingModel`,
  `gmeow:embeddingDimensions`, `gmeow:DistanceMetric` / `gmeow:distanceMetric`,
  `gmeow:VectorIndex`, `gmeow:RetrievalEvent` — the shared vector/embedding/
  retrieval primitives (extensions/graphrag is the other consumer). The
  projection aggregates the per-object rows rather than redefining them.
- **graphrag** — `gmeow:indexAlgorithm` / `gmeow:indexParameters` for the
  derived index's algorithm/parameters.
- **kernel** — `gmeow:SensitivityLevel`, `gmeow:hasSensitivity`,
  `gmeow:hasDisclosurePolicy` for disclosure control (the slice only adds a
  `gmeow:sensitivityRank` total order over the existing levels, so monotonicity
  is a rank comparison).
- **sources / GTS** — `gmeow:contentDigest` for identity and `gmeow:gtsHeadId`
  for committed history.
- **provenance** — `gmeow:wasGeneratedBy` / `gmeow:wasDerivedFrom` for build and
  derivation attribution.
- **`logic:`** — `logic:Correspondence` and its preservation / determinacy
  vocabulary carry the honest lossy-lens verdict (`ex:corrProjA`: a vector
  VALIDATES proximity, it does not ENTAIL meaning) and the cross-space bridges.
- **`math:`** — `math:Quantity` grounds every graded number
  (`gmeow:similarityScore`, `gmeow:approximationRecall`,
  `gmeow:reproducibilityTolerance`) so it travels with its metric context rather
  than as a bare float.

What is genuinely new is the pack-level projection itself, the effective-space /
family split, the ordered target namespace, the profile-surface agreement model,
and the disclosure-monotonicity act — the terms with no prior home.

## Where the bytes live

PurRDF's `.purremb` container (the PurRDF PURREMB v1 container format) is the binary
owner: the `PURREMB1` framing, the sorted section directory, the lossless dense
matrices, the domain-separated identities, and the Exact / Certified verification
modes. It mints no vocabulary. This slice is the meaning that rides on it — the
projection, its comparability contract, its provenance, its disclosure controls,
and its integrity digests as auditable GMEOW ontology content. The normative
mapping from container to meaning is specified in
[`design/PURREMB-PROFILE.md`](design/PURREMB-PROFILE.md); the whole scene is
authored end to end in
[`examples/purremb-bookshelf.ttl`](examples/purremb-bookshelf.ttl).
