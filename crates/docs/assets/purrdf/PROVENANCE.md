<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Vendored purrdf wasm engine

These files are the **prebuilt** [purrdf](../../../rdf-wasm) wasm engine, pinned here so
the generated documentation site can run the **offline SPARQL playground** entirely in the
browser — no server, no network. They are emitted verbatim into the rendered site under
`assets/purrdf/` (a language-neutral path) and constitute the query runtime the playground
controller loads.

## Files

| File | Role |
|------|------|
| `gmeow_rdf_wasm.js` | wasm-bindgen `--target web` ES-module bindings (exposes `Dataset`, `Dataset.query`, `DataFactory`, …). |
| `gmeow_rdf_wasm_bg.wasm` | The compiled engine (the native, oxigraph-free RDF-1.2 + SPARQL evaluator). |
| `gmeow_rdf_wasm.d.ts` / `gmeow_rdf_wasm_bg.wasm.d.ts` | TypeScript type surface. |

Each carries a `.license` REUSE sidecar (AGPL-3.0-only).

## Why vendored (not built at regenerate time)

The regeneration pipeline is Rust/Python only — it does not invoke `cargo` or `wasm-bindgen`.
A browser-executable wasm engine cannot be produced during `make sync`, so it is pinned
here as a build **input** (like `crates/docs/assets/gmeow.css`). Because it is a constant
`include_bytes!` input, the rendered site stays byte-deterministic.

## Refreshing (after any change to `crates/rdf-wasm`)

```sh
make maint-refresh-purrdf-asset
```

This rebuilds the wasm package (`make wasm-pkg`) and copies the four artifacts here. It must
be run whenever the purrdf source changes; otherwise the vendored engine drifts from source.
Two gates guard against a stale/broken blob:

- a **Rust structural test** (`crates/docs/tests/purrdf_asset.rs`, on `make check`) asserts the
  vendored `.wasm` is a real wasm module and the bindings still export the `query` surface;
- a **Node execution test** (`crates/rdf-wasm/js/tests/vendored_asset.test.mjs`, on
  `make wasm-pkg-test`) loads the *vendored* engine and runs a real SPARQL query round-trip.

## Size note

`wasm-opt` is not required to build the package; when it is absent the shipped `.wasm` is
unoptimized (roughly 2× the `-Oz` size). `maint-refresh-purrdf-asset` applies `wasm-opt -Oz`
automatically when the tool is present.
