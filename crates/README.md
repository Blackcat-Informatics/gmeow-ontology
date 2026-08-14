<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Rust Crate Source Map

The Rust workspace is the native implementation surface for GMEOW's ontology,
logic, validation, documentation, and pipeline paths. Use this map to decide
where source documentation belongs before opening a crate.

Directory names are short; package names are not. `crates/ns` publishes
`gmeow-ns`, `crates/mcp` publishes `gmeow-mcp`, and so on. The authority for a
package name is always the `name` key of that directory's `Cargo.toml` — this
map names packages, and a committed test keeps it honest.

Put high-level crate orientation in each crate's `README.md` where one exists,
public API contracts in `//!` module documentation, and non-obvious invariants
next to the code that enforces them.

## Layers

The RDF 1.2 kernel itself is **not** in this workspace. It lives in the sibling
`purrdf` package and is consumed through one exact-pinned umbrella dependency
(`purrdf` in the root `[workspace.dependencies]`), which owns the data model,
codecs, canonicalization, SHACL, SPARQL, the C ABI, and its own wasm build.
Crates here compose over it; none re-implements it.

| Area | Crates | Purpose |
| --- | --- | --- |
| Foundation values | `gmeow-ns`, `gmeow-errors`, `gmeow-license`, `gmeow-math`, `gmeow-term-arena` | Registered term namespaces, the diagnostics/error substrate, the vendoring-policy classifier, exact-rational geometry, and the one hash-consed, binder-aware, content-addressed structured-term arena — carrying no reasoning runtime, so any front-end can intern a term without linking the engine. |
| RDF transport and bundle access | `gmeow-transcode`, `gmeow-bundle-view` | Parse/serialize any RDF 1.2 codec through the frozen dataset IR with realized-loss accounting, and read a materialized `gmeow.gts` bundle without the build executor. |
| Logic engines | `gmeow-logic-compile`, `gmeow-logic`, `gmeow-conformance` | The pure parse→IR→project compiler, the world-indexed reasoning core, and the native conformance harness that gates both. |
| Validation and slice authoring | `gmeow-validate`, `gmeow-slicetest`, `gmeow-slice-quality`, `gmeow-slice-brief` | Tier-1 conformance plus the authoring dev gate, the slice-resident test-DSL harness, the per-slice quality rubric, and authoring-packet assembly. |
| Build and release | `gmeow-pipeline`, `xtask` | The dogfooded single-pass build DAG over an in-memory RDF dataflow, and the worktree-local orchestration of the developer gate. |
| Documentation | `gmeow-docs`, `gmeow-docs-model`, `gmeow-docs-catalog`, `docs-print` | The typed documentation model over the slice catalog, its renderer-free core, the distribution matrix and concept lattice read from the bundle, and the deterministic Typst/PDF projection. |
| Command surfaces and services | `gmeow-cli-core`, `gmeow-cli`, `gmeow-dev-cli`, `gmeow-lsp`, `gmeow-mcp`, `gmeow-mcp-dev` | Shared console/reporter/exit-code foundation, the consumer command surface, the repo-maintenance command surface, editor diagnostics and SARIF, the bundle-only MCP engine, and the repo-reading MCP dev tools. |
| wasm packaging | `gmeow-validate-wasm`, `gmeow-reason-wasm`, `gmeow-gmn-wasm`, `gmeow-mcp-core-wasm`, `gmeow-mcp-wasm` | wasm32 bindings for the repo-free validator, the reasoner, the GMN codec, the lean MCP core image, and its demand-loaded reasoning segment. |
| Domain corpora and modeling | `gmeow-affect`, `gmeow-affect-ingest`, `gmeow-foundation-corpus`, `gmeow-lang-bridge`, `gmeow-lang-form`, `gmeow-math-lift`, `gmeow-music` | Affect-intensity geometry and attributed classifier ingestion, the narrative foundation corpus importer, the linguistic bridge and form AST, the file-reading `math:` ingestion front-ends that lift R scripts, ONNX graphs, and TSTP derivations into the shipped `math:` codomain (kept out of the pure in-bundle producers), and the music-package toolchain. |
| Off-gate measurement | `gmeow-bench-engines`, `gmeow-cost-measure`, `gmeow-gmn-cost-matrix` | Engine-vs-reference benchmarking, the harness-scoped counting allocator, and the tokenizer cost matrix. Leaf crates — nothing ships depends on them. |

## Documentation Hot Spots

The densest directories currently merit per-directory or module-level
orientation:

| Path | Why it matters |
| --- | --- |
| [`pipeline/src/stages/`](pipeline/src/stages/README.md) | Each file is a production build-DAG stage with source/output ownership rules. |
| [`logic/src/`](logic/src/README.md) | Several reasoning engines, result contracts, and certifiers share one crate. |
| [`validate/src/`](validate/src/README.md) | Validation lints mix the wasm-clean repo-free core with repo-reading dev-gate modules. |
| [`logic-compile/src/projections/`](logic-compile/src/projections/README.md) | Projection targets encode explicit preservation and loss behavior. |

## Local Checks

Use Make targets from the repository root:

```bash
make rust-docs       # Build public Rust API docs; fail on broken/redundant links.
make rust-test       # Run nextest and doctests.
make crate-check     # Verify Rust crate layering and acyclic crate DAGs.
make wasm            # Build the wasm package lane.
```
