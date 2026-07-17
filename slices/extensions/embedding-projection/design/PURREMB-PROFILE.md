<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The `.purremb` projection profile — normative specification

> The **profile charter** of the `embedding-projection` slice: what a GMEOW
> `.purremb` embedding projection MEANS, what it MUST carry, and what a
> conforming producer and consumer MUST do. The binary layout and the access API
> are owned by PurRDF (the PurRDF PURREMB v1 container format); this document owns the
> semantics that ride on it. It is the normative peer of the slice narrative in
> [`../docs.md`](../docs.md).
>
> **Reading this document.** The declarative present tense is normative: "X is"
> means a conforming `.purremb` projection carries X, established by the shipped
> vocabulary in [`../module.ttl`](../module.ttl) and exercised in its passing
> direction by the worked scene in
> [`../examples/purremb-bookshelf.ttl`](../examples/purremb-bookshelf.ttl). Every
> term named below is a shipped term of that module or a reused kernel / graphrag
> / `logic:` / `math:` term. It is not a claim that any implementation realizes
> more than the module and its competency corpus demonstrate.

## 1. Purpose and boundary

The `.purremb` container is a deterministic, mmap-native embedding companion to
an exact `.purrpck` RDF/GTS source pack. PurRDF owns the container in full: the
`PURREMB1` framing, the fixed header, the sorted section directory, the 64-byte
section alignment, the little-endian encoding, the minimal zero padding, the
exact EOF, the fixed trailer, the required / derived / unknown section rules, and
the encoders for every section (target, relation, token-span, family-contract,
target-set, matrix, external-binding, derived-index). PurRDF mints **no**
vocabulary; it moves and integrity-checks bytes.

This profile owns the other half: **what those bytes mean as GMEOW ontology
content.** A `.purremb` projection is a generated, content-addressed, lossy
`gmeow:EmbeddingProjection` over one exact source pack — its vector payloads live
outside the graph by reference (Principle 12), and quantization, pooling, and
truncation discard information, so the projection is lossy by construction. The
graph carries the identities, contracts, provenance, disclosure controls, and
integrity digests that make the container auditable without re-opening it.

The boundary is exact. PurRDF answers "are these bytes well-formed and
internally consistent?" (per-section and whole-artifact **integrity**). This
profile answers "is this projection a faithful, comparable, correctly-classified
view of the source it claims?" (**meaning**, comparability, provenance, and
disclosure). Authenticity and confidentiality of the container in transit or at
rest are external transport and storage controls; they are neither minted here
nor claimed by the container contract.

## 2. Selection

A `.purremb` projection is selected two ways, both first-class and both required
for a conforming pack:

- The **GTS profile individual** `gmeow:gtsProfilePurrEmb` (a
  `gmeow:GTSProfile`) names the `.purremb` package profile in the bundle. It is
  the handle a GTS or generated catalog references the projection through.
- The **slice profile** string `"purremb"` selects the `embedding-projection`
  slice's projection producer in the pipeline.

Selection is explicit feature selection, not optionality: once
`gmeow:gtsProfilePurrEmb` and the `"purremb"` slice profile are selected, every
identity, contract, constraint, and surface below is mandatory. A missing input
causes recomputation; a missing required datum or a failed constraint is a HARD
FAIL, never a silent degradation.

## 3. Identity mapping

Every domain-separated identity PurRDF isolates in the container maps onto a
GMEOW carrier: no part of the container's identity model is left without a
graph-side term. The mapping is total **at the carrier level**, but GMEOW is a
super-ontology, not a byte-mirror of the container — it organizes one axis (the
distance metric) at a different level than PurRDF, as the note after this table
records. It is not a claim of fold-for-fold digest identity.

| PurRDF `.purremb` domain-separated identity | GMEOW carrier |
| --- | --- |
| Exact source (source byte digest, PurRDF SHA-256 `source_exact_digest`) | recorded opaquely on `gmeow:ProfileSurface` via `gmeow:recordsSourceDigest` (the container's SHA-256, for cross-check). GMEOW's own source identity is separate: `gmeow:projectionSource`'s `gmeow:contentDigest` (BLAKE3), with `gmeow:projectionSourceDigest` its recorded drift-check copy (`gmeow:SourceDigestMatchConstraint`) |
| Certified RDF (RDFC digest / committed history) | the source's `gmeow:gtsHeadId`, propagated into the projection as `gmeow:projectionSourceGtsHeadId` |
| Chunking contract | `gmeow:chunkingContract` on the `gmeow:EmbeddingFamily`, folded into the family's `gmeow:contentDigest` |
| Embedding family (model + full generation contract) | `gmeow:EmbeddingFamily`, identity is its `gmeow:contentDigest` |
| Effective vector space | `gmeow:VectorSpaceContract`, identity is its `gmeow:contentDigest` |
| Target | `gmeow:VectorTarget` (exact identity via `gmeow:targetsResource` or its own `gmeow:contentDigest`) |
| Target set | `gmeow:TargetSet`, identity is its `gmeow:contentDigest` over the ordered membership |
| Exact stored matrix | container-internal (PurRDF); surfaced to GMEOW only as a recorded digest on `gmeow:ProfileSurface` (`gmeow:recordsModelContractDigest`) |
| Effective matrix projection | the `(gmeow:TargetSet, gmeow:VectorSpaceContract)` pair a projection binds (`gmeow:hasTargetSet` + `gmeow:hasVectorSpaceContract`) |
| External binding | `gmeow:ExternalBinding`, identity is its `gmeow:contentDigest` |
| Derived index | `gmeow:DerivedVectorIndex`, identity is its `gmeow:contentDigest` |
| Artifact integrity root | container-internal (PurRDF); surfaced to GMEOW only as recorded digests on `gmeow:ProfileSurface` |

Two identities are deliberately **not** promoted into first-class graph objects:
the **exact stored matrix** and the **artifact integrity root**. Both are
container-internal to PurRDF — the stored highest-dimensional matrix is a dense,
row-major, lossless f32/f64 payload the container owns, and the integrity root is
the container's own per-section and whole-artifact digest tree. GMEOW sees them
only through the digests a `gmeow:ProfileSurface` records, so the graph can cross-
check the container's integrity claims without duplicating its bytes (Principle
12).

The domain separation is load-bearing. A `gmeow:EmbeddingFamily` is the model
plus the full generation contract (model, inference engine, execution
configuration, tokenizer, subject-projection/text-serialization, preprocessing,
chunking, pooling, generation-time normalization, truncation, dtype, byte order,
quantization) that produces and stores the top-dimensional matrix **once**; its
`gmeow:contentDigest` folds over that whole contract. A `gmeow:VectorSpaceContract`
is an **effective** space — the full family space or a declared leading prefix of
it (Matryoshka) — carrying the effective dimension, the distance metric, the
per-prefix normalization, and the prefix policy, anchored to its family by the
functional `gmeow:effectiveOfFamily`. Comparability is therefore decidable by
digest equality on the effective space, never by prose agreement.

**One deliberate divergence from PurRDF's identity structure.** PurRDF folds the
distance metric into its *family* contract (`FamilyContractDigest → FamilyId`), so
`VectorSpaceId = H(D_SPACE; FamilyId, effective_dimension, prefix_postprocessing)`
inherits the metric through the family. GMEOW instead places `gmeow:distanceMetric`
on the *effective* `gmeow:VectorSpaceContract`: the metric is a property of
comparability, not of the vectors the family produces — the same stored matrix is
compared under different metrics — so it belongs to the space, not the
vector-producing family. The **comparability decision is identical** under both
structures: two projections that differ only in metric are a cross-space comparison
in GMEOW (distinct `gmeow:VectorSpaceContract` digests) exactly as they are distinct
`VectorSpaceId`s in PurRDF. What differs is the family/space boundary — GMEOW's
`gmeow:EmbeddingFamily` identity is **not** byte-isomorphic to PurRDF's `FamilyId`
across the metric axis (two metric-differing configurations share one GMEOW family
but are two PurRDF families). A producer computes each system's digests within that
system; it never derives one from the other.

**Digest algorithms are not interchangeable across the two identity systems.**
GMEOW content-addresses its own terms — `gmeow:contentDigest` on families, spaces,
target sets, external bindings, and derived indexes, and the recorded
`gmeow:projectionSourceDigest` — with GMEOW's ontology-wide content-addressing,
**BLAKE3**, so `gmeow:SourceDigestMatchConstraint` and every intra-graph digest
comparison stays within one algorithm. PurRDF, independently, computes the
container's exact and typed digests as **SHA-256** (domain-separated for typed
identities; see the PurRDF PURREMB v1 container format). The digests a
`gmeow:ProfileSurface` records for cross-check — `gmeow:recordsSourceDigest`,
`gmeow:recordsModelContractDigest`, and `gmeow:recordsTargetTableDigest` — are the
**container's** digests and are therefore SHA-256, distinct in both value and
algorithm from the BLAKE3 `gmeow:contentDigest` of the same referent. A conforming
producer records the container's SHA-256 digests verbatim on the profile surfaces
and never coerces them into GMEOW's BLAKE3 space; a consumer cross-checking a
surface against the opened container compares SHA-256 to SHA-256, and checks a
projection's `gmeow:projectionSourceDigest` against its source's BLAKE3
`gmeow:contentDigest` within GMEOW's own space.

## 4. Required and optional metadata

A conforming `gmeow:EmbeddingProjection` carries, as **required** content:

- `gmeow:hasVectorSpaceContract` — exactly one committed effective space
  (functional; enforced as an `owl:someValuesFrom` requiredness restriction).
- `gmeow:projectionSource` — exactly one content-addressed source information
  object (functional).
- `gmeow:hasSensitivity` — exactly one `gmeow:SensitivityLevel` (reused kernel).
- `gmeow:projectionSourceDigest` — the source byte digest recorded in the header.
- `gmeow:reproducibilityLevel` — one of the closed set `gmeow:reproducibleExact`,
  `gmeow:reproducibleWithinTolerance`, `gmeow:regenerableOnly`.

A conforming `gmeow:EmbeddingProjection` carries, as **conditionally required**
content:

- `gmeow:projectionSourceGtsHeadId` — required whenever the source carries a
  certified `gmeow:gtsHeadId`, and then equal to it (`gmeow:GtsHeadIdPropagationConstraint`).
- `gmeow:reproducibilityTolerance` — a `math:Quantity`, required exactly when
  `gmeow:reproducibilityLevel` is `gmeow:reproducibleWithinTolerance` (the
  conditional-requiredness axiom).

A conforming `gmeow:EmbeddingProjection` carries, as **optional** content:
`gmeow:hasTargetSet` (the ordered row namespace, functional when present),
`gmeow:aggregatesEmbedding` (the graphrag `gmeow:Embedding` rows it gathers),
`gmeow:bindsExternal` (each `gmeow:ExternalBinding` it references), and the
standard `gmeow:wasGeneratedBy` / `gmeow:wasDerivedFrom` provenance.

Each `gmeow:EmbeddingFamily` carries its full generation contract:
`gmeow:embeddingModel` (reused graphrag), `gmeow:inferenceEngine`,
`gmeow:executionContract` (precision mode / deterministic-inference settings),
`gmeow:tokenizerContract`, `gmeow:subjectProjectionContract` (the always-applied
text-serialization step), `gmeow:preprocessingContract`, `gmeow:chunkingContract`,
`gmeow:poolingKind`, `gmeow:generationNormalizationContract` (the family's
generation-time normalization, distinct from an effective space's per-prefix
`gmeow:normalizationKind`), `gmeow:truncationContract`, `gmeow:vectorDtype`,
`gmeow:vectorByteOrder`, `gmeow:quantizationContract`, and `gmeow:contentDigest`.
Two families that differ in any of these — including the execution,
subject-projection, and generation-time-normalization components — are materially
different generation contracts and MUST carry different `gmeow:contentDigest`
identities (competency test 2, enforced by `distinct-family-id.rq`).
Each `gmeow:VectorSpaceContract` carries `gmeow:effectiveOfFamily`,
`gmeow:embeddingDimensions` (reused), `gmeow:distanceMetric` (reused),
`gmeow:normalizationKind`, `gmeow:matryoshkaPolicy`, and `gmeow:contentDigest`.
Each `gmeow:VectorTarget` carries `gmeow:vectorTargetKind`, its exact identity,
optional `gmeow:targetParent`, optional `gmeow:targetByteStart` /
`gmeow:targetByteEnd`, and an optional non-identifying `gmeow:targetOrdinal`.

### 4.1 In-container, in-sidecar, or both

The same projection is described from two viewpoints, each modeled by a
`gmeow:ProfileSurface` distinguished by its `gmeow:profileSurfaceKind`:

- `gmeow:surfaceContainer` — the digests as the packaged `.purremb` container
  itself records them.
- `gmeow:surfaceManifest` — the digests as the accompanying sidecar manifest
  declares them.

Each surface records the three shared-referent digests that both the container
and the sidecar must agree on: `gmeow:recordsSourceDigest` (the source pack),
`gmeow:recordsModelContractDigest` (the vector-space / stored-matrix contract),
and `gmeow:recordsTargetTableDigest` (the target table). The source byte digest,
the certified head id, the family contract, the effective spaces, and the target
set live **in-container** (they are the container's own identity). The
container/manifest agreement digests live in **both** — that is the whole point
of the sidecar: it restates the container's shared referents so a consumer can
cross-check them without opening the binary. External payloads and their roles
live **outside** the container entirely, referenced by `gmeow:ExternalBinding`
(Principle 12).

## 5. Validation on disagreement is a HARD FAIL

There are five surfaces on which two descriptions of the same referent can
disagree: the container, the sidecar manifest, the source pack, the model
contract, and the target table. On every one of them, disagreement is a HARD
FAIL enforced by a shipped `logic:Constraint`, never a heuristic reconciliation
and never a silent preference for one operand:

- `gmeow:SourceDigestMatchConstraint` — the projection's recorded
  `gmeow:projectionSourceDigest` must equal the resolved source's
  `gmeow:contentDigest`. A mismatch means the projection was built over different
  bytes than the source it now resolves to; its provenance is invalid.
- `gmeow:GtsHeadIdPropagationConstraint` — when the source carries a certified
  `gmeow:gtsHeadId`, the projection must propagate the SAME id as
  `gmeow:projectionSourceGtsHeadId`. A certified source's committed history
  travels with its projection; it is never dropped or altered.
- `gmeow:ProfileSourceDigestAgreementConstraint` — the `gmeow:surfaceContainer`
  and `gmeow:surfaceManifest` surfaces of one projection must record the SAME
  `gmeow:recordsSourceDigest`.
- `gmeow:ProfileModelContractAgreementConstraint` — those two surfaces must
  record the SAME `gmeow:recordsModelContractDigest`.
- `gmeow:ProfileTargetTableAgreementConstraint` — those two surfaces must record
  the SAME `gmeow:recordsTargetTableDigest`.

Verification follows PurRDF's two modes and this profile's constraint battery in
lockstep. In **Exact** mode the source byte digest is the identity a projection
is checked against (`gmeow:SourceDigestMatchConstraint`); in **Certified** mode
the RDFC digest / committed head id is (`gmeow:GtsHeadIdPropagationConstraint`).
Source-local ordinals — `gmeow:targetOrdinal` — are verified acceleration hints
only: they let a consumer seek directly into the packed matrix, but they never
provide identity. `gmeow:TargetExactIdentityConstraint` makes this normative:
every `gmeow:VectorTarget` carries an exact identity — a `gmeow:targetsResource`
binding or its own `gmeow:contentDigest` — and a bare ordinal with neither is a
violation. A mismatch in any mode, and a disagreement on any of the five
surfaces, rejects the projection.

## 6. Deterministic generation

Identical logical input plus identical contract yields a byte-identical
projection identity. No wall-clock value, random seed, filesystem path, or
process-local value enters the content-addressed identity of a projection, a
family, an effective space, a target set, an external binding, or a derived
index — the `gmeow:contentDigest` of each folds over its logical content alone.
Build metadata that is legitimately run-specific (the building activity, its
agent, its timestamp) rides the existing `gmeow:wasGeneratedBy` provenance and
never enters the projection's `gmeow:contentDigest`.

Get-leg determinism — how faithfully re-running the generation reproduces the
same vectors — is a separate, declared axis: `gmeow:reproducibilityLevel` over
the closed set `gmeow:reproducibleExact` / `gmeow:reproducibleWithinTolerance`
(with a required `gmeow:reproducibilityTolerance` `math:Quantity`) /
`gmeow:regenerableOnly`. This is orthogonal to correspondence preservation: a
projection can be perfectly preserving yet only regenerable, or bit-exact yet
make no correspondence claim. The two are never conflated.

## 7. Multiple vector spaces over one source

One source pack legitimately supports many projections. A producer MAY declare:

- multiple `gmeow:EmbeddingFamily` individuals over one
  `gmeow:projectionSource` (different models, dtypes, or generation contracts —
  e.g. `ex:familyA` at 1024-d f32 and `ex:familyB` at 384-d int8);
- multiple `gmeow:VectorSpaceContract` effective spaces over one family,
  including Matryoshka leading prefixes via `gmeow:matryoshkaPolicy`
  (`gmeow:matryoshkaFixed` for the full family space, `gmeow:matryoshkaPrefix`
  for a declared leading prefix over the SAME stored matrix, without
  duplication);
- multiple `gmeow:TargetSet` namespaces over one source (a TEXT-family subject
  projection over `gmeow:vectorTargetChunk` rows and an RDF 1.2-family projection
  over `gmeow:vectorTargetStatement` rows are different target sets, hence
  different projections).

Because each effective space is identified by its own `gmeow:contentDigest`, a
proximity comparison between a full 1024-d `ex:vscA` vector and a 256-d
`ex:vscAprefix` vector is a **cross-space** comparison, decidable by digest
inequality. A `gmeow:SimilarityObservation` whose two projections' effective
spaces have distinct digests is well-formed ONLY when a justifying
`logic:Correspondence` exists whose `logic:sourceEndpoint` /
`logic:targetEndpoint` are exactly those two `gmeow:VectorSpaceContract`
individuals — enforced by `gmeow:CrossSpaceComparisonConstraint`. A within-space
observation (both operands over one effective space, equal digests) needs no
bridge; the guard's distinct-digest condition is false.

## 8. GTS referencing without joining source identity

A `.purremb` projection is a generated, lossy projection with its own
`gmeow:contentDigest`. It is referenced from GTS and generated catalogs through
`gmeow:gtsProfilePurrEmb` and the `"purremb"` slice profile — and referencing it
never folds it into the canonical identity of the source it projects. The source
pack's identity is its own `gmeow:contentDigest` and `gmeow:gtsHeadId`; the
projection merely **records** those (as `gmeow:projectionSourceDigest` and
`gmeow:projectionSourceGtsHeadId`) so its provenance is verifiable. The
projection is downstream, lossy, and rebuildable; the source is upstream, exact,
and canonical. The one-directional dependency is the disclosure firewall of
Section 9: the projection inherits the source's classification, never the other
way around.

## 9. Producer and consumer integration

A conforming implementation needs no private conventions. The worked scene in
[`../examples/purremb-bookshelf.ttl`](../examples/purremb-bookshelf.ttl) — Project
Lillith's bookshelf vectors — authors every node a producer and consumer touch.

### 9.1 Producer

To build the bookshelf projection, a producer:

1. Resolves the exact source pack (`ex:bookshelfPack`), reading its
   `gmeow:contentDigest` and, if certified, its `gmeow:gtsHeadId`, and its
   `gmeow:hasSensitivity` / `gmeow:hasDisclosurePolicy`.
2. Declares a `gmeow:EmbeddingFamily` (`ex:familyA`) with its full generation
   contract and content-addresses it; then declares its effective spaces — the
   full family space `ex:vscA` (`gmeow:matryoshkaFixed`, 1024-d) and, sharing the
   same stored matrix, the leading-prefix space `ex:vscAprefix`
   (`gmeow:matryoshkaPrefix`, 256-d) — each with its own `gmeow:contentDigest`
   via `gmeow:effectiveOfFamily`.
3. Builds the ordered `gmeow:TargetSet` (`ex:targetSetA`) of `gmeow:VectorTarget`
   rows, each with an exact identity (`ex:tgtDoc` via `gmeow:targetsResource`,
   `ex:tgtChunk` via its own `gmeow:contentDigest`, `ex:tgtStmt` via a reifier
   node), placing each in its subject-family hierarchy with `gmeow:targetParent`,
   and content-addresses the ordered membership.
4. Emits the `gmeow:EmbeddingProjection` (`ex:bookshelfProjA`): binds
   `gmeow:hasVectorSpaceContract` to `ex:vscA`, `gmeow:hasTargetSet` to
   `ex:targetSetA`, aggregates its graphrag `gmeow:Embedding` rows, records
   `gmeow:projectionSourceDigest` equal to `ex:bookshelfPack`'s digest, propagates
   `gmeow:projectionSourceGtsHeadId` equal to its head id, sets
   `gmeow:hasSensitivity` (inheriting the source's unless declassified),
   declares `gmeow:reproducibilityLevel` (with a tolerance when within-tolerance),
   and attributes `gmeow:wasGeneratedBy` / `gmeow:wasDerivedFrom`.
5. Emits the two agreeing `gmeow:ProfileSurface` views (`ex:containerSurfaceA`,
   `ex:manifestSurfaceA`), each recording the SAME source, model-contract, and
   target-table digests.
6. Optionally binds external content-addressed payloads with a
   `gmeow:ExternalBinding` (`ex:extChunkText`) and a caller-supplied
   `gmeow:externalRole`, and optionally builds derived accelerators
   (`ex:annIndexA`) with `gmeow:indexOfProjection`, `gmeow:indexOverSpace`, an
   explicit approximation profile (`gmeow:approximationRecall`,
   `gmeow:indexNondeterminism`, `gmeow:indexLossContract`), and a reused
   `gmeow:indexAlgorithm` / `gmeow:indexParameters`. A rebuild supersedes the
   prior index append-only rather than erasing it.

A producer that lowers a projection's `gmeow:hasSensitivity` below its source's
MUST mint an explicit `gmeow:DeclassificationAct` that `gmeow:declassifies` the
projection, attributed by the reviewing `gmeow:wasGeneratedBy`
(`gmeow:SecurityMonotonicityConstraint`). No act, no drop.

### 9.2 Consumer

A consumer determines what it may do — generate, store, search, export — from the
projection's disclosure controls, and from nothing else. `gmeow:hasSensitivity`
and `gmeow:hasDisclosurePolicy` are read **default-deny**: the absence of a
policy never implies public, and the absence of a `gmeow:DeclassificationAct`
means the projection sits at its source's classification. A consumer comparing
two projections whose effective spaces differ MUST resolve a justifying
`logic:Correspondence` before treating any `gmeow:similarityScore` as meaningful,
and MUST read every score as a `logic:Vague` graded claim under the observation's
`gmeow:withMetric`, never as equivalence or entailment. A consumer using a
`gmeow:DerivedVectorIndex` MUST treat it as a non-authoritative, rebuildable
accelerator over the authoritative projection, and MUST honour its declared
`gmeow:indexLossContract` and measured `gmeow:approximationRecall` when reasoning
about a retrieval's completeness.
