<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-mcp-wasm

The shipped **consumer MCP engine** compiled to `wasm32-unknown-unknown`, so a browser
console, an editor plugin, or an in-page LLM client can drive the full 38-tool /
5-resource JSON-RPC surface **client-side** — no server, no stdio, no repository.

It wraps [`gmeow-mcp`](../mcp) with `default-features = false, features = ["reasoning"]`:
this image is the demand-loaded **reasoning segment**, a genuine delta over the
always-resident [`gmeow-mcp-core-wasm`](../mcp-core-wasm) rather than a superset of it. It
serves the 15 reasoning-segment tools — which includes the whole grounded-memory triad,
`store_claim` / `recall` / `store_segment` / `revise_belief`, because a wasm module's claim
store is private to that module and a triad split across the two images would lose every
write — and answers a `tools/call` for one of the 23 core tools with the typed
`mcp.segment-not-loaded` signal naming the `core` segment. The whole 38-tool surface stays
advertised either way.

Every frame is dispatched through `McpServer::handle_message`, the one protocol
implementation, so `initialize`, `tools/list`, `resources/list`, `tools/call`,
`resources/read` and `shutdown` behave exactly as they do natively because they ARE the
native code paths. Byte-identity to the native engine is proven by the Node parity witness
lane (`make mcp-wasm-pkg-test`, on the `make check` gate via `wasm-parity`).

Unlike the codebook-embedding sibling shims, **the `gmeow.gts` bundle is never embedded**.
A bundle is caller data — tens of megabytes, and versioned independently of the engine —
so the caller hands the snapshot bytes over once and then drives frames.

## JavaScript API

```js
import { ready, init, mcp, loaded, version } from "gmeow-mcp-wasm";

await ready();                          // one-time wasm instantiation
init(new Uint8Array(await (await fetch("/gmeow.gts")).arrayBuffer()));
loaded();                               // -> true

const frame = mcp(JSON.stringify({
  jsonrpc: "2.0",
  id: 1,
  method: "tools/call",
  params: { name: "convert", arguments: { data: "<a> <b> <c> .", from: "nt", to: "turtle" } },
}));
// frame: the serialized JSON-RPC response
```

`ready(wasmBytesOrUrl)` performs the one-time wasm instantiation (the sibling-shim
convention). `init(snapshotBytes)` loads a `gmeow.gts` bundle and builds the engine over
it; calling it again swaps bundles wholesale (a new bundle is a new session). `mcp(frame)`
handles ONE JSON-RPC request and returns the response frame — the empty string for a
`notifications/*` frame, which by protocol has no response. Protocol and tool errors come
back IN the frame (a JSON-RPC `error` member, or a tool envelope with `isError: true`), so
the only thrown condition is calling `mcp` before `init`. `loaded()` is the wasm module's
own `ready()` export — whether a snapshot is installed — renamed at the JS wrapper only
because `ready` names the instantiation helper there.

## Build

```sh
make mcp-wasm-pkg        # release wasm + wasm-bindgen web bindings → js/pkg/
make mcp-wasm-pkg-test   # build + Node native↔wasm parity witness lane
```
