// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The wasm-bindgen-generated class declarations (DataFactory/Dataset/Quad/Sink/Term
// and the free `version()` function) are the source of truth for the engine surface.
export {
  DataFactory,
  Dataset,
  Quad,
  Sink,
  Term,
  version,
} from "./pkg/gmeow_rdf_wasm.js";

import type { Dataset, Quad } from "./pkg/gmeow_rdf_wasm.js";

/**
 * Instantiate the wasm module. Idempotent; await once before using any other API.
 * In Node the wasm bytes load from the colocated file automatically; in a browser,
 * pass the bytes/URL or omit to fetch the colocated `.wasm`.
 *
 * After `ready()`, `Dataset` is iterable (`for (const quad of dataset)`) and
 * `DataFactory.literal(value, languageOrDatatype)` accepts a `NamedNode` datatype as
 * the RDF/JS spec allows (dispatching to `typedLiteral`).
 */
export function ready(wasmBytesOrUrl?: BufferSource | URL | string): Promise<void>;

/** An RDF/JS Stream (async iterable) of the dataset's quads. */
export function datasetToStream(dataset: Dataset): AsyncIterableIterator<Quad>;

/** Consume an (async) iterable of quads into a new Dataset via the engine's Sink. */
export function streamToDataset(
  quadStream: AsyncIterable<Quad> | Iterable<Quad>,
): Promise<Dataset>;
