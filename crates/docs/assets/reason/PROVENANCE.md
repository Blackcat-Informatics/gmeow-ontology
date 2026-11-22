<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Vendored gmeow-reason-wasm engine

These files are the **prebuilt** [`gmeow-reason-wasm`](../../../reason-wasm) engine —
the repo-free GMEOW structured-DL reasoner (the native `gmeow-reason` chase over authored
RDF, returning a complete binary GTS artifact envelope with admitted verdict facets
and the full asserted and inferred RDF closure) compiled to
`wasm32-unknown-unknown` — pinned here so the generated documentation site can reason over
authored RDF entirely in the browser (no server, no network, no repository). They are
emitted verbatim into the rendered site under `assets/reason/` (a language-neutral path)
and constitute the reasoning runtime the docs controller loads.

## Files

| File | Role |
|------|------|
| `gmeow_reason_wasm.js` | wasm-bindgen `--target web` ES-module bindings (exposes `reason`, `conjecture`, `admit_artifact`, `version`). |
| `gmeow_reason_wasm_bg.wasm` | The compiled structured-DL reasoner — the native `gmeow-reason` chase, run serially on wasm. |
| `gmeow_reason_wasm.d.ts` / `gmeow_reason_wasm_bg.wasm.d.ts` | TypeScript type surface. |
| `WITNESS.reason.json` / `WITNESS.reason.gts` | The reasoner parity witness envelope and its GTS payload. |

Each carries a `.license` REUSE sidecar (AGPL-3.0-only).

## Why vendored (not built at regenerate time)

The Rust synchronization pipeline does not build wasm or invoke `wasm-bindgen`.
A browser-executable wasm engine cannot be produced during the synchronization pipeline (`make check-sync`), so it is pinned
here as a build **input** (like `crates/docs/assets/gmeow.css` and the query engine).
Because it is a constant `include_bytes!` input, the rendered site stays byte-deterministic.

## Refreshing (after any change to `crates/reason-wasm` or `crates/logic`)

```sh
make maint-refresh-reason-asset
```

This first runs the verified native producer for the closure witness pair
and `tests/WITNESS.conjecture.json`, then
rebuilds the wasm package (`make reason-wasm-pkg`) with a release build for
`wasm32-unknown-unknown`, the workspace-pinned wasm-bindgen with `--target web`, and
the required `wasm-opt -Oz`. It runs the complete Node native↔wasm witness lane before
copying the four package artifacts and both closure witness files here. The
asset producer pins the package and witness bytes in `DIGESTS.blake3`.
The recipe finishes the witness refresh
before its consumers run, including in parallel Make invocations. Refresh the asset
whenever the reasoner source changes to keep the vendored engine current.
Two gates guard against a stale/broken blob:

- a **Rust structural + digest test** (`crates/docs/tests/reason_asset.rs`, on
  `make check`) verifies the vendored WebAssembly module, the reasoning and artifact
  admission exports, and the exact bytes against the pinned BLAKE3 digests.
  It also checks both witness files against the copies used in parity tests.
- **Node native↔wasm parity witnesses** (`crates/reason-wasm/js/tests/witness.test.mjs`
  and `witness_conjecture.test.mjs`, on `make reason-wasm-pkg-test`, gate-enforced on
  every pull request via `wasm-parity` in the required CI `make heavy` lane) load the
  built engine and assert its complete reasoning and symmetric conjecture artifact responses are
  byte-identical to their verified native attestations.

Coordinated preparation uses `make maint-prepare-wasm-assets` to rebuild all six
images and record candidate digests and exact substrate identities. Those records
alone do not attest runtime parity. After current-bundle witness refresh, ordinary
PR CI must pass all six Node package lanes and shipped-console acceptance for the
final commit.

## Size note

`wasm-opt -Oz` is a REQUIRED build step of `make reason-wasm-pkg` (a missing `wasm-opt`
is a hard build failure, never a note); the shipped `.wasm` is always `-Oz`-optimized.
