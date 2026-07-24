// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// gmeow-gmn-wasm — the shipped GMN-0<->GMN-1 codec over the wasm engine. The
// wasm-bindgen `to_gmn1`/`from_gmn1`/`version` are re-exported as-is; this wrapper adds
// the one-time async wasm instantiation the synchronous boundary cannot express.

import init, { to_gmn1, from_gmn1, version } from "./pkg/gmeow_gmn_wasm.js";

let _ready = false;

export async function ready(wasmBytesOrUrl) {
  if (_ready) return;
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
  _ready = true;
}

export { to_gmn1, from_gmn1, version };
