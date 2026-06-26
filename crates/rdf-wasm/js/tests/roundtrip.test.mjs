// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Node real-execution conformance for the purrdf wasm package: drives the ACTUAL
// compiled wasm through the public RDF/JS surface, including the RDF-1.2 wedge
// (directional literals + quoted-triple terms) that no incumbent RDF/JS library has.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  ready,
  DataFactory,
  Dataset,
  Sink,
  version,
  datasetToStream,
  streamToDataset,
} from "../index.mjs";

// One-time wasm instantiation before any test runs.
await ready();

const XSD_INTEGER = "http://www.w3.org/2001/XMLSchema#integer";
const RDF_LANG_STRING = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

test("version() returns the crate semver", () => {
  assert.match(version(), /^\d+\.\d+\.\d+/);
});

test("DataFactory builds RDF/JS terms", () => {
  const f = new DataFactory();
  const n = f.namedNode("https://e/s");
  assert.equal(n.termType, "NamedNode");
  assert.equal(n.value, "https://e/s");

  const plain = f.literal("hi");
  assert.equal(plain.termType, "Literal");
  assert.equal(plain.datatype.value, "http://www.w3.org/2001/XMLSchema#string");

  const lang = f.literal("hi", "en");
  assert.equal(lang.language, "en");
  assert.equal(lang.datatype.value, RDF_LANG_STRING);
});

test("polymorphic literal(value, datatype) dispatches to a typed literal", () => {
  const f = new DataFactory();
  const xsdInteger = f.namedNode(XSD_INTEGER);
  const typed = f.literal("42", xsdInteger);
  assert.equal(typed.value, "42");
  assert.equal(typed.datatype.value, XSD_INTEGER);
});

test("parse → serialize → reparse round-trips N-Triples", () => {
  const input = "<https://e/s> <https://e/p> <https://e/o> .\n";
  const ds = Dataset.parse(input, "ntriples");
  assert.equal(ds.size, 1);
  const out = ds.serialize("ntriples");
  const reparsed = Dataset.parse(out, "ntriples");
  assert.equal(reparsed.size, 1);
});

test("DatasetCore add/has/delete/match/iterate", () => {
  const f = new DataFactory();
  const q1 = f.quad(
    f.namedNode("https://e/s1"),
    f.namedNode("https://e/p"),
    f.namedNode("https://e/o1"),
  );
  const q2 = f.quad(
    f.namedNode("https://e/s2"),
    f.namedNode("https://e/p"),
    f.namedNode("https://e/o2"),
  );
  const ds = new Dataset();
  assert.equal(ds.add(q1), true);
  assert.equal(ds.add(q1), false); // idempotent
  ds.add(q2);
  assert.equal(ds.size, 2);
  assert.equal(ds.has(q1), true);

  const matched = ds.match(f.namedNode("https://e/s1"));
  assert.equal(matched.size, 1);

  // Iterable (the wrapper's Symbol.iterator over quads()).
  const subjects = [...ds].map((q) => q.subject.value).sort();
  assert.deepEqual(subjects, ["https://e/s1", "https://e/s2"]);

  assert.equal(ds.delete(q1), true);
  assert.equal(ds.size, 1);
});

test("RDF-1.2 wedge — directional literal round-trips through N-Quads", () => {
  const f = new DataFactory();
  const dir = f.directionalLiteral("مرحبا", "ar", "rtl");
  assert.equal(dir.direction, "rtl");
  const ds = new Dataset();
  ds.add(f.quad(f.namedNode("https://e/s"), f.namedNode("https://e/p"), dir));
  const out = ds.serialize("nquads");
  const reparsed = Dataset.parse(out, "nquads");
  assert.equal(reparsed.size, 1);
  const obj = reparsed.quads()[0].object;
  assert.equal(obj.termType, "Literal");
  assert.equal(obj.language, "ar");
  assert.equal(obj.direction, "rtl");
});

test("RDF-1.2 wedge — quoted-triple term round-trips through N-Quads", () => {
  const f = new DataFactory();
  const quoted = f.quotedTriple(
    f.namedNode("https://e/s"),
    f.namedNode("https://e/p"),
    f.namedNode("https://e/o"),
  );
  assert.equal(quoted.termType, "Quad");
  const ds = new Dataset();
  ds.add(
    f.quad(f.namedNode("https://e/stmt"), f.namedNode("https://e/asserts"), quoted),
  );
  const out = ds.serialize("nquads");
  const reparsed = Dataset.parse(out, "nquads");
  assert.equal(reparsed.size, 1);
  assert.equal(reparsed.quads()[0].object.termType, "Quad");
});

test("Sink streams quads into a dataset", () => {
  const f = new DataFactory();
  const sink = new Sink();
  sink.push(
    f.quad(f.namedNode("https://e/s"), f.namedNode("https://e/p"), f.namedNode("https://e/o")),
  );
  const ds = sink.finish();
  assert.equal(ds.size, 1);
});

test("datasetToStream → streamToDataset round-trips via the Sink", async () => {
  const f = new DataFactory();
  const ds = new Dataset();
  ds.add(f.quad(f.namedNode("https://e/s"), f.namedNode("https://e/p"), f.namedNode("https://e/o")));
  const rebuilt = await streamToDataset(datasetToStream(ds));
  assert.equal(rebuilt.size, 1);
});

test("an unsupported format is a rejected error", () => {
  assert.throws(() => Dataset.parse("", "yaml-ld"));
});
