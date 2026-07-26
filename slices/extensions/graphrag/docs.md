<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The GraphRAG extension — the pipeline as auditable provenance

GraphRAG systems derive an entity knowledge graph and pre-generated community
summaries from a corpus — and throw the provenance away (arXiv:2404.16130).
This extension keeps it: every artifact content-addressed (the existing
`gmeow:contentDigest`), every step an attributed activity (the existing
`gmeow:wasGeneratedBy` / `gmeow:wasDerivedFrom`), every score a statement-level
annotation that coexists with its rivals (P9, P3).

Consumer: **Project Lillith** (manifest, P15).

## The pipeline

```text
Corpus ─corpusMember→ sources ─(core chunkOf)─ Chunk
   │                                             │ (core ai) embeddingOf⁻¹
   │ indexesCorpus⁻¹               (core ai) Embedding (model, dims, metric,
(core ai) VectorIndex (algorithm,     vectorRef → outside the graph, P12)
   params, wasGeneratedBy build run)
   │ (core ai) againstIndex⁻¹
(core ai) RetrievalEvent (forQuery; retrievedChunk + retrievalScore annotations)
   │ feeds (core) ModelInvocation
ExtractedEntity / ExtractedRelationship  — descriptions, wasDerivedFrom chunks
   │ communityMember⁻¹
Community (level) ─summarizesCommunity⁻¹─ CommunitySummary ⊑ Summary
```

The vector/embedding/retrieval primitives (`Embedding`, `VectorIndex`,
`DistanceMetric`, `RetrievalEvent`, and their properties) are core-tier
infrastructure owned by the **ai** slice — any embedding consumer needs them,
not only GraphRAG (see extensions/embedding-projection). This extension reuses
them via its existing `gmeow:sliceDependsOn ai` edge and adds the
GraphRAG-specific corpus/index-wiring and entity-graph machinery around them.

## Doctrine

- **Descriptions, not entities.** `ExtractedEntity` is an information object
  ABOUT a putative entity; promotion to a first-class `gmeow:Entity` is a
  separate, attributable curation act with coreference by reference, never
  `owl:sameAs` (P5).
- **Vectors stay outside.** `vectorRef` points; the graph holds the audit trail
  (model, dimensions, metric, build parameters), not the floats (P12).
- **Claims are the core's business.** What the pipeline EXTRACTS as claims is
  the core ai slice's StandpointClaim-with-model-vantage pattern; this
  extension carries the machinery around those claims.
- **Memory recall is a RetrievalEvent** — the read half of the core MemoryItem
  doctrine.

## Terms

### gmeow:Corpus · gmeow:corpusMember

An indexed collection of source information objects over which retrieval operates —
the working document set, distinct from the documents slice's bibliographic
Collection. `corpusMember` (⊑ `hasPart`) relates a corpus to a source it collects;
non-functional, since a source may belong to many corpora.

### gmeow:Embedding · gmeow:embeddingOf · gmeow:embeddingModel · gmeow:embeddingDimensions · gmeow:DistanceMetric · gmeow:distanceMetric (core ai slice)

A vector representation of an information object (usually a core Chunk), the
open distance-metric vocabulary it is compared under, and their properties are
owned by the **core ai slice** (see slices/core/ai/docs.md) — shared
vector/embedding primitives, not GraphRAG-specific. `vectorRef` (owned here)
points to the vector payload, which stays OUTSIDE the graph (P12) — the graph
holds the audit trail, not the floats.

### gmeow:VectorIndex (core ai slice) · gmeow:indexesCorpus · gmeow:IndexAlgorithm · gmeow:indexAlgorithm · gmeow:indexParameters

`VectorIndex` — a built retrieval structure over a corpus's embeddings, the
artifact a RetrievalEvent queries — is owned by the core ai slice; this
extension's `indexesCorpus` ties it to the corpus served (non-functional, for
federated indexes), and `indexAlgorithm` (functional) carries its ANN structure
from the open `IndexAlgorithm` vocabulary (HNSW, IVF, flat) owned here, with
`indexParameters` recording the build parameters verbatim as a JSON string for
reproducibility.

### gmeow:RetrievalEvent (core ai slice) · gmeow:forQuery · gmeow:againstIndex (core ai slice) · gmeow:retrievedChunk · gmeow:retrievalScore

`RetrievalEvent` — one retrieval against a vector index, the answer to 'why did
the model see this passage?' — and its `againstIndex` edge are owned by the
core ai slice (an agent-memory recall is a RetrievalEvent too). This
extension's `forQuery` records the query verbatim; `retrievedChunk` each chunk
returned, with per-chunk relevance riding the `retrievalScore` statement
annotation so competing re-ranker scores coexist attributed (P9, P3).

### gmeow:ExtractedEntity · gmeow:ExtractedRelationship · gmeow:relationshipSource · gmeow:relationshipTarget

A model-extracted entity DESCRIPTION (deliberately not the entity itself — promotion
is a separate, attributable curation act with coreference by reference, never
`owl:sameAs`, P5), and the extracted relationship between two such descriptions.
`relationshipSource` and `relationshipTarget` (both functional) carry the edge's tail
and head.

### gmeow:Community · gmeow:communityMember · gmeow:communityLevel

A graph-clustering community (Leiden or similar) over extracted-entity descriptions,
at a `communityLevel` of the cluster hierarchy (0 = leaf). `communityMember`
(⊑ `hasPart`) names a clustered description; the clustering run is provenance via the
existing `wasGeneratedBy`.

### gmeow:CommunitySummary · gmeow:summarizesCommunity

A pre-generated summary of a community — GraphRAG's global-question substrate,
derived from the community's members and revisable rather than a black box.
`summarizesCommunity` (functional) names the community condensed.
