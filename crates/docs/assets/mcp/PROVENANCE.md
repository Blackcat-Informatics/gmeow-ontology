<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Vendored gmeow-mcp-wasm segment — the console's demand-loaded reasoner

These files are the **prebuilt** [`gmeow-mcp-wasm`](../../../mcp-wasm) image: the REASONING
segment of the consumer GMEOW MCP engine compiled to `wasm32-unknown-unknown`. Emitted
verbatim into the rendered site under `assets/mcp/`, where it is fetched ON FIRST USE of a
reasoning tool and never as part of the first load.

## A delta, not a superset

This image links the DL reasoner (`gmeow-logic`) and the rubric kernel over it
(`gmeow-slice-quality`) and NOTHING of the core tool surface. The two segments are DISJOINT
halves of one 38-tool surface: each serves its own tools and defers the other's back with
the typed `mcp.segment-not-loaded` signal, so no byte is paid twice. `tools/list` is
byte-identical across both — a deployment tier is not a reduced theory.

## Files

| File | Role |
|------|------|
| `index.mjs` | The ES-module wrapper: `ready()`/`init()`/`mcp()`, the `{ready, init, mcp}` lifecycle `tieredMcp()`'s `loadSegment` callback expects. |
| `pkg/gmeow_mcp_wasm.js` | wasm-bindgen `--target web` bindings. |
| `pkg/gmeow_mcp_wasm_bg.wasm` | The reasoning segment image. |
| `pkg/*.d.ts` | The TypeScript surface. |
| `WITNESS.mcp.json` | The native↔wasm attestation: a real `conjecture_test` frame answered by the segment, byte-identical to what the FULL native engine returns for the same frame. |
| `DIGESTS.blake3` | BLAKE3 content-digest manifest pinning the exact vendored bytes. |

## Why vendored (not built at regenerate time)

The synchronization pipeline (`make check-sync`) does not build wasm, so the bytes
are pinned build inputs refreshed by `make maint-refresh-mcp-asset`. That target
re-pins the digests after its Node parity lane passes.

Coordinated preparation uses `make maint-prepare-wasm-assets` to rebuild all six
images and record candidate digests and exact substrate identities. Those records
alone do not attest runtime parity. After current-bundle witness refresh, ordinary
PR CI must pass all six Node lanes and console acceptance on the committed outputs.
