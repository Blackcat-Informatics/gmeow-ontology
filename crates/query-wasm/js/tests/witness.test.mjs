// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// WASM half of the query-engine parity WITNESS — the FRESHLY-BUILT package.
//
// The native half (`crates/query-wasm/tests/witness_query.rs`) runs the SAME corpus
// through the SAME purrdf entries natively and writes the attestation; this lane runs
// it through `../index.mjs`, which loads `./pkg/gmeow_query_wasm.js` — the package
// THIS crate's own `cargo build` + `wasm-bindgen` just produced, not yet vendored
// anywhere — and asserts byte-identity. It is what `make query-wasm-pkg-test` (and
// therefore `make maint-refresh-query-asset`) gates BEFORE the bytes are copied into
// `crates/docs/assets/query/`, so bytes that never passed parity can never be vendored.
//
// It does NOT prove anything about what the docs site ships: `shipped.test.mjs` loads
// the COMMITTED engine directly and re-runs the identical corpus against it — that is
// the mandatory "what ships is what was proven" lane.
//
// The corpus is COMMITTED and self-contained — deliberately not the `gmeow.gts`
// bundle. A bundle-scoped witness would red this gate on every ontology edit, making
// parity noise attributable to content rather than to the engine, and it would be
// unrunnable before the bundle is materialized. Engine parity is an engine property.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { ready, Dataset, version } from "../index.mjs";

await ready();

const CORPUS = await readFile(
  fileURLToPath(new URL("./corpus.trig", import.meta.url)),
  "utf8",
);
const QUERIES = JSON.parse(
  await readFile(fileURLToPath(new URL("./queries.json", import.meta.url)), "utf8"),
);

test("version() returns the crate semver", () => {
  assert.match(version(), /^\d+\.\d+\.\d+/);
});

test("wasm query results are byte-identical to the native witness attestation", async () => {
  const attestation = await readFile(
    fileURLToPath(new URL("../../../docs/assets/query/WITNESS.query.txt", import.meta.url)),
    "utf8",
  );
  const ds = Dataset.parse(CORPUS, "trig");
  let observed = "";
  for (const { name, sparql } of QUERIES) {
    observed += `=== ${name} ===\n${ds.query(sparql)}\n`;
  }
  assert.equal(observed, attestation, "wasm query results drifted from the native attestation");
});

test("the corpus round-trips through the wasm engine preserving named graphs", () => {
  const ds = Dataset.parse(CORPUS, "trig");
  const nquads = ds.serialize("nquads");
  assert.match(nquads, /graph\/named/, "a named graph was flattened away");
});

// ── the throw contract: a refusal is never a silent empty result ──────────────────

test("a malformed document throws rather than parsing to an empty dataset", () => {
  assert.throws(() => Dataset.parse("@@@ not turtle", "turtle"));
});

test("an unknown format throws rather than falling back to a default codec", () => {
  assert.throws(() => Dataset.parse(CORPUS, "application/not-a-format"));
});

test("a malformed query throws rather than returning zero bindings", () => {
  const ds = Dataset.parse(CORPUS, "trig");
  assert.throws(() => ds.query("SELECT ?s WHERE {"));
});

test("a SERVICE clause throws — the browser cannot resolve it", () => {
  const ds = Dataset.parse(CORPUS, "trig");
  assert.throws(() =>
    ds.query("SELECT ?s WHERE { SERVICE <https://example.org/sparql> { ?s ?p ?o } }"),
  );
});
