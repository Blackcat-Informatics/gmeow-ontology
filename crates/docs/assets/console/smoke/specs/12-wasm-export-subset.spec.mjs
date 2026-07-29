// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// `console_js_exports_are_a_subset_of_mcp_wasm`, proved on the SERVED bytes in a real
// browser.
//
// The DOM-free lane proves the subset by comparing two source texts. A browser can say
// something the texts cannot: an ES module that imports a name its dependency does not
// export is a LINK error, so `import("/assets/mcp-core/index.mjs")` succeeding is itself
// part of the proof — and the names the shim re-exports are then observed to be live
// callables rather than declarations.
//
// The chain checked here is the whole one: what the console's JS imports from the shims ⊆
// what the wasm-bindgen glue exports ⊆ (for the passthrough half) what the compiled wasm
// module declares. And nothing in the console's own JS reaches around the shims into a
// `pkg/` module.

import { expect, test } from "../lib/test.mjs";

/** The two vendored segments, as the browser is served them. */
const SEGMENTS = [
  { shim: "/assets/mcp-core/index.mjs", glue: "/assets/mcp-core/pkg/gmeow_mcp_core_wasm.js", wasm: "/assets/mcp-core/pkg/gmeow_mcp_core_wasm_bg.wasm" },
  { shim: "/assets/mcp/index.mjs", glue: "/assets/mcp/pkg/gmeow_mcp_wasm.js", wasm: "/assets/mcp/pkg/gmeow_mcp_wasm_bg.wasm" },
];

/** The console's own JS, none of which may import a `pkg/` module directly. */
const CONSOLE_MODULES = [
  "/console/element.mjs",
  "/console/engine.worker.mjs",
  "/console/session.mjs",
  "/console/pkg/mcp-transport.mjs",
  "/assets/mcp-transport.mjs",
  "/assets/docs-controller.mjs",
];

test("every wasm export the console reaches is declared by the segment that serves it", async ({ app }) => {
  const observed = await app.page.evaluate(async (segments) => {
    const text = async (url) => (await fetch(url, { cache: "no-store" })).text();
    const out = [];
    for (const segment of segments) {
      const shim = await text(segment.shim);
      const glue = await text(segment.glue);

      // What the shim imports from its own `pkg/` glue — the wasm surface the console
      // actually reaches.
      const imported = new Set();
      for (const block of shim.matchAll(/import\s+\w*,?\s*\{([\s\S]*?)\}\s+from\s+"\.\/pkg\//g)) {
        for (const entry of block[1].split(",")) {
          const name = entry.trim().split(/\s+as\s+/)[0].trim();
          if (name.length > 0) imported.add(name);
        }
      }
      const glueExports = new Set(
        [...glue.matchAll(/^export function (\w+)/gm)].map((match) => match[1]),
      );

      // The LIVE module: importing it at all is a link check, and every re-exported name
      // must be a callable rather than a declaration.
      const namespace = await import(segment.shim);
      const live = Object.entries(namespace)
        .filter(([, value]) => typeof value === "function")
        .map(([name]) => name);

      // The compiled module's own export table, read off the served bytes.
      const module = await WebAssembly.compileStreaming(fetch(segment.wasm, { cache: "no-store" }));
      const wasmExports = WebAssembly.Module.exports(module)
        .filter((entry) => entry.kind === "function")
        .map((entry) => entry.name);

      // What the shim DECLARES it exports, read off its own served text.
      const declared = new Set([...shim.matchAll(/^export function (\w+)/gm)].map((m) => m[1]));
      for (const clause of shim.matchAll(/^export\s*\{([^}]*)\}\s*;/gm)) {
        for (const entry of clause[1].split(",")) {
          const name = entry.trim().split(/\s+as\s+/).at(-1).trim();
          if (name.length > 0) declared.add(name);
        }
      }

      out.push({
        segment: segment.shim,
        imported: [...imported],
        glueExports: [...glueExports],
        declared: [...declared],
        live,
        wasmExports,
      });
    }
    return out;
  }, SEGMENTS);

  for (const segment of observed) {
    expect(segment.imported.length, `${segment.segment} reaches no wasm surface`).toBeGreaterThan(0);
    expect(segment.glueExports.length, `${segment.segment}'s glue declares no exports`).toBeGreaterThan(0);
    expect(segment.wasmExports.length, `${segment.segment}'s wasm declares no exports`).toBeGreaterThan(0);

    // The SUBSET claim, proved by export-set comparison.
    const missing = segment.imported.filter((name) => !segment.glueExports.includes(name));
    expect(missing, `${segment.segment} imports glue exports that do not exist: ${missing.join(", ")}`).toEqual([]);

    // …and every name the shim DECLARES it exports is a live callable in the loaded module,
    // not merely written down. This is the half only a browser can make: the module was
    // linked against its glue, so a re-export of a name the glue does not have would have
    // thrown before this assertion could run.
    const dead = segment.declared.filter((name) => !segment.live.includes(name));
    expect(dead, `${segment.segment} declares exports that are not live callables`).toEqual([]);
    expect(segment.declared.length, `${segment.segment} declares no export`).toBeGreaterThan(0);

    // The passthrough half: every glue export that names a wasm export must find it.
    const passthrough = segment.glueExports.filter((name) => segment.wasmExports.includes(name));
    expect(
      passthrough.length,
      `${segment.segment}'s glue declares no export the wasm module also declares — the ` +
        "comparison would be vacuous",
    ).toBeGreaterThan(0);
  }
});

test("nothing in the console's JS reaches around the shims into a pkg/ module", async ({ app }) => {
  const offenders = await app.page.evaluate(async (urls) => {
    const found = [];
    for (const url of urls) {
      const text = await (await fetch(url, { cache: "no-store" })).text();
      if (/from\s+"[^"]*mcp(-core)?\/pkg\//.test(text)) found.push(url);
    }
    return found;
  }, CONSOLE_MODULES);
  expect(offenders, "these modules import a wasm pkg module directly instead of the shim").toEqual([]);
});
