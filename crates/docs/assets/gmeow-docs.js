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
import validateInit, { validate as wasmValidate } from "./validate/gmeow_validate_wasm.js";
import reasonInit, { reason as wasmReason } from "./reason/gmeow_reason_wasm.js";
import gmnInit, {
  to_gmn1 as wasmToGmn1,
  from_gmn1 as wasmFromGmn1,
  glyph_legend as wasmGlyphLegend,
} from "./gmn/gmeow_gmn_wasm.js";

const GMEOW_NS = "https://blackcatinformatics.ca/gmeow/";

const FORMATS = [
  ["turtle", "Turtle"],
  ["ntriples", "N-Triples"],
  ["nquads", "N-Quads"],
  ["trig", "TriG"],
  ["rdfxml", "RDF/XML"],
  ["jsonld", "JSON-LD"],
];

// ── Shared browser-bundle loader ────────────────────────────────────────────
// The single client entry point every browser surface (SPARQL playground, bundle
// explorer, validation panels) uses to obtain the queryable ontology — so no
// surface invents a second fetch/parse path. It boots the purrdf engine once,
// fetches the object-level core bundle N-Quads, verifies its byte length against
// the emitted content-address manifest (a truncated or swapped asset is rejected),
// and returns the parsed Dataset. Iteration order over the returned dataset is
// stable-sorted by callers so a native↔wasm witness can byte-compare results.
let _engineReady = null;
async function ensureEngine() {
  if (!_engineReady) {
    _engineReady = init(new URL("./purrdf/gmeow_rdf_wasm_bg.wasm", import.meta.url));
  }
  await _engineReady;
}

export async function loadCoreBundle() {
  await ensureEngine();
  const manifest = await (
    await fetch(new URL("./bundle-manifest.json", import.meta.url))
  ).json();
  const nq = await (await fetch(new URL("./gmeow-core.nq", import.meta.url))).text();
  const expected = manifest["assets/gmeow-core.nq"]?.bytes;
  const actual = new TextEncoder().encode(nq).length;
  if (expected !== undefined && actual !== expected) {
    throw new Error(
      `core bundle integrity: expected ${expected} bytes, got ${actual}`,
    );
  }
  return Dataset.parse(nq, "nquads");
}

/** The URL of the full `gmeow.gts` bundle (the Tier-1 validate surface's shapes
 * source), resolved relative to this module. */
export function fullBundleUrl() {
  return new URL("./gmeow.gts", import.meta.url);
}

// ── Live Tier-1 validation (W1) ─────────────────────────────────────────────
// Each counter-example fixture on a term page ships a "run validation" button
// carrying the fixture's base64-encoded Turtle. On click we load the FULL gmeow.gts
// bundle (its shapes-archive), run the REAL Tier-1 validator (gmeow-validate-wasm)
// entirely in-browser, and render the unified-diagnostics findings — each linking
// through its helpUri into the constraint catalog. This is the SAME validator the
// on-gate authority runs; the native↔wasm parity witness lane proves they agree.
let _validatorReady = null;
async function ensureValidator() {
  if (!_validatorReady) {
    _validatorReady = validateInit(
      new URL("./validate/gmeow_validate_wasm_bg.wasm", import.meta.url),
    );
  }
  await _validatorReady;
}

let _bundleBytes = null;
async function fullBundleBytes() {
  if (!_bundleBytes) {
    _bundleBytes = new Uint8Array(await (await fetch(fullBundleUrl())).arrayBuffer());
  }
  return _bundleBytes;
}

/** Run Tier-1 conformance of `turtle` against the bundle shapes, returning the
 * canonical diagnostics report `{ tool, findings: [...] }`. */
export async function runFixtureValidation(turtle, origin) {
  await ensureValidator();
  const bundle = await fullBundleBytes();
  return JSON.parse(wasmValidate(turtle, "turtle", bundle, GMEOW_NS, origin || "fixture.ttl"));
}

function renderFindings(container, report, catalogHref) {
  const findings = report.findings ?? [];
  container.replaceChildren();
  if (findings.length === 0) {
    container.textContent = "Conforms — no Tier-1 findings.";
    return;
  }
  const ul = document.createElement("ul");
  // Stable sort so the rendered order is deterministic (native/wasm comparable).
  for (const f of [...findings].sort((a, b) =>
    `${a.code} ${a.message}`.localeCompare(`${b.code} ${b.message}`),
  )) {
    const li = document.createElement("li");
    // The finding `code` IS the constraint-catalog anchor (the validator's helpUri
    // slug); link into the catalog when a base href is supplied.
    const code = catalogHref
      ? Object.assign(document.createElement("a"), {
          href: `${catalogHref}#${f.code}`,
          textContent: f.code,
        })
      : Object.assign(document.createElement("code"), { textContent: f.code });
    li.append(`${f.severity ?? "error"}: `, code, ` — ${f.message ?? ""}`);
    ul.append(li);
  }
  container.replaceChildren(ul);
}

for (const btn of document.querySelectorAll(".gmeow-run-validation")) {
  btn.addEventListener("click", async () => {
    const results = btn.parentElement.querySelector(".gmeow-validation-results");
    results.textContent = "Validating…";
    try {
      const turtle = new TextDecoder().decode(
        Uint8Array.from(atob(btn.dataset.turtle), (c) => c.charCodeAt(0)),
      );
      renderFindings(
        results,
        await runFixtureValidation(turtle, btn.dataset.origin),
        btn.dataset.catalogHref,
      );
    } catch (e) {
      results.textContent = `Validation failed: ${e.message ?? e}`;
    }
  });
}

// ── Bundle explorer (W2b) ───────────────────────────────────────────────────
// Browser `gmeow info`/`describe` over the object-level core bundle: load it via
// the shared loader, show the `info` summary, and run a client-side `DESCRIBE` for
// the entered term. The DESCRIBE is the SAME the native `gmeow describe` produces
// (proven byte-identical by the F2 witness lane).
const explorerForm = document.getElementById("gmeow-explorer-form");
if (explorerForm) {
  const infoEl = document.getElementById("gmeow-explorer-info");
  const iriEl = document.getElementById("gmeow-explorer-iri");
  const resultsEl = document.getElementById("gmeow-explorer-results");
  let explorerDataset = null;
  loadCoreBundle()
    .then((ds) => {
      explorerDataset = ds;
      infoEl.textContent = `info — ${ds.size} triples in the object-level core bundle.`;
    })
    .catch((e) => {
      infoEl.textContent = `Failed to load the bundle: ${e.message ?? e}`;
    });
  explorerForm.addEventListener("submit", (event) => {
    event.preventDefault();
    if (!explorerDataset) return;
    const term = iriEl.value.trim();
    if (!term) return;
    // A CURIE (gmeow:Foo) or a full IRI; the describe query brackets a full IRI.
    const subject = term.includes("://") ? `<${term}>` : term;
    resultsEl.replaceChildren();
    try {
      const turtle = explorerDataset.query(
        `PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\nDESCRIBE ${subject}`,
      );
      const pre = document.createElement("pre");
      pre.textContent = turtle && turtle.trim() ? turtle : "No triples describe that term.";
      resultsEl.append(pre);
    } catch (err) {
      resultsEl.textContent = `Describe error: ${err.message ?? err}`;
    }
  });
}

// ── Live entailment panel (W4b) ─────────────────────────────────────────────
// Run the native GMEOW structured-DL reasoner (gmeow-reason-wasm) over pasted RDF
// entirely in-browser and show the inference diff (the entailed triples). The wasm
// chase is byte-identical to the native one (proven by the F3 witness lane).
const reasonForm = document.getElementById("gmeow-reason-form");
if (reasonForm) {
  let _reasonReady = null;
  const ensureReasoner = async () => {
    if (!_reasonReady) {
      _reasonReady = reasonInit(new URL("./reason/gmeow_reason_wasm_bg.wasm", import.meta.url));
    }
    await _reasonReady;
  };
  const inputEl = document.getElementById("gmeow-reason-input");
  const resultsEl = document.getElementById("gmeow-reason-results");
  reasonForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    resultsEl.textContent = "Reasoning…";
    try {
      await ensureReasoner();
      const closure = wasmReason(inputEl.value, "turtle");
      const lines = closure.split("\n").filter((l) => l.trim());
      resultsEl.replaceChildren();
      const h = document.createElement("p");
      h.textContent =
        lines.length === 0
          ? "No new entailments."
          : `Entailed ${lines.length} triple${lines.length === 1 ? "" : "s"}:`;
      const pre = document.createElement("pre");
      pre.textContent = closure.trim();
      resultsEl.append(h, pre);
    } catch (e) {
      resultsEl.textContent = `Reasoning failed: ${e.message ?? e}`;
    }
  });
}

// Transcode authored RDF into the token-compact GMN-1 surface and back
// (gmeow-gmn-wasm) entirely in-browser. `to_gmn1` then `from_gmn1` is the round-trip
// the native↔wasm witness pins byte-for-byte; the panel shows both legs plus the
// codebook's glyph legend (each glyph's real LLM-token cost).
const gmnForm = document.getElementById("gmeow-gmn-form");
if (gmnForm) {
  let _gmnReady = null;
  const ensureGmn = async () => {
    if (!_gmnReady) {
      _gmnReady = gmnInit(new URL("./gmn/gmeow_gmn_wasm_bg.wasm", import.meta.url));
    }
    await _gmnReady;
  };
  const inputEl = document.getElementById("gmeow-gmn-input");
  const legendEl = document.getElementById("gmeow-gmn-legend");
  const resultsEl = document.getElementById("gmeow-gmn-results");
  // Render the glyph legend once the engine is up (deterministic, from the codebook).
  const renderLegend = () => {
    if (!legendEl || legendEl.childElementCount) return;
    try {
      const entries = JSON.parse(wasmGlyphLegend());
      if (!entries.length) return;
      const label = document.createElement("span");
      label.textContent = "Glyphs: ";
      legendEl.append(label);
      for (const { glyph, tokenCost } of entries) {
        const chip = document.createElement("span");
        chip.className = "gmeow-gmn-glyph";
        chip.textContent = glyph;
        chip.title = `${glyph} — ${tokenCost} LLM token${tokenCost === 1 ? "" : "s"}`;
        legendEl.append(chip);
      }
    } catch {
      /* legend is auxiliary — never block the transcode on it */
    }
  };
  gmnForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    resultsEl.textContent = "Transcoding…";
    try {
      await ensureGmn();
      renderLegend();
      const gmn1 = wasmToGmn1(inputEl.value, "turtle");
      const back = wasmFromGmn1(gmn1);
      resultsEl.replaceChildren();
      const h1 = document.createElement("p");
      h1.textContent = "GMN-1 surface:";
      const pre1 = document.createElement("pre");
      pre1.textContent = gmn1.trim();
      const h2 = document.createElement("p");
      h2.textContent = "Reads back to (canonical N-Quads):";
      const pre2 = document.createElement("pre");
      pre2.textContent = back.trim();
      resultsEl.append(h1, pre1, h2, pre2);
    } catch (e) {
      resultsEl.textContent = `Transcode failed: ${e.message ?? e}`;
    }
  });
}

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
  await ensureEngine();

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
  // The Clipboard API is undefined in non-secure contexts (file://, plain http) —
  // exactly where an offline docs bundle is opened. `navigator.clipboard?.writeText`
  // would then short-circuit to `undefined` and calling `.then()` on it would throw,
  // so guard explicitly and fall through to showing the serialization inline.
  if (!navigator.clipboard) {
    showSerialization(text);
    setStatus(`Clipboard unavailable; ${label} shown below.`);
    return;
  }
  navigator.clipboard.writeText(text).then(
    () => setStatus(`Copied as ${label}.`),
    () => {
      showSerialization(text);
      setStatus(`Clipboard unavailable; ${label} shown below.`);
    },
  );
}

function showSerialization(text) {
  const pre = document.createElement("pre");
  pre.className = "gmeow-sparql-graph";
  pre.textContent = text;
  resultsEl.append(pre);
}

function setStatus(text) {
  if (statusEl) statusEl.textContent = text;
}
