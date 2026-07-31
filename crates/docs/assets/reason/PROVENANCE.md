<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Vendored gmeow-reason-wasm engine

These files are the **prebuilt** [`gmeow-reason-wasm`](../../../reason-wasm) engine —
the repo-free GMEOW structured-DL reasoner (the native `gmeow-reason` chase over authored
RDF, returning the reasoned closure — the inferred triples) compiled to
`wasm32-unknown-unknown` — pinned here so the generated documentation site can reason over
authored RDF entirely in the browser (no server, no network, no repository). They are
emitted verbatim into the rendered site under `assets/reason/` (a language-neutral path)
and constitute the reasoning runtime the docs controller loads.

## Files

| File | Role |
|------|------|
| `gmeow_reason_wasm.js` | wasm-bindgen `--target web` ES-module bindings (exposes `reason`, `version`). |
| `gmeow_reason_wasm_bg.wasm` | The compiled structured-DL reasoner — the native `gmeow-reason` chase, run serially on wasm. |
| `gmeow_reason_wasm.d.ts` / `gmeow_reason_wasm_bg.wasm.d.ts` | TypeScript type surface. |

Each carries a `.license` REUSE sidecar (AGPL-3.0-only).

## Why vendored (not built at regenerate time)

The regeneration pipeline is Rust/Python only — it does not invoke `cargo` or `wasm-bindgen`.
A browser-executable wasm engine cannot be produced during `make regen`, so it is pinned
here as a build **input** (like `crates/docs/assets/gmeow.css` and the purrdf engine).
Because it is a constant `include_bytes!` input, the rendered site stays byte-deterministic.

## Refreshing (after any change to `crates/reason-wasm` or `crates/logic`)

```sh
make maint-refresh-reason-asset
```

This rebuilds the wasm package (`make reason-wasm-pkg`: `cargo build … --target
wasm32-unknown-unknown --release`, then wasm-bindgen 0.2.125 `--target web`, then the
REQUIRED `wasm-opt -Oz`) and copies the four artifacts here, rewriting `DIGESTS.blake3`
under `GMEOW_REASON_BLESS=1`. The target now depends on `reason-wasm-pkg-test`, so it
cannot re-pin bytes that did not just pass the native↔wasm parity lane. It must be run
whenever the reasoner source changes; otherwise the vendored engine drifts from source.
Two gates guard against a stale/broken blob:

- a **Rust structural + digest test** (`crates/docs/tests/reason_asset.rs`, on
  `make check`) asserts the vendored `.wasm` is a real wasm module, the bindings still
  export the `reason` surface, and the pinned BLAKE3 digests match the exact bytes;
- a **Node native↔wasm parity witness** (`crates/reason-wasm/js/tests/witness.test.mjs`,
  on `make reason-wasm-pkg-test`, gate-enforced on every pull request via `wasm-parity`
  in the required CI `make heavy` lane) loads the
  built engine and asserts its reasoned closure is byte-identical to the native reasoner's.

## Size note

`wasm-opt -Oz` is a REQUIRED build step of `make reason-wasm-pkg` (a missing `wasm-opt`
is a hard build failure, never a note); the shipped `.wasm` is always `-Oz`-optimized.
