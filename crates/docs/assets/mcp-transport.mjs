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
 * Read one tool envelope's text content as the value the tool serialized.
 *
 * The MCP result contract is a TEXT content block, and what a tool puts in it is that
 * tool's business. Most serialize a JSON object, and their callers read fields off it.
 * Some serialize a DOCUMENT: the corpus tools answer with Markdown, which is a complete,
 * non-error answer — `isError` is false and the text is the deliverable. So the envelope's
 * text is read as JSON when it IS JSON, and handed back as `{ text }` when it is not.
 *
 * The alternative — parsing unconditionally — made every document-valued tool throw
 * `SyntaxError: Unexpected token '#'` on its own successful answer, so its pane rendered a
 * parse error instead of the document. `renderPayload` already falls through to a `<pre>`
 * for a shape it has no structure for, which is exactly how a document should render.
 *
 * No tool NAME appears in this decision, and none may: the console derives its pane set
 * from the shipped action policy precisely so that no JavaScript carries a tool list. A
 * name-keyed exception here would reintroduce the one thing the derivation exists to
 * remove, and would go stale the moment the ontology grew another document-valued tool.
 */
function toolPayload(text) {
  try {
    return JSON.parse(text);
  } catch {
    return { text };
  }
}

/**
 * Call one MCP tool and return its payload.
 *
 * The payload is the envelope's text content read through [`toolPayload`]: the parsed
 * object for a JSON-valued tool, `{ text }` for a document-valued one. A tool that answers
 * with prose is answering, not failing.
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
  const payload = toolPayload(text);
  if (parsed.result?.isError === true) {
    // A failing envelope's own message, whichever shape it carried it in: the `error`
    // field of a JSON payload, or the text itself when the engine reported in prose.
    throw new Error(payload.error ?? payload.text ?? `${name} failed`);
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
// on them. Deliberately small and TOTAL: it accepts exactly the N-Quads grammar's terms
// and REPORTS everything else — it never skips a line and never returns a half-read term
// as if it were whole.
//
// "Total" is a claim with teeth, and each clause below is a defect this reader once had:
//
//   * an unterminated literal (`"abc` with no closing quote) is REFUSED, not returned as a
//     literal whose value is the whole rest of the line;
//   * the FULL escape set is decoded — `\t \b \n \r \f \" \' \\` plus `UCHAR`
//     (`\uXXXX` / `\UXXXXXXXX`) — so `"A"` reads as `A`, not as the six characters
//     `u0041`; an escape that is neither is REFUSED rather than silently dropped;
//   * a triple term with no `)>>` closer is REFUSED, not silently accepted with the closer
//     "stripped" by a regex that matched nothing;
//   * an IRIREF carrying a character the grammar excludes (a space, a control character,
//     `<>"{}|^\``) is REFUSED, because reading one means the term boundaries were misread;
//   * a line that carries a fifth term, or does not terminate in `.`, is REFUSED.

/**
 * The delimiters an `IRIREF` may not carry unescaped, per the N-Quads grammar.
 *
 * The grammar's exclusion set is these eight plus every code point at or below U+0020
 * (the space and the C0 controls), which [`iriCharForbidden`] adds by value.
 */
const IRI_DELIMITERS = '<>"{}|^`\\';

/** Whether `c` may not appear unescaped inside an `IRIREF`. */
function iriCharForbidden(c) {
  return c.codePointAt(0) <= 0x20 || IRI_DELIMITERS.includes(c);
}

const BNODE_RE = /^(_:[^\s]+)/;
const HEX_RE = /^[0-9A-Fa-f]+$/;

/** The `ECHAR` table — the complete one. An escape outside it is not readable. */
const ECHAR = {
  t: "\t",
  b: "\b",
  n: "\n",
  r: "\r",
  f: "\f",
  '"': '"',
  "'": "'",
  "\\": "\\",
};

/** Read a `UCHAR` (`\uXXXX` or `\UXXXXXXXX`) at `s[i]`, or `null` if it is not one. */
function readUchar(s, i) {
  const width = s[i + 1] === "u" ? 4 : s[i + 1] === "U" ? 8 : 0;
  if (width === 0) return null;
  const digits = s.slice(i + 2, i + 2 + width);
  if (digits.length < width || !HEX_RE.test(digits)) return null;
  const code = Number.parseInt(digits, 16);
  if (code > 0x10ffff) return null;
  return { text: String.fromCodePoint(code), next: i + 2 + width };
}

/** Read one escape sequence at `s[i]` (`s[i] === "\\"`), or `null` if it is not one. */
function readEscape(s, i) {
  const next = s[i + 1];
  if (next === undefined) return null;
  if (next === "u" || next === "U") return readUchar(s, i);
  const mapped = ECHAR[next];
  return mapped === undefined ? null : { text: mapped, next: i + 2 };
}

/**
 * Read an `IRIREF` at `s[0..]`, returning `[value, rest]` or `null`.
 *
 * `UCHAR` escapes are decoded, and every other backslash — plus every character the
 * grammar excludes — is a refusal. An IRI carrying a raw space is not "an IRI with a
 * space in it": it is evidence the term boundary was read in the wrong place.
 */
function readIriRef(s) {
  if (!s.startsWith("<")) return null;
  const close = s.indexOf(">", 1);
  if (close < 0) return null;
  const raw = s.slice(1, close);
  let value = "";
  let i = 0;
  while (i < raw.length) {
    const c = raw[i];
    if (c === "\\") {
      const escape = readUchar(raw, i);
      if (escape === null) return null;
      value += escape.text;
      i = escape.next;
      continue;
    }
    if (iriCharForbidden(c)) return null;
    value += c;
    i += 1;
  }
  return [value, s.slice(close + 1)];
}

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
    // The closer is REQUIRED. A `replace` that matches nothing is not a parse.
    const closer = /^[\s]*\)>>/.exec(rest);
    if (closer === null) return null;
    return [{ kind: "triple", value: parts }, rest.slice(closer[0].length)];
  }
  const iri = readIriRef(s);
  if (iri !== null) return [{ kind: "iri", value: iri[0] }, iri[1]];
  const bnode = BNODE_RE.exec(s);
  if (bnode !== null) return [{ kind: "bnode", value: bnode[1] }, s.slice(bnode[0].length)];
  if (s.startsWith('"')) {
    let i = 1;
    let value = "";
    let closed = false;
    while (i < s.length) {
      const c = s[i];
      if (c === "\\") {
        const escape = readEscape(s, i);
        if (escape === null) return null;
        value += escape.text;
        i = escape.next;
        continue;
      }
      if (c === '"') {
        closed = true;
        i += 1;
        break;
      }
      // A raw LF or CR cannot appear inside an N-Quads literal; seeing one means the
      // quote was never closed on this line.
      if (c === "\n" || c === "\r") return null;
      value += c;
      i += 1;
    }
    if (!closed) return null;
    let rest = s.slice(i);
    let datatype = null;
    let language = null;
    if (rest.startsWith("^^")) {
      const dt = readIriRef(rest.slice(2));
      if (dt === null) return null;
      datatype = dt[0];
      rest = dt[1];
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
 *
 * The terminator is CHECKED, not assumed: a line carrying a fifth term (the shape a
 * careless re-graphing produces — `s p o oldGraph newGraph .`) is refused here rather
 * than read as a quad with the extra term thrown away.
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
    let tail = o[1].replace(/^[\s]+/, "");
    let graph = null;
    if (tail.length === 0) {
      throw new Error(
        `n-quads: ${JSON.stringify(raw)} does not end after its terms — it is missing its \`.\` terminator`,
      );
    }
    if (!tail.startsWith(".")) {
      const g = readTerm(tail);
      if (g === null) throw new Error(`n-quads: cannot read a graph term in ${JSON.stringify(raw)}`);
      if (g[0].kind !== "iri" && g[0].kind !== "bnode") {
        throw new Error(
          `n-quads: the graph term of ${JSON.stringify(raw)} is a ${g[0].kind}, which cannot name a graph`,
        );
      }
      graph = g[0];
      tail = g[1].replace(/^[\s]+/, "");
    }
    if (tail.replace(/[\s]+$/, "") !== ".") {
      throw new Error(
        `n-quads: ${JSON.stringify(raw)} does not end after its terms — it carries more than four ` +
          "terms or is missing its `.` terminator",
      );
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

// ── The derivation reader ───────────────────────────────────────────────────
//
// The console records an agentic trajectory, and a recorded answer that cannot say what it
// rests on is exactly the unattributable assertion the session's RDF-1.2 annotations exist
// to prevent. This is where an engine answer becomes those annotations.
//
// # Shape-driven, never name-driven
//
// [`derivationsFrom`] keys on the FIELDS a payload carries, never on the tool that produced
// it. That is the same rule the pane set follows (`actionPolicyPanes` derives the whole
// surface from the shipped action theory) and the same rule `toolPayload` follows, and for
// the same reason: a name-keyed table is a second, stale copy of the tool surface, and the
// console's whole design forbids one. A tool that grows a derivation-shaped answer starts
// carrying annotations with no edit here.
//
// Two shapes are recognized, because two are what the shipped engine actually returns:
//
//   * a CLOSURE (`closure_nquads` + `judgment_nquads`) — the entailed quads of a forward
//     chase, together with the content-addressed `logic:ReasoningResult` the same answer
//     minted. Every conclusion `gmeow:wasDerivedFrom` that result node, which is the
//     engine's OWN idiom for this predicate: the shipped `conjecture_test` judgment writes
//     `<conjecture> gmeow:wasDerivedFrom <…/graph/reasoning/result/…>` itself. The result
//     node is not a placeholder — it is addressed over the input validity, the evaluation,
//     the completeness, the consumed budget, the engine identity and the derived-axiom
//     rows of that exact run, so two runs that differ in any of those are different
//     antecedents.
//   * a PROOF TREE (`step_skeleton`) — the reconstructed, faithfulness-checked derivation
//     skeleton, in which each step names the steps it was derived from. Here the
//     antecedents are the premise QUADS themselves, which is the strongest form the
//     annotation takes.
//
// What is deliberately NOT read: the `entailments` surface. Its `conclusion` and `premises`
// are rendered CURIE prose (`"gmeow:ToolCall rdfs:subClassOf gmeow:Event"`), and turning
// that back into terms needs a prefix map the payload does not carry — which the console
// would have to author, making it a second source of truth for what `gmeow:` abbreviates.
// A surface that does not declare its term kinds is not one this module may guess at; that
// is the same rule `declaredTerm` enforces on the other side.

/** The `logic:` derivation vocabulary the reader joins on. */
export const LOGIC_REASONING_RESULT = `${LOGIC_NS}ReasoningResult`;
export const LOGIC_RESULT_INFORMATION = `${LOGIC_NS}resultInformation`;

/**
 * One parsed term ([`parseNQuads`]'s `{kind, value, …}`) as the DECLARED term the session
 * emitter takes.
 *
 * TOTAL over the reader's kind set — IRI, blank node, literal (with its datatype or its
 * language tag), and RDF-1.2 triple term, whose components are converted recursively. An
 * unknown kind is a HARD failure rather than a coerced literal: the whole point of a
 * declared term is that nothing downstream infers a kind, and a converter that quietly
 * flattened a triple term into a string would be the inference it exists to prevent.
 */
export function declaredFromTerm(term, where = "term") {
  switch (term?.kind) {
    case "iri":
      return { iri: term.value };
    case "bnode":
      return { bnode: term.value };
    case "literal":
      if (term.datatype !== null && term.datatype !== undefined) {
        return { literal: term.value, datatype: term.datatype };
      }
      if (term.language !== null && term.language !== undefined) {
        return { literal: term.value, language: term.language };
      }
      return { literal: term.value };
    case "triple":
      return { triple: term.value.map((part, i) => declaredFromTerm(part, `${where}[${i}]`)) };
    default:
      throw new Error(
        `derivation: ${where} is a \`${term?.kind}\` term, which is not an N-Quads term kind`,
      );
  }
}

/**
 * The content-addressed `logic:ReasoningResult` node an answer's judgment minted, plus the
 * judgment's own Belnap information statements about it.
 *
 * Exactly ONE result node is required. A judgment naming none cannot say what its closure
 * rests on, and one naming several cannot say which — both are hard failures rather than an
 * arbitrary pick, because an arbitrary pick would attribute a conclusion to a run that did
 * not produce it.
 */
function reasoningJudgment(nquads, where) {
  const quads = parseNQuads(nquads);
  const nodes = [
    ...new Set(
      quads
        .filter(
          (quad) =>
            quad.predicate === RDF_TYPE &&
            quad.subject.kind === "iri" &&
            quad.object.kind === "iri" &&
            quad.object.value === LOGIC_REASONING_RESULT,
        )
        .map((quad) => quad.subject.value),
    ),
  ];
  if (nodes.length !== 1) {
    throw new Error(
      `derivation: ${where} carries a closure but its judgment names ${nodes.length} ` +
        "logic:ReasoningResult nodes; a conclusion can only be attributed to exactly one " +
        "reasoning run, so neither none nor several may be resolved by picking",
    );
  }
  const node = nodes[0];
  // The Belnap axis is the engine's claim ABOUT THE RUN, so it is recorded with the run as
  // its subject. Re-stamping it on each conclusion would be the console inventing a
  // per-statement verdict: an `InfoBoth` answer says the closure contains a contradiction,
  // not that every member of it is both supported and opposed.
  const information = quads
    .filter(
      (quad) =>
        quad.subject.kind === "iri" &&
        quad.subject.value === node &&
        quad.predicate === LOGIC_RESULT_INFORMATION &&
        quad.object.kind === "iri",
    )
    .map((quad) => ({
      subject: { iri: node },
      predicate: { iri: LOGIC_RESULT_INFORMATION },
      object: { iri: quad.object.value },
    }));
  return { node, information };
}

/** The closure shape: entailed quads + the reasoning result they came out of. */
function closureDerivations(answer) {
  if (typeof answer.closure_nquads !== "string") return null;
  if (typeof answer.judgment_nquads !== "string") {
    throw new Error(
      "derivation: an answer carrying `closure_nquads` carries no `judgment_nquads`, so its " +
        "conclusions name no reasoning run — refusing to record an unattributable closure",
    );
  }
  const judgment = reasoningJudgment(answer.judgment_nquads, "the answer");
  const derived = parseNQuads(answer.closure_nquads).map((quad, i) => ({
    subject: declaredFromTerm(quad.subject, `closure quad ${i} subject`),
    predicate: { iri: quad.predicate },
    object: declaredFromTerm(quad.object, `closure quad ${i} object`),
    antecedents: [{ iri: judgment.node }],
  }));
  return { derived, judgment: judgment.information };
}

/**
 * Read one step's object out of its canonical N3 surface, through the SHIPPED reader.
 *
 * `obj_n3` is a term, not a value: `<iri>`, `"lexical"^^<datatype>`, `_:b`. It is read by
 * putting it in the object position of a throwaway statement and running [`parseNQuads`],
 * so the console keeps ONE N-Quads term reader. Writing a second one here — even a small
 * one — is how the term kinds start disagreeing.
 */
function stepObject(objN3, where) {
  const probe = "<urn:gmeow:console:term-probe>";
  let quads;
  try {
    quads = parseNQuads(`${probe} ${probe} ${objN3} .\n`);
  } catch (cause) {
    throw new Error(`derivation: ${where} object surface ${JSON.stringify(objN3)} is not an ` +
      `N-Quads term: ${cause?.message ?? cause}`, { cause });
  }
  if (quads.length !== 1) {
    throw new Error(
      `derivation: ${where} object surface ${JSON.stringify(objN3)} read as ${quads.length} ` +
        "statements, so it is not one term",
    );
  }
  return declaredFromTerm(quads[0].object, `${where} object`);
}

/** One step's subject: a bare IRI, or the blank node a chase Skolemized. */
function stepSubject(value, where) {
  const text = String(value);
  return text.startsWith("_:") ? { bnode: text } : { iri: text };
}

/** One proof-tree step as the statement it concludes. */
function stepStatement(step, where) {
  return {
    subject: stepSubject(step.subject_iri, where),
    predicate: { iri: step.predicate_iri },
    object: stepObject(step.obj_n3, where),
  };
}

/**
 * The proof-tree shape: each step, and the steps the engine says it was derived from.
 *
 * A step with no antecedent step is an ASSERTED premise — it concluded nothing, so it
 * contributes no derived statement of its own and appears only as somebody else's
 * antecedent. A step naming an antecedent the skeleton does not carry is a HARD failure:
 * the skeleton is the engine's own faithfulness-checked trace, so a dangling reference
 * means it was misread, and recording the conclusion without that premise would drop
 * exactly the basis the annotation exists to carry.
 */
function proofTreeDerivations(answer) {
  if (!Array.isArray(answer.step_skeleton)) return null;
  const byId = new Map(answer.step_skeleton.map((step) => [String(step.derivation_id), step]));
  const derived = [];
  for (const step of answer.step_skeleton) {
    const sources = Array.isArray(step.source_step_ids) ? step.source_step_ids : [];
    if (sources.length === 0) continue;
    const where = `proof step ${step.derivation_id}`;
    derived.push({
      ...stepStatement(step, where),
      antecedents: sources.map((id) => {
        const source = byId.get(String(id));
        if (source === undefined) {
          throw new Error(
            `derivation: ${where} names antecedent step \`${id}\`, which the returned proof ` +
              "skeleton does not carry — refusing to record the conclusion without the premise " +
              "the engine says it rests on",
          );
        }
        return { statement: stepStatement(source, `${where} antecedent ${id}`) };
      }),
    });
  }
  // A proof tree is reconstructed FROM a reasoning run, and carries that run's judgment
  // alongside it, so the Belnap verdict rides here for the same reason it does on a
  // closure — read off the same field, by the same function.
  const judgment =
    typeof answer.judgment_nquads === "string"
      ? reasoningJudgment(answer.judgment_nquads, "the proof skeleton").information
      : [];
  return { derived, judgment };
}

/**
 * The derived statements and engine judgment carried by one tool answer.
 *
 * Returns `{derived, judgment}`, both possibly EMPTY — and empty is the honest answer for
 * a tool that derived nothing (a transcode, a lookup, a search). Fabricating an annotation
 * where the engine stated no basis would be worse than the dark surface this replaced: a
 * recorded provenance claim nothing backs.
 *
 * A payload the recognizers do not match contributes nothing and is not an error. A payload
 * they DO match but that is malformed — a closure with no judgment, a judgment naming no
 * single run, a proof step citing a premise that is not there — is a hard failure naming
 * what was wrong, never a quietly reduced annotation set.
 */
export function derivationsFrom(answer) {
  if (answer === null || typeof answer !== "object") return { derived: [], judgment: [] };
  return closureDerivations(answer) ?? proofTreeDerivations(answer) ?? { derived: [], judgment: [] };
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
