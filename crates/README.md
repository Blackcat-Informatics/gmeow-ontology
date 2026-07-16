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

| Area | Crates | Purpose |
| --- | --- | --- |
| Foundation values | `gmeow-iri`, `gmeow-xsd`, `gmeow-rdf-events`, `gmeow-sparql-algebra`, `gmeow-sparql-results` | Small reusable value types, event streams, parsers, and result encoders. |
| RDF kernel and adapters | `gmeow-rdf-core`, `gmeow-rdf`, `gmeow-rdf-capi`, `gmeow-rdf-wasm` | The native RDF 1.2 data model, codecs, loss ledgers, C ABI, and wasm packaging. |
| Ontology and validation engines | `gmeow-slice`, `gmeow-slicetest`, `gmeow-shacl`, `gmeow-validate`, `gmeow-diagnostics` | Slice discovery, slice-local test execution, SHACL, validation lints, and diagnostic rendering. |
| Logic engines | `gmeow-logic-compile`, `gmeow-logic`, `gmeow-conformance`, `gmeow-sparql-eval`, `gmeow-sparql-conformance` | Logic IR, projections, reasoning, conformance suites, and native SPARQL evaluation. |
| Build and release | `gmeow-pipeline`, `gmeow-docs`, `gmeow-native`, `gmeow-foundation-corpus` | The dogfooded build DAG, ontology docs model/renderers, unified PyO3 module, and foundation corpus bridge. |
| User tools | `gmeow-lsp`, `gmeow-music` | Local editor diagnostics/SARIF and experimental domain tools. |

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
| [`rdf-core/src/ir/`](rdf-core/src/ir/README.md) | The frozen RDF 1.2 IR is the data kernel other crates depend on. |
| [`logic-compile/src/projections/`](logic-compile/src/projections/README.md) | Projection targets encode explicit preservation and loss behavior. |

## Local Checks

Use Make targets from the repository root:

```bash
make rust-docs       # Build public Rust API docs; fail on broken/redundant links.
make rust-test       # Run nextest and doctests.
make crate-check     # Verify Rust crate layering and acyclic crate DAGs.
make rdf-core-hygiene # Prove the RDF core leaves do not regain oxigraph-family dependencies.
make wasm            # Build the wasm package lane.
```
