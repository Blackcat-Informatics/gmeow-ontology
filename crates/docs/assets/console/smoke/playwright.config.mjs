// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The standalone console's browser smoke lane.
//
// Chromium only, and one worker. Chromium because it is the engine whose installability,
// module-worker and streaming-WebAssembly behaviour the console's contract is written
// against; one worker because every page in this lane boots a ~40 MB ontology snapshot into
// a wasm instance, and a second concurrent boot buys nothing but memory pressure. The
// positive path shares ONE booted page across every spec through a worker-scoped fixture,
// so the cost is paid once.
//
// No `webServer` entry: the server is started by `global-setup.mjs`, which also has to build
// the perturbed trees and install the packed tarball it serves. One place that knows what is
// being served is better than two that have to agree about ordering.
//
// `retries: 0` is deliberate. This is a gate: a spec that passes on the second attempt has
// found something, and retrying would hide it.

import { fileURLToPath } from "node:url";

import { defineConfig, devices } from "@playwright/test";

/**
 * Where a failing run's traces land.
 *
 * Under `target/`, never beside the sources: `crates/docs/assets/console/` is a reviewed
 * build-input tree whose every file is shipped by the producer, and a runner that drops
 * artefacts into it would be dropping them into the console's own source directory.
 */
const OUTPUT_DIR = fileURLToPath(new URL("../../../../../target/console-smoke/traces", import.meta.url));

export default defineConfig({
  testDir: "./specs",
  outputDir: OUTPUT_DIR,
  globalSetup: "./global-setup.mjs",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  forbidOnly: true,
  // The heavy specs boot the engine, run the whole 32-tool read surface, and demand-load
  // the reasoning segment; the budget is generous so a slow machine reports a real verdict
  // rather than a timeout.
  timeout: 600_000,
  expect: { timeout: 60_000 },
  reporter: [["list"]],
  use: {
    ...devices["Desktop Chrome"],
    // Nothing is recorded on a green run; a failure carries the page's own trace.
    trace: "retain-on-failure",
    video: "off",
    screenshot: "off",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
