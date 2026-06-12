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
   │                                             │ embeddingOf⁻¹
   │ indexesCorpus⁻¹                          Embedding (model, dims, metric,
VectorIndex (algorithm, params,                vectorRef → outside the graph, P12)
   wasGeneratedBy build run)
   │ againstIndex⁻¹
RetrievalEvent (forQuery; retrievedChunk + retrievalScore annotations)
   │ feeds (core) ModelInvocation
ExtractedEntity / ExtractedRelationship  — descriptions, wasDerivedFrom chunks
   │ communityMember⁻¹
Community (level) ─summarizesCommunity⁻¹─ CommunitySummary ⊑ Summary
```

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
