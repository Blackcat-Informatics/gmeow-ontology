// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Print the repo-relative directory of every package this repository publishes, one per
// line, sorted. The set is DISCOVERED from the shipped bytes (every `package.json` that
// does not declare itself `"private": true`) — so the Make lanes and the release workflow
// iterate the real set rather than a list somebody has to remember to update.
//
// Usage: `node scripts/npm-package-dirs.mjs [--names]`

import { fileURLToPath } from "node:url";

import { publishedPackages, repoRoot } from "./npm-packaging.mjs";

const root = fileURLToPath(repoRoot());
const wantNames = process.argv.includes("--names");
const packages = await publishedPackages();

if (packages.length === 0) {
  console.error("no publishable package.json found — the discovery walk is broken");
  process.exit(1);
}

for (const pkg of packages) {
  if (wantNames) {
    console.log(pkg.manifest.name);
  } else {
    console.log(fileURLToPath(pkg.dir).slice(root.length).replace(/\/$/, ""));
  }
}
