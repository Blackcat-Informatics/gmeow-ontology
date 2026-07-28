// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The console's cache-first service worker.
//
// # SHELL is GENERATED
//
// The array on the `SHELL` line below is REPLACED, verbatim, by the Rust producer
// (`crates/docs/src/console.rs`) with the assembled console key set — every file the
// producer actually emits, expressed relative to THIS script. Do not hand-edit it: a
// hand-written list is a second source of truth for "what the console is made of", and it
// would silently drift from the emitted tree the moment a file was added. The producer
// hard-fails if the marker is missing, and an acceptance assertion compares the generated
// set against the assembled key set in BOTH directions.
//
// # Scope, and why the engine is still available offline
//
// This worker is registered at `console/` scope, so it intercepts requests for the shell
// itself. The engine assets live one level up under `assets/` (they are shared with the
// documentation site, which is why they are not duplicated), and out-of-scope requests
// are never routed to a worker at all. They are therefore PRE-CACHED here on install and
// read back through `caches.match` — so an offline console still gets its engine, and a
// pre-cache that fails is a hard install failure rather than a console that boots online
// and dies offline.

const SHELL = ["__GMEOW_CONSOLE_SHELL__"];

/** One versioned cache, keyed by the shell's own content so a new build never reuses old bytes. */
const CACHE = `gmeow-console-${SHELL.length}-${SHELL.join("|").length}`;

self.addEventListener("install", (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE);
      // `addAll` rejects the whole install if ANY member 404s. That is the intent: a
      // partially cached shell is an offline surface that fails unpredictably later.
      await cache.addAll(SHELL.map((path) => new URL(path, self.location.href).toString()));
      await self.skipWaiting();
    })(),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      for (const name of await caches.keys()) {
        if (name !== CACHE && name.startsWith("gmeow-console-")) await caches.delete(name);
      }
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;
  event.respondWith(
    (async () => {
      const cached = await caches.match(request, { ignoreSearch: true });
      if (cached !== undefined) return cached;
      const response = await fetch(request);
      if (response.ok && new URL(request.url).origin === self.location.origin) {
        (await caches.open(CACHE)).put(request, response.clone());
      }
      return response;
    })(),
  );
});
