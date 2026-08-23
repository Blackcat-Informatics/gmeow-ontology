// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The single-threaded contract, bound to BYTES and then to BEHAVIOUR.
//
// GitHub Pages sends no COOP and no COEP, so a page it serves is not cross-origin isolated
// and `SharedArrayBuffer` does not exist in it. The console's claim is that it works there
// anyway. Three observations, and none of them substitutes for another:
//
//   1. the served `.wasm` bytes declare every memory `shared=false` and contain NO atomic
//      instruction — decoded, not grepped (see `lib/wasm-shape.mjs`);
//   2. the served responses carry neither COOP nor COEP, and the page is therefore not
//      cross-origin isolated, with `globalThis.SharedArrayBuffer === undefined`;
//   3. the whole read surface still answers in that context — which is asserted by the
//      tool-surface sweep, running in this same non-isolated context, and re-affirmed here
//      across both engine segments.

import { expect, test } from "../lib/test.mjs";
import { atomicInstructions, memories } from "../lib/wasm-shape.mjs";
import { SUBSUMPTION } from "../lib/fixtures.mjs";

/** The engine images, as the browser is served them. */
const WASM_URLS = [
  "/assets/mcp-core/pkg/gmeow_mcp_core_wasm_bg.wasm",
  "/assets/mcp/pkg/gmeow_mcp_wasm_bg.wasm",
];

test("every served .wasm declares shared=false and contains no atomic instruction", async ({ origin }) => {
  for (const path of WASM_URLS) {
    const response = await fetch(`${origin}${path}`);
    expect(response.status, `${path} must be served`).toBe(200);
    expect(response.headers.get("content-type"), `${path} must be typed as wasm`).toBe(
      "application/wasm",
    );
    const bytes = new Uint8Array(await response.arrayBuffer());

    const declared = memories(bytes);
    expect(declared.length, `${path} declares or imports no memory`).toBeGreaterThan(0);
    for (const memory of declared) {
      expect(memory.shared, `${path} carries a ${memory.kind} SHARED memory`).toBe(false);
    }

    // Decoded instruction by instruction: each function body must land exactly on its
    // declared end, so an empty result is a proof rather than a scan that found nothing.
    expect(
      atomicInstructions(bytes),
      `${path} contains atomic instructions, which require a shared memory`,
    ).toEqual([]);
  }
});

test("the served responses carry no COOP/COEP, so the page is not cross-origin isolated", async ({
  origin,
  app,
}) => {
  for (const path of ["/console/", ...WASM_URLS]) {
    const response = await fetch(`${origin}${path}`);
    expect(
      response.headers.get("cross-origin-opener-policy"),
      `${path} must be served exactly as a static host serves it`,
    ).toBeNull();
    expect(response.headers.get("cross-origin-embedder-policy"), path).toBeNull();
  }

  const environment = await app.page.evaluate(() => ({
    isolated: globalThis.crossOriginIsolated,
    sharedArrayBuffer: typeof globalThis.SharedArrayBuffer,
  }));
  expect(environment.isolated, "the deployment leg is NOT cross-origin isolated").toBe(false);
  expect(environment.sharedArrayBuffer, "…so SharedArrayBuffer does not exist at boot").toBe(
    "undefined",
  );
});

test("both engine segments still answer with no SharedArrayBuffer available", async ({ app }) => {
  // The core image …
  const looked = await app.call("lookup_term", { term: "gmeow:ToolCall" });
  expect(looked.iri).toBe("https://blackcatinformatics.ca/gmeow/ToolCall");
  // … and the demand-loaded reasoning segment, in the same non-isolated context.
  const reasoned = await app.call("reason_graph", { data: SUBSUMPTION, format: "turtle" });
  expect(reasoned.entailed_count).toBeGreaterThan(0);

  // The absence is re-checked AFTER the segment loaded: a second image that quietly needed
  // shared memory would have had to create it.
  const stillAbsent = await app.page.evaluate(() => typeof globalThis.SharedArrayBuffer);
  expect(stillAbsent).toBe("undefined");
});
