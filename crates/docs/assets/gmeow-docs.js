// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The offline documentation SPARQL playground controller.
//
// Loaded as an ES module only on the playground page. It boots the vendored purrdf
// wasm engine, parses the bundled TriG asset once, runs the reader's SPARQL query
// entirely in-browser (no server, no network), and renders the result — a table for
// SELECT/ASK, and a graph with "copy as <format>" transcoding for CONSTRUCT/DESCRIBE.
//
// Every path is resolved relative to THIS module's URL, so it works at any site depth
// and offline (file://): the purrdf bindings sit in ./purrdf/, the RDF asset is
// ./playground.trig.

import init, { Dataset } from "./purrdf/gmeow_rdf_wasm.js";

const FORMATS = [
  ["turtle", "Turtle"],
  ["ntriples", "N-Triples"],
  ["nquads", "N-Quads"],
  ["trig", "TriG"],
  ["rdfxml", "RDF/XML"],
  ["jsonld", "JSON-LD"],
];

const form = document.getElementById("gmeow-sparql");
const queryEl = document.getElementById("gmeow-sparql-query");
const statusEl = document.getElementById("gmeow-sparql-status");
const resultsEl = document.getElementById("gmeow-sparql-results");

// Only activate on the playground page.
if (form && queryEl && resultsEl) {
  main().catch((err) => setStatus(`Failed to start the engine: ${err.message ?? err}`));
}

let dataset = null;

async function main() {
  setStatus("Loading the query engine…");
  await init(new URL("./purrdf/gmeow_rdf_wasm_bg.wasm", import.meta.url));

  setStatus("Loading the ontology…");
  const trig = await (await fetch(new URL("./playground.trig", import.meta.url))).text();
  dataset = Dataset.parse(trig, "trig");
  setStatus(`Ready — ${dataset.size} triples loaded. Run a query.`);

  // Prefill from the ?q= query parameter (e.g. a term page's "DESCRIBE" link).
  const prefill = new URLSearchParams(window.location.search).get("q");
  if (prefill) queryEl.value = prefill;

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    runQuery();
  });
}

function runQuery() {
  resultsEl.replaceChildren();
  const sparql = queryEl.value.trim();
  if (!sparql) return;
  let out;
  try {
    out = dataset.query(sparql);
  } catch (err) {
    setStatus(`Query error: ${err.message ?? err}`);
    return;
  }
  // SELECT/ASK return SPARQL Results JSON; CONSTRUCT/DESCRIBE return Turtle (which is
  // not JSON, so the parse throws and we fall through to the graph branch).
  let json = null;
  try {
    json = JSON.parse(out);
  } catch {
    json = null;
  }
  if (json && (json.results || typeof json.boolean === "boolean")) {
    renderSolutions(json);
  } else {
    renderGraph(out);
  }
}

function renderSolutions(json) {
  if (typeof json.boolean === "boolean") {
    setStatus(`ASK → ${json.boolean}`);
    const p = document.createElement("p");
    p.textContent = String(json.boolean);
    resultsEl.append(p);
    return;
  }
  const vars = json.head?.vars ?? [];
  const rows = json.results?.bindings ?? [];
  setStatus(`${rows.length} row${rows.length === 1 ? "" : "s"}.`);
  const table = document.createElement("table");
  table.className = "gmeow-sparql-table";
  const thead = table.createTHead().insertRow();
  for (const v of vars) {
    const th = document.createElement("th");
    th.textContent = v;
    thead.append(th);
  }
  const tbody = table.createTBody();
  for (const binding of rows) {
    const tr = tbody.insertRow();
    for (const v of vars) {
      const td = tr.insertCell();
      td.textContent = binding[v] ? binding[v].value : "";
    }
  }
  resultsEl.append(table);
}

function renderGraph(turtle) {
  setStatus("Graph result — copy in any RDF serialization:");
  const bar = document.createElement("div");
  bar.className = "gmeow-sparql-copybar";
  for (const [fmt, label] of FORMATS) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = label;
    button.addEventListener("click", () => copyAs(turtle, fmt, label));
    bar.append(button);
  }
  resultsEl.append(bar);
  const pre = document.createElement("pre");
  pre.className = "gmeow-sparql-graph";
  pre.textContent = turtle;
  resultsEl.append(pre);
}

function copyAs(turtle, fmt, label) {
  let text;
  try {
    // Transcode the result graph client-side through the same engine.
    text = fmt === "turtle" ? turtle : Dataset.parse(turtle, "turtle").serialize(fmt);
  } catch (err) {
    setStatus(`Cannot serialize as ${label}: ${err.message ?? err}`);
    return;
  }
  navigator.clipboard?.writeText(text).then(
    () => setStatus(`Copied as ${label}.`),
    () => setStatus(`Clipboard unavailable; ${label} shown below.`),
  );
}

function setStatus(text) {
  if (statusEl) statusEl.textContent = text;
}
