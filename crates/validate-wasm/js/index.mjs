// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// gmeow-validate-wasm — the repo-free Tier-1 GMEOW validator over the wasm engine.
//
// The wasm-bindgen-generated functions (`validate`/`version`) are re-exported as-is;
// this wrapper adds the one-time async wasm instantiation the synchronous wasm
// boundary cannot express, matching the `web` target's init contract.

import init, {
  bundle_dataset,
  validate,
  version,
} from "./pkg/gmeow_validate_wasm.js";

let _ready = false;

/**
 * Instantiate the wasm module. Idempotent. In Node the wasm bytes are read from the
 * colocated file; in a browser, pass the bytes/URL (or omit to fetch the colocated
 * `.wasm`). Must be awaited once before `validate` / `version` are used.
 */
export async function ready(wasmBytesOrUrl) {
  if (_ready) return;
  if (wasmBytesOrUrl !== undefined) {
    await init({ module_or_path: wasmBytesOrUrl });
  } else if (typeof process !== "undefined" && process.versions?.node) {
    const { readFile } = await import("node:fs/promises");
    const { fileURLToPath } = await import("node:url");
    const wasmPath = fileURLToPath(
      new URL("./pkg/gmeow_validate_wasm_bg.wasm", import.meta.url),
    );
    await init({ module_or_path: await readFile(wasmPath) });
  } else {
    await init();
  }
  _ready = true;
}

export { bundle_dataset, validate, version };
