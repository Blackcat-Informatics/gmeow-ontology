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
 * Run the structured-DL chase over `data` (RDF text in `format`) and return the reasoned
 * closure — the inferred triples — as N-Quads text. Throws on unparsable input, a
 * reasoning failure, or a serialization failure.
 */
export function reason(data: string, format: string): string;

/**
 * Test a candidate `logic:` formula against a KB with the native SYMMETRIC conjecture
 * engine, returning the deterministic verdict as N-Triples text. `standpoint` is
 * REQUIRED: a conjecture verdict is always standpoint-scoped, never global. Throws if the
 * candidate does not name exactly one formula, the KB cannot be parsed, or the engine
 * fails.
 */
export function conjecture(
  kb: string,
  kb_format: string,
  formula: string,
  standpoint: string,
): string;

/** The reasoner version (the crate's SemVer). */
export function version(): string;
