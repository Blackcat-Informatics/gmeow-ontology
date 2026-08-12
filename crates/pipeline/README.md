<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-pipeline

The DAG-driven, single-pass build executor for GMEOW.

The build is a directed acyclic graph of typed **stages** that exchange an
in-memory RDF dataset / bundle instead of re-parsing `generated/dist/gmeow.gts`
from disk per generator. The graph itself is **dogfooded** as `gmeow:Pipeline` /
`gmeow:PipelineStage` individuals in `slices/core/pipeline/` and read back by the
loader — the build is a first-class ontological citizen.

## Module map

| Module | Responsibility |
| --- | --- |
| `node` | The `Stage` trait, the resource/capability IRIs, the in-memory product/input/output handles. |
| `graph` | Acyclicity (`tarjan_scc`) + deterministic topological levelling (producers first). |
| `loader` | Parse the dogfooded DAG (`gmeow:` individuals), validate it, bind stages to impls. |
| `registry` | The `STAGE_REGISTRY`: `gmeow:stageImpl` → Rust `Stage`. |
| `cache` | Content-addressed, self-verifying per-stage cache (P2). |
| `scheduler` | Level-parallel execution + per-resource serialization (the reasoner's engine resource) (P2). |
| `provenance` | Per-stage `OriginKind` / `UnitId` quad stamping (P2). |
| `stages` | The concrete production stages (P3–P5). |
| `py` | The PyO3 `run_pipeline` surface (P6, `python` feature). |

## Invariants (proven before any stage runs — no-optionality)

- The DAG is **acyclic** and **complete** (no dangling `dataflowConsumes`).
- There is **exactly one** stage holding `gmeow:sinkCapability` — the gts narrow
  waist (one canonical exit).
- Every bound stage's `capabilities` / `consumes` / `requiresResource` / typed
  dataflow **agree** with its RDF declaration: RDF and Rust cannot disagree
  (single source of truth).

## Layering

`gmeow-pipeline` sits at the top of the engine DAG: it may depend on the engine
crates and on the `purrdf` substrate, and is depended on **only** by the two
command surfaces, `gmeow-cli` and `gmeow-dev-cli`. It must never introduce a
cycle (`make crate-check`).
