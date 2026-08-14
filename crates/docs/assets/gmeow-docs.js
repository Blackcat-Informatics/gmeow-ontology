// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The offline documentation SPARQL playground controller.
//
// Loaded as an ES module only on the playground page. It boots the committed query
// wasm engine, parses the bundled TriG asset once, runs the reader's SPARQL query
// entirely in-browser (no server, no network), and renders the result — a table for
// SELECT/ASK, and a graph with "copy as <format>" transcoding for CONSTRUCT/DESCRIBE.
//
// Every path is resolved relative to THIS module's URL, so it works at any site depth
// and offline (file://): the query engine's bindings sit in ./query/, and the
// playground's own RDF asset is ./playground.trig.

import init, { Dataset, blake3Hex } from "./query/gmeow_query_wasm.js";
import validateInit, { validate as wasmValidate } from "./validate/gmeow_validate_wasm.js";
import reasonInit, {
  reason as wasmReason,
  conjecture as wasmConjecture,
} from "./reason/gmeow_reason_wasm.js";
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
// surface invents a second fetch/parse path. It boots the engine once, obtains the
// SHIPPED `gmeow.gts` bundle through the single verified fetch in `fullBundleBytes`
// (byte length AND the manifest's blake3 content address), and returns the dataset
// read from it.
//
// It reads the BUNDLE, not a flattened `gmeow-core.nq` extract: flattening collapses
// the named-graph structure and destroys the RDF 1.2 statement layer, so a browser
// query could not reach either. Reading the bundle keeps every named graph and every
// quoted triple addressable — information is trimmed only at exit gates, and the
// playground IS the exit gate.
let _engineReady = null;
async function ensureEngine() {
  if (!_engineReady) {
    _engineReady = init({
      module_or_path: new URL("./query/gmeow_query_wasm_bg.wasm", import.meta.url),
    });
  }
  await _engineReady;
}

export async function loadCoreBundle() {
  return Dataset.fromGts(await fullBundleBytes());
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
    _validatorReady = validateInit({
      module_or_path: new URL("./validate/gmeow_validate_wasm_bg.wasm", import.meta.url),
    });
  }
  await _validatorReady;
}

// ONE manifest fetch and ONE integrity rule, shared by every verified asset.
//
// Each promise is cached BEFORE the first await, so concurrent callers share one
// request: caching resolved values afterwards let every caller that arrived during the
// flight start its own, and the bundle is ~45 MB. The playground, the bundle explorer,
// the Tier-1 validation panels and the conjecture library all reach their bytes through
// here, so no surface invents a second fetch, a second parse, or a second integrity rule.
//
// Integrity is the manifest's own blake3 content address, not a byte-length comparison:
// a length check accepts any same-length substitution, which is the substitution worth
// worrying about. The engine is booted first because it supplies the hash.
let _manifest = null;
function bundleManifest() {
  if (!_manifest) {
    _manifest = fetch(new URL("./bundle-manifest.json", import.meta.url)).then((r) =>
      r.json(),
    );
  }
  return _manifest;
}

async function verifiedAssetBytes(assetPath, url) {
  await ensureEngine();
  const entry = (await bundleManifest())[assetPath];
  if (entry === undefined) {
    throw new Error(
      `asset integrity: manifest is missing the ${assetPath} entry — ` +
        "cannot verify the asset (a missing manifest entry is a hard failure, not a bypass)",
    );
  }
  const bytes = new Uint8Array(await (await fetch(url)).arrayBuffer());
  if (bytes.length !== entry.bytes) {
    throw new Error(
      `asset integrity: ${assetPath} expected ${entry.bytes} bytes, got ${bytes.length}`,
    );
  }
  if (typeof entry.blake3 !== "string") {
    throw new Error(
      `asset integrity: manifest records no blake3 content address for ${assetPath} — ` +
        "refusing to accept the asset on byte length alone",
    );
  }
  const actual = `blake3:${blake3Hex(bytes)}`;
  if (actual !== entry.blake3) {
    throw new Error(
      `asset integrity: ${assetPath} expected ${entry.blake3}, got ${actual} — the ` +
        "fetched asset is not the one this site was built from",
    );
  }
  return bytes;
}

let _bundleBytes = null;
async function fullBundleBytes() {
  if (!_bundleBytes) {
    _bundleBytes = verifiedAssetBytes("assets/gmeow.gts", fullBundleUrl());
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

// ── Shared reasoner single-flight ───────────────────────────────────────────
// The vendored gmeow-reason-wasm engine backs BOTH the live entailment panel (W4b)
// and the conjecture playground (W4). Boot it exactly once, sharing one instantiation
// promise across both surfaces (reuse `_reasonReady`, never a second wasm fetch).
let _reasonReady = null;
const ensureReasoner = async () => {
  if (!_reasonReady) {
    _reasonReady = reasonInit({
      module_or_path: new URL("./reason/gmeow_reason_wasm_bg.wasm", import.meta.url),
    });
  }
  await _reasonReady;
};

// ── Live entailment panel (W4b) ─────────────────────────────────────────────
// Run the native GMEOW structured-DL reasoner (gmeow-reason-wasm) over pasted RDF
// entirely in-browser and show the inference diff (the entailed triples). The wasm
// chase is byte-identical to the native one (proven by the F3 witness lane).
const reasonForm = document.getElementById("gmeow-reason-form");
if (reasonForm) {
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

// ── Conjecture playground (W4) ──────────────────────────────────────────────
// Run the native GMEOW SYMMETRIC conjecture engine (gmeow-reason-wasm's `conjecture`
// export) entirely in-browser and render BOTH legs of the test: the proof leg
// (KB ⊨ φ), the counterproof leg (KB ∪ {φ} ⊨ ⊥) with its contradiction witness, and
// the Belnap classification. The wasm verdict is byte-identical to the native one
// (proven by the W4 conjecture witness lane). On boot the controller fetches +
// byte-verifies the curated demo library against the bundle manifest (a truncated or
// swapped asset is rejected), then offers a set of runnable demos exercising every
// Belnap-to-lifecycle branch.
const conjectureForm = document.getElementById("gmeow-conjecture-form");
if (conjectureForm) {
  const LOGIC_PREFIX =
    "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n" +
    "@prefix ex:  <http://ex/> .\n" +
    "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n";
  const STANDPOINT =
    "https://blackcatinformatics.ca/gmeow/examples/conjecture/demo-standpoint";
  // A reified ground-atom candidate `ex:a rdf:type ex:B`.
  const GROUND_ATOM =
    LOGIC_PREFIX +
    "ex:phi a logic:Formula ;\n" +
    "    logic:relation rdf:type ;\n" +
    "    logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;\n" +
    "    logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\n";
  // A universally-quantified Horn candidate `∀x. trigger(x, mark) → rdf:type(x, B)`.
  const FORALL_HORN =
    LOGIC_PREFIX +
    "ex:cand a logic:Formula ;\n" +
    "    logic:forall ex:body ;\n" +
    '    logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "x" ] .\n' +
    "ex:body a logic:Formula ;\n" +
    "    logic:antecedent ex:ant ;\n" +
    "    logic:consequent ex:con .\n" +
    "ex:ant a logic:Formula ;\n" +
    "    logic:relation ex:trigger ;\n" +
    '    logic:argument [ logic:termIndex 0 ; logic:termVariable "x" ] ;\n' +
    "    logic:argument [ logic:termIndex 1 ; logic:termIri ex:mark ] .\n" +
    "ex:con a logic:Formula ;\n" +
    "    logic:relation rdf:type ;\n" +
    '    logic:argument [ logic:termIndex 0 ; logic:termVariable "x" ] ;\n' +
    "    logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\n";
  // Each demo is a self-contained (formula, KB, standpoint) triple that decisively
  // exercises one Belnap-to-lifecycle branch — so the symmetric legs are observable.
  const DEMOS = [
    {
      id: "corroborated",
      label: "Corroborated — the proof leg fires (KB ⊨ φ)",
      formula: GROUND_ATOM,
      kb:
        "@prefix ex:  <http://ex/> .\n" +
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n" +
        "ex:a rdf:type ex:B .\n",
    },
    {
      id: "refuted",
      label: "Refuted-in-standpoint — the counterproof leg fires (KB ∪ {φ} ⊨ ⊥), with witness",
      formula: FORALL_HORN,
      kb:
        "@prefix ex:  <http://ex/> .\n" +
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n" +
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n" +
        "ex:a ex:trigger ex:mark .\n" +
        "ex:a rdf:type ex:A .\n" +
        "ex:A owl:disjointWith ex:B .\n",
    },
    {
      id: "open",
      label: "Open — neither leg fires (no proof, no counterproof)",
      formula: FORALL_HORN,
      kb:
        "@prefix ex:  <http://ex/> .\n" +
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n" +
        "ex:a ex:trigger ex:mark .\n" +
        "ex:a rdf:type ex:A .\n",
    },
  ];

  const statusEl = document.getElementById("gmeow-conjecture-status");
  const selectEl = document.getElementById("gmeow-conjecture-select");
  const resultsEl = document.getElementById("gmeow-conjecture-results");
  for (const demo of DEMOS) {
    const opt = document.createElement("option");
    opt.value = demo.id;
    opt.textContent = demo.label;
    selectEl.append(opt);
  }

  // Verify the shipped curated demo library through the SAME verified-fetch helper the
  // bundle rides (a missing manifest entry, a byte-length mismatch, or a content-address
  // mismatch is a HARD FAILURE, never a silent bypass).
  const verifyLibrary = async () => {
    await verifiedAssetBytes(
      "assets/conjectures.ttl",
      new URL("./conjectures.ttl", import.meta.url),
    );
  };

  // Parse the deterministic verdict N-Triples for the facets the panel renders: the
  // lifecycle, the Belnap information state (which decides the two legs), and the
  // contradiction-witness premises (present exactly for a refutation).
  const parseVerdict = (nt) => {
    const local = (iri) => iri.slice(iri.lastIndexOf("/") + 1);
    let lifecycle = null;
    let information = null;
    const premises = [];
    for (const line of nt.split("\n")) {
      const m = line.match(/<([^>]*)>\s+<([^>]*)>\s+(.*)\s\.\s*$/);
      if (!m) continue;
      const [, , pred, obj] = m;
      if (pred.endsWith("/conjectureLifecycleState")) {
        lifecycle = local(obj.replace(/[<>]/g, ""));
      } else if (pred.endsWith("/resultInformation")) {
        information = local(obj.replace(/[<>]/g, ""));
      } else if (pred.endsWith("/witnessPremise")) {
        const lit = obj.match(/^"((?:[^"\\]|\\.)*)"/);
        if (lit) premises.push(lit[1]);
      }
    }
    // The symmetric legs, read off the Belnap information state.
    const hasProof = information === "InfoSupported" || information === "InfoBoth";
    const hasCounterproof = information === "InfoOpposed" || information === "InfoBoth";
    return { lifecycle, information, premises, hasProof, hasCounterproof };
  };

  const leg = (ok) => (ok ? "✓ holds" : "✗ does not hold");

  verifyLibrary().then(
    () => {
      statusEl.textContent =
        "Ready — pick a curated demo and press Test to run the symmetric engine.";
    },
    (e) => {
      statusEl.textContent = `Conjecture library failed to load: ${e.message ?? e}`;
    },
  );

  conjectureForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    resultsEl.textContent = "Testing…";
    try {
      await ensureReasoner();
      const demo = DEMOS.find((d) => d.id === selectEl.value) ?? DEMOS[0];
      const nt = wasmConjecture(demo.kb, "turtle", demo.formula, STANDPOINT);
      const v = parseVerdict(nt);
      resultsEl.replaceChildren();

      const verdict = document.createElement("p");
      verdict.className = "gmeow-conjecture-verdict";
      verdict.textContent = `Lifecycle: ${v.lifecycle ?? "unknown"} · Belnap: ${
        v.information ?? "unknown"
      }`;

      const legs = document.createElement("ul");
      const proof = document.createElement("li");
      proof.textContent = `Proof leg (KB ⊨ φ): ${leg(v.hasProof)}`;
      const counter = document.createElement("li");
      counter.textContent = `Counterproof leg (KB ∪ {φ} ⊨ ⊥): ${leg(
        v.hasCounterproof,
      )}`;
      legs.append(proof, counter);
      resultsEl.append(verdict, legs);

      if (v.premises.length) {
        const wh = document.createElement("p");
        wh.textContent = "Contradiction witness — the jointly-inconsistent premises:";
        const wl = document.createElement("ul");
        for (const p of v.premises.slice().sort()) {
          const li = document.createElement("li");
          const code = document.createElement("code");
          code.textContent = p;
          li.append(code);
          wl.append(li);
        }
        resultsEl.append(wh, wl);
      }

      const h = document.createElement("p");
      h.textContent = "Verdict (deterministic N-Triples projection):";
      const pre = document.createElement("pre");
      pre.textContent = nt.trim();
      resultsEl.append(h, pre);
    } catch (e) {
      resultsEl.textContent = `Conjecture test failed: ${e.message ?? e}`;
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
      _gmnReady = gmnInit({
        module_or_path: new URL("./gmn/gmeow_gmn_wasm_bg.wasm", import.meta.url),
      });
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
  const trig = new TextDecoder().decode(
    await verifiedAssetBytes(
      "assets/playground.trig",
      new URL("./playground.trig", import.meta.url),
    ),
  );
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
