<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Vendored gmeow-reason-wasm engine

These files are the **prebuilt** [`gmeow-reason-wasm`](../../../reason-wasm) engine —
the repo-free Tier-1 GMEOW validator (SHACL + OntoUML disciplines over a `gmeow.gts`
bundle) compiled to `wasm32-unknown-unknown` — pinned here so the generated documentation
site can reason authored RDF entirely in the browser (no server, no network, no
repository). They are emitted verbatim into the rendered site under `assets/reason/`
(a language-neutral path) and constitute the validation runtime the docs controller loads.

## Files

| File | Role |
|------|------|
| `gmeow_reason_wasm.js` | wasm-bindgen `--target web` ES-module bindings (exposes `reason`, `version`). |
| `gmeow_reason_wasm_bg.wasm` | The compiled structured-DL reasoner (the native `gmeow-reason` core, reasoner-free). |
