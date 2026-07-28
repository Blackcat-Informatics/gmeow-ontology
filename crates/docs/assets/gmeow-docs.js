// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The offline documentation controller.
//
// Loaded as an ES module on every interactive page. It boots the console's MCP engine and
// drives EVERY widget — the SPARQL playground, the bundle explorer, the Tier-1 validate
// buttons, the live entailment panel, the conjecture playground and the GMN transcode —
// through ONE protocol: JSON-RPC frames against the same 37-tool surface an agent talks to.
//
// # Why one engine
//
// This module used to import four separate wasm shims (`./purrdf/`, `./validate/`,
// `./reason/`, `./gmn/`), each with its own bespoke export surface, its own boot ritual and
// its own controller code path. Three of them were duplicate capability: `validate_local`,
// `reason_graph`, `encode_gmn1`/`gmn_expand`/`gmn_glyph_legend` and `convert` answer the
// same questions through the shipped agent surface. They are retired. What the reader can
// do in the browser is now, by construction, exactly what an agent can do — there is no
// second implementation to drift.
//
// purrdf is the one engine that stayed, and it stayed because it is NOT duplicate: the
// playground and the explorer query a caller-supplied graph STANDALONE, and while
// `query_local` now serves CONSTRUCT/DESCRIBE and an `input` scope, the purrdf `Dataset`
// also transcodes result graphs locally without a round trip. Its wasm parity is owned
// upstream in the sibling purrdf repo and attested here by `WITNESS.describe.nt`.
//
// # Tiering
//
// `tieredMcp()` dispatches a frame at the always-resident core image; when the engine
// answers with the typed `mcp.segment-not-loaded` signal it fetches the reasoning segment
// and replays the IDENTICAL frame. A reader who never reasons never downloads the reasoner.
// Every path is resolved relative to THIS module's URL, so it works at any site depth and
// offline (file://).

import init, { Dataset } from "./purrdf/gmeow_rdf_wasm.js";
import {
  initTiered,
  ready as mcpReady,
  tieredMcp,
} from "./mcp-core/index.mjs";

const FORMATS = [
  ["turtle", "Turtle"],
  ["ntriples", "N-Triples"],
  ["nquads", "N-Quads"],
  ["trig", "TriG"],
  ["rdfxml", "RDF/XML"],
  ["jsonld", "JSON-LD"],
];

// ── The MCP transport ───────────────────────────────────────────────────────
// One frame in, one parsed tool payload out. Every widget below goes through here, so
// there is exactly one place that knows the wire shape, one place that boots the engine,
// and one place that routes a deferral to the reasoning segment.

let _mcpReady = null;
let _frameId = 0;

/** Boot the core engine over the site's gmeow.gts and retain it for segment loads. */
async function ensureMcp() {
  if (!_mcpReady) {
    _mcpReady = (async () => {
      await mcpReady();
      const bundle = new Uint8Array(
        await (await fetch(fullBundleUrl())).arrayBuffer(),
      );
      initTiered(bundle);
    })();
  }
  await _mcpReady;
}

/** The `loadSegment` callback: fetch and boot a demand-loaded segment by wire name. */
async function loadSegment(segment) {
  if (segment !== "reasoning") {
    throw new Error(`unknown engine segment \`${segment}\` — nothing to load`);
  }
  return import("./mcp/index.mjs");
}

/**
 * Call one MCP tool and return its parsed payload.
 *
 * A tool failure arrives IN the envelope (`isError` + an `ok:false` payload), exactly as it
 * does for an agent; this throws on it so each widget's catch renders the engine's own
 * message rather than a paraphrase.
 */
async function callTool(name, args, onSegmentLoad) {
  await ensureMcp();
  _frameId += 1;
  const frame = JSON.stringify({
    jsonrpc: "2.0",
    id: _frameId,
    method: "tools/call",
    params: { name, arguments: args },
  });
  const response = await tieredMcp(frame, { loadSegment, onSegmentLoad });
  const parsed = JSON.parse(response);
  if (parsed.error) {
    throw new Error(`${name}: ${parsed.error.message ?? "protocol error"}`);
  }
  const text = parsed.result?.content?.[0]?.text;
  if (typeof text !== "string") {
    throw new Error(`${name}: the tool envelope carried no text content`);
  }
  const payload = JSON.parse(text);
  if (payload.ok === false) {
    throw new Error(payload.error ?? `${name} failed`);
  }
  return payload;
}
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
  if (expected === undefined) {
    throw new Error(
      "core bundle integrity: manifest is missing the assets/gmeow-core.nq entry — " +
        "cannot verify the bundle byte length (a missing manifest entry is a hard failure, not a bypass)",
    );
  }
  if (actual !== expected) {
    throw new Error(
      `core bundle integrity: expected ${expected} bytes, got ${actual}`,
    );
  }
  return Dataset.parse(nq, "nquads");
}

/** The URL of the full `gmeow.gts` bundle, resolved relative to this module — the ONE place
 * the snapshot's site path is spelled. `ensureMcp` boots the engine over it; the Tier-1
 * validate surface no longer needs it separately, because `validate_local` reads the shapes
 * out of the bundle the engine already holds. */
export function fullBundleUrl() {
  return new URL("./gmeow.gts", import.meta.url);
}

// ── Live Tier-1 validation (W1) ─────────────────────────────────────────────
// Each counter-example fixture on a term page ships a "run validation" button carrying the
// fixture's base64-encoded Turtle. On click we run the REAL Tier-1 validator through the
// MCP `validate_local` tool — the SAME validator core the on-gate authority runs, reached
// through the same protocol an agent uses — and render the unified-diagnostics findings,
// each linking through its helpUri into the constraint catalog.
//
// This used to boot a second wasm image (`gmeow-validate-wasm`) and hand it the whole
// gmeow.gts to re-read on every call. The engine already holds the bundle.

/** Run Tier-1 conformance of `turtle` against the bundle shapes, returning the
 * canonical diagnostics report `{ tool, findings: [...] }`. */
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
        await runFixtureValidation(turtle),
        btn.dataset.catalogHref,
      );
    } catch (e) {
      results.textContent = `Validation failed: ${e.message ?? e}`;
    }
  });
}


// ── Bundle explorer (W2b) ───────────────────────────────────────────────────
// Browser `gmeow info`/`describe` over the object-level core bundle: load it via the shared
// loader, show the `info` summary, and run a client-side `DESCRIBE` for the entered term.
// The DESCRIBE is the SAME the native `gmeow describe` produces (proven byte-identical by
// the F2 witness lane), so it stays on the purrdf `Dataset` — a standalone graph query with
// no bundle union, which is exactly what the explorer means.
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
// the MCP `reason_graph` tool and show the entailed closure. `reason_graph` is a
// REASONING-segment tool, so the first use of this panel is what pulls the reasoning image
// down — `onSegmentLoad` renders that as a loading state rather than a silent stall.
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
          if (phase === "loading") {
            resultsEl.textContent = "Fetching the reasoning engine…";
          }
        },
      );
      const closure = out.closure_nquads ?? "";
      resultsEl.replaceChildren();
      const h = document.createElement("p");
      const n = out.entailed_count ?? 0;
      h.textContent =
        n === 0 ? "No new entailments." : `Entailed ${n} triple${n === 1 ? "" : "s"}:`;
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

  // Verify the shipped curated demo library is byte-intact against the manifest — the
  // same integrity discipline `loadCoreBundle` applies (a missing manifest entry or a
  // byte-length mismatch is a HARD FAILURE, never a silent bypass).
  const verifyLibrary = async () => {
    const manifest = await (
      await fetch(new URL("./bundle-manifest.json", import.meta.url))
    ).json();
    const ttl = await (
      await fetch(new URL("./conjectures.ttl", import.meta.url))
    ).text();
    const expected = manifest["assets/conjectures.ttl"]?.bytes;
    const actual = new TextEncoder().encode(ttl).length;
    if (expected === undefined) {
      throw new Error(
        "conjecture library integrity: manifest is missing the assets/conjectures.ttl " +
          "entry — cannot verify the demo library (a missing manifest entry is a hard failure)",
      );
    }
    if (actual !== expected) {
      throw new Error(
        `conjecture library integrity: expected ${expected} bytes, got ${actual}`,
      );
    }
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
      const demo = DEMOS.find((d) => d.id === selectEl.value) ?? DEMOS[0];
      // `conjecture_test` is a REASONING-segment tool: the same symmetric engine, reached
      // through the agent protocol. Its `judgment_nquads` is the deterministic N-Triples
      // verdict projection the panel already knows how to read.
      const out = await callTool(
        "conjecture_test",
        { formula: demo.formula, kb: demo.kb, standpoint: STANDPOINT },
        ({ phase }) => {
          if (phase === "loading") {
            resultsEl.textContent = "Fetching the reasoning engine…";
          }
        },
      );
      const nt = out.judgment_nquads ?? "";
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

// ── GMN-1 transcode (W3) ────────────────────────────────────────────────────
// Transcode authored RDF into the token-compact GMN-1 surface and back, entirely
// in-browser, through the MCP codec tools: `encode_gmn1` is the forward leg and
// `gmn_expand` reads it back to canonical N-Quads. That pair is the SAME round trip the
// native↔wasm witness pins byte-for-byte, and both carry an internal round-trip witness of
// their own, so a lossy leg is a hard error rather than a rendered answer.
//
// `encode_gmn1` did not exist when this panel was written — the four GMN tools all CONSUMED
// the notation, so the forward direction had no expression on the agent surface and this
// widget needed its own wasm image to reach it. It is a tool now, and the widget is a
// caller like any other.
const gmnForm = document.getElementById("gmeow-gmn-form");
if (gmnForm) {
  const inputEl = document.getElementById("gmeow-gmn-input");
  const legendEl = document.getElementById("gmeow-gmn-legend");
  const resultsEl = document.getElementById("gmeow-gmn-results");
  // Render the glyph legend once (deterministic, from the bundle-carried codebook).
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
      const encoded = await callTool("encode_gmn1", {
        data: inputEl.value,
        format: "turtle",
      });
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
