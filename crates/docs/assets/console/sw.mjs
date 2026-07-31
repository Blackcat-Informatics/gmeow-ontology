// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The console's cache-first service worker.
//
// # SHELL and BUILD are GENERATED
//
// The array on the `SHELL` line and the string on the `BUILD` line below are REPLACED,
// verbatim, by the Rust producer (`crates/docs/src/console.rs`). Do not hand-edit either:
// a hand-written list is a second source of truth for "what the console is made of", and it
// would silently drift from the emitted tree the moment a file was added. The producer
// hard-fails if a marker is missing, and an acceptance assertion compares the generated set
// against the producer's first-load tier in BOTH directions.
//
// `SHELL` is the FIRST-LOAD tier only — the shell, the transport, the always-resident core
// engine image and the ontology snapshot. It deliberately does NOT carry the demand-loaded
// reasoning segment: pre-caching a 10 MB image at install would download it for every
// reader who only ever looks things up, which is exactly the cost the tiered engine exists
// to avoid, and it would make "demand-loaded" a claim the artifact contradicts. The segment
// is cached by the `fetch` handler below, the first time something actually asks for it.
//
// `BUILD` is a BLAKE3 content digest of the assembled tree, and it is the whole cache name.
// Keying the cache on the shell's entry count and joined path length — which is what this
// did — means any rebuild that keeps the same paths keeps the same cache name, so a
// returning reader is served the PREVIOUS build's bytes for ever. A content digest cannot
// do that: change any byte and the cache name changes, the old cache is deleted on
// activate, and the new bytes are fetched.
//
// # Scope, and why the engine is reachable at all
//
// This worker is registered at `console/` scope, so it CONTROLS the console's own page —
// and a service worker intercepts every request a page it controls makes, whatever the
// request's own path. The engine assets live one level up under `assets/` (they are shared
// with the documentation site, which is why they are not duplicated), and they are
// intercepted here just like the shell is. The first-load ones are pre-cached on install so
// an offline console still gets its engine, and a pre-cache that fails is a hard install
// failure rather than a console that boots online and dies offline.

const SHELL = ["__GMEOW_CONSOLE_SHELL__"];
const BUILD = "__GMEOW_CONSOLE_BUILD__";

/** One versioned cache, named by the assembled tree's own content digest. */
const CACHE = `gmeow-console-${BUILD}`;

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
      // The deployed entry URL is the DIRECTORY — `/console/` — and that is also what
      // `manifest.webmanifest`'s `start_url: "."` opens, so a launched PWA never asks for
      // `index.html` by name. The shell is cached under its file name, and `caches.match`
      // is URL-keyed, so without this the front door misses the cache, `fetch` throws with
      // the network gone, and the console is dead offline at exactly the entry the offline
      // reader uses. Serve the cached shell for any navigation the cache does not hold.
      if (request.mode === "navigate") {
        const shell = await caches.match(new URL("./index.html", self.location.href));
        if (shell !== undefined) return shell;
      }
      const response = await fetch(request);
      // Cache-on-first-use. This is what makes the demand-loaded reasoning segment real:
      // it is absent from the cache until a pane needs it, and offline-available from the
      // moment one did.
      if (response.ok && new URL(request.url).origin === self.location.origin) {
        (await caches.open(CACHE)).put(request, response.clone());
      }
      return response;
    })(),
  );
});
