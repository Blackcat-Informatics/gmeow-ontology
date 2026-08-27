// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The producer's own contract, checked against the tree this lane is driving.
//
//   * assembling twice yields byte-identical trees;
//   * the tree under test IS what the producer emits, key for key — a stale leftover from a
//     previous assemble is how `smoke/` stayed deployed after it was removed from the shell
//     file set, and how it stayed pre-cached in every reader's offline storage;
//   * `--out ontology-docs` is REFUSED, naming the one writer of that base;
//   * nothing under `smoke/` is emitted, and Playwright — this lane's own dependency — is in
//     no published package's `dependencies`.
//
// No browser is needed for any of it, and it rides this lane rather than a separate one
// because these are claims about the very bytes the specs above drive.

import { execFileSync } from "node:child_process";
import { promises as fs } from "node:fs";
import { join } from "node:path";

import { expect, test } from "../lib/test.mjs";
import { REPO_ROOT } from "../lib/paths.mjs";
import { listFiles } from "../lib/tree.mjs";

/** The canonical producer target; CI injects its authenticated `GMEOW_DEV` into Make. */
const PRODUCER = ["--no-print-directory", "console-assemble"];

/** Assemble into `out` and return its file list. */
function assemble(out) {
  execFileSync("make", [...PRODUCER, `CONSOLE_OUT=${out}`], {
    cwd: REPO_ROOT,
    stdio: ["ignore", "pipe", "inherit"],
  });
  return listFiles(out);
}

/** The BLAKE3 of every file in a tree, keyed by tree-relative path. */
async function digests(root, names) {
  const { blake3Hex } = await import(
    /* the console's own client-side BLAKE3, so the comparison uses the shipped function */
    new URL(`file://${join(root, "assets", "blake3.mjs")}`).href
  );
  const out = {};
  for (const name of names) {
    out[name] = blake3Hex(new Uint8Array(await fs.readFile(join(root, ...name.split("/")))));
  }
  return out;
}

test("console-assemble is byte-reproducible across two independent runs", async () => {
  const scratch = join(REPO_ROOT, "target", "console-smoke");
  const first = join(scratch, "reproducible-a");
  const second = join(scratch, "reproducible-b");
  await fs.rm(first, { recursive: true, force: true });
  await fs.rm(second, { recursive: true, force: true });

  const namesA = await assemble(first);
  const namesB = await assemble(second);
  expect(namesA.length, "the producer must emit files").toBeGreaterThan(0);
  expect(namesB, "two runs emit the same key set").toEqual(namesA);
  expect(await digests(second, namesB), "two runs emit the same bytes").toEqual(
    await digests(first, namesA),
  );

  await fs.rm(first, { recursive: true, force: true });
  await fs.rm(second, { recursive: true, force: true });
});

test("the tree under test is exactly what the producer emits", async ({ assembled }) => {
  const scratch = join(REPO_ROOT, "target", "console-smoke", "reference");
  await fs.rm(scratch, { recursive: true, force: true });
  const reference = await assemble(scratch);
  const driven = await listFiles(assembled);

  const stale = driven.filter((name) => !reference.includes(name));
  const absent = reference.filter((name) => !driven.includes(name));
  expect(
    stale,
    "the tree this lane is driving carries files the producer does not emit — a previous " +
      "assemble left them behind, and a served leftover is a deployed leftover",
  ).toEqual([]);
  expect(absent, "the tree this lane is driving is missing emitted files").toEqual([]);

  await fs.rm(scratch, { recursive: true, force: true });
});

test("console-assemble REFUSES the regen-owned bases, naming the one writer", async () => {
  for (const base of ["ontology-docs", "ontology-docs/console", "dist/gmeow-docs"]) {
    let failed = false;
    let message = "";
    try {
      execFileSync("make", [...PRODUCER, `CONSOLE_OUT=${base}`], {
        cwd: REPO_ROOT,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      });
    } catch (error) {
      failed = true;
      message = `${error.stdout ?? ""}${error.stderr ?? ""}`;
    }
    expect(failed, `console-assemble --out ${base} must refuse`).toBe(true);
    expect(message, "the refusal must name the one writer of that base").toContain(
      "make regen SYNC_OUTPUTS=docs",
    );
  }
});

test("nothing under smoke/ is emitted, and Playwright ships in no published package", async ({
  assembled,
}) => {
  const emitted = await listFiles(assembled);
  expect(
    emitted.filter((name) => name.includes("/smoke/")),
    "the dev-only Playwright lane must not be deployed or pre-cached",
  ).toEqual([]);

  // The published set is DISCOVERED from the shipped bytes, never restated here.
  const dirs = execFileSync("node", ["scripts/npm-package-dirs.mjs"], {
    cwd: REPO_ROOT,
    encoding: "utf8",
  })
    .trim()
    .split("\n")
    .filter((line) => line.length > 0);
  expect(dirs.length, "the repository must publish packages for this to mean anything").toBeGreaterThan(0);

  for (const dir of dirs) {
    const manifest = JSON.parse(await fs.readFile(join(REPO_ROOT, dir, "package.json"), "utf8"));
    const runtime = Object.keys(manifest.dependencies ?? {});
    expect(
      runtime.filter((name) => name.includes("playwright")),
      `${dir} declares Playwright as a runtime dependency`,
    ).toEqual([]);
    expect(
      (manifest.files ?? []).filter((name) => name.startsWith("smoke/")),
      `${dir} publishes something under smoke/`,
    ).toEqual([]);
  }

  // …and this lane's own manifest is private and dev-only, so it can never be published.
  const smoke = JSON.parse(
    await fs.readFile(join(REPO_ROOT, "crates/docs/assets/console/smoke/package.json"), "utf8"),
  );
  expect(smoke.private, "the smoke lane's package must be private").toBe(true);
  expect(Object.keys(smoke.devDependencies ?? {}), "Playwright is a DEV dependency of the lane").toContain(
    "@playwright/test",
  );
  expect(Object.keys(smoke.dependencies ?? {}), "the lane declares no runtime dependency").toEqual([]);
});
