// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// gmeow-reason-wasm — the native GMEOW structured-DL reasoner over the wasm engine.
// The wasm-bindgen `reason`/`version` are re-exported as-is; this wrapper adds the
// one-time async wasm instantiation the synchronous boundary cannot express.

import init, { reason, version } from "./pkg/gmeow_reason_wasm.js";

let _ready = false;

export async function ready(wasmBytesOrUrl) {
  if (_ready) return;
  if (wasmBytesOrUrl !== undefined) {
    await init({ module_or_path: wasmBytesOrUrl });
  } else if (typeof process !== "undefined" && process.versions?.node) {
    const { readFile } = await import("node:fs/promises");
    const { fileURLToPath } = await import("node:url");
    const wasmPath = fileURLToPath(new URL("./pkg/gmeow_reason_wasm_bg.wasm", import.meta.url));
    await init({ module_or_path: await readFile(wasmPath) });
  } else {
    await init();
  }
  _ready = true;
}

export { reason, version };
