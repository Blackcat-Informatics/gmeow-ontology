// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Instantiate the wasm module. Idempotent; await once before `validate`/`version`.
 * In Node the wasm bytes are read from the colocated file; in a browser pass the
 * bytes/URL or omit to fetch the colocated `.wasm`.
 */
export function ready(wasmBytesOrUrl?: BufferSource | URL | string): Promise<void>;

/**
 * Extract a `gmeow.gts` bundle's RDF as graph-preserving N-Quads text, so an in-browser
 * RDF engine can parse and query the SAME bundle the pipeline shipped. The graph
 * component of each quad is retained — the query surface sees the bundle's real graph
 * structure, not a flattened union. Throws on an unreadable container.
 */
export function bundle_dataset(gts: Uint8Array): string;

/**
 * The blake3 content digest of the GMN-1 codebook embedded in this wasm image, as
 * lowercase hex — the same content address `gmeow gmn digest` reports over the carrier
 * bytes, so a caller can pin the EXACT codebook `gmn_validate` checked against.
 */
export function gmn_codebook_digest(): string;

/**
 * Validate a GMN-1 document against the EMBEDDED codebook, returning a JSON verdict:
 * `{ "conformant": true }`, or `{ "conformant": false, "failureClass": "<lang: IRI>",
 * "detail": "…" }`. Because the codebook is embedded, glyphs, dictionary aliases, and
 * prefixed terms are actually RESOLVED — a grammar-valid document naming an uncovered
 * term is rejected. Throws only if the document bytes are not valid UTF-8.
 */
export function gmn_validate(bytes: Uint8Array): string;

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
