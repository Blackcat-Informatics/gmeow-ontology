// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The offline documentation controller.
//
// Loaded as an ES module on every interactive page. It drives EVERY widget — the SPARQL
// playground, the bundle explorer, the Tier-1 validate buttons, the live entailment panel,
// the conjecture playground and the GMN transcode — through ONE protocol: JSON-RPC frames
// against the same 37-tool surface an agent talks to.
//
// # One transport
//
// The engine boot, the frame shape and the reasoning-segment demand loading all live in
// `./mcp-transport.mjs`, which the standalone console's worker imports too. This file is
// only the page wiring: what a widget reads out of the DOM, and how it renders an answer.
//
// purrdf is the one engine that stayed alongside the MCP image, because it is NOT
// duplicate: the playground and the explorer query a caller-supplied graph STANDALONE, and
// the purrdf `Dataset` also transcodes result graphs locally without a round trip. Its
// wasm parity is owned upstream in the sibling purrdf repo and attested here by
// `WITNESS.describe.nt`.

import init, { Dataset } from "./purrdf/gmeow_rdf_wasm.js";
import {
  assetUrl,
  callTool,
  conjectureLibrary,
  localName,
  recordedLegs,
} from "./mcp-transport.mjs";

const FORMATS = [
  ["turtle", "Turtle"],
  ["ntriples", "N-Triples"],
  ["nquads", "N-Quads"],
  ["trig", "TriG"],
  ["rdfxml", "RDF/XML"],
  ["jsonld", "JSON-LD"],
];

// ── Shared browser-bundle loader ────────────────────────────────────────────
// The single client entry point every browser surface uses to obtain the queryable
// ontology — so no surface invents a second fetch/parse path. It boots the purrdf engine
// once, fetches the object-level core bundle N-Quads, verifies its byte length against the
// emitted content-address manifest (a truncated or swapped asset is rejected), and returns
// the parsed Dataset.
let _engineReady = null;
async function ensureEngine() {
  if (_engineReady === null) {
    _engineReady = init(assetUrl("./purrdf/gmeow_rdf_wasm_bg.wasm"));
  }
  await _engineReady;
}

/** The emitted browser-bundle integrity manifest. */
async function bundleManifest() {
  return (await fetch(assetUrl("./bundle-manifest.json"))).json();
}

/**
 * Fetch a site sub-asset as text and verify its byte length against the manifest.
 *
 * A missing manifest entry or a byte-length mismatch is a HARD FAILURE, never a silent
 * bypass — the same integrity discipline for every integrity-pinned sub-asset.
 */
async function verifiedAssetText(sitePath, relative) {
  const manifest = await bundleManifest();
  const text = await (await fetch(assetUrl(relative))).text();
  const expected = manifest[sitePath]?.bytes;
  const actual = new TextEncoder().encode(text).length;
  if (expected === undefined) {
    throw new Error(
      `integrity: the manifest is missing the ${sitePath} entry — cannot verify the asset ` +
        "(a missing manifest entry is a hard failure, not a bypass)",
    );
  }
  if (actual !== expected) {
    throw new Error(`integrity: ${sitePath} expected ${expected} bytes, got ${actual}`);
  }
  return text;
}

export async function loadCoreBundle() {
  await ensureEngine();
  const nq = await verifiedAssetText("assets/gmeow-core.nq", "./gmeow-core.nq");
  return Dataset.parse(nq, "nquads");
}

// ── Live Tier-1 validation (W1) ─────────────────────────────────────────────
// Each counter-example fixture on a term page ships a "run validation" button carrying the
// fixture's base64-encoded Turtle. On click we run the REAL Tier-1 validator through the
// MCP `validate_local` tool — the SAME validator core the on-gate authority runs, reached
// through the same protocol an agent uses — and render the unified-diagnostics findings,
// each linking through its helpUri into the constraint catalog.

/**
 * The activation REQUEST a `.gmeow-run-validation` control determines.
 *
 * Exported because the shell-agreement acceptance assertion compares the static site's and
 * the mdbook's controls by the request each one would dispatch: same control ⇒ same frame
 * ⇒ same answer from the same engine.
 */
export function validationRequest(button) {
  const turtle = new TextDecoder().decode(
    Uint8Array.from(atob(button.dataset.turtle), (c) => c.charCodeAt(0)),
  );
  return { tool: "validate_local", args: { data: turtle, format: "turtle" } };
}

/** Run Tier-1 conformance of `turtle` against the bundle shapes. */
export async function runFixtureValidation(turtle) {
  return callTool("validate_local", { data: turtle, format: "turtle" });
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
    // The finding `code` IS the constraint-catalog anchor (the validator's helpUri slug);
    // link into the catalog when a base href is supplied.
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
      const { args } = validationRequest(btn);
      renderFindings(results, await runFixtureValidation(args.data), btn.dataset.catalogHref);
    } catch (e) {
      results.textContent = `Validation failed: ${e.message ?? e}`;
    }
  });
}

// ── Bundle explorer (W2b) ───────────────────────────────────────────────────
// Browser `gmeow info`/`describe` over the object-level core bundle: load it via the shared
// loader, show the `info` summary, and run a client-side `DESCRIBE` for the entered term.
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
// Run the native GMEOW structured-DL reasoner over pasted RDF entirely in-browser through
// the MCP `reason_graph` tool. `reason_graph` is a REASONING-segment tool, so the first use
// of this panel is what pulls the reasoning image down — `onSegmentLoad` renders that as a
// loading state rather than a silent stall.
const reasonForm = document.getElementById("gmeow-reason-form");
if (reasonForm) {
  const inputEl = document.getElementById("gmeow-reason-input");
  const resultsEl = document.getElementById("gmeow-reason-results");
  reasonForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    resultsEl.textContent = "Reasoning…";
    try {
      const out = await callTool(
        "reason_graph",
        { data: inputEl.value, format: "turtle" },
        ({ phase }) => {
          if (phase === "loading") resultsEl.textContent = "Fetching the reasoning engine…";
        },
      );
      const closure = out.closure_nquads ?? "";
      resultsEl.replaceChildren();
      const h = document.createElement("p");
      const n = out.entailed_count ?? 0;
      h.textContent = n === 0 ? "No new entailments." : `Entailed ${n} triple${n === 1 ? "" : "s"}:`;
      const pre = document.createElement("pre");
      pre.textContent = closure.trim();
      resultsEl.append(h, pre);
      // A budget-cut closure is a SOUND UNDER-APPROXIMATION, not a small one. Saying so is
      // the difference between an honest partial answer and a misleading complete-looking
      // one, so the governor's verdict is surfaced rather than swallowed.
      if (out.evaluation && out.evaluation !== "completed") {
        const note = document.createElement("p");
        note.className = "gmeow-reason-note";
        note.textContent =
          `Evaluation: ${out.evaluation} (completeness: ${out.completeness ?? "unknown"}) — ` +
          "the chase was cut by its step governor, so this closure is a sound " +
          "under-approximation rather than the full one.";
        resultsEl.append(note);
      }
    } catch (e) {
      resultsEl.textContent = `Reasoning failed: ${e.message ?? e}`;
    }
  });
}

// ── Conjecture playground (W4) ──────────────────────────────────────────────
// The curated demo library was previously fetched, byte-verified and then NEVER PARSED,
// while the selector carried three hand-written entries — so the selector could not agree
// with the shipped six by construction. It is DERIVED now: the library is transcoded by the
// engine's own `convert` tool (so the console ships no second RDF parser) and read back
// through `conjectureLibrary`, and the selector is exactly its entries.
//
// Each entry renders what the corpus actually asserts: the alpha-normalized formula content
// key, the reified standpoint, the recorded Belnap lifecycle and its two symmetric legs, the
// contradiction-witness premises for a refutation, and the anti-conjecture obligation. The
// LIVE symmetric engine additionally runs for an entry whose corpus record links a runnable
// `logic:Formula` AST via `logic:hasFormula`; the shipped corpus links none today
// (`logic:conjectureFormula` is documented as the content KEY of the AST, not the AST), and
// the panel says so in place rather than fabricating one.
const conjectureForm = document.getElementById("gmeow-conjecture-form");
if (conjectureForm) {
  const statusEl = document.getElementById("gmeow-conjecture-status");
  const selectEl = document.getElementById("gmeow-conjecture-select");
  const resultsEl = document.getElementById("gmeow-conjecture-results");
  let library = [];

  const leg = (ok) => (ok === null ? "· not recorded" : ok ? "✓ holds" : "✗ does not hold");

  /** Load, byte-verify, transcode and parse the shipped library — the ONE derivation. */
  const loadLibrary = async () => {
    const ttl = await verifiedAssetText("assets/conjectures.ttl", "./conjectures.ttl");
    const converted = await callTool("convert", { data: ttl, from: "turtle", to: "nquads" });
    return conjectureLibrary(converted.output);
  };

  loadLibrary().then(
    (entries) => {
      library = entries;
      for (const entry of library) {
        const opt = document.createElement("option");
        opt.value = entry.id;
        opt.textContent = entry.label;
        selectEl.append(opt);
      }
      statusEl.textContent =
        `Ready — ${library.length} curated conjecture${library.length === 1 ? "" : "s"} read from ` +
        "the shipped library. Pick one and press Test.";
    },
    (e) => {
      statusEl.textContent = `Conjecture library failed to load: ${e.message ?? e}`;
    },
  );

  conjectureForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const entry = library.find((d) => d.id === selectEl.value) ?? library[0];
    if (entry === undefined) {
      resultsEl.textContent = "The conjecture library did not load; there is nothing to test.";
      return;
    }
    resultsEl.textContent = "Testing…";
    try {
      resultsEl.replaceChildren();
      const legs = recordedLegs(entry.lifecycle);
      const verdict = document.createElement("p");
      verdict.className = "gmeow-conjecture-verdict";
      verdict.textContent =
        `Lifecycle: ${localName(entry.lifecycle ?? "unknown")} · Discharge: ` +
        `${localName(entry.discharge ?? "unknown")} · Standpoint: ${localName(entry.standpoint ?? "unknown")}`;
      const legList = document.createElement("ul");
      const proof = document.createElement("li");
      proof.textContent = `Proof leg (KB ⊨ φ): ${leg(legs.proof)}`;
      const counter = document.createElement("li");
      counter.textContent = `Counterproof leg (KB ∪ {φ} ⊨ ⊥): ${leg(legs.counterproof)}`;
      legList.append(proof, counter);
      resultsEl.append(verdict, legList);

      const key = document.createElement("p");
      key.append("Formula content key: ");
      const code = document.createElement("code");
      code.textContent = entry.formulaKey ?? "(none recorded)";
      key.append(code);
      resultsEl.append(key);

      if (entry.witnessPremises.length > 0) {
        const wh = document.createElement("p");
        wh.textContent = "Contradiction witness — the jointly-inconsistent premises:";
        const wl = document.createElement("ul");
        for (const p of entry.witnessPremises) {
          const li = document.createElement("li");
          const c = document.createElement("code");
          c.textContent = p;
          li.append(c);
          wl.append(li);
        }
        resultsEl.append(wh, wl);
      }
      if (entry.obligation !== null) {
        const ob = document.createElement("p");
        ob.textContent = `Symmetric anti-conjecture obligation: ${localName(entry.obligation)}`;
        resultsEl.append(ob);
      }

      if (entry.formulaAst !== null) {
        // A runnable AST: run the SAME symmetric engine an agent reaches, through the
        // agent protocol, and render its deterministic verdict projection.
        const out = await callTool(
          "conjecture_test",
          { formula: entry.formulaAst, kb: "", standpoint: entry.standpoint },
          ({ phase }) => {
            if (phase === "loading") statusEl.textContent = "Fetching the reasoning engine…";
          },
        );
        const h = document.createElement("p");
        h.textContent = "Live verdict (deterministic N-Triples projection):";
        const pre = document.createElement("pre");
        pre.textContent = (out.judgment_nquads ?? "").trim();
        resultsEl.append(h, pre);
      } else {
        const note = document.createElement("p");
        note.className = "gmeow-conjecture-note";
        note.textContent =
          "This corpus entry records the formula's alpha-normalized content KEY " +
          "(logic:conjectureFormula), not a runnable logic:Formula AST, and links none via " +
          "logic:hasFormula — so the recorded verdict above is what the library asserts. Use " +
          "the standalone console's conjecture_test pane to run the symmetric engine over an " +
          "AST of your own.";
        resultsEl.append(note);
      }
    } catch (e) {
      resultsEl.textContent = `Conjecture test failed: ${e.message ?? e}`;
    }
  });
}

// ── GMN-1 transcode (W3) ────────────────────────────────────────────────────
// Transcode authored RDF into the token-compact GMN-1 surface and back, entirely
// in-browser, through the MCP codec tools: `encode_gmn1` is the forward leg and
// `gmn_expand` reads it back to canonical N-Quads.
const gmnForm = document.getElementById("gmeow-gmn-form");
if (gmnForm) {
  const inputEl = document.getElementById("gmeow-gmn-input");
  const legendEl = document.getElementById("gmeow-gmn-legend");
  const resultsEl = document.getElementById("gmeow-gmn-results");
  const renderLegend = async () => {
    if (!legendEl || legendEl.childElementCount) return;
    try {
      const { legend } = await callTool("gmn_glyph_legend", {});
      const entries = typeof legend === "string" ? JSON.parse(legend) : legend;
      if (!entries?.length) return;
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
      await renderLegend();
      const encoded = await callTool("encode_gmn1", { data: inputEl.value, format: "turtle" });
      const expanded = await callTool("gmn_expand", { gmn: encoded.gmn1 });
      resultsEl.replaceChildren();
      const h1 = document.createElement("p");
      h1.textContent = "GMN-1 surface:";
      const pre1 = document.createElement("pre");
      pre1.textContent = (encoded.gmn1 ?? "").trim();
      const h2 = document.createElement("p");
      h2.textContent = "Reads back to (canonical N-Quads):";
      const pre2 = document.createElement("pre");
      pre2.textContent = (expanded.expanded_nquads ?? "").trim();
      resultsEl.append(h1, pre1, h2, pre2);
    } catch (e) {
      resultsEl.textContent = `Transcode failed: ${e.message ?? e}`;
    }
  });
}

// ── SPARQL playground ───────────────────────────────────────────────────────
const form = document.getElementById("gmeow-sparql");
const queryEl = document.getElementById("gmeow-sparql-query");
const statusEl = document.getElementById("gmeow-sparql-status");
const resultsEl = document.getElementById("gmeow-sparql-results");

let dataset = null;

// Only activate on the playground page.
if (form && queryEl && resultsEl) {
  main().catch((err) => setStatus(`Failed to start the engine: ${err.message ?? err}`));
}

async function main() {
  setStatus("Loading the query engine…");
  await ensureEngine();

  setStatus("Loading the ontology…");
  const trig = await (await fetch(assetUrl("./playground.trig"))).text();
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
  // SELECT/ASK return SPARQL Results JSON; CONSTRUCT/DESCRIBE return Turtle (which is not
  // JSON, so the parse throws and we fall through to the graph branch).
  let json = null;
  try {
    json = JSON.parse(out);
  } catch {
    json = null;
  }
  if (json && (json.results || typeof json.boolean === "boolean")) renderSolutions(json);
  else renderGraph(out);
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
  // The Clipboard API is undefined in non-secure contexts (file://, plain http) — exactly
  // where an offline docs bundle is opened. `navigator.clipboard?.writeText` would then
  // short-circuit to `undefined` and calling `.then()` on it would throw, so guard
  // explicitly and fall through to showing the serialization inline.
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
