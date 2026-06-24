<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-pipeline

The DAG-driven, single-pass build executor for GMEOW (#861).

The build is a directed acyclic graph of typed **stages** that exchange an
in-memory RDF dataset / bundle instead of re-parsing `generated/dist/gmeow.gts`
from disk per generator. The graph itself is **dogfooded** as `gmeow:Pipeline` /
`gmeow:PipelineStage` individuals in `slices/core/pipeline/` and read back by the
loader — the build is a first-class ontological citizen.

## Module map

| Module | Responsibility |
| --- | --- |
| `node` | The `Stage` trait, the `StageKind` taxonomy, the in-memory product/input/output handles. |
| `graph` | Acyclicity (`tarjan_scc`) + deterministic topological levelling (producers first). |
| `loader` | Parse the dogfooded DAG (`gmeow:` individuals), validate it, bind stages to impls. |
| `registry` | The `STAGE_REGISTRY`: `gmeow:stageImpl` → Rust `Stage`. |
| `cache` | Content-addressed, self-verifying per-stage cache (P2). |
| `scheduler` | Level-parallel execution + the `Reason` engine lock (P2). |
| `provenance` | Per-stage `OriginKind` / `UnitId` quad stamping (P2). |
| `stages` | The concrete production stages (P3–P5). |
| `py` | The PyO3 `run_pipeline` surface (P6, `python` feature). |

## Invariants (proven before any stage runs — no-optionality)

- The DAG is **acyclic** and **complete** (no dangling `dataflowConsumes`).
- There is **exactly one `Sink`** — the gts narrow waist (one canonical exit).
- `gmeow:carriesEngineLock` **equals** the kind-derived value (`kind is Reason`):
  RDF and Rust cannot disagree (single source of truth).
- Every bound stage's `kind` / `consumes` **agree** with its RDF declaration.

## Layering

`gmeow-pipeline` sits at the top of the engine DAG: it may depend on the kernel
and engine crates, and is depended on **only** by `crates/native`. It must never
introduce a cycle, and it keeps the `gmeow-rdf-core` kernel pure
(`make crate-check`).
