// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// purrdf — the idiomatic RDF/JS surface over the wasm engine.
//
// The wasm-bindgen-generated classes (DataFactory/Dataset/Quad/Sink/Term) are
// re-exported as-is; this wrapper adds the isomorphic glue that the synchronous
// wasm boundary cannot express in Rust:
//   * `ready()` — one-time async wasm instantiation (required for the `web` target).
//   * the polymorphic RDF/JS `DataFactory.literal(value, languageOrDatatype)` —
//     dispatching a NamedNode datatype argument to `typedLiteral` (a wasm-bindgen
//     exported type can't be recovered from an untyped value in Rust).
//   * `Dataset` iterability (`for (const quad of dataset)`).
//   * `datasetToStream` / `streamToDataset` — the async RDF/JS Stream/Sink primitives
//     over the synchronous `Dataset.quads()` / `Sink` engine surface.

import init, {
  DataFactory,
  Dataset,
  Quad,
  Sink,
  Term,
  version,
} from "./pkg/gmeow_rdf_wasm.js";

let _ready = false;

/**
 * Instantiate the wasm module. Idempotent. In Node the wasm bytes are read from the
 * colocated file; in a browser, pass the bytes/URL (or omit to fetch the colocated
 * `.wasm`). Must be awaited once before any other API is used.
 */
export async function ready(wasmBytesOrUrl) {
  if (_ready) return;
  if (wasmBytesOrUrl !== undefined) {
    await init({ module_or_path: wasmBytesOrUrl });
  } else if (typeof process !== "undefined" && process.versions?.node) {
    const { readFile } = await import("node:fs/promises");
    const { fileURLToPath } = await import("node:url");
    const wasmPath = fileURLToPath(
      new URL("./pkg/gmeow_rdf_wasm_bg.wasm", import.meta.url),
    );
    await init({ module_or_path: await readFile(wasmPath) });
  } else {
    await init();
  }

  // RDF/JS DatasetCore is iterable over its quads.
  if (!Dataset.prototype[Symbol.iterator]) {
    Dataset.prototype[Symbol.iterator] = function () {
      return this.quads()[Symbol.iterator]();
    };
  }

  // Present the RDF/JS-spec polymorphic literal(value, languageOrDatatype). The wasm
  // method takes `(value, language?)`; a NamedNode second argument is a datatype.
  if (!DataFactory.prototype.__purrdfPolymorphicLiteral) {
    const wasmLiteral = DataFactory.prototype.literal;
    DataFactory.prototype.literal = function (value, languageOrDatatype) {
      if (
        languageOrDatatype != null &&
        typeof languageOrDatatype === "object" &&
        languageOrDatatype.termType === "NamedNode"
      ) {
        return this.typedLiteral(value, languageOrDatatype);
      }
      return wasmLiteral.call(this, value, languageOrDatatype ?? undefined);
    };
    DataFactory.prototype.__purrdfPolymorphicLiteral = true;
  }

  _ready = true;
}

/**
 * An RDF/JS Stream of the dataset's quads — an async iterable. (The engine is
 * synchronous; the async wrapper is the RDF/JS Stream contract.)
 */
export function datasetToStream(dataset) {
  const quads = dataset.quads();
  return (async function* () {
    for (const quad of quads) yield quad;
  })();
}

/**
 * Consume an (async) iterable of quads into a new Dataset, via the engine's streaming
 * Sink (the gmeow-rdf-events ingestion protocol + its finish() resolution).
 */
export async function streamToDataset(quadStream) {
  const sink = new Sink();
  for await (const quad of quadStream) sink.push(quad);
  return sink.finish();
}

export { DataFactory, Dataset, Quad, Sink, Term, version };
