// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Every read tool, dispatched through the ASSEMBLED worker, on a real input.
//
// The sweep is total by construction: `TOOL_INPUTS`'s key set is asserted EQUAL to the pane
// set the shipped `action_policy` derives, in both directions, before anything is run. A
// tool added to the ontology's read surface therefore fails this lane until it is given a
// real input — the coverage claim can never be satisfied by quietly skipping one.
//
// Each call goes through `element.ask("invoke", …)`, so the frame crosses the real worker
// boundary, resolves the emitted `console/pkg/mcp-transport.mjs` forwarder, and demand-loads
// the reasoning segment for the tools that live there. Nothing is imported into the page and
// called directly.

import { expect, test } from "../lib/test.mjs";
import { TOOL_INPUTS } from "../lib/fixtures.mjs";

test("the argument table covers exactly the derived read surface", async ({ app }) => {
  const policy = await app.call("action_policy", {});
  const { panes } = await app.page.evaluate(async (nquads) => {
    const { actionPolicyPanes } = await import("/assets/mcp-transport.mjs");
    return actionPolicyPanes(nquads);
  }, policy.nquads);

  expect([...Object.keys(TOOL_INPUTS)].sort(), "the lane drives exactly the derived panes").toEqual(
    [...panes].sort(),
  );
});

test("every read tool dispatches through the assembled worker and answers", async ({ app }) => {
  const policy = await app.call("action_policy", {});
  const { panes } = await app.page.evaluate(async (nquads) => {
    const { actionPolicyPanes } = await import("/assets/mcp-transport.mjs");
    return actionPolicyPanes(nquads);
  }, policy.nquads);

  const context = { call: (tool, args) => app.call(tool, args) };
  const refused = [];
  const answered = [];
  for (const name of panes) {
    const args = await TOOL_INPUTS[name](context);
    const outcome = await app.attempt(name, args);
    if (outcome.ok) answered.push({ name, value: outcome.value });
    else refused.push({ name, error: outcome.error });
  }

  expect(
    refused,
    `these read tools could not be dispatched through the assembled console worker: ${JSON.stringify(
      refused,
      null,
      2,
    )}`,
  ).toEqual([]);
  expect(answered.length).toBe(panes.length);

  // Non-vacuity: every answer is a structured payload, and the engine reported an
  // affirmative verdict for all but the ones whose verdict is itself the answer.
  for (const { name, value } of answered) {
    expect(value === null || typeof value !== "object", `${name} returned no payload`).toBe(false);
  }
  const negative = answered.filter(({ value }) => value.ok === false);
  for (const { name, value } of negative) {
    // A payload-level `ok: false` is an engine VERDICT, not a dispatch failure, and it must
    // say what it decided. `explain_quad` reports "this quad is not in the budgeted closure"
    // that way, naming the content-addressed reifier it computed for the target it was
    // handed — which is what makes the call non-vacuous even when the verdict is negative.
    expect(typeof value.error, `${name} reported a verdict with no reason`).toBe("string");
    expect(value.error.length).toBeGreaterThan(0);
  }
});

test("the grounded-memory triad round-trips in the browser: store → recall → store_segment", async ({
  app,
}) => {
  // `store_claim`, `recall` and `store_segment` are served by ONE image, so a claim written
  // in the browser must be readable and serializable in the browser. When they were split
  // across the two wasm images this round trip was impossible and the session export was
  // dead — which is why the assertion is the round trip and not three separate calls.
  const text = `the console smoke lane wrote this claim ${Date.now()}`;
  const stored = await app.call("store_claim", { text, confidence: 0.8 });
  expect(stored.ok, `store_claim must commit: ${JSON.stringify(stored).slice(0, 300)}`).toBe(true);
  expect(stored.claim.text).toBe(text);

  const recalled = await app.call("recall", { query: "console smoke lane" });
  expect(recalled.ok).toBe(true);
  expect(
    recalled.claims.map((claim) => claim.text),
    "the claim written in this browser must be recallable in it",
  ).toContain(text);

  const segment = await app.call("store_segment", {});
  expect(segment.ok).toBe(true);
  expect(segment.claim_count, "the serialized store must carry the claim").toBeGreaterThan(0);
  expect(segment.nquads, "…as RDF, with its text intact").toContain(text);

  // The serialization is real N-Quads, parsed back by the console's own reader.
  const parsed = await app.page.evaluate(async (nquads) => {
    const { parseNQuads } = await import("/assets/mcp-transport.mjs");
    return parseNQuads(nquads).length;
  }, segment.nquads);
  expect(parsed, "the store segment must parse as N-Quads").toBeGreaterThan(0);

  // `list_candidates` is the other store holder the export reads; it answers over the same
  // in-memory store.
  const candidates = await app.call("list_candidates", {});
  expect(candidates.ok).toBe(true);
  expect(Array.isArray(candidates.candidates)).toBe(true);

  // …and the session export, driven through the worker, carries what the store held.
  const exported = await app.ask("export", {});
  expect(exported.gts).toContain("store-segment");
  expect(exported.gts, "the exported segment carries the claim the browser stored").toContain(text);
});
