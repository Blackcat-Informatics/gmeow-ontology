// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// gmeow-mcp-wasm — the consumer GMEOW MCP engine over the wasm engine.
// The wasm-bindgen `init`/`mcp`/`version` are re-exported as-is; this wrapper adds the
// one-time async wasm instantiation the synchronous boundary cannot express.
//
// Naming: the sibling shims all call the instantiation helper `ready()`, and this module
// keeps that convention. The wasm module's OWN `ready()` export (is a gmeow.gts snapshot
// installed?) is therefore re-exported here as `loaded()` — one rename at the JS wrapper,
// rather than two different meanings for one name.

import wasmInit, { init, mcp, ready as snapshotLoaded, version } from "./pkg/gmeow_mcp_wasm.js";

// Cache the in-flight instantiation PROMISE, not a post-resolution boolean: two
// callers that both reach `ready()` before the first `wasmInit()` resolves must share
// one instantiation, not each trigger a full wasm fetch/instantiate. On failure the
// cache is cleared so a later call can retry.
let _ready = null;

async function instantiate(wasmBytesOrUrl) {
  if (wasmBytesOrUrl !== undefined) {
    await wasmInit({ module_or_path: wasmBytesOrUrl });
  } else if (typeof process !== "undefined" && process.versions?.node) {
    const { readFile } = await import("node:fs/promises");
    const { fileURLToPath } = await import("node:url");
    const wasmPath = fileURLToPath(new URL("./pkg/gmeow_mcp_wasm_bg.wasm", import.meta.url));
    await wasmInit({ module_or_path: await readFile(wasmPath) });
  } else {
    await wasmInit();
  }
}

export function ready(wasmBytesOrUrl) {
  if (_ready === null) {
    _ready = instantiate(wasmBytesOrUrl).catch((error) => {
      _ready = null;
      throw error;
    });
  }
  return _ready;
}

export { init, loaded, mcp, version };

// The wasm module's own `ready()` — whether a snapshot has been installed by `init`.
function loaded() {
  return snapshotLoaded();
}
