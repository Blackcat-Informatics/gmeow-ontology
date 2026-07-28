<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-mcp-core-wasm

The **first-load half** of the tiered browser console: the shipped consumer MCP engine
compiled to `wasm32-unknown-unknown` *without* the DL reasoner, so a page can boot and
start answering on roughly half the bytes and fetch the reasoning segment only if a caller
actually asks for reasoning.

It wraps [`gmeow-mcp`](../mcp) with `default-features = false`. Its full sibling is
[`gmeow-mcp-wasm`](../mcp-wasm), which links everything; the two expose the identical
`init` / `ready` / `mcp` / `version` lifecycle so a host swaps one for the other without
changing its calling code.

## The split is a deployment tier, not a smaller engine

- `tools/list` advertises **all 35 tools** with identical descriptors; `resources/list`
  all 5. Discovery cannot tell the images apart.
- `action_policy` serves the same **total** action theory: every tool has one schema and
  every schema one tool.
- A `tools/call` for a deferred tool returns the typed, machine-readable
  **`mcp.segment-not-loaded`** signal — the stable code, the tool asked for, and the
  segment that serves it — which the JS layer turns into "load that segment and re-send
  this exact frame". Nothing is refused, nothing is answered by a weaker path.

The twelve deferred tools are the ones whose implementation reaches the reasoner:
`verify_graph`, `explain_quad`, `coherence_certificate`, `store_claim`, `conjecture_test`,
`store_conjecture`, `refute_conjecture`, `revise_belief`, `slice_quality`,
`submit_candidate`, `withdraw_candidate`, `list_candidates`. Read the list from
`deferredTools()` rather than copying it.

## JavaScript API

```js
import { ready, initTiered, tieredMcp, deferredTools } from "gmeow-mcp-core-wasm";

await ready();                              // one-time wasm instantiation (small image)
initTiered(new Uint8Array(await (await fetch("/gmeow.gts")).arrayBuffer()));

const answer = await tieredMcp(JSON.stringify({
  jsonrpc: "2.0",
  id: 1,
  method: "tools/call",
  params: { name: "verify_graph", arguments: { data: "<a> <b> <c> .", format: "nt" } },
}), {
  // Fetched only on the first frame that needs it, then cached.
  loadSegment: () => import("gmeow-mcp-wasm"),
  onSegmentLoad: ({ phase }) => setLoadingIndicator(phase === "loading"),
});
```

`tieredMcp` dispatches to the core engine, and — only if the engine returns the deferral
signal — loads the segment, installs the SAME snapshot into it, and replays the identical
frame string. The caller sees a slower answer, never a failure. `onSegmentLoad` is the seam
a UI uses to render that wait: deferral must be **visible as a loading state**, since a
silent multi-second stall would be its own kind of degradation.

The lower-level surface is also exported: `init`/`mcp`/`loaded` (the raw engine lifecycle),
`segmentDeferral(frame)` (structurally recognise the signal, never a substring match),
`SEGMENT_NOT_LOADED`, `deferredTools()`, `deferredSegment()`.

## Build

```sh
make mcp-core-wasm-pkg        # release wasm + wasm-bindgen web bindings → js/pkg/
make mcp-core-wasm-pkg-test   # build both tiers + the Node parity / demand-load lane
```

## Parity

Byte-identity to the native engine — for a core answer AND for the deferral signal — is
proven by the Node parity witness lane (`make mcp-core-wasm-pkg-test`, on the `make check`
gate via `wasm-parity`). The same lane executes the demand loader end to end against the
real `gmeow-mcp-wasm` segment and asserts the replayed answer is byte-identical to sending
the frame to the full engine directly.
