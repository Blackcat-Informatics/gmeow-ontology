// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The console's engine worker: it OWNS the MCP engine and the session, so the shell's
// main thread never blocks on a chase.
//
// One protocol in both directions. In: `{id, op, args}`. Out: exactly one of
// `{id, ok: true, value}`, `{id, ok: false, error, op}`, or an unsolicited
// `{event, …}` progress frame. There is no third shape and no silent path: EVERY handler
// runs inside one try/catch whose catch POSTS the failure back with the operation that
// raised it. A swallowed error here would surface as a pane that simply never answered,
// which is the exact failure mode the visible-hard-error rule exists to prevent.
//
// Deferral is progress, not failure: a reasoning-segment tool posts
// `{event: "segment", phase: "loading" | "loaded", …}` before its answer, so the pane can
// show a loading state instead of a multi-second stall.

// The transport is imported as `./pkg/…`, not `../assets/…`: ONE specifier that resolves in
// BOTH trees this worker ships in. In the site tree `console/pkg/mcp-transport.mjs` is a
// generated forwarder to the shared `assets/mcp-transport.mjs`, so the site still carries
// one engine copy rather than two. In the published npm package the package root IS the
// console — there is no `../assets/` above it, which is why `../assets/mcp-transport.mjs`
// resolved to a 404 outside the package and the installed console could not boot at all —
// and the whole engine payload is staged under `pkg/` by the package's `prepack` step.
import {
  actionPolicyPanes,
  callTool,
  configure,
  conjectureLibrary,
  ensureMcp,
  listTools,
} from "./pkg/mcp-transport.mjs";
import { ConsoleSession, exportSegment, storeReading } from "./session.mjs";

// Same-origin guard. A worker inherits its creator's origin, but a message can be posted
// by any context that holds a reference to the port; frames whose declared origin is not
// ours are refused rather than executed. `""` is the file:// origin an offline shell has.
const SELF_ORIGIN = typeof location === "undefined" ? "" : location.origin;

let session = null;
let policy = null;

function post(message) {
  self.postMessage(message);
}

/** The action policy, read once from the engine and cached for the session's lifetime. */
async function ensurePolicy() {
  if (policy === null) {
    const payload = await callTool("action_policy", {});
    policy = { ...actionPolicyPanes(payload.nquads), graph: payload.graph, nquads: payload.nquads };
  }
  return policy;
}

/** The `logic:ActionSchema` IRI a tool's recorded call must name. */
async function schemaFor(tool) {
  const { byTool } = await ensurePolicy();
  const schema = byTool.get(tool);
  if (schema === undefined) {
    throw new Error(
      `\`${tool}\` has no subject in the shipped action policy — refusing to record an unbound call`,
    );
  }
  return schema;
}

const OPS = {
  /** Boot the engine and hand back the derived surface: panes, excluded writes, schemas. */
  async boot({ sessionId } = {}) {
    await ensureMcp();
    session = new ConsoleSession({ id: sessionId ?? "s0" });
    const derived = await ensurePolicy();
    const tools = await listTools();
    const byName = new Map(tools.map((t) => [t.name, t]));
    // A pane the policy names but the engine does not advertise is a HARD failure: the
    // two surfaces are meant to be bijective, and a quietly dropped pane would hide it.
    const missing = derived.panes.filter((name) => !byName.has(name));
    if (missing.length > 0) {
      throw new Error(
        `the action policy names ${missing.length} tool(s) the engine does not advertise: ${missing.join(", ")}`,
      );
    }
    return {
      graph: derived.graph,
      panes: derived.panes.map((name) => ({
        name,
        schema: derived.byTool.get(name),
        description: byName.get(name).description,
        inputSchema: byName.get(name).inputSchema,
      })),
      excluded: derived.excluded,
    };
  },

  /**
   * Invoke one tool and record the invocation into the session trajectory.
   *
   * `derived` is the caller's list of `{subject, predicate, object, antecedents}` result
   * statements, and every term in it DECLARES its kind — `{iri: "…"}`,
   * `{literal: "…", datatype?, language?}`, or a bare string for a plain literal. The
   * session emitter refuses an undeclared term rather than inferring a kind from its text:
   * a tool that answers with a `urn:` IRI and a tool that answers with prose quoting a URL
   * are making different statements, and only the caller knows which one it got.
   */
  async invoke({ tool, args, derived }) {
    const schema = await schemaFor(tool);
    const value = await callTool(tool, args, ({ phase, tool: deferred, segment }) => {
      post({ event: "segment", phase, tool: deferred, segment });
    });
    session.record({ tool, schema, args, result: value, derived: derived ?? [] });
    return value;
  },

  /**
   * The round-trip differential: transcode one document into EVERY requested target and
   * report the realized loss ledger per target.
   *
   * A per-format failure on user-pasted data is RECORDED and returned, never aborting the
   * differential: the reader learns which targets their document survives and which it
   * does not, which is the whole point of the pane. The row shape is uniform —
   * `{format, ok, …}` — so a caller never has to guess whether a target ran.
   */
  async roundtrip({ data, from, formats }) {
    const schema = await schemaFor("convert");
    const rows = [];
    for (const to of formats) {
      try {
        const out = await callTool("convert", { data, from, to });
        rows.push({ format: to, ok: true, bytes: out.bytes, output: out.output, loss: out.loss ?? [] });
        session.record({
          tool: "convert",
          schema,
          args: { from, to },
          result: { bytes: out.bytes, loss: out.loss ?? [] },
          derived: [],
        });
      } catch (error) {
        rows.push({ format: to, ok: false, error: String(error?.message ?? error), loss: [] });
        session.record({
          tool: "convert",
          schema,
          args: { from, to },
          result: { error: String(error?.message ?? error) },
          derived: [],
        });
      }
    }
    return { rows, lattice: lossLattice(rows) };
  },

  /** The curated conjecture library, transcoded by the engine and read back structurally. */
  async conjectures({ turtle }) {
    const converted = await callTool("convert", { data: turtle, from: "turtle", to: "nquads" });
    return conjectureLibrary(converted.output);
  },

  /** The distribution catalog + the derived formal-concept lattice. */
  async runtime() {
    return callTool("distribution_matrix", {});
  },

  /** The recorded trajectory, in the shape the shipped native auditor discovers. */
  async trajectory() {
    return { nquads: session.trajectoryNQuads(), anchor: session.anchor, calls: session.calls.length };
  },

  /** The permalink for the session so far. */
  async permalink() {
    return { fragment: session.permalink() };
  },

  /**
   * Export the session as a `.gts` segment.
   *
   * The store is read out of the engine itself — `store_segment` for the claim package
   * (the one tool that SERIALIZES it; `recall` answers a query and returns a ranked JSON
   * view, which is not a snapshot) and `list_candidates` for the candidate library — and
   * re-graphed into the session-store segment graph, so the export carries the store AS IT
   * STOOD, not a reconstruction.
   */
  async export() {
    const store = await callTool("store_segment", {});
    const candidates = await callTool("list_candidates", {}, ({ phase, tool, segment }) => {
      post({ event: "segment", phase, tool, segment });
    });
    return { gts: exportSegment(session, storeReading(store, candidates)) };
  },
};

self.addEventListener("message", (event) => {
  // Same-origin guard: refuse a frame that did not come from our own origin.
  if (event.origin !== undefined && event.origin !== "" && event.origin !== SELF_ORIGIN) {
    post({ id: null, ok: false, op: "origin", error: `refused a message from origin ${event.origin}` });
    return;
  }
  const { id, op, args } = event.data ?? {};
  // Dispatch is TOTAL, and "total" means the lookup itself cannot be tricked. `OPS[op]`
  // is not a table lookup — it walks the prototype chain, so `op: "constructor"`,
  // `"toString"` or `"valueOf"` resolves to an inherited function and gets INVOKED, which
  // is neither a registered operation nor the named hard error this worker promises. An
  // OWN, callable entry is the only thing that dispatches.
  const handler = Object.hasOwn(OPS, op) && typeof OPS[op] === "function" ? OPS[op] : null;
  if (handler === null) {
    post({ id, ok: false, op, error: `unknown console operation \`${op}\`` });
    return;
  }
  // Every handler is try/caught, and every failure is POSTED. Nothing is swallowed.
  (async () => {
    try {
      post({ id, ok: true, value: await handler(args ?? {}) });
    } catch (error) {
      post({ id, ok: false, op, error: String(error?.message ?? error) });
    }
  })().catch((error) => {
    post({ id, ok: false, op, error: `console worker dispatch failed: ${String(error?.message ?? error)}` });
  });
});

/**
 * The DERIVED loss lattice over a round-trip differential.
 *
 * Each successful target realized a set of loss codes; those sets are ordered by
 * INCLUSION, and that containment order is the lattice the realized ledger is read
 * against. It is derived from the run — there is no authored per-format loss table in the
 * console. A target that failed carries no drop set and is reported separately, so a
 * failure is never silently read as "lossless".
 */
export function lossLattice(rows) {
  const nodes = rows
    .filter((row) => row.ok)
    .map((row) => ({ format: row.format, codes: [...new Set(row.loss.map((l) => l.code))].sort() }));
  const edges = [];
  for (const lower of nodes) {
    for (const upper of nodes) {
      if (lower.format === upper.format) continue;
      const strictlyBelow =
        lower.codes.every((code) => upper.codes.includes(code)) && lower.codes.length < upper.codes.length;
      if (!strictlyBelow) continue;
      // Keep only COVER edges: no third node strictly between the two.
      const covered = nodes.some(
        (mid) =>
          mid.format !== lower.format &&
          mid.format !== upper.format &&
          lower.codes.every((c) => mid.codes.includes(c)) &&
          mid.codes.every((c) => upper.codes.includes(c)) &&
          mid.codes.length > lower.codes.length &&
          mid.codes.length < upper.codes.length,
      );
      if (!covered) edges.push({ from: lower.format, to: upper.format });
    }
  }
  return {
    nodes: nodes.sort((a, b) => a.format.localeCompare(b.format)),
    edges: edges.sort((a, b) => `${a.from}${a.to}`.localeCompare(`${b.from}${b.to}`)),
    failed: rows.filter((row) => !row.ok).map((row) => ({ format: row.format, error: row.error })),
  };
}
