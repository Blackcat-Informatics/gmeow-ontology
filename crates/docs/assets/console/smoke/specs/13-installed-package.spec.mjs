// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The PUBLISHED package, installed and booted.
//
// This is the assertion whose absence let a completely non-functional package ship: the
// element's worker imported `../assets/mcp-transport.mjs`, which resolves in the repository
// and in the assembled site tree and in NO published tarball, so an installed console
// answered `404 /node_modules/@blackcatinformatics/assets/mcp-transport.mjs` and told the
// reader its engine worker had failed. Every in-tree lane was green.
//
// So nothing here is in-tree. The global setup ran the package's own `npm pack` — which runs
// its `prepack`, which stages the engine payload out of `gmeow-dev console-assemble` —
// installed the tarball BY NAME into a scratch project, and wrote a page whose body is the
// Install section of the SHIPPED README, verbatim. What is driven below is that page.

import { promises as fs } from "node:fs";
import { join } from "node:path";

import { expect, test } from "../lib/test.mjs";
import { installedProjectDir } from "../lib/paths.mjs";

/** The installed package's own root inside the scratch project. */
const INSTALLED = "node_modules/@blackcatinformatics/gmeow-console";

test("the packed tarball installs by name and carries its declared file set", async () => {
  const project = installedProjectDir();
  const root = join(project, ...INSTALLED.split("/"));
  const manifest = JSON.parse(await fs.readFile(join(root, "package.json"), "utf8"));

  expect(manifest.name).toBe("@blackcatinformatics/gmeow-console");
  expect(Array.isArray(manifest.files), "the package declares a files set").toBe(true);

  // npm drops a `files` entry that does not exist, without a word. Every declared member is
  // checked BY NAME against the installed tree, with a non-zero size: that silence is
  // exactly how an engine payload goes missing.
  const missing = [];
  for (const name of manifest.files) {
    const size = await fs
      .stat(join(root, ...name.split("/")))
      .then((info) => (info.isFile() ? info.size : 0))
      .catch(() => 0);
    if (size === 0) missing.push(name);
  }
  expect(missing, `the installed package carries no bytes for: ${missing.join(", ")}`).toEqual([]);

  // Playwright is a DEV-only smoke dependency and must not follow the console to a consumer.
  expect(manifest.dependencies ?? {}, "a published package declares no runtime dependency").toEqual({});
  expect(JSON.stringify(manifest)).not.toContain("playwright");

  // Nothing under `smoke/` may ship: it was deployed once, and pre-cached offline with the
  // rest of the shell, while the README said it does not ship.
  const shipped = manifest.files.filter((name) => name.startsWith("smoke/"));
  expect(shipped, "nothing under smoke/ is part of the published package").toEqual([]);
  await expect(fs.stat(join(root, "smoke"))).rejects.toThrow();
});

test("the installed console boots the way the shipped README says to load it", async ({
  browser,
  origin,
}) => {
  const snippet = process.env.GMEOW_CONSOLE_SMOKE_INSTALL_SNIPPET;
  expect(typeof snippet, "the global setup must publish the README's Install snippet").toBe("string");
  expect(snippet, "the README loads the element from the installed package").toContain(
    "node_modules/@blackcatinformatics/gmeow-console/element.mjs",
  );

  const context = await browser.newContext({ serviceWorkers: "block" });
  const page = await context.newPage();
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(String(error?.message ?? error)));
  const failures = [];
  page.on("response", (response) => {
    if (response.status() >= 400) failures.push(`${response.status()} ${response.url()}`);
  });
  try {
    await page.goto(`${origin}/installed/`, { waitUntil: "load" });

    // Ready is read off the ELEMENT, not off a shell: the README's snippet ships no banner,
    // no version chip and no status region, so the only honest signal is the element's own
    // rendered surface. Either it paints its derived nav, or it paints its failure — and the
    // wait covers both so a broken package reports its own message instead of timing out.
    await page.waitForFunction(
      () => {
        const root = document.querySelector("gmeow-console")?.shadowRoot;
        if (root === undefined || root === null) return false;
        return root.querySelectorAll("nav button").length > 0 || root.querySelector(".failure") !== null;
      },
      undefined,
      { timeout: 480_000 },
    );

    const failure = await page.locator("gmeow-console .failure").count();
    if (failure > 0) {
      const text = await page.locator("gmeow-console .failure").innerText();
      throw new Error(`the installed console did not start: ${text}`);
    }

    const panes = await page.evaluate(
      () => document.querySelector("gmeow-console").shadowRoot.querySelectorAll("nav button").length,
    );
    expect(panes, "the installed console derives its pane set from the packaged bundle").toBeGreaterThan(5);

    // A real tool call, through the INSTALLED worker and the INSTALLED engine payload.
    const answer = await page.evaluate(() =>
      document.querySelector("gmeow-console").ask("invoke", {
        tool: "lookup_term",
        args: { term: "gmeow:ToolCall" },
      }),
    );
    expect(answer.iri).toBe("https://blackcatinformatics.ca/gmeow/ToolCall");

    // The reasoning segment demand-loads out of the package too — the second half of the
    // payload, and the one a `files` list is most likely to have dropped.
    const closure = await page.evaluate(() =>
      document.querySelector("gmeow-console").ask("invoke", {
        tool: "reason_graph",
        args: {
          data:
            "@prefix ex: <https://example.org/gmeow/console/> .\n" +
            "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n" +
            "ex:Recorded rdfs:subClassOf ex:Audited .\nex:call a ex:Recorded .\n",
          format: "turtle",
        },
      }),
    );
    expect(closure.entailed_count, "the installed reasoning segment must answer").toBeGreaterThan(0);

    expect(failures, "the installed console fetched nothing that 404ed").toEqual([]);
    expect(pageErrors, "no uncaught error may reach a booted installed console").toEqual([]);
  } finally {
    await context.close();
  }
});
