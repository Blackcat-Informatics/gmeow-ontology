// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// `panes_are_derived_from_the_shipped_action_policy`, observed on the RENDERED nav.
//
// The DOM-free acceptance lane proves the derivation function agrees with the policy. What
// only a browser can say is that the thing on the screen is that derivation: the nav the
// reader clicks is built from `boot`'s answer, and `boot`'s answer is read out of the
// shipped `action_policy` at run time.
//
// So the comparison here is between the RENDERED pane ids and the policy the engine served,
// in both directions, plus the structural panes the console adds. And the negative half —
// that no tool name and no exclusion list is written down in the console's JavaScript — is
// checked over the bytes the SERVER answered, not over the source tree.

import { expect, test } from "../lib/test.mjs";

/**
 * The console's SURFACE-DERIVING modules, as the browser receives them.
 *
 * `examples/gallery.mjs` is deliberately absent: it is authored RDF and a pane binding per
 * worked vignette, so it names tools BY DESIGN. `session.mjs` reaches the store holders by
 * name for the same operational reason the worker does and is covered below.
 */
const CONSOLE_MODULES = [
  "/console/element.mjs",
  "/console/engine.worker.mjs",
  "/console/session.mjs",
  "/console/pkg/mcp-transport.mjs",
  "/assets/mcp-transport.mjs",
];

/**
 * The tool names the console's own code is allowed to spell, and why each one is there.
 *
 * `action_policy` derives the surface; `distribution_matrix` backs the runtime pane;
 * `convert` and `query_local` are the transcode/query verbs the transport exposes;
 * `store_segment` and `list_candidates` are the two store holders the session export reads.
 * Every one is a USE of a tool, never a list of the surface — and the set is pinned so that
 * a seventh name has to be argued for rather than appearing.
 */
const OPERATIONAL_NAMES = [
  "action_policy",
  "convert",
  "distribution_matrix",
  "list_candidates",
  "query_local",
  "store_segment",
];

test("the rendered pane set equals the shipped policy's read half, both directions", async ({ app }) => {
  const policy = await app.call("action_policy", {});
  const derived = await app.page.evaluate(async (nquads) => {
    const { actionPolicyPanes } = await import("/assets/mcp-transport.mjs");
    const { panes, excluded } = actionPolicyPanes(nquads);
    return { panes, excluded };
  }, policy.nquads);

  const rendered = await app.paneIds();
  const structural = rendered.filter((id) => id.startsWith("@"));
  const toolPanes = rendered.filter((id) => !id.startsWith("@"));

  expect(derived.panes.length, "the derived pane set must be non-empty").toBeGreaterThan(0);
  expect([...toolPanes].sort(), "rendered tool panes = the policy's read half").toEqual(
    [...derived.panes].sort(),
  );
  for (const name of derived.excluded) {
    expect(toolPanes, `${name} is a governed write and must not be rendered`).not.toContain(name);
  }
  expect(structural, "the five structural panes ride alongside the derived ones").toEqual([
    "@roundtrip",
    "@structure",
    "@gallery",
    "@session",
    "@runtime",
  ]);

  // The partition is exact against what the engine advertises.
  const advertised = await app.page.evaluate(async () => {
    const { listTools } = await import("/assets/mcp-transport.mjs");
    return (await listTools()).map((tool) => tool.name);
  });
  expect(
    derived.panes.length + derived.excluded.length,
    "panes ⊎ excluded = the advertised surface",
  ).toBe(advertised.length);
  expect([...derived.panes, ...derived.excluded].sort()).toEqual([...advertised].sort());

  // The write half, by VALUE: a policy that quietly re-typed a write as a read would grow a
  // pane instead of failing here.
  expect([...derived.excluded].sort()).toEqual([
    "refute_conjecture",
    "revise_belief",
    "store_claim",
    "store_conjecture",
    "submit_candidate",
    "withdraw_candidate",
  ]);
});

test("the rendered nav carries a working pane per derived tool", async ({ app }) => {
  const rendered = (await app.paneIds()).filter((id) => !id.startsWith("@"));
  // Selecting a derived pane renders that tool's own advertised schema as a form. Two are
  // driven here — one with arguments and one without — because a pane that renders nothing
  // is indistinguishable from a pane that is not there.
  await app.selectPane("lookup_term");
  await expect(app.page.locator("gmeow-console h2")).toHaveText("lookup_term");
  await expect(app.page.locator('gmeow-console input[id="f-lookup_term-term"]')).toHaveCount(1);

  await app.selectPane("action_policy");
  await expect(app.page.locator("gmeow-console h2")).toHaveText("action_policy");
  await expect(app.page.locator("gmeow-console fieldset .hint")).toHaveText(
    "This tool takes no arguments.",
  );

  // Every derived pane has a nav entry, and the nav has no entry that is not a derived pane
  // or a structural one — the both-directions claim on the rendering itself.
  expect(new Set(rendered).size, "no pane is rendered twice").toBe(rendered.length);
});

test("no tool name and no exclusion list is written into the console's JavaScript", async ({ app }) => {
  const policy = await app.call("action_policy", {});
  const derived = await app.page.evaluate(async (nquads) => {
    const { actionPolicyPanes } = await import("/assets/mcp-transport.mjs");
    return actionPolicyPanes(nquads);
  }, policy.nquads);

  const sources = await app.page.evaluate(async (urls) => {
    const out = {};
    for (const url of urls) out[url] = await (await fetch(url, { cache: "no-store" })).text();
    return out;
  }, CONSOLE_MODULES);

  // The names that must NOT appear: every governed write (an exclusion list), and every
  // derived pane name (a pane list). Comments are stripped first, because the modules
  // legitimately DISCUSS the derivation in prose — `store_claim` is named in
  // `element.mjs`'s header comment, and a naive substring scan would read that as a list.
  const strip = (text) =>
    text
      .replace(/\/\*[\s\S]*?\*\//g, " ")
      .split("\n")
      .map((line) => (/^\s*\/\//.test(line) ? "" : line))
      .join("\n");

  const spelled = new Set();
  for (const [url, text] of Object.entries(sources)) {
    const code = strip(text);
    for (const name of derived.excluded) {
      expect(code, `${url} names the governed write \`${name}\` in code`).not.toContain(`"${name}"`);
    }
    for (const name of [...derived.panes, ...derived.excluded]) {
      if (code.includes(`"${name}"`)) spelled.add(name);
    }
  }

  // What the console's code spells is exactly the operational set — never the surface. The
  // equality is asserted in both directions, so a pane list could not hide inside it and an
  // operational name that stopped being used could not linger.
  expect([...spelled].sort(), "the console's code spells exactly its operational tool uses").toEqual(
    [...OPERATIONAL_NAMES].sort(),
  );
  for (const name of spelled) {
    expect(derived.panes, `\`${name}\` must be a read tool`).toContain(name);
    expect(derived.excluded, `\`${name}\` must not be a governed write`).not.toContain(name);
  }
  expect(
    spelled.size,
    "the console must spell far fewer tool names than the surface has, or it is a list",
  ).toBeLessThan(derived.panes.length / 2);
});
