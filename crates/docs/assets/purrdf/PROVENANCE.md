<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# Vendored purrdf browser engine

These files are the **published** [`@blackcatinformatics/purrdf`](https://www.npmjs.com/package/@blackcatinformatics/purrdf)
npm package, unpacked verbatim. purrdf is the sibling RDF-1.2 kernel — a separate
repository, `MIT OR Apache-2.0`, not part of this codebase — consumed here as a Rust
library (the workspace's `purrdf` dependency) and, in this directory, as its `wasm32`
RDF/JS build. They are emitted into the rendered site under `assets/purrdf/`, which is the
path a reader's page or an embedder's module imports.

`UPSTREAM.txt` records exactly which release these bytes are. It is not a pin: see
*Refreshing* below.

## Files

| File | Role | Emitted |
|------|------|---------|
| `index.mjs` | The package root: the RDF/JS surface (`ready`, `DataFactory`, `Dataset`, `QueryEngine`, the Stream/Sink primitives) over the wasm engine. | yes |
| `pkg/purrdf_wasm.js` | wasm-bindgen `--target web` ES-module glue. | yes |
| `pkg/purrdf_wasm_bg.wasm` | The compiled engine — RDF 1.2 parsing/serialization, SPARQL evaluation, SHACL. | yes |
| `index.d.ts` | The hand-written package type surface a TypeScript consumer of the root sees. | no |
| `pkg/purrdf_wasm.d.ts`, `pkg/purrdf_wasm_bg.wasm.d.ts` | The generated type surfaces. | no |
| `UPSTREAM.txt` | `<package>@<version>` — the release these bytes came from. | no |
| `DIGESTS.blake3` | BLAKE3 content-digest manifest over every file above. | no |

Each file that does not carry an inline SPDX header carries a `.license` REUSE sidecar, and
every one of them states **`MIT OR Apache-2.0`** — upstream purrdf's license. Vendoring
bytes does not relicense them, and a sidecar claiming this repository's AGPL over an
upstream package would be a licensing claim nobody made.

## What it is for, and what it is not

It is **not** a second protocol surface. Every interactive widget the documentation site and
the standalone console ship dispatches JSON-RPC to the GMEOW-owned MCP segments under
`assets/mcp-core/` and `assets/mcp/`; the SPARQL playground, the bundle explorer and the
term/slice `DESCRIBE` prefills all run `query_local` against the shipped bundle, and none of
them touch this engine. It backs no `Capability` and carries no native↔wasm witness
attestation, because it would be attesting nothing about gmeow's own engine.

What it adds is the surface a **consumer of the published tree** can import — an offline,
zero-server RDF-1.2 store with an RDF/JS API — to run SPARQL over *their own* dataset. That
is the one question `query_local` does not answer: its scopes are the shipped bundle and the
frame the caller hands it, not a standing dataset a page keeps and queries.

## Why vendored (not built at regenerate time)

The regeneration pipeline is Rust — it does not invoke `cargo`, `wasm-bindgen` or `npm`. A
browser-executable wasm engine cannot be produced during `make regen`, so it is pinned here
as a build **input** (like `crates/docs/assets/gmeow.css`) and included with
`include_bytes!`. Because it is a constant input, the rendered site stays byte-deterministic.

Nothing here is post-processed. In particular **no local `wasm-opt` step is applied**: the
published tarball is already optimized by purrdf's own CI, and re-optimizing on the way in
would make the committed bytes depend on whether `binaryen` happened to be installed on the
refresher's machine — the shipped artifact would differ between two refreshes of the same
upstream release.

## Refreshing

```sh
make maint-refresh-purrdf-asset
```

**Lower bound, always newest — there is no exact pin.** The lane resolves every published
version satisfying `>=$(PURRDF_NPM_MIN)` (declared in the `Makefile`), takes the newest,
`npm pack`s it, copies the six published files above into place, writes `UPSTREAM.txt`, and
re-pins `DIGESTS.blake3`. So a refresh always moves forward, and the floor only ever has to
be edited to *require* a new upstream capability — never to *permit* a new upstream release.
A published file the package stops shipping is a hard failure of the lane, named by path,
rather than a silently thinner vendored tree.

The floor starts at the npm version built from the same purrdf source tree the workspace's
`purrdf` Cargo dependency is pinned to, so the browser engine can never be older than the
native engine gmeow links.

Three gates guard the result, all on `make check`:

- `crates/docs/tests/purrdf_asset.rs` drives the shared vendored-wasm-asset harness
  (`gmeow_docs::vendored_asset`): the `.wasm` is a real WebAssembly module of plausible
  size, the glue / wrapper / both type surfaces declare **one** export set, and
  `DIGESTS.blake3` describes the exact on-disk bytes — so a stale-but-still-functional
  engine, or any hand-edited vendored file, fails. Integrity is a **content digest**, never
  a byte length;
- the same test proves `UPSTREAM.txt` satisfies the declared `PURRDF_NPM_MIN`, so the floor
  is a checked fact rather than a comment;
- `crates/docs/tests/refresh_targets_exist.rs` proves `make maint-refresh-purrdf-asset` is a
  real, maintainer-scoped, `make help`-listed target — the instruction every failure message
  above prints has to be one a reader can actually follow.
