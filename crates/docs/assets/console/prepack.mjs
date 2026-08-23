// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Stage the console's engine payload into `pkg/` before the tarball is cut.
//
// # Why the package needs a payload at all
//
// `<gmeow-console>` is not a view over somebody else's engine: it OWNS one. `element.mjs`
// starts `engine.worker.mjs`, which imports the browser transport, which imports the
// client-side BLAKE3 and the always-resident core wasm image, boots that image over the
// `gmeow.gts` snapshot (verified against the integrity manifest), and demand-loads the
// reasoning segment when a pane first needs it. A package that ships the element without
// those bytes ships a console that cannot start — the relative import walks out of the
// package and 404s, and the reader is told the engine worker failed to load.
//
// So the published package carries the whole payload, and it carries EXACTLY the bytes the
// assembled console tree carries, because it is cut from that tree: this script runs the
// one producer (`gmeow-dev console-assemble`) and copies its `assets/` output into `pkg/`.
// There is no second copy of anything in the repository and nothing is hand-copied.
//
// # Why `pkg/`
//
// The same reason every wasm engine package in this repository uses it: `pkg/` is the
// build-output directory a package materializes at pack time and git ignores. The console's
// payload is the same kind of thing — a generated product of the producer, not a reviewed
// source file — and putting it anywhere else would make the console's `files` list claim
// checked-in sources that are not.
//
// # Fail-closed
//
// Every failure here aborts the pack. npm silently omits a `files` entry that does not
// exist, so a payload that quietly failed to stage would publish an element with no engine
// — the exact defect this script exists to make impossible.
//
// Run automatically by `npm pack` / `npm publish` via the `prepack` script. Set `GMEOW_DEV`
// to a prebuilt binary (`GMEOW_DEV=target/release/gmeow-dev`) to skip the cargo build.

import { execFileSync } from "node:child_process";
import { cp, mkdtemp, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL("./", import.meta.url));
const repoRoot = fileURLToPath(new URL("../../../../", import.meta.url));
const payload = join(here, "pkg");

/**
 * The producer command, as `[program, ...args]`.
 *
 * Mirrors the Makefile's `GMEOW_DEV ?= cargo run -q -p gmeow-dev-cli --` so a caller can
 * point both at the same prebuilt binary.
 */
const producer = (process.env.GMEOW_DEV ?? "cargo run -q -p gmeow-dev-cli --")
  .split(/\s+/)
  .filter((word) => word.length > 0);

/**
 * Every payload file the package's `files` list declares, relative to `pkg/`.
 *
 * Checked AFTER the copy, by name, because npm's own behaviour on a missing `files` entry
 * is to drop it without a word: an unchecked stage would publish a console whose engine is
 * simply absent and whose failure only appears in a consumer's browser.
 */
const REQUIRED = [
  "mcp-transport.mjs",
  "blake3.mjs",
  "bundle-manifest.json",
  "gmeow.gts",
  "mcp-core/index.mjs",
  "mcp-core/pkg/gmeow_mcp_core_wasm.js",
  "mcp-core/pkg/gmeow_mcp_core_wasm_bg.wasm",
  "mcp/index.mjs",
  "mcp/pkg/gmeow_mcp_wasm.js",
  "mcp/pkg/gmeow_mcp_wasm_bg.wasm",
];

const scratch = await mkdtemp(join(tmpdir(), "gmeow-console-prepack-"));
try {
  execFileSync(producer[0], [...producer.slice(1), "console-assemble", "--out", scratch], {
    cwd: repoRoot,
    stdio: ["ignore", "inherit", "inherit"],
  });

  await rm(payload, { recursive: true, force: true });
  await cp(join(scratch, "assets"), payload, { recursive: true });

  const missing = [];
  for (const name of REQUIRED) {
    const path = join(payload, name);
    const size = await stat(path).then(
      (info) => (info.isFile() ? info.size : 0),
      () => 0,
    );
    if (size === 0) missing.push(name);
  }
  if (missing.length > 0) {
    throw new Error(
      `the assembled console tree carried no bytes for: ${missing.join(", ")} — refusing to ` +
        "pack a console package whose engine payload is incomplete",
    );
  }
  console.log(`prepack: staged ${REQUIRED.length} payload files into ${payload}`);
} finally {
  await rm(scratch, { recursive: true, force: true });
}
