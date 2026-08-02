<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Pipeline

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/pipeline` · **tier: core**

The project's own build, authored as data. GMEOW is built by a directed acyclic graph of typed
**stages** that exchange an in-memory RDF dataset rather than re-parsing the GTS bundle from disk per
generator. This slice models that graph — `gmeow:Pipeline` and `gmeow:PipelineStage` — so the build is
a first-class ontological citizen: the `gmeow-pipeline` Rust executor reads these individuals back,
validates the graph, binds each stage to its Rust implementation, and runs it single-pass.

## The dogfooded build graph

`gmeow:pipeline-build` is the canonical build. Its spine flows:

```text
source-load → (statements, mappings) → reason → gts-compose → {docs-render + export leaves} → gts-sink
```

Each `gmeow:PipelineStage` is a `logic:ActionSchema` and declares:

| Property | Meaning |
|---|---|
| `gmeow:stageImpl` | The `STAGE_REGISTRY` key binding the stage to its Rust `Stage`. |
| `gmeow:dataflowConsumes` | The upstream stages whose products it reads (consumer → producer). |
| `gmeow:requiresResource` | The shared `gmeow:Resource`s it must hold exclusively while running (e.g. `gmeow:engineResource`); stages competing for one serialize. |
| `gmeow:hasCapability` | The `gmeow:StageCapability`s it holds — the executor reads these in place of a kind enum. |
| `gmeow:producesFormat` | Output-format tags (export leaves). |

Reified `gmeow:BuildDataFlow` edges (`gmeow:buildFlowFrom` / `gmeow:buildFlowTo` / `gmeow:flowEntity`)
narrow a consumer's dependency to the specific named graphs it reads, so the executor keys that
stage's cache on only those graphs' digests — artifact-level incremental rebuild.

## Stage capabilities

`gmeow:StageCapability` is the value vocabulary the executor reads instead of a kind enum:
`gmeow:sinkCapability` (the single serialization exit — the gts narrow waist; the loader HARD-fails
unless exactly one stage holds it) and `gmeow:sourceOrigin` (the authored-source loader, whose emitted
quads' provenance origin is `Source`). A stage holding no capability is a plain transform / validate /
export leaf — provenance-`Generated`, non-sink, parallel-eligible within its topological level.
Serialization treatment is not a kind either — it is a declared resource conflict. Two resources are
minted, for two different reasons. The reasoning stage `gmeow:requiresResource` `gmeow:engineResource`
because the process-wide reasoning state is shared mutable state. The two whole-dataset serialization
leaves (`gmeow:stage-export-export`, `gmeow:stage-export-yaml-ld`) `gmeow:requiresResource`
`gmeow:serializationBufferResource` because their PEAK RESIDENCY is exclusive: each turns the entire
carrier into in-memory documents (9.06 GiB and 8.37 GiB of measured peak allocation, an order of
magnitude above any other export leaf), so concurrently they add their peaks and exhaust a 16 GB build
host. Stages holding a resource serialize against every other stage holding it; every other leaf in
the level keeps full parallelism.

## Invariants (HARD-failed before any stage runs)

The loader proves, before scheduling: the DAG is **acyclic** and **complete** (no dangling
`gmeow:dataflowConsumes`); there is **exactly one** stage holding `gmeow:sinkCapability`; and every
bound stage's `capabilities` / `consumes` / `requiresResource` / typed dataflow **agree** with its RDF
declaration (single source of truth — RDF and Rust cannot disagree). None of these is optional or
repairable.

## Consumer

The `gmeow-pipeline` crate. The migrated `meta:gate-generator-registry` queries
`gmeow:pipeline-build`'s stages instead of the retired Python generator list, so the registry gate is
itself dogfooded against this slice.
