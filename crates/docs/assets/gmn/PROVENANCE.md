<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Vendored gmeow-gmn-wasm engine

These files are the **prebuilt** [`gmeow-gmn-wasm`](../../../gmn-wasm) engine — the
shipped GMEOW Model Notation codec (`gmeow-lang-bridge`'s GMN-0↔GMN-1 codec + glyph
symbology) compiled to `wasm32-unknown-unknown` — pinned here so the generated
documentation site can transcode authored RDF into the token-compact GMN-1 surface, and
back, entirely in the browser (no server, no network, no repository). They are emitted
verbatim into the rendered site under `assets/gmn/` (a language-neutral path) and
constitute the GMN transcode runtime the docs controller loads. GMN-2 (lossy compaction)
and the zstd-dictionary transport are NOT here — that notation is still being built.

## Files

| File | Role |
|------|------|
| `gmeow_gmn_wasm.js` | wasm-bindgen `--target web` ES-module bindings (exposes `to_gmn1`, `from_gmn1`, `glyph_legend`, `version`). |
| `gmeow_gmn_wasm_bg.wasm` | The compiled GMN-0↔GMN-1 codec, with the `lang:` glyph/alias codebook embedded. |
