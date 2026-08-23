// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The console distribution's declared capability contract, both sides, non-vacuously.
//
// The catalog declares `console` REPRESENTABLE for `LiveSparql`, `Interactivity`,
// `LiveReasoning` and `Diagrams`, and DROPPED for `SearchIndex` and `CrossLinkFidelity`.
// A capability claim that is only ever read back out of the catalog it was written into
// proves nothing, so each one is observed here on the running artifact:
//
//   * the three with a mapped tool are RUN, on a real input, and must answer;
//   * `Diagrams` has no mapped tool, so it carries a DOM observation instead — the
//     "About this runtime" pane's `<svg>` node-label set must EQUAL the `concepts` the
//     `distribution_matrix` tool returned, in both directions;
//   * the two dropped ones are observed absent in the emitted tree: no search index, and
//     no relative link reaching for a documentation-site page the console does not carry.

import { promises as fs } from "node:fs";
import { join, posix } from "node:path";

import { expect, test } from "../lib/test.mjs";
import { listFiles } from "../lib/tree.mjs";
import { BUNDLE_QUERY, SUBSUMPTION, SUBSUMPTION_ENTAILMENT } from "../lib/fixtures.mjs";

/** The absolute base every cross-distribution documentation link must carry. */
const PUBLISHED_SITE_BASE = "https://blackcatinformatics.ca/gmeow/docs/";

test("LiveSparql — a real query against the shipped bundle returns bindings", async ({ app }) => {
  const answer = await app.call("query_local", {
    data: "",
    format: "turtle",
    scope: "bundle",
    query: BUNDLE_QUERY,
  });
  expect(answer.ok, `the query must succeed: ${JSON.stringify(answer).slice(0, 300)}`).toBe(true);
  expect(answer.form).toBe("bindings");
  expect(answer.results.bindings.length, "the bundle must answer with rows").toBeGreaterThan(0);
  for (const row of answer.results.bindings) {
    expect(row.term.value.startsWith("http"), "every bound term is an IRI").toBe(true);
    expect(row.label.value.length).toBeGreaterThan(0);
  }
});

test("LiveReasoning — the chase derives the entailed triple", async ({ app }) => {
  const answer = await app.call("reason_graph", { data: SUBSUMPTION, format: "turtle" });
  expect(answer.closure_nquads.trim().split("\n")).toContain(SUBSUMPTION_ENTAILMENT);
  expect(answer.entailed_count, "the closure must report what it derived").toBeGreaterThan(0);
});

test("Interactivity — a pane's own form runs the tool and renders the answer", async ({ app }) => {
  // Driven through the RENDERED form: the field is the one the tool's advertised JSON
  // Schema produced, the button is the shipped one, and the result is what the pane painted.
  await app.selectPane("lookup_term");
  await app.page.fill('gmeow-console input[id="f-lookup_term-term"]', "gmeow:ToolCall");
  await app.page.click('gmeow-console button.run[type="submit"]');
  await expect(app.page.locator("gmeow-console .status")).toHaveText("`lookup_term` answered.");
  await expect(app.page.locator("gmeow-console pre")).toContainText(
    "https://blackcatinformatics.ca/gmeow/ToolCall",
  );

  // A missing REQUIRED argument is reported by the form rather than dispatched.
  await app.page.fill('gmeow-console input[id="f-lookup_term-term"]', "");
  await app.page.click('gmeow-console button.run[type="submit"]');
  await expect(app.page.locator("gmeow-console .status")).toHaveText(
    "Missing required argument(s): term",
  );
});

test("Diagrams — the runtime pane's SVG node set EQUALS the tool's concept set", async ({ app }) => {
  const matrix = await app.call("distribution_matrix", {});
  expect(matrix.concepts.length, "the catalog must declare formal concepts").toBeGreaterThan(0);

  await app.selectPane("@runtime");
  await expect(app.page.locator("gmeow-console svg")).toHaveCount(1, { timeout: 240_000 });

  const rendered = await app.page.evaluate(() =>
    [...document.getElementById("console").shadowRoot.querySelectorAll("svg text")].map(
      (node) => node.textContent,
    ),
  );
  // The element's own label rule, applied to the tool's answer: the extent, joined, or ⊥
  // for the bottom concept. Comparing sorted ARRAYS rather than sets, so two concepts that
  // collapsed to one drawn node fail here rather than passing a set comparison.
  const expected = matrix.concepts
    .map((concept) => (concept.extent.length === 0 ? "⊥" : concept.extent.join("+")))
    .sort();
  expect([...rendered].sort(), "the drawn node labels equal the returned concepts").toEqual(expected);

  // The diagram is a real order diagram, not a decoration: it carries its accessible name
  // and the cover edges the extents determine.
  await expect(app.page.locator("gmeow-console svg")).toHaveAttribute("role", "img");
  const edges = await app.page.evaluate(
    () => document.getElementById("console").shadowRoot.querySelectorAll("svg line").length,
  );
  expect(edges, "a lattice over more than one concept has cover edges").toBeGreaterThan(0);

  // …and the distributions table beside it is the tool's own answer too.
  const slugs = await app.page.evaluate(() =>
    [...document.getElementById("console").shadowRoot.querySelectorAll("table code")].map(
      (node) => node.textContent,
    ),
  );
  for (const distribution of matrix.distributions) {
    expect(slugs, `the runtime pane must list the ${distribution.slug} distribution`).toContain(
      distribution.slug,
    );
  }
});

test("SearchIndex is DROPPED — the assembled tree carries no search index", async ({ assembled }) => {
  const files = await listFiles(assembled);
  const indexes = files.filter((name) => /search[-_]?index/i.test(name));
  expect(indexes, "the console distribution declares SearchIndex dropped").toEqual([]);
});

test("CrossLinkFidelity is DROPPED — no relative link reaches for a documentation page", async ({
  app,
  assembled,
}) => {
  const files = (await listFiles(assembled)).filter((name) => name.startsWith("console/"));
  const textual = files.filter((name) => /\.(md|html|webmanifest|mjs)$/.test(name));
  expect(textual.length, "the console tree must carry text to scan").toBeGreaterThan(0);

  /** Every link target the console tree declares, with the file that declares it. */
  const links = [];
  for (const name of textual) {
    const text = await fs.readFile(join(assembled, ...name.split("/")), "utf8");
    for (const match of text.matchAll(/(?:href|src)="([^"]+)"|\]\(([^)\s]+)\)/g)) {
      links.push({ from: name, target: match[1] ?? match[2] });
    }
  }
  expect(links.length, "the console tree must declare links to scan").toBeGreaterThan(0);

  // A documentation-site page reached from the console MUST be an absolute published-site
  // URL: the console distribution ships none of those pages, so a relative path would be a
  // 404 for every reader.
  const documentationish = /(^|\/)(terms|classes|properties|datatypes|slices|glossary|explorer)\//;
  for (const link of links) {
    if (link.target.startsWith("#")) continue;
    if (/^[a-z][a-z0-9+.-]*:/i.test(link.target)) {
      if (documentationish.test(link.target)) {
        expect(link.target.startsWith(PUBLISHED_SITE_BASE), `${link.from} → ${link.target}`).toBe(true);
      }
      continue;
    }
    expect(
      documentationish.test(link.target),
      `${link.from} links relatively at a documentation page (${link.target}), which the ` +
        "console distribution does not ship",
    ).toBe(false);
    expect(
      posix.normalize(posix.join("console", link.target)).startsWith(".."),
      `${link.from} links outside the assembled tree (${link.target})`,
    ).toBe(false);
  }

  // And the shell's own same-origin links resolve — a dropped capability is not licence for
  // a broken one.
  const shellLinks = await app.page.evaluate(async () => {
    const targets = [...document.querySelectorAll("link[href], a[href]")]
      .map((node) => node.getAttribute("href"))
      .filter((href) => href !== null && !href.startsWith("#"));
    const out = {};
    for (const href of targets) out[href] = (await fetch(href, { cache: "no-store" })).status;
    return out;
  });
  for (const [href, status] of Object.entries(shellLinks)) {
    expect(status, `the shell links at ${href}`).toBe(200);
  }
});
