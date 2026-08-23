// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The OFFLINE contract, driven with the service worker actually running.
//
// Every other context in this lane is created with `serviceWorkers: "block"`, for a good
// reason: a cache-first worker that pre-caches its whole shell on install would
// fetch every shell member a second time and turn the byte measurement into a measurement of
// the harness. But the console ships `sw.mjs`, ships a `manifest.webmanifest` that declares a
// PWA, and publishes an "Offline" section telling a reader the console works with no network.
// With the worker blocked everywhere, none of that was ever executed: the `SHELL` array's
// CONTENTS were checked against the emitted bytes, while whether the worker installs at all,
// what it actually put in the cache, and whether the console comes back without a network
// were not.
//
// That gap is where the last defect lived — the shipped README claimed the reasoning
// segment was never fetched for a reader who only looks things up, while the worker
// pre-cached it on install — and `cache.addAll` over a tier of that size is exactly the
// operation that fails silently on a quota or a moved path, leaving a console that boots
// online and dies offline.
//
// So this spec runs in the ONE context in the lane with the worker enabled, and asserts the
// three things the published section claims:
//
//   1. the registration reaches `activated` and takes control of the page;
//   2. the cache holds EXACTLY the generated pre-cache set — set equality in both
//      directions, so neither a missing member (an install that half-succeeded) nor an extra
//      one (the demand-loaded `assets/mcp/` segment) passes;
//   3. with the network gone, the console loads again and answers a real tool call.
//
// The expected key set is derived from the assembled tree's own generated `SHELL`, never
// listed here — a hand-written list would be a second source of truth for the partition the
// producer exists to own.
//
// # Why this spec runs its own server
//
// `context.setOffline(true)` is NOT sufficient to take a service-worker-controlled page
// offline, and believing it is would make this whole spec vacuous. Chromium applies a
// context's network emulation to the PAGE; a request the page makes while controlled is
// handed to the worker's `fetch` handler, and the `fetch()` the worker then makes is issued
// from the worker's own network context, which the emulation does not cover. Measured: with
// `setOffline(true)` in force, a page fetch of an uncached URL still came back `200`.
//
// So the origin itself is killed. This spec starts its own static server over the same
// assembled tree — its own port, therefore its own origin, therefore its own registration and
// its own cache, sharing nothing with the lane's server — and closes it, sockets and all,
// before the offline half. What follows is a genuinely dead origin, which no layer of the
// browser can paper over. `setOffline(true)` is set as well, to cut the page's direct network
// too, and a control fetch proves the cut before anything is concluded from it.

import { expect, test } from "../lib/test.mjs";
import { startStaticServer } from "../lib/http-server.mjs";
import { generatedShell, shellEntryPaths } from "../lib/tree.mjs";

/** How long a 56 MB `addAll` over loopback is given before the install is called failed. */
const INSTALL_TIMEOUT = 300_000;

/** The engine-ready signal the shipped shell paints into its version chip. */
const READY_CHIP = "engine: ready";

/**
 * The pathnames the console's cache holds, plus the cache names, read from the page.
 *
 * Read through `caches` in the page's own origin, which is the same storage the worker
 * writes: this is the cache as it exists, not a report the worker made about itself.
 */
async function cachedPaths(page) {
  return page.evaluate(async () => {
    const consoleCaches = (await caches.keys()).filter((name) => name.startsWith("gmeow-console-"));
    const paths = [];
    for (const name of consoleCaches) {
      const cache = await caches.open(name);
      for (const request of await cache.keys()) paths.push(new URL(request.url).pathname);
    }
    return { consoleCaches, paths: paths.sort() };
  });
}

/** Resolve once the console's registration is `activated` AND controlling the page. */
async function activatedRegistration(page) {
  return page.evaluate(async () => {
    const registration = await navigator.serviceWorker.ready;
    const worker = registration.active;
    if (worker.state !== "activated") {
      await new Promise((settle) => {
        worker.addEventListener("statechange", function seen() {
          if (worker.state !== "activated") return;
          worker.removeEventListener("statechange", seen);
          settle();
        });
      });
    }
    if (navigator.serviceWorker.controller === null) {
      await new Promise((settle) =>
        navigator.serviceWorker.addEventListener("controllerchange", settle, { once: true }),
      );
    }
    return { scope: registration.scope, state: worker.state };
  });
}

test("the service worker installs, caches exactly the pre-cache set, and serves the console offline", async ({
  browser,
  assembled,
}) => {
  const shell = (await generatedShell(assembled)).map((entry) => shellEntryPaths(entry).url).sort();

  // This spec's OWN origin, over the same assembled tree — see the header.
  const server = await startStaticServer({ "/": assembled });
  let alive = true;
  const kill = async () => {
    if (!alive) return;
    alive = false;
    await server.close();
  };

  // The one context in this lane that does NOT block service workers.
  const context = await browser.newContext();
  const page = await context.newPage();
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(String(error?.message ?? error)));
  try {
    await page.goto(`${server.origin}/console/`, { waitUntil: "load" });

    // Registration failure is reported by the shipped shell into `#error-banner`, so a
    // worker that refused to register says so here rather than timing out below.
    const banner = await page.locator("#error-banner").innerText().catch(() => "");
    expect(banner, "the shipped shell reported a service-worker registration failure").not.toContain(
      "offline:",
    );

    // ACTIVATED, and controlling this page. `skipWaiting` + `clients.claim()` are what make
    // the second half true; without the claim the worker exists but intercepts nothing, and
    // a reload against a dead origin would go straight to the dead origin.
    //
    // Awaited in the page rather than polled: `install` holds activation open until `addAll`
    // over the whole pre-cache set resolves, so this is the point — and the only point — at
    // which "the pre-cache finished" is a fact rather than a guess.
    const registration = await activatedRegistration(page);
    expect(registration.state, "the registration must reach activated").toBe("activated");
    expect(registration.scope, "the worker is registered at the console's own scope").toContain(
      "/console/",
    );

    // The cache, before any tool call: `install` ran `addAll(SHELL)` and nothing has yet asked
    // for the demand-loaded segment, so this is the pre-cache and only the pre-cache.
    const { consoleCaches, paths } = await cachedPaths(page);
    expect(consoleCaches.length, "the worker opens exactly one content-keyed console cache").toBe(1);
    expect(
      paths,
      "the pre-cache must EQUAL the generated pre-cache set — a missing member is a half-installed " +
        "offline surface, an extra one is a tier the producer says is demand-loaded",
    ).toEqual(shell);
    for (const path of paths) {
      expect(path.startsWith("/assets/mcp/"), `${path} is the demand-loaded reasoning segment`).toBe(
        false,
      );
    }

    // The console itself has to be up before "offline" means anything.
    await page.waitForFunction(
      (chip) => document.getElementById("version-chip")?.textContent === chip,
      READY_CHIP,
      { timeout: INSTALL_TIMEOUT },
    );

    // NETWORK GONE. The origin is closed, sockets and all, and the page's own network is cut
    // as well. Every byte below comes out of the cache the install wrote.
    await context.setOffline(true);
    await kill();

    // …and the cut is PROVED before it is relied on. `console/README.md` ships in the tree but
    // is not a shell member and nothing has fetched it, so it is in no cache; with the origin
    // up it is a 200. A request quietly answered from anywhere at all would make everything
    // below meaningless.
    const control = await page.evaluate(async () => {
      try {
        const response = await fetch("./README.md", { cache: "no-store" });
        return `reached the network: ${response.status}`;
      } catch {
        return null;
      }
    });
    expect(control, "the network is not actually cut — nothing below would prove anything").toBeNull();

    const served = [];
    page.on("response", (response) =>
      served.push({ path: new URL(response.url()).pathname, worker: response.fromServiceWorker() }),
    );
    await page.reload({ waitUntil: "load" });
    await page.waitForFunction(
      (chip) => document.getElementById("version-chip")?.textContent === chip,
      READY_CHIP,
      { timeout: INSTALL_TIMEOUT },
    );

    // …and a real tool call, through the offline engine: the whole point of pre-caching a
    // 56 MB tier is that the console still ANSWERS, not that it still paints.
    const answer = await page.evaluate(() =>
      document
        .getElementById("console")
        .ask("invoke", { tool: "lookup_term", args: { term: "gmeow:ToolCall" } }),
    );
    expect(answer.iri, "the offline console must answer a core tool call from cached bytes").toBe(
      "https://blackcatinformatics.ca/gmeow/ToolCall",
    );

    // Served BY THE WORKER. The ontology snapshot dominates the load and is the one member
    // whose provenance decides whether this measured the offline contract at all.
    expect(served.length, "an offline reload still fetches — through the worker").toBeGreaterThan(0);
    const snapshot = served.find((entry) => entry.path === "/assets/gmeow.gts");
    expect(snapshot, "the offline reload did not load the ontology snapshot at all").toBeDefined();
    expect(snapshot.worker, "the snapshot must be served by the service worker, from its cache").toBe(
      true,
    );
    expect(pageErrors, "no uncaught error may reach an offline console").toEqual([]);
  } finally {
    await context.close();
    await kill();
  }
});
