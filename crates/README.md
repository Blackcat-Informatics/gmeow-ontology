<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Rust Crate Source Map

The Rust workspace is the native implementation surface for GMEOW's RDF,
logic, validation, documentation, and pipeline paths. Use this map to decide
where source documentation belongs before opening a crate.

Every crate directory under `crates/` has a `README.md`, and every crate
manifest points Cargo at that README.

## Layers

The RDF 1.2 kernel itself is **not** in this workspace. It lives in the sibling
`purrdf` package and is consumed through one exact-pinned umbrella dependency
(`purrdf` in the root `[workspace.dependencies]`), which owns the data model,
codecs, canonicalization, SHACL, SPARQL, the C ABI, and its own wasm build.
Crates here compose over it; none re-implements it.

| Area | Crates | Purpose |
| --- | --- | --- |
| Foundation values | `gmeow-errors`, `gmeow-ns`, `gmeow-term-arena`, `gmeow-license` | The diagnostics substrate, the registered term namespaces, the hash-consed structured-term arena, and the license/axiom-copy policy classifier. |
| Ontology and validation engines | `gmeow-validate`, `gmeow-slicetest`, `gmeow-slice-quality`, `gmeow-slice-brief`, `gmeow-conformance` | The wasm-clean repo-free Tier-1 conformance core, slice-resident test-DSL execution, per-slice quality reports, authoring packets, and the logic-conformance harness. |
| Logic engines | `gmeow-logic-compile`, `gmeow-logic` | The pure wasm-able parse→IR→project compiler, and the world-indexed reasoning engine core. |
| Build and release | `gmeow-pipeline`, `gmeow-docs`, `gmeow-gts-profile`, `gmeow-foundation-corpus`, `docs-print`, `xtask` | The dogfooded build DAG, the typed documentation model, the single mandated GTS authorship entry, the corpus importer, the Typst/PDF renderer, and worktree-local workflow orchestration. |
| Command-line surfaces | `gmeow-cli`, `gmeow-cli-core`, `gmeow-dev-cli` | The shippable consumer `gmeow` command, the shared CLI foundation, and the repo-maintenance `gmeow-dev` command. |
| Browser engines (wasm32) | `gmeow-query-wasm`, `gmeow-validate-wasm`, `gmeow-reason-wasm`, `gmeow-gmn-wasm` | The four engines the documentation site ships: the SPARQL playground / bundle-explorer query engine, the repo-free validator, the structured-DL reasoner, and the GMN codec. |
| Domain and lifting | `gmeow-math`, `gmeow-math-lift`, `gmeow-affect`, `gmeow-affect-ingest`, `gmeow-music`, `gmeow-lang-bridge`, `gmeow-lang-form`, `gmeow-gmn-cost-matrix` | Exact-rational geometry, executable-math ingestion, affect-intensity geometry and classifier ingestion, the music toolchain, and the linguistic surfaces. |
| Developer tools and measurement | `gmeow-lsp`, `gmeow-bench-engines`, `gmeow-cost-measure` | Local editor diagnostics/SARIF, the engine benchmark harness, and the deterministic allocation-measurement allocator. |

## Documentation Hot Spots

Put high-level crate orientation in each crate's `README.md`, public API
contracts in `//!` module documentation, and non-obvious invariants next to the
code that enforces them.

The densest directories currently merit per-directory or module-level
orientation:

| Path | Why it matters |
| --- | --- |
| [`pipeline/src/stages/`](pipeline/src/stages/README.md) | Each file is a production build-DAG stage with source/output ownership rules. |
| [`logic/src/`](logic/src/README.md) | Several reasoning engines, result contracts, Python-facing seams, and certifiers share one crate. |
| [`validate/src/`](validate/src/README.md) | Validation lints mix PyO3 surfaces with PyO3-free engine modules. |
| [`logic-compile/src/projections/`](logic-compile/src/projections/README.md) | Projection targets encode explicit preservation and loss behavior. |

## Local Checks

Use Make targets from the repository root:

```bash
make rust-docs       # Build public Rust API docs; fail on broken/redundant links.
make rust-test       # Run nextest and doctests.
make crate-check     # Verify Rust crate layering and acyclic crate DAGs.
make wasm            # Build the wasm package lane.
```
