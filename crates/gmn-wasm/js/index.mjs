// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// gmeow-gmn-wasm — the shipped GMN-0<->GMN-1 codec over the wasm engine. EVERY
// wasm-bindgen export (`to_gmn1`/`from_gmn1`/`glyph_legend`/`version`) is re-exported
// as-is; this wrapper adds only the one-time async wasm instantiation the synchronous
// boundary cannot express. The re-export is TOTAL by contract — a shipped engine export
// that the package does not surface is a silent capability degradation, and the
// export-set-equality gate (`tests/exports.test.mjs` +
// `crates/gmeow-dev-cli/tests/npm_packaging_contract.rs`) refuses it.

import init, { from_gmn1, glyph_legend, to_gmn1, version } from "./pkg/gmeow_gmn_wasm.js";

// Cache the in-flight instantiation PROMISE, not a post-resolution boolean: two
// callers that both reach `ready()` before the first `init()` resolves must share
// one instantiation, not each trigger a full wasm fetch/instantiate. On failure the
// cache is cleared so a later call can retry.
let _ready = null;

async function instantiate(wasmBytesOrUrl) {
  if (wasmBytesOrUrl !== undefined) {
    await init({ module_or_path: wasmBytesOrUrl });
  } else if (typeof process !== "undefined" && process.versions?.node) {
    const { readFile } = await import("node:fs/promises");
    const { fileURLToPath } = await import("node:url");
    const wasmPath = fileURLToPath(new URL("./pkg/gmeow_gmn_wasm_bg.wasm", import.meta.url));
    await init({ module_or_path: await readFile(wasmPath) });
  } else {
    await init();
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

export { from_gmn1, glyph_legend, to_gmn1, version };
