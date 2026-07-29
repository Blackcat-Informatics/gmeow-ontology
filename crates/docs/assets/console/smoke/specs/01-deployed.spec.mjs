// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The title criterion: DEPLOYED.
//
// The assembled tree, served over plain static HTTP with no COOP/COEP, reaches a ready
// console; every member of the generated first-load set answers 200 with the bytes that
// were assembled; and a first-load run of a real tool answers.
//
// The integrity claim is checked on BOTH surfaces the console has: for the two assets the
// emitted manifest pins, the served bytes must match the manifest's own `blake3` and byte
// count; for every other shell member, the served bytes must match the assembled file. Both
// digests are computed with the SHIPPED `assets/blake3.mjs` — the same function the console
// verifies its engine with — so a JS/Rust disagreement would surface here rather than
// silently turning verification into rejection.

import { promises as fs } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

import { expect, test } from "../lib/test.mjs";
import { generatedShell, shellEntryPaths } from "../lib/tree.mjs";
import { TOOL_INPUTS } from "../lib/fixtures.mjs";

test("the assembled console reaches ready over plain static HTTP", async ({ app }) => {
  await expect(app.page.locator("#version-chip")).toHaveText("engine: ready");
  await expect(app.page.locator("#error-banner")).toHaveText("");
  expect(app.consoleErrors, "no uncaught page error may reach a ready console").toEqual([]);

  // The status region carries the derived pane count — the shell's own evidence that boot
  // read the shipped action policy rather than a list.
  await expect(app.page.locator("#status-banner")).toContainText(/\d+ panes derived from /);
});

test("every generated SHELL asset answers 200 with the assembled bytes", async ({ app, assembled }) => {
  const shell = await generatedShell(assembled);
  const entries = shell.map(shellEntryPaths);
  expect(entries.length, "the generated SHELL must not be empty").toBeGreaterThan(0);

  // The digests the browser computed, with the console's own BLAKE3, over what the server
  // actually answered.
  const served = await app.page.evaluate(async (urls) => {
    const { blake3Hex } = await import("/assets/blake3.mjs");
    const out = {};
    for (const url of urls) {
      const response = await fetch(url, { cache: "no-store" });
      const bytes = new Uint8Array(await response.arrayBuffer());
      out[url] = {
        status: response.status,
        bytes: bytes.length,
        blake3: `blake3:${blake3Hex(bytes)}`,
      };
    }
    return out;
  }, entries.map((entry) => entry.url));

  // …and the digests of the assembled files, computed with the SAME shipped module.
  const { blake3Hex } = await import(pathToFileURL(join(assembled, "assets", "blake3.mjs")).href);
  const manifest = JSON.parse(
    await fs.readFile(join(assembled, "assets", "bundle-manifest.json"), "utf8"),
  );

  let pinned = 0;
  for (const entry of entries) {
    const answer = served[entry.url];
    expect(answer.status, `${entry.url} must be served`).toBe(200);

    const onDisk = await fs.readFile(join(assembled, ...entry.file.split("/")));
    const digest = `blake3:${blake3Hex(new Uint8Array(onDisk))}`;
    expect(answer.bytes, `${entry.url} byte count`).toBe(onDisk.length);
    expect(answer.blake3, `${entry.url} content address`).toBe(digest);

    // The manifest-pinned subset is additionally checked against the MANIFEST's own
    // declaration — the number the console itself verifies against at boot.
    const declared = manifest[entry.file];
    if (declared !== undefined) {
      pinned += 1;
      expect(answer.blake3, `${entry.file} against the emitted manifest`).toBe(declared.blake3);
      expect(answer.bytes, `${entry.file} against the emitted manifest`).toBe(declared.bytes);
    }
  }
  expect(pinned, "the manifest must pin at least one shell asset, or this check is vacuous").toBeGreaterThan(0);
});

test("a first-load tool call answers on the freshly booted console", async ({ app }) => {
  // `lookup_term` is a CORE-segment tool, so this runs before anything demand-loads the
  // reasoning segment — a genuine first-load answer, not one behind a second fetch.
  const answer = await app.call("lookup_term", await TOOL_INPUTS.lookup_term({ call: app.call.bind(app) }));
  expect(answer.iri, "the engine must answer with the term's IRI").toBe(
    "https://blackcatinformatics.ca/gmeow/ToolCall",
  );
  expect(typeof answer.definition, "…and its definition").toBe("string");
  expect(answer.definition.length).toBeGreaterThan(0);
});
