// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Anti-rot EXECUTION gate for the vendored purrdf wasm engine.
//
// The docs SPARQL playground ships a PINNED copy of the wasm package under
// crates/docs/assets/purrdf/. This test loads THAT vendored copy (not the freshly
// built js/pkg/) and runs a real SPARQL query, proving the shipped engine actually
// evaluates — catching behaviour rot that the structural Rust gate cannot. Runs on
// `make wasm-pkg-test`.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const VENDORED = new URL("../../../docs/assets/purrdf/", import.meta.url);

// The vendored bindings are wasm-bindgen `--target web`: `default` is the async init
// that accepts the wasm bytes; the classes are named exports.
const { default: init, Dataset } = await import(
  new URL("gmeow_rdf_wasm.js", VENDORED).href
);
await init({
  module_or_path: await readFile(
    fileURLToPath(new URL("gmeow_rdf_wasm_bg.wasm", VENDORED)),
  ),
});

test("the VENDORED engine evaluates a SPARQL SELECT", () => {
  const ds = Dataset.parse(
    '@prefix ex: <https://e/> . ex:a ex:name "Ann" . ex:b ex:name "Bob" .',
    "turtle",
  );
  const json = JSON.parse(
    ds.query("PREFIX ex: <https://e/> SELECT ?name WHERE { ?p ex:name ?name } ORDER BY ?name"),
  );
  const names = json.results.bindings.map((b) => b.name.value);
  assert.deepEqual(names, ["Ann", "Bob"]);
});

test("the VENDORED engine hard-fails a malformed query", () => {
  const ds = Dataset.parse("<https://e/s> <https://e/p> <https://e/o> .", "ntriples");
  assert.throws(() => ds.query("SELECT ?x WHERE { not sparql"));
});
