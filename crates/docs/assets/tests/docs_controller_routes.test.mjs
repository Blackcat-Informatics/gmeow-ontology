// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The docs controller's four engine routes, EXECUTED against the shipped wasm image and
// the shipped bundle — no browser, no mocks.
//
// These four call sites used to run on a separate vendored engine (purrdf's 8.15 MB wasm
// build), on the claim that the MCP surface could not answer them. Each assertion below is
// the executed refutation of that claim, in the same order the controller performs them:
//
//   1. the bundle count the explorer's `info` line and the playground's ready state read;
//   2. the explorer's `describe`, which must come back as a GRAPH;
//   3. a standalone query over a caller-supplied dataset, unioned with nothing — the one
//      capability the retired engine was genuinely kept for;
//   4. the copy-as bar's format conversion, for every format the bar offers.
//
// `docs-controller.mjs` itself touches `document` at import time, so it cannot be imported
// under Node. What is shared with it — the frame shape (`queryBundle`, `convertRdf`) and
// the describe query text — is imported from the DOM-free modules the controller builds on,
// which is what makes this an exercise of the controller's routes rather than a re-write of
// them.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { callTool, configure, convertRdf, queryBundle } from "../mcp-transport.mjs";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));

// The same two seams the console lane configures: Node has no `fetch` for `file:` URLs,
// and `bundle-manifest.json` is a RENDERER output that this build-input tree has no copy
// of, so the one entry this lane boots over is computed the way the renderer computes it.
const SNAPSHOT = here("../../../../generated/dist/gmeow.gts");
const { blake3Hex } = await import("../blake3.mjs");
const snapshotBytes = new Uint8Array(await readFile(SNAPSHOT));
const MANIFEST = {
  "assets/gmeow.gts": {
    blake3: `blake3:${blake3Hex(snapshotBytes)}`,
    bytes: snapshotBytes.length,
  },
};
const encodeJson = (value) => new TextEncoder().encode(JSON.stringify(value));

configure({
  assetBase: new URL("../", import.meta.url),
  fetchBytes: async (url) => {
    const name = url.toString();
    if (name.endsWith("/bundle-manifest.json")) return encodeJson(MANIFEST);
    if (name.endsWith("/gmeow.gts")) return snapshotBytes;
    return new Uint8Array(await readFile(fileURLToPath(url)));
  },
});

// The GMEOW term the explorer describes in these assertions. Deterministic and present in
// the object-level ontology; `crates/mcp/tests/witness_explore.rs` pins its exact
// description against the committed attestation, so this lane asserts SHAPE (a non-empty
// graph came back over the wire) and leaves CONTENT to that witness.
const TERM = "https://blackcatinformatics.ca/gmeow/AboutnessMode";

// `describeQuery` in `docs-controller.mjs`, verbatim. A bound-subject CONSTRUCT, not a
// DESCRIBE: this engine's DESCRIBE gathers across every named graph, while the explorer
// means the object-level (default-graph) description alone.
const describeQuery = (subject) => `CONSTRUCT { ${subject} ?p ?o } WHERE { ${subject} ?p ?o }`;

// ── 1. The explorer's `info` / the playground's ready state ──────────────────

test("the bundle count the explorer and playground boot on is a real answer", async () => {
  const out = await queryBundle("SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }");
  assert.equal(out.ok, true, JSON.stringify(out));
  assert.equal(out.form, "bindings", "a COUNT is a bindings result");
  const n = Number(out.results?.bindings?.[0]?.n?.value ?? 0);
  assert.ok(
    n > 0,
    "the object-level triple count must be positive — the explorer renders it as `info` and " +
      "the playground as its ready state, so a zero here is a surface that reads as empty",
  );
});

// ── 2. The explorer's describe — a CONSTRUCT that must return a GRAPH ────────

test("the explorer describe returns a graph, not bindings", async () => {
  const out = await queryBundle(describeQuery(`<${TERM}>`));
  assert.equal(out.ok, true, JSON.stringify(out));
  assert.equal(
    out.form,
    "graph",
    "the engine must DECLARE a graph form — the controller switches on this field rather " +
      "than guessing from the payload's shape",
  );
  assert.ok(out.quad_count > 0, `describe of ${TERM} must be non-empty: ${out.quad_count}`);
  assert.match(
    out.graph_nquads,
    new RegExp(TERM.replaceAll(".", "\\.")),
    "the returned N-Quads must mention the described subject",
  );
});

test("a CURIE and a full IRI describe the same term", async () => {
  // The explorer accepts either; `subjectTerm` brackets a full IRI and passes a CURIE
  // through, so both must reach the same subject.
  const prefix = "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\n";
  const viaIri = await queryBundle(`${prefix}${describeQuery(`<${TERM}>`)}`);
  const viaCurie = await queryBundle(`${prefix}${describeQuery("gmeow:AboutnessMode")}`);
  assert.equal(viaCurie.ok, true, JSON.stringify(viaCurie));
  assert.equal(
    viaCurie.quad_count,
    viaIri.quad_count,
    "a CURIE and its expansion must describe the same term",
  );
});

// ── 3. A standalone query over a caller-supplied dataset ─────────────────────

test("a standalone dataset query unions nothing from the bundle", async () => {
  // This is the capability the vendored engine was kept for: read a pasted graph ON ITS
  // OWN TERMS. `scope: "input"` expresses it on the shipped surface, so the second engine
  // was answering a question this one already answers.
  const data = "<urn:ex:widget> <urn:ex:label> \"Local Widget\" .\n";
  const alone = await callTool("query_local", {
    data,
    format: "turtle",
    scope: "input",
    query: "SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }",
  });
  assert.equal(alone.ok, true, JSON.stringify(alone));
  assert.equal(
    alone.results.bindings[0].n.value,
    "1",
    "an input-scoped query must see the pasted triple and NOTHING else — a bundle triple " +
      "leaking in is the silent union that made a standalone reading unaskable",
  );

  // A CONSTRUCT over the same standalone dataset also comes back as a graph.
  const graph = await callTool("query_local", {
    data,
    format: "turtle",
    scope: "input",
    query: "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }",
  });
  assert.equal(graph.form, "graph", JSON.stringify(graph));
  assert.equal(graph.quad_count, 1, JSON.stringify(graph));

  // And the bundle scope over the SAME input sees strictly more — the two scopes are real
  // alternatives, not a default and a degradation of it.
  const unioned = await callTool("query_local", {
    data,
    format: "turtle",
    scope: "bundle",
    query: "SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }",
  });
  assert.ok(
    Number(unioned.results.bindings[0].n.value) > 1,
    "the bundle scope must read the canon too",
  );
});

// ── 4. The copy-as bar's format conversion ───────────────────────────────────

test("the copy-as bar converts a result graph into every offered format", async () => {
  // `FORMATS` in `docs-controller.mjs`. `nquads` is the engine's own graph form and is
  // handed back untranscoded by the controller, so it needs no conversion leg here.
  const FORMATS = ["turtle", "ntriples", "nquads", "trig", "rdfxml", "jsonld"];
  const out = await queryBundle(describeQuery(`<${TERM}>`));
  const nquads = out.graph_nquads;

  for (const format of FORMATS) {
    if (format === "nquads") continue;
    const text = await convertRdf(nquads, "nquads", format);
    assert.ok(
      typeof text === "string" && text.length > 0,
      `convert nquads→${format} produced no output`,
    );
    assert.match(
      text,
      /AboutnessMode/,
      `the ${format} serialization must still carry the described subject`,
    );
  }
});

test("a malformed query is a thrown error, never a silent empty result", async () => {
  // The controller renders a thrown message as `Query error: …`. An engine that answered
  // an unparsable query with an empty result set would render as "no matches" — the exact
  // ambiguity that let four dead queries look like honest negatives.
  await assert.rejects(
    () => queryBundle("SELECT ?s WHERE { ?s ?p"),
    "an unparsable query must reject",
  );
});
