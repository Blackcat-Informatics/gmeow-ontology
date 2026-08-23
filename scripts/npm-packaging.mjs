// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The one JavaScript-side authority for "what this repository publishes to npm".
//
// # Names come from the shipped bytes
//
// There is no list of package names anywhere in this file. The published set is
// DISCOVERED by walking the repository for `package.json` files and keeping the ones that
// are not `"private": true` — so the dev-only Playwright smoke manifest excludes itself,
// by its own declaration, and adding a package cannot forget to update a registry.
//
// # Export-set EQUALITY, never a substring probe
//
// `assertPackageExportSets` compares three surfaces as SETS:
//
//   1. the wasm-bindgen-generated `pkg/<module>.d.ts` — what the engine actually exports;
//   2. the package entry (`exports["."].import`) — what the shipped JS re-exports;
//   3. the hand-written declaration file (`exports["."].types`) — what a TypeScript
//      consumer is told exists.
//
// Legs 2 and 3 must be EQUAL: a runtime export with no declaration is invisible to a
// typed consumer, and a declaration with no runtime export is a lie. Leg 1 must be equal
// to the set of names the entry IMPORTS from `./pkg/…`: an engine export the package
// never imports is shipped-but-unreachable capability, which is exactly the silent
// degradation the no-optionality rule forbids. A wrapper is allowed to RENAME an engine
// export on the way out (the MCP images rename the engine's own `ready` to `loaded`, so
// `ready` can keep the sibling shims' instantiation meaning) — but only via an explicit
// `as` alias that is then used in the module body, never by dropping it.

import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";

/** Directory names never descended into when discovering packages. */
const PRUNED = new Set([
  ".git",
  ".worktrees",
  "node_modules",
  "target",
  "dist",
  "generated",
  "ontology-docs",
  "coverage",
  "imports",
  ".venv",
  ".cache",
]);

/** The repository root, from this file's own location. */
export function repoRoot() {
  return new URL("../", import.meta.url);
}

async function walkForManifests(dirUrl, found) {
  let entries;
  try {
    entries = await readdir(fileURLToPath(dirUrl), { withFileTypes: true });
  } catch {
    return found;
  }
  for (const entry of entries) {
    if (entry.isDirectory()) {
      if (PRUNED.has(entry.name)) continue;
      await walkForManifests(new URL(`${entry.name}/`, dirUrl), found);
    } else if (entry.name === "package.json") {
      found.push(new URL(entry.name, dirUrl));
    }
  }
  return found;
}

/**
 * Every `package.json` in the repository, published or not, as
 * `{ dir, path, manifest }` sorted by path. The dev-only manifests are included here —
 * `publishedPackages()` is what filters them out, using their own `private` declaration.
 */
export async function allManifests() {
  const found = await walkForManifests(repoRoot(), []);
  const read = await Promise.all(
    found.map(async (url) => ({
      dir: new URL("./", url),
      path: fileURLToPath(url).slice(fileURLToPath(repoRoot()).length),
      manifest: JSON.parse(await readFile(url, "utf8")),
    })),
  );
  return read.sort((a, b) => (a.path < b.path ? -1 : 1));
}

/**
 * The manifests this repository publishes TO THE NPM REGISTRY.
 *
 * Two exclusions, both read from the manifest's own bytes rather than from a name list:
 *
 *  * `"private": true` — the dev-only Playwright smoke manifest excludes itself;
 *  * `engines.vscode` — a VS Code extension manifest. It is published, but by `vsce` to
 *    the Visual Studio Marketplace, on that registry's own cadence and with that
 *    registry's own metadata contract (`publisher`, `contributes`, `activationEvents`).
 *    Holding it to the npm contract (scope, `exports`, `files`, `types`) would be holding
 *    it to the wrong registry's rules.
 */
export async function publishedPackages() {
  return (await allManifests()).filter(
    (entry) =>
      entry.manifest.private !== true && entry.manifest.engines?.vscode === undefined,
  );
}

/** The VS Code Marketplace manifests: published, but not to npm. */
export async function marketplacePackages() {
  return (await allManifests()).filter(
    (entry) => entry.manifest.private !== true && entry.manifest.engines?.vscode !== undefined,
  );
}

// ── ESM / TypeScript surface parsing ────────────────────────────────────────────────

/** Strip line and block comments so a commented-out `export` is never counted. */
function stripComments(source) {
  return source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^[ \t]*\/\/.*$/gm, "");
}

const DECLARED = /^\s*export\s+(?:declare\s+)?(?:async\s+)?(?:function|class|const|let|var)\s+([A-Za-z_$][\w$]*)/gm;
const RE_EXPORT_BLOCK = /^\s*export\s*\{([^}]*)\}\s*;?/gm;

/**
 * The set of VALUE names a module (or declaration file) exports.
 *
 * Type-only declarations (`export interface`, `export type`) are deliberately excluded:
 * they have no runtime counterpart, so including them would make a `.d.ts` that carries
 * an interface unequal to the module it describes for a reason that is not a defect.
 */
export function exportedValueNames(source) {
  const clean = stripComments(source);
  const names = new Set();
  for (const match of clean.matchAll(DECLARED)) names.add(match[1]);
  for (const match of clean.matchAll(RE_EXPORT_BLOCK)) {
    for (const clause of match[1].split(",")) {
      const parts = clause.trim().split(/\s+as\s+/);
      if (parts[0] === "") continue;
      names.add((parts[1] ?? parts[0]).trim());
    }
  }
  return names;
}

/**
 * The set of names a module imports from its colocated wasm-bindgen bindings, as
 * `{ source, local }` pairs. The default import (wasm-bindgen's `init`) is NOT an engine
 * export and is excluded.
 */
export function engineImports(source) {
  const clean = stripComments(source);
  // Anchored on the `./pkg/…` STATEMENT and walked backwards, mirroring `engine_imports`
  // in `crates/gmeow-dev-cli/tests/npm_packaging_contract.rs` exactly.
  //
  // The regex this replaces started at the FIRST `import` in the file and let a lazy
  // `[\s\S]*?` run all the way to the pkg `from`, then took a GREEDY `\{([\s\S]*)\}` over
  // that span — so any import statement preceding the engine one contributed its braces
  // to the clause, and every downstream export-set EQUALITY assertion could fail for a
  // reason that has nothing to do with the engine. It passed only because the `./pkg/*.js`
  // import happens to be first in every entry today.
  const from = clean.indexOf('from "./pkg/');
  if (from === -1) return [];
  const head = clean.slice(0, from);
  const open = head.lastIndexOf("{");
  if (open === -1) return [];
  const close = head.indexOf("}", open);
  if (close === -1) return [];
  return head
    .slice(open + 1, close)
    .split(",")
    .map((clause) => clause.trim())
    .filter((clause) => clause.length > 0)
    .map((clause) => {
      const parts = clause.split(/\s+as\s+/);
      return { source: parts[0].trim(), local: (parts[1] ?? parts[0]).trim() };
    });
}

/**
 * The set of function names a wasm-bindgen `.d.ts` declares, minus the instantiation
 * boilerplate (`initSync` and the default `__wbg_init`), which is not engine capability
 * and is consumed by the wrapper's own `ready()`.
 */
export function wasmBindgenExports(source) {
  const names = exportedValueNames(source);
  names.delete("initSync");
  names.delete("__wbg_init");
  return names;
}

function sorted(set) {
  return [...set].sort();
}

/** Resolve a package `exports` conditional entry to `{ importUrl, typesUrl }`. */
function entryUrls(dirUrl, entry) {
  assert.ok(entry.import, "every exports entry declares an `import` target");
  assert.ok(entry.types, "every exports entry declares a `types` target");
  return {
    importUrl: new URL(entry.import, dirUrl),
    typesUrl: new URL(entry.types, dirUrl),
  };
}

/**
 * Assert the export sets of one package agree, for EVERY subpath its `exports` map
 * declares, and — for a wasm engine package — that the entry's engine imports are exactly
 * the engine's own exports.
 *
 * `dirUrl` is the package directory (the one holding `package.json`).
 */
export async function assertPackageExportSets(dirUrl) {
  const manifest = JSON.parse(await readFile(new URL("package.json", dirUrl), "utf8"));
  const name = manifest.name;
  assert.ok(manifest.exports, `${name} declares an \`exports\` map`);

  for (const [subpath, entry] of Object.entries(manifest.exports)) {
    const { importUrl, typesUrl } = entryUrls(dirUrl, entry);
    const runtime = exportedValueNames(await readFile(importUrl, "utf8"));
    const declared = exportedValueNames(await readFile(typesUrl, "utf8"));
    assert.deepEqual(
      sorted(runtime),
      sorted(declared),
      `${name} ${subpath}: the module's runtime exports and its \`types\` declarations ` +
        "are not the same set",
    );
  }

  // The engine leg: only a package whose entry imports wasm-bindgen bindings has one.
  const rootEntry = manifest.exports["."];
  const entrySource = await readFile(new URL(rootEntry.import, dirUrl), "utf8");
  const imports = engineImports(entrySource);
  if (imports.length === 0) return;

  const bindings = entrySource.match(/from\s+"(\.\/pkg\/[^"]+\.js)"/)[1];
  const engineDts = new URL(bindings.replace(/\.js$/, ".d.ts"), dirUrl);
  const engine = wasmBindgenExports(await readFile(engineDts, "utf8"));

  assert.deepEqual(
    sorted(new Set(imports.map((i) => i.source))),
    sorted(engine),
    `${name}: the entry does not import EXACTLY the wasm-bindgen export set — an engine ` +
      "export the package never imports is shipped-but-unreachable capability",
  );

  // Every imported engine name must reach the consumer: either re-exported under its own
  // name, or renamed via an `as` alias that the module body actually uses.
  const runtime = exportedValueNames(entrySource);
  const body = stripComments(entrySource).replace(/import[\s\S]*?from\s+"[^"]+";/g, "");
  for (const { source, local } of imports) {
    if (runtime.has(source)) continue;
    assert.notEqual(
      local,
      source,
      `${name}: engine export \`${source}\` is imported but neither re-exported nor aliased`,
    );
    assert.ok(
      body.includes(local),
      `${name}: engine export \`${source}\` is aliased to \`${local}\`, which the module ` +
        "body never uses — the capability is dropped, not renamed",
    );
  }
}
