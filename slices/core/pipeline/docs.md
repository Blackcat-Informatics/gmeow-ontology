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

Each `gmeow:PipelineStage` declares:

| Property | Meaning |
|---|---|
| `gmeow:stageKind` | One `gmeow:StageKind` — selects scheduling treatment. |
| `gmeow:stageImpl` | The `STAGE_REGISTRY` key binding the stage to its Rust `Stage`. |
| `gmeow:dataflowConsumes` | The upstream stages whose products it reads (consumer → producer). |
| `gmeow:producesFormat` | Output-format tags (export leaves). |
| `gmeow:carriesEngineLock` | **Derived**: true exactly when the kind is `gmeow:kindReason`. |

## Stage kinds

`gmeow:StageKind` is a closed value vocabulary: `kindSourceLoad`, `kindTransform`, `kindReason`,
`kindValidate`, `kindDocsRender`, `kindExportLeaf`, `kindSink`. Only `kindReason` carries the
process-wide engine lock (the Nemo/Scryer engines are not concurrency-safe); everything else is
parallel-eligible within its topological level. There is exactly one `kindSink` — the gts narrow
waist, the single serialization exit.

## Invariants (HARD-failed before any stage runs)

The loader proves, before scheduling: the DAG is **acyclic** and **complete** (no dangling
`gmeow:dataflowConsumes`); there is **exactly one Sink**; `gmeow:carriesEngineLock` **equals** the
kind-derived value (single source of truth — RDF and Rust cannot disagree); and every bound stage's
`kind` / `consumes` **agree** with its RDF declaration. None of these is optional or repairable.

## Consumer

The `gmeow-pipeline` crate. The migrated `meta:gate-generator-registry` queries
`gmeow:pipeline-build`'s stages instead of the retired Python generator list, so the registry gate is
itself dogfooded against this slice.
