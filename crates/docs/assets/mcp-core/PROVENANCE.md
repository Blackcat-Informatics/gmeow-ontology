<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Vendored gmeow-mcp-core-wasm segment — the console's first-load engine

These files are the **prebuilt** [`gmeow-mcp-core-wasm`](../../../mcp-core-wasm) image: the
LEAN core of the consumer GMEOW MCP engine compiled to `wasm32-unknown-unknown`, plus its
`index.mjs` wrapper carrying the tiered dispatcher. Emitted verbatim into the rendered site
under `assets/mcp-core/`, where it is the ONLY engine the site loads eagerly.

## Why one engine and not four

The site used to vendor four separate engines — the purrdf runtime plus a bespoke
`#[wasm_bindgen]` shim each for validation, reasoning and the GMN codec — every one with its
own export surface, its own boot ritual, and its own controller code path. All four have
been retired in favour of ONE protocol: every widget now speaks JSON-RPC to the MCP surface,
so the docs controller drives the same 37-tool engine an agent does. A capability the
console has is a capability an agent has, by construction rather than by parallel
maintenance.

purrdf was the last to go, and the only one kept back on a capability argument rather than
inertia: the playground and the explorer were said to need a STANDALONE query over a
caller-supplied graph. They do not — both query the SHIPPED ontology, which this segment is
booted over, and `query_local` with `scope: "bundle"` answers every result form they ask
for. The describe property its `WITNESS.describe.nt` attested is still proven, against this
engine, by `crates/mcp/tests/witness_explore.rs`.

## Files

| File | Role |
|------|------|
| `index.mjs` | The ES-module wrapper: `ready()`/`init()`/`mcp()` plus `tieredMcp()`, the demand-loader that turns `mcp.segment-not-loaded` into "fetch the reasoning segment and replay this frame". |
| `pkg/gmeow_mcp_core_wasm.js` | wasm-bindgen `--target web` bindings. |
| `pkg/gmeow_mcp_core_wasm_bg.wasm` | The core segment image. |
| `pkg/*.d.ts` | The TypeScript surface. |
| `WITNESS.core-deferral.json` | The native↔wasm attestation: the typed deferral frame the core image returns for a reasoning tool, reproduced byte-for-byte by the shipped wasm. |
| `DIGESTS.blake3` | BLAKE3 content-digest manifest pinning the exact vendored bytes. |

## Why vendored (not built at regenerate time)

The regeneration pipeline never invokes `cargo` for a wasm target: `make regen` must run
without a wasm toolchain. The bytes are therefore pinned build inputs, refreshed by
`make maint-refresh-mcp-core-asset` — which runs the Node parity lane FIRST and only then
re-pins `DIGESTS.blake3`, so the digests always describe an engine whose native≡wasm parity
has just been proven.
