// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * Instantiate the wasm module. Idempotent and single-flighted: concurrent callers share
 * one instantiation. In Node the wasm bytes are read from the colocated file; in a
 * browser pass the bytes/URL, or omit to fetch the colocated `.wasm`. Must be awaited
 * once before any other export is used.
 */
export function ready(wasmBytesOrUrl?: BufferSource | URL | string): Promise<void>;

/**
 * Read a GMN-1 surface back to canonical N-Quads text. Throws if the GMN-1 text cannot be
 * read back against the embedded codebook.
 */
export function from_gmn1(gmn1_text: string): string;

/**
 * The GMN-1 glyph legend as JSON: the pinned cost table, row order, and JSON shape all
 * come from the one `gmeow-lang-bridge` legend implementation the MCP `gmn_glyph_legend`
 * tool uses. Throws if the embedded codebook cannot be read.
 */
export function glyph_legend(): string;

/**
 * Transcode RDF text (in `format`) to the GMN-1 surface. Throws if the RDF cannot be
 * parsed or the GMN-1 write fails.
 */
export function to_gmn1(data: string, format: string): string;

/** The codec version (the crate's SemVer). */
export function version(): string;
