<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-gmn-wasm

The shipped **GMEOW Model Notation** codec compiled to `wasm32-unknown-unknown`, so
editor plugins, browsers, and LLM clients can transcode authored GMEOW RDF into the
token-compact GMN-1 surface — and back — **client-side**, no server, no repository.

It wraps the wasm-clean [`gmeow-lang-bridge`](../lang-bridge) GMN-0↔GMN-1 codec with the
`lang:` glyph/alias codebook embedded. Byte-exact round-trip and parity with the native
codec are proven by the Node witness lane (`make gmn-wasm-pkg-test`, gate-enforced on
every pull request via `wasm-parity` in the required CI `make heavy` lane). GMN-2 (lossy
compaction) and the zstd-dictionary transport are
NOT built here — that notation is still being developed.

## JavaScript API

```js
import { ready, to_gmn1, from_gmn1, version } from "gmeow-gmn-wasm";

await ready();                          // one-time wasm instantiation
const gmn1 = to_gmn1(
  "@prefix ex: <https://example.org/> . ex:a a ex:Cat .",
  "turtle",
);
const back = from_gmn1(gmn1);           // round-trips to the source RDF
```

`to_gmn1(data, format)` transcodes RDF text into GMN-1; `from_gmn1(gmn1_text)` decodes it
back. The underlying wasm package also exposes `glyph_legend()` (the glyph/alias codebook,
as JSON) for the docs controller's glyph-hover glosses. Each throws on malformed input.

## Build

```sh
make gmn-wasm-pkg        # release wasm + wasm-bindgen web bindings → js/pkg/
make gmn-wasm-pkg-test   # build + Node native↔wasm parity witness lane
```
