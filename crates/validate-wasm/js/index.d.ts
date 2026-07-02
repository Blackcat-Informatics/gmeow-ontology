// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Instantiate the wasm module. Idempotent; await once before `validate`/`version`.
 * In Node the wasm bytes are read from the colocated file; in a browser pass the
 * bytes/URL or omit to fetch the colocated `.wasm`.
 */
export function ready(wasmBytesOrUrl?: BufferSource | URL | string): Promise<void>;

/**
 * Run Tier-1 conformance of `data` (RDF text in `format`) against the shapes and
 * disciplines carried in the `gts` bundle bytes, returning the canonical diagnostics
 * `Report` as a JSON string. Throws on a malformed bundle or unparsable input.
 */
export function validate(
  data: string,
  format: string,
  gts: Uint8Array,
  namespace: string,
  origin: string,
): string;

/** The validator version (the crate's SemVer). */
export function version(): string;
