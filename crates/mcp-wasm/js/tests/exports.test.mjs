// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Export-set EQUALITY for this package, over the BUILT wasm-bindgen bindings.
//
// Three surfaces, compared as sets rather than probed for substrings: the generated
// `pkg/<module>.d.ts` (what the engine exports), `index.mjs` (what the package
// re-exports), and `index.d.ts` (what a typed consumer is told exists). The shared
// checker is `scripts/npm-packaging.mjs`; it discovers everything from the shipped bytes.
//
// This lane runs under `make <crate>-pkg-test`, i.e. after the wasm-bindgen output
// exists. The engine-independent half of the same contract (crate source ↔ index.mjs ↔
// index.d.ts) rides the always-on Rust gate in
// `crates/gmeow-dev-cli/tests/npm_packaging_contract.rs`.
import { test } from "node:test";

import { assertPackageExportSets } from "../../../../scripts/npm-packaging.mjs";

test("the wasm .d.ts, index.mjs and index.d.ts declare the same export set", async () => {
  await assertPackageExportSets(new URL("../", import.meta.url));
});
