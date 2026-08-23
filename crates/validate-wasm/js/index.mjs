// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// gmeow-validate-wasm — the repo-free Tier-1 GMEOW validator over the wasm engine.
//
// EVERY wasm-bindgen-generated function (`bundle_dataset`/`gmn_codebook_digest`/
// `gmn_validate`/`validate`/`version`) is re-exported as-is; this wrapper adds only the
// one-time async wasm instantiation the synchronous wasm boundary cannot express,
// matching the `web` target's init contract. The re-export is TOTAL by contract — a
// shipped engine export that the package does not surface is a silent capability
// degradation, and the export-set-equality gate (`tests/exports.test.mjs` +
// `crates/gmeow-dev-cli/tests/npm_packaging_contract.rs`) refuses it.

import init, {
  bundle_dataset,
  gmn_codebook_digest,
  gmn_validate,
  validate,
  version,
} from "./pkg/gmeow_validate_wasm.js";

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
    const wasmPath = fileURLToPath(
      new URL("./pkg/gmeow_validate_wasm_bg.wasm", import.meta.url),
    );
    await init({ module_or_path: await readFile(wasmPath) });
  } else {
    await init();
  }
}

/**
 * Instantiate the wasm module. Idempotent and single-flighted: concurrent callers
 * share one instantiation. In Node the wasm bytes are read from the colocated file;
 * in a browser, pass the bytes/URL (or omit to fetch the colocated `.wasm`). Must be
 * awaited once before `validate` / `version` are used.
 */
export function ready(wasmBytesOrUrl) {
  if (_ready === null) {
    _ready = instantiate(wasmBytesOrUrl).catch((error) => {
      _ready = null;
      throw error;
    });
  }
  return _ready;
}

export { bundle_dataset, gmn_codebook_digest, gmn_validate, validate, version };
