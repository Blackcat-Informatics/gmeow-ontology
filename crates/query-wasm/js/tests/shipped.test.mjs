// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Anti-rot EXECUTION gate for the SHIPPED gmeow-query-wasm engine.
//
// `witness.test.mjs` proves the freshly-built `js/pkg/` package — this crate's own
// `cargo build` + `wasm-bindgen` output, not yet vendored anywhere — reproduces the
// native witness. That is necessary but not sufficient: the docs site does not ship
// `js/pkg/`, it ships the COMMITTED copy `make maint-refresh-query-asset` vendors into
// `crates/docs/assets/query/`. This lane loads THAT committed copy directly — never
// `../index.mjs`, which is a harness over the freshly-built package — and re-runs the
// same corpus/queries/refusal contract against it, so what is proven parity-correct is
// what actually ships.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const SHIPPED = new URL("../../../docs/assets/query/", import.meta.url);

// The shipped bindings are wasm-bindgen `--target web`: `default` is the async init
// that accepts the wasm bytes; the classes/functions are named exports.
const { default: init, Dataset, version } = await import(
  new URL("gmeow_query_wasm.js", SHIPPED).href
);
await init({
  module_or_path: await readFile(
    fileURLToPath(new URL("gmeow_query_wasm_bg.wasm", SHIPPED)),
  ),
});

const CORPUS = await readFile(
  fileURLToPath(new URL("corpus.trig", import.meta.url)),
  "utf8",
);
const QUERIES = JSON.parse(
  await readFile(fileURLToPath(new URL("queries.json", import.meta.url)), "utf8"),
);

test("version() returns the crate semver", () => {
  assert.match(version(), /^\d+\.\d+\.\d+/);
});

test("the SHIPPED engine's query results are byte-identical to the native witness attestation", async () => {
  const attestation = await readFile(
    fileURLToPath(new URL("WITNESS.query.txt", SHIPPED)),
    "utf8",
  );
  const ds = Dataset.parse(CORPUS, "trig");
  let observed = "";
  for (const { name, sparql } of QUERIES) {
    observed += `=== ${name} ===\n${ds.query(sparql)}\n`;
  }
  assert.equal(
    observed,
    attestation,
    "shipped wasm query results drifted from the native attestation",
  );
});

test("the SHIPPED engine round-trips the corpus preserving named graphs", () => {
  const ds = Dataset.parse(CORPUS, "trig");
  const nquads = ds.serialize("nquads");
  assert.match(nquads, /graph\/named/, "a named graph was flattened away");
});

test("the SHIPPED engine's Dataset.fromGts is callable and refuses garbage bytes", () => {
  // A structural smoke test only: the full quad-count + named-graph-survival proof
  // against a real profile-emitted `gmeow.gts` lives natively
  // (`crates/query-wasm/tests/from_gts.rs`), because building GTS bytes with
  // `SnapshotBuilder`/`emit_gmeow_gts` is native-only machinery, not part of the
  // wasm-bindgen export surface. Here we only confirm the shipped binding itself is
  // present and throws rather than returning a silently-empty dataset.
  assert.throws(() => Dataset.fromGts(new Uint8Array([0, 1, 2, 3])));
});

// ── the throw contract, against the bytes that actually ship ──────────────────────

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
