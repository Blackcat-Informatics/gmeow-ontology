// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The permalink, encoded and decoded in the browser over a non-trivial recorded session.
//
// A permalink is the console's one durable artifact, and the claim it makes is exact: the
// invocation list goes in and the identical invocation list comes back — never a
// best-effort replay. So the session driven here is not one call: it is several, with
// different tools, structured arguments, prose carrying a quote and a backslash, and a
// numeric argument, because a codec that round-trips one flat string proves nothing about
// the one the console actually emits.
//
// The session runs on its OWN page, so the shared booted page's trajectory is not perturbed
// and the recorded list is exactly what this test recorded.

import { expect, test } from "../lib/test.mjs";
import { openConsole } from "../lib/console-page.mjs";
import { ANNOTATED_RECORD, BUNDLE_QUERY } from "../lib/fixtures.mjs";

test("permalink encode → decode round-trips a non-trivial session EXACTLY", async ({
  browser,
  origin,
}) => {
  const app = await openConsole(browser, origin);
  try {
    await app.ready();

    const invocations = [
      ["lookup_term", { term: "gmeow:ToolCall" }],
      ["docs_search", { query: 'a "quoted" needle and a \\ backslash', limit: 3 }],
      ["convert", { data: ANNOTATED_RECORD, from: "turtle", to: "nquads" }],
      ["query_local", { data: "", format: "turtle", scope: "bundle", query: BUNDLE_QUERY }],
    ];
    for (const [tool, args] of invocations) await app.call(tool, args);

    const { fragment } = await app.ask("permalink", {});
    expect(fragment.length, "a recorded session must produce a permalink").toBeGreaterThan(0);
    expect(fragment, "the permalink is <content address>.<payload>").toContain(".");

    const decoded = await app.page.evaluate(async (link) => {
      const { decodePermalink } = await import("/console/session.mjs");
      return decodePermalink(link);
    }, fragment);

    // EXACTLY: the tool names, in order, and every argument object byte for byte.
    expect(decoded.calls.map((call) => call.tool)).toEqual(invocations.map(([tool]) => tool));
    expect(decoded.calls.map((call) => call.args)).toEqual(invocations.map(([, args]) => args));
    for (const call of decoded.calls) {
      expect(typeof call.schema, "every replayed call names the action schema it instantiates").toBe(
        "string",
      );
      expect(call.schema.startsWith("http")).toBe(true);
    }
    // Results are NOT carried — a link replays against the reader's own engine.
    expect(JSON.stringify(decoded.calls)).not.toContain("blackcatinformatics.ca/gmeow/ToolCall");

    // A tampered payload is REFUSED, not best-effort replayed.
    const tampered = await app.page.evaluate(async (link) => {
      const { decodePermalink } = await import("/console/session.mjs");
      const [address, payload] = link.split(/\.(.*)/s);
      try {
        decodePermalink(`${address}.${payload.slice(0, -2)}`);
        return null;
      } catch (error) {
        return String(error.message);
      }
    }, fragment);
    expect(tampered, "a tampered permalink must be refused").not.toBeNull();
    expect(tampered).toContain("content address");

    // The session pane renders the same permalink the protocol returned.
    await app.selectPane("@session");
    await app.page.click('gmeow-console button.run:has-text("Permalink")');
    await expect(app.page.locator("gmeow-console pre")).toContainText(fragment, { timeout: 120_000 });
  } finally {
    await app.close();
  }
});
