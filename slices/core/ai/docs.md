<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The AI Claim Module — LLM output as claim-not-truth

An LLM output is a **claim**: emitted by a `gmeow:ModelInvocation`, attributed to
the model agent, confidence-weighted, grounded (or not) in pinned evidence, and —
when models or sources conflict — held `gmeow:accordingTo` a standpoint, never
adjudicated by fiat. This slice names the whole RAG/GraphRAG pipeline as one
provenance graph, reusing the provenance, standpoint, evidence, and statement
layers (#54; CONSTITUTION P9, P14).

## The spine

```text
Corpus ─corpusMember→ source ─(chunkOf)─ Chunk ─(spanOfChunk)─ EvidenceSpan
                                                   │ groundedIn (supportPolarity)
RetrievalEvent → ModelInvocation ──wasGeneratedBy── Claim ⟵ roles: GeneratedClaim /
   (prompt, params, model)        confidence / accordingTo /        ExtractedClaim /
                                  validFrom-validUntil (RDF 1.2)    MemoryItem
                                                   │
                              Contradiction ─contradictsClaim (≥2, surfaced never ranked)
```

- **A claim with no `groundedIn` span is a flagged hallucination** — retained and
  flagged (P10), never deleted. Faithfulness ≡ AIS coverage over those spans.
- **Evaluation is meta-claims**: a `gmeow:MetricObservation` scores a subject under
  a `gmeow:EvaluationMetric` (RAGAS/AIS/RAGTruth family), attributed to its
  `gmeow:EvaluationRun` — contestable like every other claim.
- **GraphRAG derivations are auditable**: `ExtractedEntity`/`ExtractedRelationship`
  are *descriptions* (promotion to first-class entities is a separate curation
  act, by reference — P5); `CommunitySummary` is `wasDerivedFrom` its members.
- **Memory is claims, not vectors**: `gmeow:MemoryItem` is a claim in its memory
  role; revision is supersession + `displayable false` (P10). This is the
  substrate under the `gmeow` client and the MCP memory triad.
- **Vectors stay outside the graph** (`gmeow:vectorRef`, P12); the graph carries
  the provenance that makes them auditable (model, dimensions, metric, index
  build).

Statement-level epistemics (confidence, standpoint, the clocks, retrieval scores)
ride RDF 1.2 statement annotations, keeping the OWL downcast DL-clean (P2–P3).
The worked end-to-end pattern — fixture, extraction prompt, SHACL, audit queries,
projections — is #55's cookbook (`docs/hallucination-resistant-kg.md`).
