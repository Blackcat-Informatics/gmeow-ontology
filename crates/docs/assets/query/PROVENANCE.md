<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Committed gmeow-query-wasm engine

These files are the prebuilt [`gmeow-query-wasm`](../../../query-wasm) engine — the
RDF 1.2 / SPARQL engine behind the offline documentation query playground and bundle
explorer, compiled to `wasm32-unknown-unknown` — pinned here so the generated site can
parse, serialize and query the shipped `gmeow.gts` bundle entirely in the browser (no
server, no network, no repository). They are emitted verbatim into the rendered site
under `assets/query/` (a language-neutral path) and constitute the query runtime the
docs controller loads.

## Files

| File | Role |
|------|------|
| `gmeow_query_wasm.js` | wasm-bindgen `--target web` ES-module bindings (exposes `Dataset`, `Dataset.query`, `Dataset.fromGts`, `version`). |
| `gmeow_query_wasm_bg.wasm` | The compiled engine — the pinned `purrdf` RDF 1.2 parser, serializer and SPARQL evaluator. |
| `gmeow_query_wasm.d.ts` / `gmeow_query_wasm_bg.wasm.d.ts` | TypeScript type surface. |
| `WITNESS.describe.nt` | The bundle-explorer `describe` attestation (native, bundle-scoped). |

Each carries a `.license` REUSE sidecar (AGPL-3.0-only).

## Built here, not vendored from elsewhere

This engine is compiled **in this repository** from the workspace `purrdf` pin. That is
a deliberate correction: the playground engine used to be a prebuilt blob copied from
the sibling `purrdf` repository, pinned only by BLAKE3 of its bytes — no version, no
revision — with a refresh target that **did not exist** and a provenance file pointing
at a path this repository does not contain. It could not be rebuilt here, and nothing
detected it drifting from the `purrdf` the workspace actually pins.

## Why pinned (not built at regenerate time)

The regeneration pipeline is Rust-only — it does not invoke `cargo` or `wasm-bindgen`.
A browser-executable wasm engine cannot be produced during the synchronization pipeline
(`make check-sync`), so it is pinned here as a build **input** (like
`crates/docs/assets/gmeow.css`). Because it is a constant `include_bytes!` input, the
rendered site stays byte-deterministic.

## Refreshing

```sh
make maint-refresh-query-asset
```

This rebuilds the wasm package (`make query-wasm-pkg`: release `wasm32` build,
`wasm-bindgen --target web`, then a **required** `wasm-opt -Oz`), runs the Node parity
lane, copies the four artifacts here, and re-pins `DIGESTS.blake3`. It depends on
`query-wasm-pkg-test`, so bytes that never passed parity can never be pinned.

Two gates guard against a stale or broken blob:

- a **Rust structural + digest test** (`crates/docs/tests/query_asset.rs`, on
  `make check`) asserts the committed `.wasm` is a real wasm module, the bindings still
  export the `query` surface, and the pinned digests match;
- a **Node execution test** (`crates/query-wasm/js/tests/witness.test.mjs`, on
  `make wasm-parity`) loads the *shipped* engine and runs the committed query corpus,
  byte-comparing every result against the native attestation.
