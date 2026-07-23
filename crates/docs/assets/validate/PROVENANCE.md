<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Vendored gmeow-validate-wasm engine

These files are the **prebuilt** [`gmeow-validate-wasm`](../../../validate-wasm) engine —
the repo-free Tier-1 GMEOW validator (SHACL + OntoUML disciplines over a `gmeow.gts`
bundle) compiled to `wasm32-unknown-unknown` — pinned here so the generated documentation
site can validate authored RDF entirely in the browser (no server, no network, no
repository). They are emitted verbatim into the rendered site under `assets/validate/`
(a language-neutral path) and constitute the validation runtime the docs controller loads.

## Files

| File | Role |
|------|------|
| `gmeow_validate_wasm.js` | wasm-bindgen `--target web` ES-module bindings (exposes `validate`, `version`). |
| `gmeow_validate_wasm_bg.wasm` | The compiled Tier-1 validator (the native `gmeow-validate` core, reasoner-free). |
| `gmeow_validate_wasm.d.ts` / `gmeow_validate_wasm_bg.wasm.d.ts` | TypeScript type surface. |

Each carries a `.license` REUSE sidecar (AGPL-3.0-only).

## Why vendored (not built at regenerate time)

The regeneration pipeline is Rust/Python only — it does not invoke `cargo` or `wasm-bindgen`.
A browser-executable wasm engine cannot be produced during `make sync`, so it is pinned
here as a build **input** (like `crates/docs/assets/gmeow.css` and the purrdf engine).
Because it is a constant `include_bytes!` input, the rendered site stays byte-deterministic.

## Refreshing (after any change to `crates/validate-wasm` or `crates/validate`)

```sh
make maint-refresh-validate-asset
```

This rebuilds the wasm package (`make validate-wasm-pkg`: `cargo build … --target
wasm32-unknown-unknown --release`, then wasm-bindgen 0.2.125 `--target web`, then the
REQUIRED `wasm-opt -Oz`) and copies the four artifacts here, rewriting `DIGESTS.blake3`
under `GMEOW_VALIDATE_BLESS=1`. It must be run whenever the validator source changes;
otherwise the vendored engine drifts from source. Two gates guard against a stale/broken
blob:

- a **Rust structural + digest test** (`crates/docs/tests/validate_asset.rs`, on
  `make check`) asserts the vendored `.wasm` is a real wasm module, the bindings still
  export the `validate` surface, and the pinned BLAKE3 digests match the exact bytes;
- a **Node execution test** (`crates/validate-wasm/js/tests/validate.test.mjs`, on
  `make validate-wasm-pkg-test`) loads the built engine and runs a real validation
  round-trip against the committed `gmeow.gts`.

## Size note

`wasm-opt -Oz` is a REQUIRED build step of `make validate-wasm-pkg` (a missing `wasm-opt`
is a hard build failure, never a note); the shipped `.wasm` is always `-Oz`-optimized.
