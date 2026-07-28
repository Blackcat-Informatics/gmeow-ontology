// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The ONE browser transport to the shipped MCP engine, plus the pure readers every
// caller derives its surface from.
//
// This module is DOM-free and Node-importable: it touches no `document`, no `window`,
// and no `navigator`. Three consumers share it — the documentation controller
// (`assets/docs-controller.mjs`), the standalone console's engine worker
// (`console/engine.worker.mjs`), and the Node acceptance lanes
// (`console/tests/*.test.mjs`) — so the wire shape, the boot ritual and the segment
// routing are written once.
//
// # Tiering
//
// `tieredMcp()` (shipped by `mcp-core/index.mjs`, NOT reimplemented here) dispatches a
// frame at the always-resident core image; when the engine answers with the typed
// `mcp.segment-not-loaded` signal it fetches the reasoning segment and replays the
// IDENTICAL frame. A reader who never reasons never downloads the reasoner. Every path
// is resolved relative to THIS module's URL, so it works at any site depth and offline
// (file://).
//
// # Derivation, not restatement
//
// The readers below (`actionPolicyPanes`, `conjectureLibrary`) are PURE functions of
// bytes the engine itself served. There is no hard-coded tool list, no exclusion list
// and no hard-coded demo library anywhere in the console's JavaScript: what a surface
// offers is read back out of the shipped ontology at run time.

import { blake3Hex } from "./blake3.mjs";
import { initTiered, ready as mcpReady, tieredMcp } from "./mcp-core/index.mjs";

// ── Host configuration ──────────────────────────────────────────────────────
// Two seams, both with a browser default: where the sibling engine assets live, and
// how bytes are fetched. Node has no `fetch` for `file:` URLs, so the acceptance lanes
// inject a reader; a browser never touches this.

let _assetBase = new URL("./", import.meta.url);
let _fetchBytes = async (url) => {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`${url}: HTTP ${response.status} ${response.statusText}`);
  }
  return new Uint8Array(await response.arrayBuffer());
};

/**
 * Point the transport at a different `assets/` base and/or byte reader.
 *
 * A HARD requirement of the no-optionality rule rides here: `fetchBytes` must REJECT on
 * a missing or short read. It must never resolve with a placeholder — a missing asset is
 * a visible error, never a degraded boot.
 */
export function configure({ assetBase, fetchBytes } = {}) {
  if (assetBase !== undefined) _assetBase = new URL(assetBase);
  if (fetchBytes !== undefined) _fetchBytes = fetchBytes;
}

/** The URL of a sibling asset under the configured `assets/` base. */
export function assetUrl(relative) {
  return new URL(relative, _assetBase);
}

/** The URL of the full `gmeow.gts` bundle — the ONE place the snapshot's path is spelled. */
export function fullBundleUrl() {
  return assetUrl("./gmeow.gts");
}

// ── Integrity ───────────────────────────────────────────────────────────────
// Every integrity-pinned site sub-asset is fetched through ONE verified reader, so no
// surface can invent a second, unchecked fetch path. The renderer emits
// `assets/bundle-manifest.json` as a pure function of the asset bytes — one
// `{ "blake3": "blake3:<hex>", "bytes": n }` entry per pinned asset — and the client
// recomputes BOTH.
//
// The byte length alone is not integrity: it rejects a truncated asset and accepts a
// same-length substitution, which is the only substitution an attacker who can rewrite the
// asset would bother to make. BLAKE3 is the project's content address (the Rust side
// produces every one of these digests with the `blake3` crate), so `./blake3.mjs`
// recomputes exactly that function in the browser.

let _manifest = null;

/**
 * The emitted browser-bundle integrity manifest, fetched once.
 *
 * The in-flight PROMISE is cached so concurrent verifications share one fetch. A manifest
 * that cannot be read is a HARD FAILURE: without it nothing downstream can be verified, and
 * "no manifest" must never degrade to "skip the check".
 */
export async function bundleManifest() {
  if (_manifest === null) {
    _manifest = (async () => {
      const url = assetUrl("./bundle-manifest.json");
      let bytes;
      try {
        bytes = await _fetchBytes(url);
      } catch (cause) {
        throw new Error(
          `integrity: the bundle manifest ${url} could not be loaded, so no site asset can ` +
            `be verified: ${cause?.message ?? cause}`,
          { cause },
        );
      }
      return JSON.parse(new TextDecoder().decode(bytes));
    })().catch((error) => {
      _manifest = null;
      throw error;
    });
  }
  return _manifest;
}

/**
 * Fetch an integrity-pinned site sub-asset and verify it against the manifest.
 *
 * A missing manifest entry, a byte-length mismatch, or a BLAKE3 mismatch is a HARD FAILURE
 * naming the asset and both digests — never a silent bypass.
 */
export async function verifiedAssetBytes(sitePath, relative) {
  const manifest = await bundleManifest();
  const entry = manifest[sitePath];
  if (entry === undefined) {
    throw new Error(
      `integrity: the manifest is missing the ${sitePath} entry — cannot verify the asset ` +
        "(a missing manifest entry is a hard failure, not a bypass)",
    );
  }
  const bytes = await _fetchBytes(assetUrl(relative));
  if (bytes.length !== entry.bytes) {
    throw new Error(`integrity: ${sitePath} expected ${entry.bytes} bytes, got ${bytes.length}`);
  }
  const digest = `blake3:${blake3Hex(bytes)}`;
  if (digest !== entry.blake3) {
    throw new Error(`integrity: ${sitePath} expected ${entry.blake3}, computed ${digest}`);
  }
  return bytes;
}

/** [`verifiedAssetBytes`] decoded as UTF-8 text. */
export async function verifiedAssetText(sitePath, relative) {
  return new TextDecoder().decode(await verifiedAssetBytes(sitePath, relative));
}

// ── Boot ────────────────────────────────────────────────────────────────────

let _mcpReady = null;
let _frameId = 0;

/**
 * Boot the core engine over the site's `gmeow.gts` and retain it for segment loads.
 *
 * The in-flight PROMISE is cached (not a post-resolution flag) so two concurrent callers
 * share one instantiation. On failure the cache is cleared and the rejection propagates
 * NAMING the asset that could not be loaded — a boot that cannot reach its engine is a
 * hard error, never a quietly inert surface.
 *
 * The snapshot is read through [`verifiedAssetBytes`], so the bytes the engine is
 * initialized over are the bytes the site's own manifest pins. It is the LARGEST and most
 * consequential asset the client fetches — the whole ontology the engine then answers
 * from — so booting it unverified was the one place a swapped asset would have gone
 * unnoticed while every smaller sub-asset was checked.
 */
export function ensureMcp() {
  if (_mcpReady === null) {
    _mcpReady = (async () => {
      await mcpReady();
      const url = fullBundleUrl();
      let bundle;
      try {
        bundle = await verifiedAssetBytes("assets/gmeow.gts", "./gmeow.gts");
      } catch (cause) {
        throw new Error(`engine asset ${url} could not be loaded: ${cause?.message ?? cause}`, {
          cause,
        });
      }
      if (!(bundle instanceof Uint8Array) || bundle.length === 0) {
        throw new Error(`engine asset ${url} loaded as ${bundle?.length ?? 0} bytes — refusing to boot`);
      }
      initTiered(bundle);
    })().catch((error) => {
      _mcpReady = null;
      throw error;
    });
  }
  return _mcpReady;
}

/** The `loadSegment` callback: fetch and boot a demand-loaded segment by wire name. */
async function loadSegment(segment) {
  if (segment !== "reasoning") {
    throw new Error(`unknown engine segment \`${segment}\` — nothing to load`);
  }
  return import("./mcp/index.mjs");
}

/** Dispatch one raw JSON-RPC frame and return the raw response frame string. */
export async function dispatch(frame, onSegmentLoad) {
  await ensureMcp();
  return tieredMcp(frame, { loadSegment, onSegmentLoad });
}

/**
 * Call one MCP tool and return its parsed payload.
 *
 * A tool FAILURE is the envelope's `isError` flag, and only that. The payload's own `ok`
 * field is NOT a failure signal: `validate_local` sets `ok: false` to mean "this document
 * has an Error-severity finding", which is a successful validation with a negative verdict
 * — the answer a counter-example fixture exists to produce. Treating that as a thrown
 * failure is what made the Tier-1 "run validation" buttons report `validate_local failed`
 * instead of rendering the findings they were pointing at.
 *
 * A genuine failure throws with the ENGINE's own message, so each caller's catch renders
 * that rather than a paraphrase.
 */
export async function callTool(name, args, onSegmentLoad) {
  _frameId += 1;
  const frame = JSON.stringify({
    jsonrpc: "2.0",
    id: _frameId,
    method: "tools/call",
    params: { name, arguments: args ?? {} },
  });
  const response = await dispatch(frame, onSegmentLoad);
  const parsed = JSON.parse(response);
  if (parsed.error) {
    throw new Error(`${name}: ${parsed.error.message ?? "protocol error"}`);
  }
  const text = parsed.result?.content?.[0]?.text;
  if (typeof text !== "string") {
    throw new Error(`${name}: the tool envelope carried no text content`);
  }
  const payload = JSON.parse(text);
  if (parsed.result?.isError === true) {
    throw new Error(payload.error ?? `${name} failed`);
  }
  return payload;
}

/**
 * Query the SHIPPED bundle — the ONE place the bundle-scope query frame is spelled.
 *
 * Every browser surface that asks the ontology a question (the SPARQL playground, the
 * bundle explorer, a term page's export link) goes through here, so "what does the site
 * query?" has exactly one answer: the `gmeow.gts` bytes [`ensureMcp`] booted the engine
 * over. There is no second client-side dataset and no second parser.
 *
 * `scope: "bundle"` reads the signed canon. The overlay arguments are the tool's
 * REQUIRED external-annex pair and are passed EMPTY on purpose: this surface has no local
 * annex to union in, and saying so explicitly is how the tool is told that. An empty
 * overlay contributes no quad, so the answer is the canon's alone.
 *
 * A plain (non-`GRAPH`) pattern therefore reads the bundle's DEFAULT graph — the
 * object-level ontology. The named graphs (`graph/documentation`, `graph/reasoning`,
 * `graph/diagnostics`, …) are reachable through an explicit `GRAPH` clause. That
 * distinction is load-bearing and is why the retired `playground.trig` asset answered
 * nothing: it routed EVERY statement into a named graph, so the default-graph patterns
 * the page shipped matched an empty default graph.
 *
 * # A refused query THROWS
 *
 * `query_local` reports a bad query in its PAYLOAD (`{ok: false, error}`) rather than
 * through the envelope's `isError`, so [`callTool`] — which treats only `isError` as
 * failure, deliberately, because `validate_local` uses `ok: false` for a negative verdict
 * that is nonetheless a successful call — returns it as a value. For a query that is a
 * genuine failure with nothing to render, so it is raised here. Without this a syntax
 * error arrives at a caller as a result object with no `form`, and the widget reports
 * something about an unknown result shape instead of the engine's own parse message.
 */
export async function queryBundle(sparql, onSegmentLoad) {
  const out = await callTool(
    "query_local",
    { data: "", format: "turtle", scope: "bundle", query: sparql },
    onSegmentLoad,
  );
  if (out.ok === false && typeof out.error === "string") {
    throw new Error(out.error);
  }
  return out;
}

/**
 * Transcode RDF text through the engine's own `convert` tool — the same transcode hub the
 * CLI drives, reached through the same protocol.
 *
 * The console ships no second serializer: a caller that wants a graph result in another
 * RDF syntax pipes the engine's canonical N-Quads back through the engine.
 */
export async function convertRdf(data, from, to) {
  const out = await callTool("convert", { data, from, to });
  if (typeof out.output !== "string") {
    throw new Error(`convert: ${from}→${to} returned no output text`);
  }
  return out.output;
}

/** The engine's advertised tool descriptors (`tools/list`), sorted by name. */
export async function listTools() {
  _frameId += 1;
  const frame = JSON.stringify({ jsonrpc: "2.0", id: _frameId, method: "tools/list", params: {} });
  const parsed = JSON.parse(await dispatch(frame));
  if (parsed.error) {
    throw new Error(`tools/list: ${parsed.error.message ?? "protocol error"}`);
  }
  const tools = parsed.result?.tools;
  if (!Array.isArray(tools) || tools.length === 0) {
    throw new Error("tools/list returned no tools — the engine advertises an empty surface");
  }
  return [...tools].sort((a, b) => a.name.localeCompare(b.name));
}

// ── N-Quads reader ──────────────────────────────────────────────────────────
// A term-level reader over the canonical N-Quads the engine emits (`action_policy`,
// `convert`, `reason_graph`, `conjecture_test`). It understands RDF-1.2 triple terms
// (`<<( s p o )>>`) because the session annotations and the star loss ledger both turn
// on them. Deliberately small and total: an unparseable line is REPORTED, never skipped.

const IRI_RE = /^<([^>]*)>/;
const BNODE_RE = /^(_:[^\s]+)/;

/** Read one term at `text[0..]`, returning `[term, rest]` or `null`. */
function readTerm(text) {
  const s = text.replace(/^[\s]+/, "");
  if (s.startsWith("<<(")) {
    let rest = s.slice(3);
    const parts = [];
    for (let i = 0; i < 3; i += 1) {
      const read = readTerm(rest);
      if (read === null) return null;
      parts.push(read[0]);
      rest = read[1];
    }
    rest = rest.replace(/^[\s]*\)>>/, "");
    return [{ kind: "triple", value: parts }, rest];
  }
  const iri = IRI_RE.exec(s);
  if (iri !== null) return [{ kind: "iri", value: iri[1] }, s.slice(iri[0].length)];
  const bnode = BNODE_RE.exec(s);
  if (bnode !== null) return [{ kind: "bnode", value: bnode[1] }, s.slice(bnode[0].length)];
  if (s.startsWith('"')) {
    let i = 1;
    let value = "";
    while (i < s.length) {
      const c = s[i];
      if (c === "\\") {
        const next = s[i + 1];
        value +=
          next === "n" ? "\n" : next === "t" ? "\t" : next === "r" ? "\r" : next === "\\" ? "\\" : next;
        i += 2;
        continue;
      }
      if (c === '"') break;
      value += c;
      i += 1;
    }
    let rest = s.slice(i + 1);
    let datatype = null;
    let language = null;
    if (rest.startsWith("^^")) {
      const dt = IRI_RE.exec(rest.slice(2));
      if (dt === null) return null;
      datatype = dt[1];
      rest = rest.slice(2 + dt[0].length);
    } else if (rest.startsWith("@")) {
      const tag = /^@([A-Za-z0-9-]+)/.exec(rest);
      if (tag === null) return null;
      language = tag[1];
      rest = rest.slice(tag[0].length);
    }
    return [{ kind: "literal", value, datatype, language }, rest];
  }
  return null;
}

/**
 * Parse N-Quads (or N-Triples) into `{subject, predicate, object, graph}` records.
 *
 * `graph` is `null` for the default graph. Every term is `{kind, value, …}`; a triple
 * term's `value` is the three-element component array. A line that does not parse is a
 * HARD error naming the line — a silently dropped quad would make every derived surface
 * quietly incomplete.
 */
export function parseNQuads(text) {
  const quads = [];
  for (const raw of String(text).split("\n")) {
    const line = raw.trim();
    if (line.length === 0 || line.startsWith("#")) continue;
    const s = readTerm(line);
    if (s === null) throw new Error(`n-quads: cannot read a subject term in ${JSON.stringify(raw)}`);
    const p = readTerm(s[1]);
    if (p === null) throw new Error(`n-quads: cannot read a predicate term in ${JSON.stringify(raw)}`);
    const o = readTerm(p[1]);
    if (o === null) throw new Error(`n-quads: cannot read an object term in ${JSON.stringify(raw)}`);
    const tail = o[1].replace(/^[\s]+/, "");
    let graph = null;
    if (!tail.startsWith(".")) {
      const g = readTerm(tail);
      if (g === null) throw new Error(`n-quads: cannot read a graph term in ${JSON.stringify(raw)}`);
      graph = g[0];
    }
    quads.push({ subject: s[0], predicate: p[0].value, object: o[0], graph });
  }
  return quads;
}

/** Index quads by subject IRI → predicate → array of object terms. */
export function indexBySubject(quads) {
  const index = new Map();
  for (const quad of quads) {
    if (quad.subject.kind !== "iri") continue;
    let byPredicate = index.get(quad.subject.value);
    if (byPredicate === undefined) {
      byPredicate = new Map();
      index.set(quad.subject.value, byPredicate);
    }
    const objects = byPredicate.get(quad.predicate);
    if (objects === undefined) byPredicate.set(quad.predicate, [quad.object]);
    else objects.push(quad.object);
  }
  return index;
}

// ── Vocabulary the readers below join on ────────────────────────────────────
// Full IRIs, never CURIEs: the payloads are N-Quads, which carry no prefix map.

export const RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
export const RDF_REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
export const RDFS_LABEL = "http://www.w3.org/2000/01/rdf-schema#label";
export const GMEOW_NS = "https://blackcatinformatics.ca/gmeow/";
export const LOGIC_NS = "https://blackcatinformatics.ca/logic/";
export const LOGIC_ACTION_SCHEMA = `${LOGIC_NS}ActionSchema`;
export const LOGIC_MCP_ACTION_SCHEMA = `${LOGIC_NS}McpActionSchema`;
export const LOGIC_MCP_TOOL_NAME = `${LOGIC_NS}mcpToolName`;

/**
 * Derive the READ surface from the shipped action theory.
 *
 * A tool is a pane exactly when its policy subject is asserted `logic:ActionSchema` AND
 * NOT asserted `logic:McpActionSchema` — the read/write partition the engine's own gate
 * enforces. Both clauses are evaluated literally, so a future run that materializes the
 * `McpActionSchema ⊑ ActionSchema` subclass edge into the projection still excludes the
 * governed writes.
 *
 * Returns `{panes, excluded, byTool}`: two sorted name arrays that PARTITION the policy's
 * tool names, plus the tool → schema IRI map (the `logic:instantiatesSchema` target a
 * recorded call must name — see `console/session.mjs`).
 *
 * There is no hard-coded tool name and no exclusion list here or anywhere downstream: an
 * `action_policy` that grew a 38th tool would grow a 38th pane with no code change.
 */
export function actionPolicyPanes(nquads) {
  const index = indexBySubject(parseNQuads(nquads));
  const panes = [];
  const excluded = [];
  const byTool = new Map();
  for (const [subject, byPredicate] of index) {
    const names = (byPredicate.get(LOGIC_MCP_TOOL_NAME) ?? [])
      .filter((term) => term.kind === "literal")
      .map((term) => term.value);
    if (names.length === 0) continue;
    const types = new Set(
      (byPredicate.get(RDF_TYPE) ?? []).filter((t) => t.kind === "iri").map((t) => t.value),
    );
    const isRead = types.has(LOGIC_ACTION_SCHEMA) && !types.has(LOGIC_MCP_ACTION_SCHEMA);
    for (const name of names) {
      byTool.set(name, subject);
      (isRead ? panes : excluded).push(name);
    }
  }
  if (panes.length === 0) {
    throw new Error(
      "the shipped action policy declares no read schemas — refusing to render an empty console",
    );
  }
  panes.sort();
  excluded.sort();
  return { panes, excluded, byTool };
}

// ── The curated conjecture library ──────────────────────────────────────────
// The shipped `logic:Conjecture` corpus, read back as structured records. Before this
// existed the corpus was fetched, byte-verified and then NEVER PARSED, while the demo
// selector carried three hand-written entries — so the selector could not agree with the
// library by construction. It is derived from the library now.

const CONJECTURE_TYPE = `${LOGIC_NS}Conjecture`;

/** The local name of an IRI (its last path segment) — the corpus's own rendering convention. */
export function localName(iri) {
  return String(iri).slice(String(iri).lastIndexOf("/") + 1);
}

function one(byPredicate, predicate) {
  const objects = byPredicate.get(predicate);
  return objects === undefined || objects.length === 0 ? null : objects[0];
}

function iriValue(byPredicate, predicate) {
  const term = one(byPredicate, predicate);
  return term !== null && term.kind === "iri" ? term.value : null;
}

function literalValue(byPredicate, predicate) {
  const term = one(byPredicate, predicate);
  return term !== null && term.kind === "literal" ? term.value : null;
}

/**
 * Read the curated conjecture demo library out of its N-Quads projection.
 *
 * The input is the corpus transcoded by the engine's OWN `convert` tool (Turtle →
 * N-Quads), so the console ships no second RDF parser and the library it renders is the
 * library the engine read.
 *
 * Each record carries what the corpus actually asserts: the alpha-normalized formula
 * content key (`logic:conjectureFormula` is a KEY string, not an AST), the reified
 * standpoint, the recorded Belnap lifecycle and discharge verdict, the contradiction
 * witness premises for a refutation, the symmetric anti-conjecture obligation, and the
 * promotion candidate. `formulaAst` is the `logic:hasFormula` target when the corpus
 * links a runnable AST and `null` otherwise — a caller renders that difference rather
 * than inventing an AST.
 */
export function conjectureLibrary(nquads) {
  const index = indexBySubject(parseNQuads(nquads));
  const entries = [];
  for (const [subject, byPredicate] of index) {
    const types = (byPredicate.get(RDF_TYPE) ?? []).filter((t) => t.kind === "iri").map((t) => t.value);
    if (!types.includes(CONJECTURE_TYPE)) continue;
    const witness = iriValue(byPredicate, `${LOGIC_NS}conjectureRefutationWitness`);
    const witnessPremises =
      witness === null
        ? []
        : ((index.get(witness) ?? new Map()).get(`${LOGIC_NS}witnessPremise`) ?? [])
            .filter((t) => t.kind === "literal")
            .map((t) => t.value)
            .sort();
    entries.push({
      iri: subject,
      id: localName(subject),
      label: literalValue(byPredicate, RDFS_LABEL) ?? localName(subject),
      formulaKey: literalValue(byPredicate, `${LOGIC_NS}conjectureFormula`),
      formulaAst: iriValue(byPredicate, `${LOGIC_NS}hasFormula`),
      standpoint: iriValue(byPredicate, `${LOGIC_NS}conjectureStandpoint`),
      lifecycle: iriValue(byPredicate, `${LOGIC_NS}conjectureLifecycleState`),
      discharge: iriValue(byPredicate, `${LOGIC_NS}conjectureDischargeVerdict`),
      verdict: iriValue(byPredicate, `${LOGIC_NS}conjectureVerdict`),
      witness,
      witnessPremises,
      obligation: iriValue(byPredicate, `${LOGIC_NS}antiConjectureObligationCandidate`),
      promotion: iriValue(byPredicate, `${LOGIC_NS}candidatePromotionTarget`),
    });
  }
  if (entries.length === 0) {
    throw new Error("the shipped conjecture library declares no logic:Conjecture individuals");
  }
  entries.sort((a, b) => a.iri.localeCompare(b.iri));
  return entries;
}

/**
 * The two symmetric legs, read off a recorded Belnap lifecycle.
 *
 * `Corroborated` ⇒ the proof leg fired; `RefutedInStandpoint` ⇒ the counterproof leg
 * fired; `Open` ⇒ neither. An unrecognized state yields `null` legs rather than a guess.
 */
export function recordedLegs(lifecycle) {
  const state = lifecycle === null ? null : localName(lifecycle);
  switch (state) {
    case "ConjectureCorroborated":
      return { proof: true, counterproof: false };
    case "ConjectureRefutedInStandpoint":
      return { proof: false, counterproof: true };
    case "ConjectureOpen":
      return { proof: false, counterproof: false };
    default:
      return { proof: null, counterproof: null };
  }
}
