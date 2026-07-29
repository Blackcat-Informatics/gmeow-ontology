// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// A plain static HTTP server — deliberately the dumbest one that can be written.
//
// # No COOP/COEP, and that is the point
//
// GitHub Pages serves static files with no `Cross-Origin-Opener-Policy` and no
// `Cross-Origin-Embedder-Policy`, so a page it serves is NOT cross-origin isolated and
// `globalThis.SharedArrayBuffer` is `undefined` in it. The console's single-threaded
// contract is exactly the claim that it still works there. Serving the assembled tree with
// those headers — which every "dev server" convenience preset adds — would make the lane
// pass in a runtime the deployment does not provide, so this server sets NEITHER, and a
// spec asserts their absence on the wire rather than trusting this comment.
//
// # Mounts and variants
//
// One server, several roots: the pristine assembled tree, the two PERTURBED trees the
// negative tests drive (a truncated `element.mjs`, a removed engine asset), and the scratch
// project the published npm tarball was installed into. Each is a real directory on disk —
// nothing is rewritten in flight, so what the browser fetches is what a static host would
// have served.

import { createReadStream, promises as fs } from "node:fs";
import { createServer } from "node:http";
import { extname, join, normalize, resolve, sep } from "node:path";

/**
 * Content types, by extension.
 *
 * A wrong type is not cosmetic here: a browser refuses a module script served as
 * `text/plain`, and `WebAssembly.instantiateStreaming` refuses anything that is not
 * `application/wasm` — both of which are precisely the failures a static host would produce
 * and this lane exists to observe.
 */
const CONTENT_TYPES = new Map(
  Object.entries({
    ".html": "text/html; charset=utf-8",
    ".mjs": "text/javascript; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".json": "application/json; charset=utf-8",
    ".webmanifest": "application/manifest+json; charset=utf-8",
    ".wasm": "application/wasm",
    ".ttl": "text/turtle; charset=utf-8",
    ".md": "text/markdown; charset=utf-8",
    ".svg": "image/svg+xml",
    ".png": "image/png",
    ".gts": "application/octet-stream",
    ".map": "application/json; charset=utf-8",
    ".d.ts": "text/plain; charset=utf-8",
  }),
);

/** The content type for `path`, defaulting to an opaque byte stream. */
function contentType(path) {
  return CONTENT_TYPES.get(extname(path)) ?? "application/octet-stream";
}

/**
 * Resolve a URL path against `root`, refusing anything that escapes it.
 *
 * Returns `null` for a traversal attempt, so the server answers 404 rather than reading
 * outside the mount.
 */
function underRoot(root, urlPath) {
  const decoded = decodeURIComponent(urlPath);
  const candidate = resolve(join(root, normalize(decoded)));
  return candidate === root || candidate.startsWith(root + sep) ? candidate : null;
}

/**
 * Start the lane's static server over `mounts` (a `{ prefix: rootDir }` map).
 *
 * The longest matching prefix wins, so `/installed/…` is served from the scratch project
 * while `/` serves the assembled tree. A directory request is answered with its
 * `index.html`, exactly as a static host does; anything absent is a real 404, because a
 * fabricated placeholder would hide the missing-asset failures this lane asserts.
 *
 * @returns `{ origin, close }` — `origin` is the `http://127.0.0.1:<port>` the browser
 *   drives, `close` shuts the listener down.
 */
export async function startStaticServer(mounts) {
  const table = Object.entries(mounts)
    .map(([prefix, root]) => [prefix.endsWith("/") ? prefix : `${prefix}/`, resolve(root)])
    .sort((a, b) => b[0].length - a[0].length);

  const server = createServer((request, response) => {
    const url = new URL(request.url, "http://127.0.0.1");
    const path = url.pathname.endsWith("/") ? `${url.pathname}index.html` : url.pathname;
    const mount = table.find(([prefix]) => path.startsWith(prefix));
    if (mount === undefined) {
      response.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
      response.end(`no mount serves ${path}\n`);
      return;
    }
    const [prefix, root] = mount;
    const file = underRoot(root, path.slice(prefix.length - 1));
    if (file === null) {
      response.writeHead(403, { "content-type": "text/plain; charset=utf-8" });
      response.end("path escapes the mount root\n");
      return;
    }
    fs.stat(file).then(
      (info) => {
        if (info.isDirectory()) {
          response.writeHead(301, { location: `${url.pathname}/` });
          response.end();
          return;
        }
        // Exactly these three headers. No COOP, no COEP, no cache directives that a static
        // host would not send.
        response.writeHead(200, {
          "content-type": contentType(file),
          "content-length": String(info.size),
          "accept-ranges": "none",
        });
        if (request.method === "HEAD") {
          response.end();
          return;
        }
        createReadStream(file).pipe(response);
      },
      () => {
        response.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
        response.end(`${path} not found\n`);
      },
    );
  });

  await new Promise((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const { port } = server.address();
  return {
    origin: `http://127.0.0.1:${port}`,
    close: () => new Promise((done) => server.close(done)),
  };
}
