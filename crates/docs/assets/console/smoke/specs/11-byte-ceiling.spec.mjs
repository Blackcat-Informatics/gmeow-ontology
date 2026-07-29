// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The first-load byte ceiling, asserted against what the BROWSER actually fetches.
//
// The producer already asserts the assembled first-load tier against
// `FIRST_LOAD_CEILING_BYTES` at render time, and publishes both numbers into the console
// README's measured table. That is a claim about a partition of the emitted tree. What it
// cannot say is what a browser opening `/console/` actually downloads — and the two came
// apart before: the shipped README claimed the 10 MB reasoning segment was "never fetched at
// all for a reader who only looks things up" while the service worker pre-cached it on
// install.
//
// So both are measured here, from the two sides:
//
//   * the GENERATED first-load set (the service worker's `SHELL`, which the producer emits
//     from the emitted key set) is summed on disk and must equal the published total and sit
//     under the published ceiling;
//   * a FRESH page load is recorded on the wire, and every byte it fetched must belong to
//     that set — in particular nothing from the demand-loaded reasoning segment.
//
// The ceiling is read out of the assembled README rather than restated, so the assertion
// moves when the producer's measurement does and cannot drift from it.

import { promises as fs } from "node:fs";
import { join } from "node:path";

import { expect, test } from "../lib/test.mjs";
import { openConsole } from "../lib/console-page.mjs";
import { generatedShell, publishedByteBudget, shellEntryPaths } from "../lib/tree.mjs";

test("the generated first-load set matches the published measurement and sits under the ceiling", async ({
  assembled,
}) => {
  const { firstLoadTotal, ceiling } = await publishedByteBudget(assembled);
  const entries = (await generatedShell(assembled)).map(shellEntryPaths);

  let measured = 0;
  for (const entry of entries) {
    measured += (await fs.stat(join(assembled, ...entry.file.split("/")))).size;
  }

  expect(measured, "the summed first-load set must equal the published first-load total").toBe(
    firstLoadTotal,
  );
  expect(ceiling, "the published ceiling must exceed the published measurement").toBeGreaterThan(
    firstLoadTotal,
  );
  expect(measured, `the assembled first load is ${measured} bytes, over the ceiling`).toBeLessThanOrEqual(
    ceiling,
  );
});

test("a fresh page load fetches only the first-load tier, under the published ceiling", async ({
  browser,
  origin,
  assembled,
}) => {
  const { ceiling } = await publishedByteBudget(assembled);
  const shell = new Set((await generatedShell(assembled)).map((entry) => shellEntryPaths(entry).url));

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
      "a first load fetched assets outside the generated first-load tier — the demand-loaded " +
        "reasoning segment is the one this has been wrong about before",
    ).toEqual([]);
    for (const path of fetched.keys()) {
      expect(path.startsWith("/assets/mcp/"), `${path} is the demand-loaded segment`).toBe(false);
    }
    expect(total, `the browser fetched ${total} bytes on first load, over the ceiling`).toBeLessThanOrEqual(
      ceiling,
    );
    // Non-vacuity: the ontology snapshot dominates the first load, so a measurement that
    // somehow missed it would be meaningless.
    expect(fetched.get("/assets/gmeow.gts"), "the snapshot must be part of the measured load").toBeGreaterThan(
      1_000_000,
    );
  } finally {
    await app.close();
  }
});
