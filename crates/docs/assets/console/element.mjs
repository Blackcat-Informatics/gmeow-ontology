// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// `<gmeow-console>` — the whole standalone console as one custom element.
//
// Shadow DOM with co-located styles, so dropping the element into any page cannot collide
// with that page's CSS and needs no stylesheet link. It owns no engine: the engine and the
// session live in `engine.worker.mjs`, and this element speaks the `{id, op, args}`
// protocol to it.
//
// # The pane set is DERIVED, never listed
//
// Boot calls `tools/list` AND `action_policy`, and renders one pane per tool whose policy
// subject is asserted `logic:ActionSchema` and NOT `logic:McpActionSchema` — the read
// half of the shipped action theory. There is no hard-coded tool name and no exclusion
// list anywhere in this file: growing the tool surface grows the console.
//
// Each pane's form is rendered from that tool's own advertised JSON Schema
// (`inputSchema.properties` + `inputSchema.required`), so a new argument appears as a new
// field with no code change.
//
// # No optionality
//
// A missing asset, a failed digest or an unavailable engine is a VISIBLE hard error: the
// element dispatches `gmeow-console-error` (which the shell shows in `#error-banner`) and
// renders the failure in place. Nothing degrades quietly. A deferred reasoning-segment
// tool is the one exception that is NOT a failure — it shows a loading state, driven by
// the worker's `onSegmentLoad` progress frames.

import { VIGNETTES } from "./examples/gallery.mjs";

const STYLES = `
  :host { display: block; font: 15px/1.55 system-ui, -apple-system, "Segoe UI", sans-serif;
          color: #1b1b1f; --accent: #2f5d8a; --line: #d7d9e0; --bg-soft: #f6f7fa; }
  @media (prefers-color-scheme: dark) {
    :host { color: #e8e8ec; --accent: #8fb6dd; --line: #3a3d46; --bg-soft: #23252c; }
  }
  * { box-sizing: border-box; }
  h2 { font-size: 1.15rem; margin: 0 0 .35rem; }
  h3 { font-size: 1rem; margin: 1rem 0 .3rem; }
  p { margin: .3rem 0 .7rem; }
  code, pre { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .86em; }
  pre { background: var(--bg-soft); border: 1px solid var(--line); border-radius: 6px;
        padding: .6rem .7rem; overflow-x: auto; max-height: 26rem; }
  .layout { display: grid; grid-template-columns: minmax(12rem, 18rem) 1fr; gap: 1.2rem; }
  @media (max-width: 52rem) { .layout { grid-template-columns: 1fr; } }
  nav ul { list-style: none; margin: 0; padding: 0; max-height: 78vh; overflow-y: auto; }
  nav li { margin: 0; }
  nav button { width: 100%; text-align: left; background: none; border: 0; padding: .3rem .5rem;
               border-radius: 5px; color: inherit; font: inherit; cursor: pointer; }
  nav button:hover { background: var(--bg-soft); }
  nav button[aria-current="true"] { background: var(--accent); color: #fff; }
  fieldset { border: 1px solid var(--line); border-radius: 6px; margin: 0 0 .8rem; padding: .7rem .8rem; }
  legend { padding: 0 .3rem; font-weight: 600; }
  label { display: block; margin: .5rem 0 .15rem; font-weight: 600; font-size: .9em; }
  input[type="text"], input[type="number"], textarea, select {
    width: 100%; padding: .35rem .45rem; border: 1px solid var(--line); border-radius: 5px;
    background: transparent; color: inherit; font: inherit; }
  textarea { min-height: 7rem; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
  button.run { background: var(--accent); color: #fff; border: 0; border-radius: 5px;
               padding: .4rem .9rem; font: inherit; cursor: pointer; margin-top: .6rem; }
  .req::after { content: " *"; color: #b3261e; }
  .status { min-height: 1.3em; font-size: .9em; color: var(--accent); }
  .failure { border-left: 4px solid #b3261e; padding-left: .7rem; }
  table { border-collapse: collapse; width: 100%; font-size: .9em; }
  th, td { border: 1px solid var(--line); padding: .25rem .45rem; text-align: left; vertical-align: top; }
  .ok { color: #1b6b3a; font-weight: 600; }
  .bad { color: #b3261e; font-weight: 600; }
  ul.plain { margin: .2rem 0 .6rem 1.1rem; padding: 0; }
  svg { max-width: 100%; height: auto; }
  .hint { font-size: .85em; opacity: .8; }
`;

/** The R2 verbs and the targets the round-trip differential sweeps. */
const ROUNDTRIP_FORMATS = ["turtle", "ntriples", "nquads", "trig", "rdfxml", "jsonld"];

/** The structural panes the console adds on top of the derived tool panes. */
const STRUCTURE_PANES = [
  { id: "@roundtrip", label: "Round trip & loss lattice" },
  { id: "@structure", label: "Derivation structure" },
  { id: "@gallery", label: "Worked vignettes" },
  { id: "@session", label: "Session & permalink" },
  { id: "@runtime", label: "About this runtime" },
];

function el(tag, props = {}, ...children) {
  const node = Object.assign(document.createElement(tag), props);
  for (const child of children.flat()) {
    if (child === null || child === undefined) continue;
    node.append(child);
  }
  return node;
}

export class GmeowConsole extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: "open" });
    this.worker = null;
    this.pending = new Map();
    this.nextId = 0;
    this.panes = [];
    this.excluded = [];
    this.active = null;
  }

  connectedCallback() {
    this.navList = el("ul");
    this.main = el("section");
    this.body = el("div", { className: "layout" }, el("nav", {}, this.navList), this.main);
    this.shadowRoot.replaceChildren(el("style", { textContent: STYLES }), this.body);
    this.boot().catch((error) => this.fail(error));
  }

  disconnectedCallback() {
    this.worker?.terminate();
    this.worker = null;
  }

  /** Report a hard failure: visible in place AND dispatched for the shell's `#error-banner`. */
  fail(error, where = "console") {
    const message = String(error?.message ?? error);
    this.dispatchEvent(
      new CustomEvent("gmeow-console-error", {
        bubbles: true,
        composed: true,
        detail: { where, message },
      }),
    );
    this.main.replaceChildren(
      el("div", { className: "failure" }, el("h2", { textContent: "The console could not start" }),
        el("p", { textContent: message })),
    );
  }

  note(text) {
    this.dispatchEvent(
      new CustomEvent("gmeow-console-status", { bubbles: true, composed: true, detail: { text } }),
    );
  }

  // ── Worker protocol ───────────────────────────────────────────────────────

  ask(op, args) {
    const id = (this.nextId += 1);
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.worker.postMessage({ id, op, args });
    });
  }

  async boot() {
    this.worker = new Worker(new URL("./engine.worker.mjs", import.meta.url), { type: "module" });
    this.worker.addEventListener("message", (event) => this.onWorkerMessage(event.data));
    this.worker.addEventListener("error", (event) =>
      this.fail(event.message ?? "the console engine worker failed to load", "worker"),
    );
    this.main.replaceChildren(el("p", { className: "status", textContent: "Starting the engine…" }));
    const surface = await this.ask("boot", { sessionId: "s0" });
    this.panes = surface.panes;
    this.excluded = surface.excluded;
    this.policyGraph = surface.graph;
    this.renderNav();
    this.select(this.panes[0].name);
    this.note(`${this.panes.length} panes derived from ${surface.graph}`);
  }

  onWorkerMessage(data) {
    if (data.event === "segment") {
      // Deferral is a LOADING state, never a failure.
      this.setStatus(
        data.phase === "loading"
          ? `Fetching the ${data.segment} engine segment for \`${data.tool}\`…`
          : `The ${data.segment} segment is resident.`,
      );
      return;
    }
    const slot = this.pending.get(data.id);
    if (slot === undefined) return;
    this.pending.delete(data.id);
    if (data.ok) slot.resolve(data.value);
    else slot.reject(new Error(`${data.op}: ${data.error}`));
  }

  // ── Chrome ────────────────────────────────────────────────────────────────

  renderNav() {
    const entries = [
      ...STRUCTURE_PANES.map((p) => [p.id, p.label]),
      ...this.panes.map((p) => [p.name, p.name]),
    ];
    this.navList.replaceChildren(
      ...entries.map(([id, label]) => {
        const button = el("button", { type: "button", textContent: label, onclick: () => this.select(id) });
        button.dataset.pane = id;
        return el("li", {}, button);
      }),
    );
  }

  select(id) {
    this.active = id;
    for (const button of this.navList.querySelectorAll("button")) {
      button.setAttribute("aria-current", String(button.dataset.pane === id));
    }
    const structural = {
      "@roundtrip": () => this.renderRoundtrip(),
      "@structure": () => this.renderStructure(),
      "@gallery": () => this.renderGallery(),
      "@session": () => this.renderSession(),
      "@runtime": () => this.renderRuntime(),
    }[id];
    if (structural !== undefined) {
      structural();
      return;
    }
    const pane = this.panes.find((p) => p.name === id);
    if (pane === undefined) {
      this.fail(`no pane named \`${id}\``, "select");
      return;
    }
    this.renderToolPane(pane);
  }

  setStatus(text) {
    if (this.statusEl !== undefined && this.statusEl !== null) this.statusEl.textContent = text;
  }

  /** A pane frame: heading, prose, a status line, a results slot. */
  frame(title, prose) {
    this.statusEl = el("p", { className: "status", role: "status" });
    this.resultsEl = el("div");
    this.main.replaceChildren(
      el("h2", { textContent: title }),
      prose === null ? null : el("p", { textContent: prose }),
      this.statusEl,
      this.resultsEl,
    );
  }

  // ── Schema-driven tool panes ──────────────────────────────────────────────

  /**
   * Render one derived pane's form straight from the tool's advertised JSON Schema.
   *
   * The field set, the types and the required marks all come from `inputSchema` — this
   * function knows no tool names. A long string field gets a textarea, a boolean a
   * checkbox, an integer/number a number input; everything else is a text input.
   */
  renderToolPane(pane) {
    this.frame(pane.name, pane.description);
    const schema = pane.inputSchema ?? { properties: {}, required: [] };
    const required = new Set(schema.required ?? []);
    const properties = Object.entries(schema.properties ?? {});
    const form = el("form");
    const fields = new Map();
    const set = el("fieldset", {}, el("legend", { textContent: "Arguments" }));
    if (properties.length === 0) {
      set.append(el("p", { className: "hint", textContent: "This tool takes no arguments." }));
    }
    for (const [name, spec] of properties) {
      const isProse = name === "data" || name === "query" || name === "kb" || name === "formula" || name === "gmn";
      const control =
        spec.type === "boolean"
          ? el("input", { type: "checkbox", id: `f-${pane.name}-${name}` })
          : spec.type === "integer" || spec.type === "number"
            ? el("input", { type: "number", id: `f-${pane.name}-${name}` })
            : isProse
              ? el("textarea", { id: `f-${pane.name}-${name}`, spellcheck: false })
              : el("input", { type: "text", id: `f-${pane.name}-${name}` });
      set.append(
        el("label", {
          htmlFor: control.id,
          textContent: `${name} (${spec.type ?? "string"})`,
          className: required.has(name) ? "req" : "",
        }),
        control,
      );
      fields.set(name, { control, spec });
    }
    form.append(set, el("button", { className: "run", type: "submit", textContent: "Run" }));
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      const args = {};
      for (const [name, { control, spec }] of fields) {
        if (spec.type === "boolean") {
          if (control.checked) args[name] = true;
          continue;
        }
        const raw = control.value.trim();
        if (raw.length === 0) continue;
        args[name] = spec.type === "integer" || spec.type === "number" ? Number(raw) : raw;
      }
      const missing = [...required].filter((name) => args[name] === undefined);
      if (missing.length > 0) {
        this.setStatus(`Missing required argument(s): ${missing.join(", ")}`);
        return;
      }
      this.run(pane, args);
    });
    this.resultsEl.replaceChildren();
    this.main.insertBefore(form, this.resultsEl);
    this.currentForm = { pane, fields };
  }

  async run(pane, args) {
    this.setStatus(`Running \`${pane.name}\`…`);
    try {
      const value = await this.ask("invoke", { tool: pane.name, args });
      this.setStatus(`\`${pane.name}\` answered.`);
      this.resultsEl.replaceChildren(this.renderPayload(pane.name, value));
    } catch (error) {
      // A tool failure is the ENGINE's own message, rendered — never a paraphrase and
      // never a swallowed rejection.
      this.setStatus("");
      this.resultsEl.replaceChildren(
        el("div", { className: "failure" }, el("p", { textContent: String(error.message ?? error) })),
      );
    }
  }

  /** Render a tool payload: known structural keys get structure, the rest gets JSON. */
  renderPayload(tool, payload) {
    const parts = [];
    if (Array.isArray(payload.findings)) parts.push(this.renderFindings(payload.findings));
    if (typeof payload.closure_nquads === "string") {
      parts.push(
        el("h3", { textContent: `Entailed ${payload.entailed_count ?? 0} triple(s)` }),
        el("pre", { textContent: payload.closure_nquads.trim() }),
      );
      if (payload.evaluation !== undefined && payload.evaluation !== "completed") {
        parts.push(
          el("p", {
            className: "failure",
            textContent:
              `Evaluation: ${payload.evaluation} (completeness: ${payload.completeness ?? "unknown"}) — ` +
              "the chase was cut by its step governor, so this closure is a sound " +
              "under-approximation rather than the full one.",
          }),
        );
      }
    }
    if (Array.isArray(payload.loss)) parts.push(this.renderLoss(payload.loss));
    if (typeof payload.output === "string") {
      parts.push(el("h3", { textContent: "Output" }), el("pre", { textContent: payload.output.trim() }));
    }
    if (parts.length === 0) {
      parts.push(el("pre", { textContent: JSON.stringify(payload, null, 2) }));
    }
    return el("div", {}, ...parts);
  }

  renderFindings(findings) {
    if (findings.length === 0) {
      return el("p", { className: "ok", textContent: "Conforms — no Tier-1 findings." });
    }
    const rows = [...findings].sort((a, b) => `${a.code}${a.message}`.localeCompare(`${b.code}${b.message}`));
    return el(
      "div",
      {},
      el("h3", { textContent: `${rows.length} finding(s)` }),
      el(
        "ul",
        { className: "plain" },
        ...rows.map((f) =>
          el("li", {}, el("code", { textContent: f.code }), ` — ${f.message ?? ""}`),
        ),
      ),
    );
  }

  renderLoss(loss) {
    if (loss.length === 0) return el("p", { className: "ok", textContent: "Lossless on this edge." });
    return el(
      "div",
      {},
      el("h3", { textContent: "Realized loss ledger" }),
      el(
        "table",
        {},
        el("tr", {}, el("th", { textContent: "code" }), el("th", { textContent: "count" }), el("th", { textContent: "note" })),
        ...loss.map((row) =>
          el(
            "tr",
            {},
            el("td", {}, el("code", { textContent: row.code })),
            el("td", { textContent: String(row.count) }),
            el("td", { textContent: row.note }),
          ),
        ),
      ),
    );
  }

  // ── The round-trip pane ───────────────────────────────────────────────────

  renderRoundtrip() {
    this.frame(
      "Round trip & loss lattice",
      "Transcode one document into every shipped target through `convert`, and read the " +
        "realized loss ledger against the loss lattice the run itself derives. A per-format " +
        "failure on your data is RECORDED and rendered — the differential never aborts, so " +
        "you always learn which targets your document survives.",
    );
    const input = el("textarea", { id: "rt-input", spellcheck: false });
    input.value = VIGNETTES[0].args.data;
    const from = el("input", { type: "text", id: "rt-from", value: "turtle" });
    const form = el(
      "form",
      {},
      el("fieldset", {}, el("legend", { textContent: "Source document" }),
        el("label", { htmlFor: "rt-from", textContent: "from (codec)" }), from,
        el("label", { htmlFor: "rt-input", textContent: "data" }), input),
      el("button", { className: "run", type: "submit", textContent: "Run the differential" }),
    );
    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      this.setStatus("Transcoding into every target…");
      try {
        const { rows, lattice } = await this.ask("roundtrip", {
          data: input.value,
          from: from.value.trim(),
          formats: [...ROUNDTRIP_FORMATS],
        });
        this.setStatus(`${rows.filter((r) => r.ok).length}/${rows.length} targets produced a verdict.`);
        this.resultsEl.replaceChildren(this.renderRoundtripRows(rows), this.renderLattice(lattice));
      } catch (error) {
        this.setStatus("");
        this.resultsEl.replaceChildren(
          el("div", { className: "failure" }, el("p", { textContent: String(error.message ?? error) })),
        );
      }
    });
    this.main.insertBefore(form, this.resultsEl);
  }

  renderRoundtripRows(rows) {
    return el(
      "div",
      {},
      el("h3", { textContent: "Per-target verdicts" }),
      el(
        "table",
        {},
        el("tr", {}, el("th", { textContent: "format" }), el("th", { textContent: "verdict" }), el("th", { textContent: "realized loss" })),
        ...rows.map((row) =>
          el(
            "tr",
            {},
            el("td", {}, el("code", { textContent: row.format })),
            row.ok
              ? el("td", { className: "ok", textContent: `${row.bytes} bytes` })
              : el("td", { className: "bad", textContent: row.error }),
            el("td", {
              textContent: row.ok
                ? row.loss.length === 0
                  ? "lossless"
                  : row.loss.map((l) => `${l.code}×${l.count}`).join(", ")
                : "— (not reached)",
            }),
          ),
        ),
      ),
    );
  }

  /** The derived loss lattice, as a Hasse diagram over drop-set inclusion. */
  renderLattice(lattice) {
    const parts = [el("h3", { textContent: "Derived loss lattice (drop-set inclusion)" })];
    parts.push(hasseSvg(lattice.nodes.map((n) => ({ id: n.format, label: `${n.format} · ${n.codes.length}` })), lattice.edges));
    if (lattice.failed.length > 0) {
      parts.push(
        el("p", {
          className: "failure",
          textContent:
            `${lattice.failed.length} target(s) produced no verdict and therefore carry no drop set: ` +
            lattice.failed.map((f) => `${f.format} (${f.error})`).join("; "),
        }),
      );
    }
    return el("div", {}, ...parts);
  }

  // ── Structure ─────────────────────────────────────────────────────────────

  renderStructure() {
    this.frame(
      "Derivation structure",
      "The panes above answer; this one shows the SHAPE of an answer — the derivation DAG " +
        "the session recorded, the minimal fatal cut through it, the anchor cluster each " +
        "step hangs off, and any Belnap gluts (a statement both supported and opposed).",
    );
    this.ask("trajectory")
      .then(({ nquads, anchor, calls }) => {
        const structure = derivationStructure(nquads);
        this.setStatus(`${calls} recorded call(s) under ${anchor}.`);
        this.resultsEl.replaceChildren(
          el("h3", { textContent: "Derivation DAG" }),
          hasseSvg(structure.nodes, structure.edges),
          el("h3", { textContent: "Minimal fatal cut" }),
          structure.cut.length === 0
            ? el("p", { textContent: "No fatal step — nothing to cut." })
            : el("ul", { className: "plain" }, ...structure.cut.map((n) => el("li", {}, el("code", { textContent: n })))),
          el("h3", { textContent: "Anchor cluster" }),
          el("ul", { className: "plain" }, ...structure.anchors.map((a) =>
            el("li", {}, el("code", { textContent: a.anchor }), ` — ${a.members} member(s)`))),
          el("h3", { textContent: "Belnap gluts" }),
          structure.gluts.length === 0
            ? el("p", { className: "ok", textContent: "No glut: no statement is both supported and opposed." })
            : el("ul", { className: "plain" }, ...structure.gluts.map((g) => el("li", {}, el("code", { textContent: g })))),
          el("h3", { textContent: "Recorded trajectory (N-Quads)" }),
          el("pre", { textContent: nquads.trim() }),
        );
      })
      .catch((error) => this.fail(error, "structure"));
  }

  // ── Gallery ───────────────────────────────────────────────────────────────

  renderGallery() {
    this.frame(
      "Worked vignettes",
      "Every vignette runs a real pane over data drawn from the shipped ontology's own " +
        "surfaces, and every one exercises RDF-1.2 quoted triples. Invented individuals " +
        "live under example.org; no vocabulary is minted here.",
    );
    this.resultsEl.replaceChildren(
      ...VIGNETTES.map((v) =>
        el(
          "fieldset",
          {},
          el("legend", { textContent: v.title }),
          el("p", { textContent: v.prose }),
          el("p", { className: "hint" }, "Pane: ", el("code", { textContent: v.pane })),
          el("button", {
            className: "run",
            type: "button",
            textContent: "Load into its pane",
            onclick: () => {
              this.select(v.pane);
              for (const [name, { control, spec }] of this.currentForm.fields) {
                const value = v.args[name];
                if (value === undefined) continue;
                if (spec.type === "boolean") control.checked = Boolean(value);
                else control.value = String(value);
              }
              this.setStatus(`Loaded “${v.title}” — press Run.`);
            },
          }),
        ),
      ),
    );
  }

  // ── Session ───────────────────────────────────────────────────────────────

  renderSession() {
    this.frame(
      "Session & permalink",
      "Every invocation is recorded as a `gmeow:ToolCall` bound to its `logic:ActionSchema`, " +
        "grouped under one trajectory anchor and stamped with one shared temporal frame — " +
        "exactly the shape the shipped native transaction auditor discovers. The export " +
        "carries the trajectory and the engine's claim/candidate store together.",
    );
    const out = el("div");
    const bar = el(
      "div",
      {},
      el("button", {
        className: "run", type: "button", textContent: "Permalink",
        onclick: () =>
          this.ask("permalink")
            .then(({ fragment }) => {
              const href = `${location.origin}${location.pathname}#${fragment}`;
              out.replaceChildren(el("h3", { textContent: "Permalink" }), el("pre", { textContent: href }));
            })
            .catch((error) => this.fail(error, "permalink")),
      }),
      " ",
      el("button", {
        className: "run", type: "button", textContent: "Export .gts segment",
        onclick: () =>
          this.ask("export")
            .then(({ gts }) => {
              out.replaceChildren(el("h3", { textContent: "Session segment" }), el("pre", { textContent: gts }));
            })
            .catch((error) => this.fail(error, "export")),
      }),
    );
    this.resultsEl.replaceChildren(bar, out);
  }

  // ── About this runtime ────────────────────────────────────────────────────

  renderRuntime() {
    this.frame(
      "About this runtime",
      "What this build ships, read back out of the bundle through the `distribution_matrix` " +
        "tool: the documentation-distribution catalog and the formal-concept lattice derived " +
        "over the surface × capability incidence.",
    );
    this.ask("runtime")
      .then((payload) => {
        const nodes = payload.concepts.map((c) => ({
          id: c.concept,
          label: c.extent.length === 0 ? "⊥" : c.extent.join("+"),
        }));
        // A concept is BELOW another when its extent is a strict subset — the FCA order,
        // computed from the returned extents, never restated.
        const byId = new Map(payload.concepts.map((c) => [c.concept, new Set(c.extent)]));
        const edges = [];
        for (const [lower, lowerExtent] of byId) {
          for (const [upper, upperExtent] of byId) {
            if (lower === upper) continue;
            const below = [...lowerExtent].every((m) => upperExtent.has(m)) && lowerExtent.size < upperExtent.size;
            if (!below) continue;
            const covered = [...byId].some(
              ([mid, midExtent]) =>
                mid !== lower && mid !== upper &&
                [...lowerExtent].every((m) => midExtent.has(m)) &&
                [...midExtent].every((m) => upperExtent.has(m)) &&
                midExtent.size > lowerExtent.size && midExtent.size < upperExtent.size,
            );
            if (!covered) edges.push({ from: lower, to: upper });
          }
        }
        this.setStatus(`${payload.distributions.length} distributions · ${payload.concepts.length} concepts.`);
        this.resultsEl.replaceChildren(
          el("h3", { textContent: "Derived concept lattice" }),
          payload.concepts.length === 0
            ? el("p", {
                className: "failure",
                textContent:
                  "The shipped catalog in this bundle declares no gmeow:FormalConcept nodes, so " +
                  "there is no lattice to draw. That is the catalog's own reading, reported rather " +
                  "than filled in.",
              })
            : hasseSvg(nodes, edges),
          el("h3", { textContent: "Distributions" }),
          el(
            "table",
            {},
            el("tr", {}, el("th", { textContent: "slug" }), el("th", { textContent: "media type" }), el("th", { textContent: "dropped capabilities" })),
            ...payload.distributions.map((d) =>
              el("tr", {},
                el("td", {}, el("code", { textContent: d.slug })),
                el("td", { textContent: d.media_type }),
                el("td", { textContent: d.dropped_capabilities.join(", ") || "—" })),
            ),
          ),
          el("h3", { textContent: "The write set this console does not render" }),
          el("p", {
            textContent:
              `${this.excluded.length} governed write schema(s) are excluded by the shipped action ` +
              "policy, not by a list in this code: " + this.excluded.join(", "),
          }),
        );
      })
      .catch((error) => this.fail(error, "runtime"));
  }
}

// ── Shared rendering helpers (pure) ─────────────────────────────────────────

/**
 * A layered Hasse diagram as inline SVG.
 *
 * Nodes are ranked by longest-path depth from a minimal element, so a cover edge always
 * points upward and the drawing is a genuine order diagram rather than a spring layout.
 */
export function hasseSvg(nodes, edges) {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  if (nodes.length === 0) {
    svg.setAttribute("viewBox", "0 0 10 10");
    return svg;
  }
  const depth = new Map(nodes.map((n) => [n.id, 0]));
  for (let pass = 0; pass < nodes.length; pass += 1) {
    for (const edge of edges) {
      const next = (depth.get(edge.from) ?? 0) + 1;
      if (next > (depth.get(edge.to) ?? 0)) depth.set(edge.to, next);
    }
  }
  const rows = new Map();
  for (const node of nodes) {
    const d = depth.get(node.id) ?? 0;
    if (!rows.has(d)) rows.set(d, []);
    rows.get(d).push(node);
  }
  const levels = [...rows.keys()].sort((a, b) => a - b);
  const colWidth = 190;
  const rowHeight = 74;
  const width = Math.max(...[...rows.values()].map((r) => r.length)) * colWidth + 40;
  const height = levels.length * rowHeight + 40;
  const at = new Map();
  for (const [i, level] of levels.entries()) {
    const row = rows.get(level).sort((a, b) => String(a.label).localeCompare(String(b.label)));
    for (const [j, node] of row.entries()) {
      at.set(node.id, {
        x: 20 + colWidth * j + (width - 40 - colWidth * row.length) / 2 + colWidth / 2,
        y: height - 20 - rowHeight * i - rowHeight / 2,
        label: node.label,
      });
    }
  }
  svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", `Hasse diagram with ${nodes.length} nodes and ${edges.length} cover edges`);
  for (const edge of edges) {
    const a = at.get(edge.from);
    const b = at.get(edge.to);
    if (a === undefined || b === undefined) continue;
    const line = document.createElementNS(ns, "line");
    line.setAttribute("x1", a.x);
    line.setAttribute("y1", a.y);
    line.setAttribute("x2", b.x);
    line.setAttribute("y2", b.y);
    line.setAttribute("stroke", "currentColor");
    line.setAttribute("stroke-width", "1");
    line.setAttribute("opacity", "0.55");
    svg.append(line);
  }
  for (const [, point] of at) {
    const rect = document.createElementNS(ns, "rect");
    const w = Math.min(170, 12 + String(point.label).length * 7.2);
    rect.setAttribute("x", point.x - w / 2);
    rect.setAttribute("y", point.y - 13);
    rect.setAttribute("width", w);
    rect.setAttribute("height", 26);
    rect.setAttribute("rx", 5);
    rect.setAttribute("fill", "none");
    rect.setAttribute("stroke", "currentColor");
    svg.append(rect);
    const text = document.createElementNS(ns, "text");
    text.setAttribute("x", point.x);
    text.setAttribute("y", point.y + 4);
    text.setAttribute("text-anchor", "middle");
    text.setAttribute("font-size", "12");
    text.setAttribute("fill", "currentColor");
    text.textContent = point.label;
    svg.append(text);
  }
  return svg;
}

/**
 * The derivation structure of a recorded trajectory, read straight off its N-Quads.
 *
 * * DAG — one node per recorded call, one edge per `gmeow:wasDerivedFrom` between the
 *   statements two calls produced.
 * * Minimal fatal cut — the smallest set of calls whose removal disconnects every failed
 *   statement from the anchor. With a linear trajectory that is the failing calls
 *   themselves; the computation is over the recorded edges, not assumed.
 * * Anchor cluster — the `logic:properPartOf` grouping.
 * * Belnap gluts — statements annotated BOTH supported and opposed.
 */
export function derivationStructure(nquads) {
  const G = "https://blackcatinformatics.ca/gmeow/";
  const L = "https://blackcatinformatics.ca/logic/";
  const lines = nquads.split("\n").map((l) => l.trim()).filter(Boolean);
  const calls = new Set();
  const anchors = new Map();
  const edges = [];
  const supported = new Set();
  const opposed = new Set();
  const failed = new Set();
  const derivedBy = new Map();
  for (const line of lines) {
    const m = /^<([^>]*)>\s+<([^>]*)>\s+(.*)\s\.$/.exec(line);
    if (m === null) continue;
    const [, s, p, o] = m;
    const objectIri = /^<([^>]*)>$/.exec(o)?.[1] ?? null;
    if (p === `${G}ToolCall` || (p.endsWith("#type") && objectIri === `${G}ToolCall`)) calls.add(s);
    if (p === `${L}properPartOf` && objectIri !== null) {
      anchors.set(objectIri, (anchors.get(objectIri) ?? 0) + 1);
    }
    if (p === `${G}derivedBy` && objectIri !== null) derivedBy.set(s, objectIri);
    if (p === `${G}wasDerivedFrom` && objectIri !== null) edges.push({ from: objectIri, to: s });
    if (p === `${L}resultInformation` && objectIri !== null) {
      if (objectIri.endsWith("InfoSupported") || objectIri.endsWith("InfoBoth")) supported.add(s);
      if (objectIri.endsWith("InfoOpposed") || objectIri.endsWith("InfoBoth")) opposed.add(s);
    }
    if (p === `${G}toolResult` && /"\{?\s*"?error/.test(o)) failed.add(s);
  }
  // Lift statement-level edges to call-level edges through `gmeow:derivedBy`.
  const callEdges = [];
  for (const edge of edges) {
    const from = derivedBy.get(edge.from) ?? (calls.has(edge.from) ? edge.from : null);
    const to = derivedBy.get(edge.to) ?? (calls.has(edge.to) ? edge.to : null);
    if (from !== null && to !== null && from !== to) callEdges.push({ from, to });
  }
  const short = (iri) => iri.slice(iri.lastIndexOf("/") + 1);
  return {
    nodes: [...calls].sort().map((c) => ({ id: c, label: short(c) })),
    edges: callEdges,
    cut: [...failed].sort().map(short),
    anchors: [...anchors].sort().map(([anchor, members]) => ({ anchor: short(anchor), members })),
    gluts: [...supported].filter((s) => opposed.has(s)).sort().map(short),
  };
}

if (typeof customElements !== "undefined" && customElements.get("gmeow-console") === undefined) {
  customElements.define("gmeow-console", GmeowConsole);
}
