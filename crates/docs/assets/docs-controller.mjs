// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The offline documentation controller.
//
// Loaded as an ES module on every interactive page. It drives EVERY widget — the SPARQL
// playground, the bundle explorer, the Tier-1 validate buttons, the live entailment panel,
// the conjecture playground and the GMN transcode — through ONE protocol: JSON-RPC frames
// against the same 38-tool surface an agent talks to.
//
// # One transport
//
// The engine boot, the frame shape and the reasoning-segment demand loading all live in
// `./mcp-transport.mjs`, which the standalone console's worker imports too. This file is
// only the page wiring: what a widget reads out of the DOM, and how it renders an answer.
//
// Every widget here dispatches to exactly ONE engine. A fourth runtime (the vendored purrdf
// wasm build) had a route of its own through the first three retirements, on the claim that
// it was not duplicate — that the playground and the explorer needed a STANDALONE query over
// a caller-supplied graph. That claim did not survive measurement. The two surfaces query the
// SHIPPED ontology, not a caller's graph, and the engine already holds it: `query_local` with
// `scope: "bundle"` answers SELECT/ASK/CONSTRUCT/DESCRIBE over the same `gmeow.gts` the
// worker booted, and `convert` transcodes a result graph through the same hub the CLI drives.
//
// Retiring that route also retired the 311 MB of client-side substrate that existed only to
// feed it — and fixed the playground, whose own default query returned nothing because that
// substrate put every statement in a named graph. See `mcp-transport.mjs`'s `queryBundle`.
//
// The purrdf package is still SHIPPED at `assets/purrdf/`, and this file deliberately never
// imports it: it is there for a page that embeds the tree and wants an offline RDF/JS store
// over its own dataset, which is not a question any widget below asks.

import {
  callTool,
  conjectureLibrary,
  convertRdf,
  localName,
  queryBundle,
  recordedLegs,
  verifiedAssetText,
} from "./mcp-transport.mjs";

const FORMATS = [
  ["turtle", "Turtle"],
  ["ntriples", "N-Triples"],
  ["nquads", "N-Quads"],
  ["trig", "TriG"],
  ["rdfxml", "RDF/XML"],
  ["jsonld", "JSON-LD"],
];

// ── The object-level describe ───────────────────────────────────────────────
// The explorer's `describe <term>` and a term page's export link ask the same question, so
// it is spelled once. A bound-subject CONSTRUCT is deliberate rather than a DESCRIBE: this
// engine's DESCRIBE gathers across every named graph, while a bound-subject pattern reads
// the DEFAULT graph alone — the object-level ontology. That is the scope the explorer has
// always answered over and the scope `WITNESS.describe.nt` attests, so pinning it in the
// query keeps the surface's meaning independent of SPARQL's implementation-defined
// DESCRIBE. `crates/mcp/tests/witness_explore.rs` proves this exact query reproduces the
// committed attestation.
export function describeQuery(subject) {
  return `CONSTRUCT { ${subject} ?p ?o } WHERE { ${subject} ?p ?o }`;
}

/** A CURIE (`gmeow:Foo`) passes through; a full IRI is bracketed. */
export function subjectTerm(term) {
  return term.includes("://") ? `<${term}>` : term;
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
// Browser `gmeow info`/`describe` over the object-level ontology, answered by the engine
// the page already booted. `info` counts the default graph; `describe` runs the shared
// bound-subject CONSTRUCT. Neither fetches a dataset: the bundle the engine holds IS the
// object-level ontology, so the 27 MB N-Quads re-serialization the explorer used to parse
// client-side was a second copy of bytes already in memory.
const explorerForm = document.getElementById("gmeow-explorer-form");
if (explorerForm) {
  const infoEl = document.getElementById("gmeow-explorer-info");
  const iriEl = document.getElementById("gmeow-explorer-iri");
  const resultsEl = document.getElementById("gmeow-explorer-results");
  queryBundle("SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }")
    .then((out) => {
      const n = out.results?.bindings?.[0]?.n?.value ?? "0";
      infoEl.textContent = `info — ${n} triples in the object-level ontology.`;
    })
    .catch((e) => {
      infoEl.textContent = `Failed to reach the engine: ${e.message ?? e}`;
    });
  explorerForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const term = iriEl.value.trim();
    if (!term) return;
    resultsEl.replaceChildren();
    resultsEl.textContent = "Describing…";
    try {
      const out = await queryBundle(
        `PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\n${describeQuery(subjectTerm(term))}`,
      );
      const nquads = (out.graph_nquads ?? "").trim();
      resultsEl.replaceChildren();
      const pre = document.createElement("pre");
      pre.textContent = nquads ? nquads : "No triples describe that term.";
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

// Only activate on the playground page.
if (form && queryEl && resultsEl) {
  main().catch((err) => setStatus(`Failed to start the engine: ${err.message ?? err}`));
}

async function main() {
  setStatus("Loading the query engine…");
  // The count doubles as the readiness probe: it is a real frame answered by the real
  // engine over the real bundle, so "Ready" is an executed fact rather than a boot flag.
  const out = await queryBundle("SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }");
  const n = out.results?.bindings?.[0]?.n?.value ?? "0";
  setStatus(`Ready — ${n} object-level triples. Run a query.`);

  // Prefill from the ?q= query parameter (e.g. a term page's "DESCRIBE" link).
  const prefill = new URLSearchParams(window.location.search).get("q");
  if (prefill) queryEl.value = prefill;

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    runQuery();
  });
}

async function runQuery() {
  resultsEl.replaceChildren();
  const sparql = queryEl.value.trim();
  if (!sparql) return;
  setStatus("Running…");
  let out;
  try {
    out = await queryBundle(sparql);
  } catch (err) {
    setStatus(`Query error: ${err.message ?? err}`);
    return;
  }
  // The engine DECLARES the result form in its envelope, so the shape is read rather than
  // guessed. The old code inferred it by trying `JSON.parse` on the payload and treating a
  // throw as "this must be a graph" — which silently mislabels any future non-JSON error
  // text as a graph result.
  switch (out.form) {
    case "graph":
      renderGraph(out.graph_nquads ?? "", out.quad_count ?? 0);
      break;
    case "boolean":
    case "bindings":
      renderSolutions(out);
      break;
    default:
      setStatus(`Unknown result form \`${out.form}\` — the engine answered in a shape this page cannot render.`);
  }
}

function renderSolutions(json) {
  if (json.form === "boolean") {
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

// A graph result arrives as the engine's canonical N-Quads — the ONE lossless RDF-1.2 text
// form every tool here hands back — and the copy bar transcodes it through `convert`.
function renderGraph(nquads, quadCount) {
  setStatus(
    `Graph result — ${quadCount} quad${quadCount === 1 ? "" : "s"}. Copy in any RDF serialization:`,
  );
  const bar = document.createElement("div");
  bar.className = "gmeow-sparql-copybar";
  for (const [fmt, label] of FORMATS) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = label;
    button.addEventListener("click", () => copyAs(nquads, fmt, label));
    bar.append(button);
  }
  resultsEl.append(bar);
  const pre = document.createElement("pre");
  pre.className = "gmeow-sparql-graph";
  pre.textContent = nquads;
  resultsEl.append(pre);
}

async function copyAs(nquads, fmt, label) {
  let text;
  try {
    // Transcode the result graph through the engine's own hub — no second serializer.
    text = fmt === "nquads" ? nquads : await convertRdf(nquads, "nquads", fmt);
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
