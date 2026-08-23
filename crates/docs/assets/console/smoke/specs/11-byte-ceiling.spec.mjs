// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The published byte partition, asserted against what the BROWSER actually fetches.
//
// The producer measures the assembled tree and publishes two numbers into the console
// README: what the service worker pre-caches at install, and what a page load itself
// fetches. Both are claims about a partition of the emitted tree. What neither can say is
// what a browser opening `/console/` actually downloads — and the two came apart twice:
//
//   * the shipped README claimed the reasoning segment was "never fetched at all for a
//     reader who only looks things up" while the service worker pre-cached it on install;
//   * the shipped README then headed the whole pre-cache set "First load — everything
//     fetched before any pane runs", counting the entire vendored purrdf engine, a PWA
//     manifest and four icons that no page load asks for.
//
// So both sides are measured here, and the page-load claim is checked in BOTH directions:
//
//   * the GENERATED pre-cache set (the service worker's `SHELL`, which the producer emits
//     from the emitted key set) is summed on disk and must EQUAL the published pre-cache
//     total. Equality against the bytes on disk is the whole of the claim: the README's
//     published "ceiling" is that same total times a constant, so nothing here is asserted
//     against it — see the note at the assertion, and `NO_SIZE_GATE_DISCLOSURE` in
//     `crates/docs/src/console.rs` for what the shipped document now says about it;
//   * a FRESH page load is recorded on the wire, and the recorded set must EQUAL the
//     published page-load table — not merely be contained in it. Containment alone is what
//     let a phantom row inflate the published number: nothing was ever required to fetch it.
//
// The numbers are read out of the assembled README rather than restated, so the assertions
// move when the producer's measurement does and cannot drift from it.

import { promises as fs } from "node:fs";
import { join } from "node:path";

import { expect, test } from "../lib/test.mjs";
import { openConsole } from "../lib/console-page.mjs";
import {
  generatedShell,
  publishedByteBudget,
  publishedPageLoadAssets,
  shellEntryPaths,
} from "../lib/tree.mjs";

/** The tree key a recorded response path names (`/console/` IS `console/index.html`). */
function keyOf(pathname) {
  return pathname === "/console/" ? "console/index.html" : pathname.replace(/^\//, "");
}

test("the generated pre-cache set matches the published measurement", async ({ assembled }) => {
  const { pageLoadTotal, installOnlyTotal, precacheTotal } = await publishedByteBudget(assembled);
  const entries = (await generatedShell(assembled)).map(shellEntryPaths);

  let measured = 0;
  for (const entry of entries) {
    measured += (await fs.stat(join(assembled, ...entry.file.split("/")))).size;
  }

  expect(measured, "the summed pre-cache set must equal the published pre-cache total").toBe(
    precacheTotal,
  );
  expect(
    pageLoadTotal + installOnlyTotal,
    "the two published sections must sum to the published pre-cache total",
  ).toBe(precacheTotal);
  expect(
    pageLoadTotal,
    "the page load must cost strictly less than the install pre-cache — one number under two " +
      "headings is the defect the split exists to close",
  ).toBeLessThan(precacheTotal);
  // The published ceiling is DELIBERATELY not asserted against the measurement here. It is
  // `precacheTotal × 2`, computed from that same measurement on every render, so
  // `ceiling > precacheTotal` is `2n > n` and `measured <= ceiling` is
  // `n <= 2n` once the equality two assertions up has held — neither could ever red, and
  // both stood here while the reasoning segment grew by megabytes and the ceiling floated
  // up behind it. What the ceiling IS, and the fact that no size-regression gate exists, is
  // asserted where it belongs: over the shipped prose, in the Rust producer lane.
});

test("a fresh page load fetches EXACTLY the published page-load set", async ({
  browser,
  origin,
  assembled,
}) => {
  const { pageLoadTotal } = await publishedByteBudget(assembled);
  const shell = new Set((await generatedShell(assembled)).map((entry) => shellEntryPaths(entry).url));
  const published = await publishedPageLoadAssets(assembled);

  const app = await openConsole(browser, origin);
  try {
    await app.ready();
    // Sizes are read off the wire (the server sets `content-length` on every response), so
    // this is the transfer a reader pays for, not a re-measurement of the tree. Recording
    // starts before the navigation, so nothing the first load fetched is outside it.
    const fetched = new Map();
    for (const response of app.responses) {
      expect(response.status, `${response.url} failed on first load`).toBe(200);
      fetched.set(new URL(response.url).pathname, response.bytes);
    }

    expect(fetched.size, "a first load must fetch something").toBeGreaterThan(0);

    let total = 0;
    const unexpected = [];
    for (const [path, bytes] of fetched) {
      total += bytes;
      // `/console/` IS `./index.html`; everything else must be a generated shell member.
      if (path === "/console/" || shell.has(path)) continue;
      unexpected.push(path);
    }
    expect(
      unexpected,
      "a first load fetched assets outside the generated pre-cache set — the demand-loaded " +
        "reasoning segment is the one this has been wrong about before",
    ).toEqual([]);
    for (const path of fetched.keys()) {
      expect(path.startsWith("/assets/mcp/"), `${path} is the demand-loaded segment`).toBe(false);
    }

    // BOTH directions against the published page-load table. The converse is the one that
    // was missing: every row the producer publishes as a page-load asset must appear in the
    // recorded wire log, so an asset silently classified into that tier reds this gate
    // instead of inflating the number a reader reads.
    const recorded = new Set([...fetched.keys()].map(keyOf));
    const claimed = new Set(published.map((row) => row.key));
    expect(
      [...claimed].filter((key) => !recorded.has(key)).sort(),
      "the README publishes these as page-load assets, and no page load fetched them",
    ).toEqual([]);
    expect(
      [...recorded].filter((key) => !claimed.has(key)).sort(),
      "the page load fetched these, and the README's page-load table does not publish them",
    ).toEqual([]);
    expect(total, "the recorded wire total must EQUAL the published page-load total").toBe(
      pageLoadTotal,
    );
    // No ceiling assertion here either: `total` has just been pinned EQUAL to
    // `pageLoadTotal`, which is a summand of the pre-cache total the ceiling is twice, so
    // `total <= ceiling` follows arithmetically from the line above and reds on nothing.
    // Non-vacuity: the ontology snapshot dominates the page load, so a measurement that
    // somehow missed it would be meaningless.
    expect(fetched.get("/assets/gmeow.gts"), "the snapshot must be part of the measured load").toBeGreaterThan(
      1_000_000,
    );
  } finally {
    await app.close();
  }
});
