// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The console session: a recorded agentic trajectory, in the SHAPE the shipped auditor
// already reads.
//
// DOM-free and Node-importable on purpose — every acceptance assertion about the session
// runs here, with no browser. It imports nothing: it is pure data in, RDF text out.
//
// # Why this shape and not another
//
// `crates/logic/src/transaction/trajectory.rs` discovers a recorded trajectory by joining
// EXACTLY these facts, and hard-fails on any of them being absent or plural:
//
//   * `<call> rdf:type gmeow:ToolCall`
//   * `<call> logic:instantiatesSchema <schema>`   — exactly one; this is what makes the
//     call BOUND (an unbound ToolCall is plain provenance and is correctly ignored)
//   * `<call> logic:properPartOf <anchor>`         — exactly one trajectory anchor
//   * `<anchor> logic:transitionFromState <state>` — the anchor MUST bear a start state
//   * `<call> gmeow:atTime "…"^^xsd:dateTime`      — exactly one
//   * `<call> gmeow:eventTemporalFrame <frame>`    — exactly one, and every call in one
//     trajectory MUST share it (`order_steps` refuses a trajectory that mixes frames,
//     because a lexical sort of `gmeow:atTime` is coherent only within one frame)
//
// The auditor then orders by `gmeow:atTime`, right-folds into the binary
// `logic:SerialConjunction`, and bridges on `logic:instantiatesSchema`. NONE of that is
// re-implemented here: this module only RECORDS in the discovered shape, so the shipped
// auditor is the single folder. Authoring a second trajectory folder in JavaScript would
// be a second source of truth for transaction identity.
//
// # Result annotations
//
// A derived result triple is annotated with an RDF-1.2 triple term (`<<( s p o )>>`)
// through a reifier node, which then carries `gmeow:derivedBy` (the call the assertion
// came out of) and one `gmeow:wasDerivedFrom` per ANTECEDENT. Quoted triples are the
// mechanism because the annotation is about the STATEMENT, not about an entity — exactly
// the distinction `gmeow:derivedBy`'s own definition draws against `gmeow:wasGeneratedBy`.

const GMEOW_NS = "https://blackcatinformatics.ca/gmeow/";
const LOGIC_NS = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
const RDFS_LABEL = "http://www.w3.org/2000/01/rdf-schema#label";
const XSD_DATE_TIME = "http://www.w3.org/2001/XMLSchema#dateTime";

/** The example base every console-minted IRI lives under. Never a `gmeow:` term. */
export const SESSION_BASE = "https://example.org/gmeow/console/session/";

/**
 * The single temporal frame a console session stamps on every recorded call.
 *
 * `gmeow:eventTemporalFrame` is Principle 11's "every crisp timestamp names its frame".
 * The console records wall-clock UTC, so the frame is UTC — asserted ONCE and reused for
 * every call, because the auditor refuses a trajectory that mixes frames.
 */
export const SESSION_TEMPORAL_FRAME = `${GMEOW_NS}temporalFrameUtc`;

/** The named graph the exported session store segment rides in. */
export const SESSION_STORE_GRAPH = `${SESSION_BASE}store-segment`;

// ── Small serializers ───────────────────────────────────────────────────────

function iri(value) {
  return `<${value}>`;
}

function literal(value, { datatype = null, language = null } = {}) {
  const escaped = String(value)
    .replace(/\\/g, "\\\\")
    .replace(/"/g, '\\"')
    .replace(/\n/g, "\\n")
    .replace(/\r/g, "\\r")
    .replace(/\t/g, "\\t");
  if (language !== null) return `"${escaped}"@${language}`;
  if (datatype !== null) return `"${escaped}"^^<${datatype}>`;
  return `"${escaped}"`;
}

/** An RDF-1.2 triple term (`<<( s p o )>>`) over three already-serialized terms. */
export function tripleTerm(subject, predicate, object) {
  return `<<( ${subject} ${predicate} ${object} )>>`;
}

function quad(subject, predicate, object, graph = null) {
  return graph === null
    ? `${subject} ${predicate} ${object} .`
    : `${subject} ${predicate} ${object} ${graph} .`;
}

// ── Content addressing ──────────────────────────────────────────────────────
// A dependency-free 128-bit FNV-1a over UTF-8, rendered as 32 lowercase hex digits. It
// addresses console-local identity only (call IRIs, permalinks) — it is NEVER presented
// as a `blake3:` content address, which is the project's cryptographic digest and is
// computed by the Rust producer, not here. Naming it `fnv1a128:` keeps the two apart.

const FNV_OFFSET_BASIS = 144066263297769815596495629667062367629n;
const FNV_PRIME = 309485009821345068724781371n;
const FNV_MASK = (1n << 128n) - 1n;

function fnv1a128(bytes) {
  let hash = FNV_OFFSET_BASIS;
  for (const byte of bytes) {
    hash ^= BigInt(byte);
    hash = (hash * FNV_PRIME) & FNV_MASK;
  }
  return hash.toString(16).padStart(32, "0");
}

/** The console's content address of a string — `fnv1a128:<32 hex>`. */
export function contentAddress(text) {
  return `fnv1a128:${fnv1a128(new TextEncoder().encode(String(text)))}`;
}

/**
 * The byte size above which a recorded payload rides as a content digest rather than
 * verbatim, per `gmeow:toolResult`'s own guidance ("for large payloads store a content
 * digest literal instead").
 */
const VERBATIM_PAYLOAD_LIMIT = 512;

/**
 * The `gmeow:toolResult` literal for one recorded call.
 *
 * A SMALL payload rides verbatim; a large one rides as a content digest. The threshold is
 * not cosmetic: a failure payload is small, so it stays verbatim and remains READABLE in
 * the recorded trajectory — which is what lets the derivation-structure pane compute a
 * minimal fatal cut from the session itself. Digesting every payload uniformly would make
 * every failure indistinguishable from every success in the record.
 */
function toolResultLiteral(result) {
  if (result === null || result === undefined) return "null";
  const json = JSON.stringify(result);
  return json.length <= VERBATIM_PAYLOAD_LIMIT ? json : contentAddress(json);
}

// ── The session ─────────────────────────────────────────────────────────────

/**
 * One console session: an ordered run of recorded invocations under ONE trajectory anchor.
 *
 * `now` is injectable so the acceptance lanes get a deterministic recording; the browser
 * passes nothing and gets wall-clock UTC.
 */
export class ConsoleSession {
  constructor({ id = "s0", now = () => new Date().toISOString() } = {}) {
    this.id = id;
    this.now = now;
    this.calls = [];
    this.anchor = `${SESSION_BASE}${id}/trajectory`;
    this.startState = `${SESSION_BASE}${id}/start`;
    this.frame = SESSION_TEMPORAL_FRAME;
  }

  /** The IRI of the `n`-th recorded call. Content-addressed over what the call IS. */
  callIri(index, tool, args) {
    const key = contentAddress([this.id, index, tool, JSON.stringify(args)].join("\u001f"));
    return `${SESSION_BASE}${this.id}/call/${index}-${key.slice("fnv1a128:".length, "fnv1a128:".length + 12)}`;
  }

  /**
   * Record one invocation.
   *
   * `schema` is the tool's `logic:ActionSchema` IRI — the `action_policy` subject whose
   * `logic:mcpToolName` is `tool`. It is REQUIRED: a call with no schema is unbound, and
   * an unbound call is invisible to the auditor, so accepting one would silently drop the
   * invocation out of the trajectory.
   *
   * `derived` is the list of `{subject, predicate, object, antecedents}` result statements
   * the call produced. Each becomes an annotated quoted triple.
   */
  record({ tool, schema, args, result, derived = [], storeSegment = null }) {
    if (typeof tool !== "string" || tool.length === 0) {
      throw new Error("session.record: `tool` is required");
    }
    if (typeof schema !== "string" || schema.length === 0) {
      throw new Error(
        `session.record: \`${tool}\` has no logic:ActionSchema — an unbound gmeow:ToolCall is ` +
          "invisible to the trajectory auditor, so recording one would silently drop the call",
      );
    }
    const index = this.calls.length;
    const call = {
      index,
      iri: this.callIri(index, tool, args ?? {}),
      tool,
      schema,
      args: args ?? {},
      result: result ?? null,
      derived,
      atTime: this.now(index),
      storeSegment: storeSegment ?? String(index).padStart(4, "0"),
    };
    this.calls.push(call);
    return call;
  }

  /**
   * The recorded trajectory as N-Quads, in the exact shape the shipped auditor discovers.
   *
   * Emitted into the DEFAULT graph: the auditor reads world facts, not a named graph, and
   * a named graph here would make the calls invisible to it.
   */
  trajectoryNQuads() {
    const lines = [];
    // A plain `gmeow:Activity` anchor (NOT a `logic:Plan`): it grafts the audit context
    // without standing up the full plan-governance surface, exactly as the shipped
    // trajectory-audit worked example does.
    lines.push(quad(iri(this.anchor), iri(RDF_TYPE), iri(`${GMEOW_NS}Activity`)));
    lines.push(
      quad(
        iri(this.anchor),
        iri(RDFS_LABEL),
        literal(`console session ${this.id}`, { language: "x-gmeow-english" }),
      ),
    );
    // The anchor MUST bear a start state — `trajectory_roots` hard-fails without one.
    lines.push(quad(iri(this.anchor), iri(`${LOGIC_NS}transitionFromState`), iri(this.startState)));
    lines.push(quad(iri(this.startState), iri(RDF_TYPE), iri(`${LOGIC_NS}Situation`)));

    for (const call of this.calls) {
      const c = iri(call.iri);
      lines.push(quad(c, iri(RDF_TYPE), iri(`${GMEOW_NS}ToolCall`)));
      // The bridge the auditor folds on. Exactly one.
      lines.push(quad(c, iri(`${LOGIC_NS}instantiatesSchema`), iri(call.schema)));
      // The mereological spine grouping this call under the trajectory. Exactly one.
      lines.push(quad(c, iri(`${LOGIC_NS}properPartOf`), iri(this.anchor)));
      // The crisp timestamp AND the frame that makes it orderable. Exactly one each, and
      // the frame is the SAME for every call — a mixed frame is a hard auditor failure.
      lines.push(quad(c, iri(`${GMEOW_NS}atTime`), literal(call.atTime, { datatype: XSD_DATE_TIME })));
      lines.push(quad(c, iri(`${GMEOW_NS}eventTemporalFrame`), iri(this.frame)));
      lines.push(quad(c, iri(`${GMEOW_NS}usedTool`), iri(`${SESSION_BASE}tool/${call.tool}`)));
      lines.push(quad(iri(`${SESSION_BASE}tool/${call.tool}`), iri(RDF_TYPE), iri(`${GMEOW_NS}SoftwareAgent`)));
      lines.push(quad(c, iri(`${GMEOW_NS}toolArguments`), literal(JSON.stringify(call.args))));
      lines.push(quad(c, iri(`${GMEOW_NS}toolResult`), literal(toolResultLiteral(call.result))));
      lines.push(quad(c, iri(`${GMEOW_NS}sessionStoreSegment`), literal(call.storeSegment)));
      lines.push(...this.annotationsFor(call));
    }
    lines.sort();
    return `${lines.join("\n")}\n`;
  }

  /**
   * The RDF-1.2 annotations for one call's derived statements.
   *
   * Every derived triple is asserted AND reified through a triple term, and the reifier
   * carries the call it came out of plus its antecedent set. A derived statement with no
   * antecedents is refused: an answer with no stated basis is exactly the unattributable
   * assertion this annotation exists to prevent.
   */
  annotationsFor(call) {
    const lines = [];
    for (const [n, statement] of call.derived.entries()) {
      const { subject, predicate, object, antecedents } = statement;
      if (!Array.isArray(antecedents) || antecedents.length === 0) {
        throw new Error(
          `session: derived statement ${n} of \`${call.tool}\` names no antecedents — a derived ` +
            "result triple must carry the set it was derived from",
        );
      }
      const s = iri(subject);
      const p = iri(predicate);
      const o = typeof object === "string" && object.startsWith("http") ? iri(object) : literal(object);
      lines.push(quad(s, p, o));
      const reifier = iri(`${call.iri}/statement/${n}`);
      lines.push(quad(reifier, iri(RDF_REIFIES), tripleTerm(s, p, o)));
      lines.push(quad(reifier, iri(`${GMEOW_NS}derivedBy`), iri(call.iri)));
      for (const antecedent of antecedents) {
        lines.push(
          quad(
            reifier,
            iri(`${GMEOW_NS}wasDerivedFrom`),
            typeof antecedent === "string" && antecedent.startsWith("http")
              ? iri(antecedent)
              : literal(antecedent),
          ),
        );
      }
    }
    return lines;
  }

  // ── Permalink ─────────────────────────────────────────────────────────────

  /**
   * The session as a content-addressed permalink fragment.
   *
   * The payload is the invocation list — tool, arguments and schema — never the results:
   * a permalink REPLAYS a session against the reader's own engine rather than shipping
   * someone else's answers. The address is over the payload, so an edited link fails the
   * decode instead of silently replaying something else.
   */
  permalink() {
    const payload = JSON.stringify({
      v: 1,
      id: this.id,
      calls: this.calls.map((c) => ({ tool: c.tool, schema: c.schema, args: c.args })),
    });
    return `${contentAddress(payload)}.${base64UrlEncode(payload)}`;
  }
}

/** URL-safe base64 of a UTF-8 string, without padding. */
export function base64UrlEncode(text) {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/** The inverse of [`base64UrlEncode`]. */
export function base64UrlDecode(text) {
  const padded = text.replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(padded + "=".repeat((4 - (padded.length % 4)) % 4));
  return new TextDecoder().decode(Uint8Array.from(binary, (c) => c.charCodeAt(0)));
}

/**
 * Decode a permalink fragment back into its invocation list.
 *
 * A digest mismatch is a HARD failure naming both addresses — a tampered or truncated
 * permalink is never replayed on a best-effort basis.
 */
export function decodePermalink(fragment) {
  const dot = String(fragment).indexOf(".");
  if (dot < 0) {
    throw new Error("permalink: missing the `<address>.<payload>` separator");
  }
  const address = fragment.slice(0, dot);
  const payload = base64UrlDecode(fragment.slice(dot + 1));
  const actual = contentAddress(payload);
  if (actual !== address) {
    throw new Error(`permalink: content address ${address} does not match the payload's ${actual}`);
  }
  const decoded = JSON.parse(payload);
  if (decoded.v !== 1) {
    throw new Error(`permalink: unknown payload version ${decoded.v}`);
  }
  return decoded;
}

/**
 * Rebuild a session from a decoded permalink (identity round trip for the invocations).
 */
export function sessionFromPermalink(fragment, options = {}) {
  const decoded = decodePermalink(fragment);
  const session = new ConsoleSession({ id: decoded.id, ...options });
  for (const call of decoded.calls) {
    session.record({ tool: call.tool, schema: call.schema, args: call.args, derived: [] });
  }
  return session;
}

// ── Export ──────────────────────────────────────────────────────────────────

/**
 * The exportable `.gts` segment text for a session.
 *
 * Two graphs, both required:
 *
 *  * the DEFAULT graph — the recorded trajectory, in the auditor's shape, so an exported
 *    session can be replayed through `gmeow-dev`'s native transaction auditor unchanged;
 *  * a NAMED graph — the serialized wasm claim/candidate store as it stood at export
 *    time, so the export is self-contained: the trajectory's writes and the store they
 *    landed in travel together. The graph is named for `gmeow:sessionStoreSegment`, the
 *    property that locates a call's audit record inside the append-only store, and each
 *    call's segment identifier is recorded on the call itself.
 *
 * `store` is the engine's own serialization (N-Quads text). An ABSENT store is a hard
 * failure, not an empty graph: an export that silently dropped the store would claim to
 * be a session snapshot while carrying only half of one.
 */
export function exportSegment(session, store) {
  if (typeof store !== "string") {
    throw new Error(
      "session export: the wasm claim/candidate store serialization is required — an export " +
        "without it is not a session snapshot",
    );
  }
  const graph = `<${SESSION_STORE_GRAPH}>`;
  const storeLines = String(store)
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith("#"))
    .map((line) => {
      // Re-graph each store triple/quad into the session-store segment graph: strip the
      // trailing `.` (and any graph term the store already carried) and re-terminate.
      const body = line.replace(/\s*\.\s*$/, "");
      return `${body} ${graph} .`;
    });
  const header = [
    `# gmeow console session ${session.id}`,
    `# trajectory anchor: ${session.anchor}`,
    `# store segment graph: ${SESSION_STORE_GRAPH}`,
  ].join("\n");
  return `${header}\n${session.trajectoryNQuads()}${storeLines.join("\n")}${storeLines.length ? "\n" : ""}`;
}
