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
//
// Every one of these is TOTAL: it emits a term the N-Quads grammar admits, or it throws.
// Nothing here guesses a term's KIND from the shape of its text — see [`declaredTerm`].

/** The delimiters an `IRIREF` may not carry, per the N-Quads grammar. */
const IRI_DELIMITERS = '<>"{}|^`\\';

/**
 * Serialize an IRI, refusing one the grammar cannot express.
 *
 * `<…>` admits no unescaped delimiter and no code point at or below U+0020. A caller that
 * hands us prose (`"see http://example.org/a b"`) gets a NAMED failure here rather than an
 * `<…>` term that silently truncates at the first space and makes the whole export
 * unparsable.
 */
function iri(value) {
  const text = String(value);
  for (const c of text) {
    if (c.codePointAt(0) <= 0x20 || IRI_DELIMITERS.includes(c)) {
      throw new Error(
        `session: \`${text}\` cannot be serialized as an IRI — an N-Quads IRIREF admits no ` +
          `space, control character, or any of ${IRI_DELIMITERS}. Emit it as a literal if that ` +
          "is what it is.",
      );
    }
  }
  return `<${text}>`;
}

/** The C0 controls the grammar gives a short `ECHAR` name. */
const ECHAR_BY_CHAR = { "\n": "\\n", "\r": "\\r", "\t": "\\t", "\b": "\\b", "\f": "\\f" };

/**
 * Escape a lexical form for an N-Quads quoted literal.
 *
 * The WHOLE C0 range is escaped, not the five characters with short names. That is not
 * fastidiousness: this module's own `callIri` joins its key components with U+001F, and
 * `toolArguments`/`toolResult` carry JSON built from arbitrary engine payloads, so a raw
 * control character reaching a `"…"` term is a live possibility rather than a hypothetical
 * — and one such character makes the exported `.gts` unparsable in its entirety.
 * Everything without a short name rides as a UCHAR escape, which the reader in
 * `assets/mcp-transport.mjs` decodes back to the same code point.
 */
function escapeLiteral(value) {
  let out = "";
  for (const c of String(value)) {
    if (c === "\\") {
      out += "\\\\";
      continue;
    }
    if (c === '"') {
      out += '\\"';
      continue;
    }
    const named = ECHAR_BY_CHAR[c];
    if (named !== undefined) {
      out += named;
      continue;
    }
    const code = c.codePointAt(0);
    if (code <= 0x1f || code === 0x7f) {
      out += `\\u${code.toString(16).toUpperCase().padStart(4, "0")}`;
      continue;
    }
    out += c;
  }
  return out;
}

function literal(value, { datatype = null, language = null } = {}) {
  if (datatype !== null && language !== null) {
    throw new Error(
      `session: the literal \`${value}\` declares BOTH a datatype and a language tag — an RDF ` +
        "literal carries at most one of them",
    );
  }
  const escaped = escapeLiteral(value);
  if (language !== null) {
    if (!/^[A-Za-z]+(-[A-Za-z0-9]+)*$/.test(language)) {
      throw new Error(`session: \`${language}\` is not a well-formed language tag`);
    }
    return `"${escaped}"@${language}`;
  }
  if (datatype !== null) return `"${escaped}"^^${iri(datatype)}`;
  return `"${escaped}"`;
}

/**
 * Serialize a DECLARED term: the caller states the kind, this function never infers it.
 *
 *   * `{iri: "…"}`                                   — an IRI;
 *   * `{literal: "…", datatype?: "…", language?: "…"}` — a literal;
 *   * a plain string                                 — a plain literal, the common case.
 *
 * The heuristic this replaced was `object.startsWith("http") ? iri : literal`, which is
 * wrong in BOTH directions and silently: a prose answer that quotes a URL was emitted as
 * an IRI (invalid the moment it contained a space or a `>`), and a `urn:`, `did:` or
 * `file:` IRI was emitted as a literal, which is not the same statement. The caller knows
 * which it produced — an emitter that guesses is a second, wrong source of truth for term
 * kind.
 */
function declaredTerm(value, where) {
  if (typeof value === "string") return literal(value);
  if (value !== null && typeof value === "object") {
    if (typeof value.iri === "string") return iri(value.iri);
    if (typeof value.literal === "string") {
      return literal(value.literal, {
        datatype: value.datatype ?? null,
        language: value.language ?? null,
      });
    }
  }
  throw new Error(
    `session: ${where} is not a declared RDF term. Pass {iri: "…"} for an IRI, ` +
      '{literal: "…", datatype?, language?} or a plain string for a literal. The term kind is ' +
      "never inferred from the text of the value.",
  );
}

/** [`declaredTerm`] restricted to the positions only an IRI can occupy. */
function declaredIri(value, where) {
  if (value !== null && typeof value === "object" && typeof value.iri === "string") {
    return iri(value.iri);
  }
  throw new Error(
    `session: ${where} must be a declared IRI — pass {iri: "…"}. This position cannot carry a ` +
      "literal, so a bare string is refused rather than promoted.",
  );
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
   * the call produced. Each becomes an annotated quoted triple. Every one of those term
   * positions carries a DECLARED kind — `{iri: "…"}`, `{literal: "…", datatype?, language?}`
   * or a plain string for a plain literal — because the caller knows what its tool
   * returned and [`declaredTerm`] must never infer a kind from the text of a value.
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
   *
   * Every term is DECLARED by the caller and serialized by [`declaredTerm`] /
   * [`declaredIri`] — the subject and predicate positions accept `{iri: …}` alone, because
   * neither can carry a literal. The object and each antecedent accept `{iri: …}`,
   * `{literal: …}` or a plain string.
   */
  annotationsFor(call) {
    const lines = [];
    for (const [n, statement] of call.derived.entries()) {
      const { subject, predicate, object, antecedents } = statement;
      const at = `derived statement ${n} of \`${call.tool}\``;
      if (!Array.isArray(antecedents) || antecedents.length === 0) {
        throw new Error(
          `session: ${at} names no antecedents — a derived ` +
            "result triple must carry the set it was derived from",
        );
      }
      const s = declaredIri(subject, `${at}: subject`);
      const p = declaredIri(predicate, `${at}: predicate`);
      const o = declaredTerm(object, `${at}: object`);
      lines.push(quad(s, p, o));
      const reifier = iri(`${call.iri}/statement/${n}`);
      lines.push(quad(reifier, iri(RDF_REIFIES), tripleTerm(s, p, o)));
      lines.push(quad(reifier, iri(`${GMEOW_NS}derivedBy`), iri(call.iri)));
      for (const [k, antecedent] of antecedents.entries()) {
        lines.push(
          quad(reifier, iri(`${GMEOW_NS}wasDerivedFrom`), declaredTerm(antecedent, `${at}: antecedent ${k}`)),
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
 * The engine's store reading: `{nquads, heldBy, carriedBy}`, taken off the `store_segment`
 * and `list_candidates` results that `engine.worker.mjs`'s `export` operation just made.
 *
 * * `nquads` — the engine's OWN serialization of its store, as `store_segment` returned it.
 * * `heldBy` — the tools that reported ACTUAL STATE, so [`exportSegment`] can tell "there
 *   is nothing to carry" apart from "there is something to carry and I cannot carry it".
 *   Those are opposite situations and only the second is a failure.
 * * `carriedBy` — the subset of `heldBy` whose state `nquads` actually contains. The two
 *   lists are what make the refusal exact: an export that carried the claims and dropped
 *   the candidate library would satisfy a "the serialization is non-empty" test while
 *   shipping half a snapshot, so coverage is tracked per holder rather than in aggregate.
 *
 * `store_segment` is the ONE tool that serializes the store. `recall` deliberately is not
 * and cannot be: it answers a QUERY with a ranked, truncated JSON view of matching claims,
 * which is not a snapshot of anything. Reading a serialization off a `recall` (or a
 * `list_candidates`) result is exactly the mistake this function used to make — it read
 * `store_nquads ?? nquads`, fields NO engine tool returns, so it produced `""` on every
 * export and the guard below never fired.
 *
 * It lives HERE, beside its one consumer [`exportSegment`], rather than inline in the
 * worker, for two reasons. It is the worker's only DOM-free, engine-free step, so keeping
 * it in this module makes it reachable from the Node acceptance lane — and an export
 * assertion that cannot reach this function cannot observe whether the store it exports
 * is real. And the pairing is the contract: the reader and the refusal belong together,
 * so a field rename on one side cannot drift past the other unnoticed.
 *
 * # Why the holders are read off the RESULTS, not off the recorded trajectory
 *
 * The alternative is to scan the trajectory for store-touching tool names. That is
 * strictly weaker AND needs a list. Weaker, because `export` asks the store what it holds
 * on EVERY export: the store's whole state is observed at export time regardless of what
 * the reader happened to click, so a trajectory scan is a proxy for a fact already in
 * hand. And it misreads the common case — a session that called `recall` and got nothing
 * back recorded a store-touching invocation but has no state to lose, so a name-based rule
 * would refuse an export that is perfectly complete. Asking the store what it holds needs
 * no list of tool names to keep in sync with the engine, which is the whole point.
 *
 * It deliberately mints NOTHING. The RDF shape of a stored claim is owned by the engine's
 * store; re-deriving quads here from the returned JSON would be a second source of truth
 * for that shape, and a silent one — the store could change shape and nothing would fail.
 */
export function storeReading(store, candidates) {
  const nquads = typeof store.nquads === "string" ? store.nquads : "";
  const heldBy = [];
  const carriedBy = [];
  if (Number(store.claim_count ?? 0) > 0 || Number(store.tool_call_count ?? 0) > 0) {
    heldBy.push("store_segment");
    if (nquads.trim().length > 0) carriedBy.push("store_segment");
  }
  // The candidate library is a SEPARATE append-only collection, not part of the claim
  // package `store_segment` serializes, so it is held and never carried. Saying so is the
  // point: the export then refuses instead of shipping a snapshot missing the candidates.
  if (Number(candidates.candidate_count ?? 0) > 0) heldBy.push("list_candidates");
  return { nquads, heldBy, carriedBy };
}

/**
 * Split one N-Quads line into its top-level terms, verbatim.
 *
 * A LEXER, not a reader: it decodes nothing and interprets nothing, it only says where one
 * term ends and the next begins. That is the whole job [`regraph`] needs, and it is a job
 * `line.split(/\s+/)` cannot do — a literal may contain spaces, an IRI may contain a `.`,
 * and an RDF-1.2 triple term contains three whole terms and two spaces of its own.
 *
 * The rules are the grammar's: a `"…"` literal runs to its unescaped closing quote, a
 * `<…>` IRI to its `>`, a `<<( … )>>` triple term to its matching closer (nested, so a
 * triple term inside a triple term counts), and outside those, whitespace ends a term. A
 * line the rules cannot cover throws, naming the line — this is called on bytes the engine
 * produced, and a store serialization we cannot lex is not one we may re-graph by guessing.
 */
function splitNQuadsTerms(line) {
  const terms = [];
  let i = 0;
  while (i < line.length) {
    while (i < line.length && /\s/.test(line[i])) i += 1;
    if (i >= line.length) break;
    const start = i;
    let depth = 0;
    for (; i < line.length; i += 1) {
      const c = line[i];
      if (c === '"') {
        i += 1;
        let closed = false;
        for (; i < line.length; i += 1) {
          if (line[i] === "\\") {
            i += 1;
            continue;
          }
          if (line[i] === '"') {
            closed = true;
            break;
          }
        }
        if (!closed) {
          throw new Error(
            `session export: the store line ${JSON.stringify(line)} carries an unterminated literal`,
          );
        }
        continue;
      }
      if (line.startsWith("<<(", i)) {
        depth += 1;
        i += 2;
        continue;
      }
      if (line.startsWith(")>>", i)) {
        if (depth === 0) {
          throw new Error(
            `session export: the store line ${JSON.stringify(line)} closes a triple term it never opened`,
          );
        }
        depth -= 1;
        i += 2;
        continue;
      }
      if (c === "<") {
        const close = line.indexOf(">", i);
        if (close < 0) {
          throw new Error(
            `session export: the store line ${JSON.stringify(line)} carries an unterminated IRI`,
          );
        }
        i = close;
        continue;
      }
      if (depth === 0 && /\s/.test(c)) break;
    }
    if (depth !== 0) {
      throw new Error(
        `session export: the store line ${JSON.stringify(line)} carries an unclosed triple term`,
      );
    }
    terms.push(line.slice(start, i));
  }
  return terms;
}

/**
 * Re-graph one store line into the session-store segment graph.
 *
 * The store serializes its own snapshot; the export carries that snapshot as ONE named
 * graph, so a line arrives here as either a triple (`s p o .`) or a quad the store had
 * already placed in a graph of its own (`s p o g .`). Both become `s p o <segment> .`:
 * the store's own graph term is REPLACED, which is what "the export carries the store in
 * the session-store segment graph" means, and which is why the term count is established
 * by lexing rather than by a regex that strips a trailing `.`.
 *
 * That regex is the defect this replaced. `line.replace(/\s*\.\s*$/, "")` removes the
 * terminator and NOTHING else, so a four-term quad came back out as `s p o g <segment> .`
 * — five terms, which is not N-Quads at all, and one such line makes the entire exported
 * `.gts` unparsable rather than merely mis-graphed.
 *
 * A line that is not three or four terms plus a `.` is a HARD failure naming the line. We
 * cannot emit a valid quad from it, and emitting an invalid one is the failure mode this
 * whole function exists to remove.
 */
function regraph(line, graph) {
  const terms = splitNQuadsTerms(line);
  const dot = terms[terms.length - 1];
  if (terms.length < 4 || dot !== ".") {
    throw new Error(
      `session export: the store line ${JSON.stringify(line)} is not a terminated N-Quads ` +
        `statement — it lexes as ${terms.length} term(s) ending in ${JSON.stringify(dot ?? "")}`,
    );
  }
  const body = terms.slice(0, -1);
  if (body.length > 4) {
    throw new Error(
      `session export: the store line ${JSON.stringify(line)} carries ${body.length} terms; ` +
        "an N-Quads statement carries three or four",
    );
  }
  return `${body.slice(0, 3).join(" ")} ${graph} .`;
}

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
 * `store` is the [`storeReading`] the engine just gave us — `{nquads, heldBy, carriedBy}`,
 * never a bare string. The reading is REQUIRED (an export that never asked the store what
 * it held cannot know whether it dropped anything), but an EMPTY store is not an error.
 *
 * The refusal is scoped to the case where state actually exists and cannot be carried:
 *
 *  * `heldBy` empty — the store holds nothing. There is nothing to drop, so the export
 *    succeeds and carries the trajectory alone. A session that parsed some Turtle and ran
 *    `convert` is perfectly exportable and is not made less so by an untouched store.
 *  * a holder in `heldBy` that is not in `carriedBy` — that collection holds state the
 *    engine will not serialize. THAT is the hard failure, and it names the tools whose
 *    state cannot be carried, because an export that quietly shipped without them would
 *    claim to be a session snapshot while carrying half of one. Checking coverage per
 *    HOLDER rather than in aggregate is deliberate: a whole-reading "is the serialization
 *    non-empty?" test passes as soon as any one collection serializes, which is exactly
 *    the half-a-snapshot case.
 *
 * Note what this does NOT do: when the store is empty it emits no `SESSION_STORE_GRAPH`
 * quads at all, rather than an empty named graph. An empty graph would imply the store
 * was captured and found empty, which is a claim about state; emitting nothing says only
 * that nothing was captured, which is what actually happened.
 *
 * "Empty" is judged on the QUADS the reading contributes, not on a field's type. A
 * `typeof store !== "string"` test accepts `""`, and `""` reaches the end of this
 * function as an empty `storeLines` — precisely the storeless export described above.
 * That was not hypothetical: the worker used to read `store_nquads`/`nquads` off `recall`
 * and `list_candidates`, neither of which carries either field, so it passed `""` here on
 * every export and no refusal ever fired. A guard a real caller walks straight through is
 * not a guard.
 */
export function exportSegment(session, store) {
  if (
    store === null ||
    typeof store !== "object" ||
    typeof store.nquads !== "string" ||
    !Array.isArray(store.heldBy) ||
    !Array.isArray(store.carriedBy)
  ) {
    throw new Error(
      "session export: the wasm claim/candidate store reading is required — an export that " +
        "never asked the store what it held cannot know whether it dropped anything. Pass the " +
        "`{nquads, heldBy, carriedBy}` value `storeReading` returns.",
    );
  }
  const graph = iri(SESSION_STORE_GRAPH);
  const storeLines = store.nquads
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith("#"))
    .map((line) => regraph(line, graph));
  // A holder whose state nothing carried, plus the case where the reading claimed coverage
  // but every line it offered was a comment: both are "state exists and did not travel".
  const uncarried = store.heldBy.filter(
    (tool) => !store.carriedBy.includes(tool) || storeLines.length === 0,
  );
  if (uncarried.length > 0) {
    throw new Error(
      `session export: ${uncarried.join(" and ")} reported stored state, but the engine ` +
        "returned no serialization for it, so the store segment carried no quads. Exporting the " +
        "trajectory alone would silently drop that state and still call itself a session snapshot.",
    );
  }
  // The header describes what the file ACTUALLY carries. Naming the store graph when no
  // store quad follows would tell a reader the store was captured into that graph and
  // found empty; the truthful statement for an untouched store is that none was captured.
  const header = [
    `# gmeow console session ${session.id}`,
    `# trajectory anchor: ${session.anchor}`,
    storeLines.length > 0
      ? `# store segment graph: ${SESSION_STORE_GRAPH}`
      : "# store segment: none — the engine store held nothing at export time",
  ].join("\n");
  return `${header}\n${session.trajectoryNQuads()}${storeLines.join("\n")}${storeLines.length ? "\n" : ""}`;
}
