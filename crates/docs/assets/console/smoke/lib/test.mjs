// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The lane's shared fixtures.
//
// `app` is WORKER-scoped: one page, at `/console/` on the assembled tree, booted to ready,
// reused by every positive spec in the run. Booting per test would re-instantiate a ~40 MB
// snapshot a dozen times over and prove the same thing a dozen times. The specs that need a
// broken tree, an installed package or a fresh session open their own pages.
//
// `assembled` is the assembled tree's path; `origin` is the static server the global setup
// started. Both are read from the environment the setup published, so no spec resolves a
// path of its own.

import { test as base } from "@playwright/test";

import { openConsole } from "./console-page.mjs";
import { consoleOut, serverOrigin } from "./paths.mjs";

export const test = base.extend({
  origin: [async ({}, use) => use(serverOrigin()), { scope: "worker" }],
  assembled: [async ({}, use) => use(consoleOut()), { scope: "worker" }],
  app: [
    async ({ browser }, use) => {
      const app = await openConsole(browser, serverOrigin());
      await app.ready();
      await use(app);
      await app.close();
    },
    { scope: "worker" },
  ],
});

export { expect } from "@playwright/test";
