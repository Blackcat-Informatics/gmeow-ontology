// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Engine identity: what the served images say they are, and what the workspace says they
// should be.
//
// Both vendored segments export `version()` out of their own wasm, so the number comes from
// the shipped bytes rather than from a manifest beside them. It must agree with the
// workspace's `[workspace.package] version` — the one authority a release is cut from — and
// the two segments must agree with each other, because a console running a core image from
// one build and a reasoning segment from another is a console with two ontologies in it.

import { promises as fs } from "node:fs";
import { join } from "node:path";

import { expect, test } from "../lib/test.mjs";
import { REPO_ROOT } from "../lib/paths.mjs";

/** The workspace version — the tag every published engine is cut at. */
async function workspaceVersion() {
  const manifest = await fs.readFile(join(REPO_ROOT, "Cargo.toml"), "utf8");
  const section = manifest.slice(manifest.indexOf("[workspace.package]"));
  const version = /^version\s*=\s*"([^"]+)"/m.exec(section);
  if (version === null) {
    throw new Error("the workspace Cargo.toml declares no [workspace.package] version");
  }
  return version[1];
}

test("both served engine segments report the workspace version", async ({ app }) => {
  const expected = await workspaceVersion();

  const reported = await app.page.evaluate(async () => {
    const core = await import("/assets/mcp-core/index.mjs");
    await core.ready();
    const reasoning = await import("/assets/mcp/index.mjs");
    await reasoning.ready();
    return { core: core.version(), reasoning: reasoning.version() };
  });

  expect(reported.core, "the core image's own version()").toBe(expected);
  expect(reported.reasoning, "the reasoning segment's own version()").toBe(expected);
  expect(reported.core, "the two segments must be one build").toBe(reported.reasoning);
});

test("the tiering split is read out of the engine, not restated in JavaScript", async ({ app }) => {
  const split = await app.page.evaluate(async () => {
    const core = await import("/assets/mcp-core/index.mjs");
    await core.ready();
    return { deferred: core.deferredTools(), segment: core.deferredSegment() };
  });
  expect(split.deferred.length, "the core image must defer a non-empty tool set").toBeGreaterThan(0);
  expect(split.segment, "…to the reasoning segment").toBe("reasoning");

  // The deferred set is a strict subset of the advertised surface: a name the engine defers
  // but does not advertise would be a tool no caller could ever reach.
  const advertised = await app.page.evaluate(async () => {
    const { listTools } = await import("/assets/mcp-transport.mjs");
    return (await listTools()).map((tool) => tool.name);
  });
  for (const name of split.deferred) {
    expect(advertised, `the engine defers \`${name}\`, which it does not advertise`).toContain(name);
  }
  expect(split.deferred.length).toBeLessThan(advertised.length);
});
