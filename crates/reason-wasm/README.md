<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-reason-wasm

The native GMEOW **structured-DL reasoner** compiled to `wasm32-unknown-unknown`, so
editor plugins, browsers, and LLM clients can run the reasoning chase over authored
GMEOW RDF **client-side** — no server, no repository, no Docker.

It wraps the wasm-clean [`gmeow-logic`](../logic) chase: it runs the same structured-DL
reasoning as the native engine (serially on wasm) and returns a complete binary GTS
artifact with its native execution receipt, asserted and inferred RDF closure, and
admitted verdict facets. Byte identity with the native reasoner is checked by the Node parity witness lane
(`make reason-wasm-pkg-test`, gate-enforced on every pull request via `wasm-parity` in
the required CI `make heavy` lane).

## JavaScript API

```js
import { admit_artifact, ready, reason } from "gmeow-reason-wasm";

await ready();                          // one-time wasm instantiation
const response = JSON.parse(reason(
  "@prefix ex: <https://example.org/> . ex:a a ex:Cat .",
  "turtle",
));
const bytes = Uint8Array.from(atob(response.artifact.output), (byte) => byte.charCodeAt(0));
const admitted = JSON.parse(admit_artifact(bytes));
const closure = admitted.closure_nquads;
// Retain response.artifact: it carries the complete evidence behind these RDF facets.
```

`reason(data, format)` runs the chase over `data` (RDF text in `format` — `turtle`,
`n-triples`, `n-quads`, `trig`, `rdf+xml`, `json-ld`) and returns JSON. Its `artifact`
envelope declares `format: "gts"`, the media type, canonical base64 encoding, decoded
byte length, and encoded bytes in `output`. `closure_nquads` is the complete asserted
and inferred RDF closure, including selected nonempty-domain witnesses;
`judgment_nquads` carries the admitted reasoning summary. `admit_artifact(bytes)`
checks the complete artifact and required attachments before returning these facets.
Missing or invalid evidence throws, as do parse, reasoning and serialization failures.

`conjecture(kb, format, formula, standpoint)` returns the same envelope and admitted
judgment facets for a standpoint-scoped conjecture verdict. The standalone
conjecture artifact carries its native receipt; it does not claim a full RDF closure.

## Build

```sh
make reason-wasm-pkg        # release wasm + wasm-bindgen web bindings → js/pkg/
make reason-wasm-pkg-test   # build + Node native↔wasm parity witness lane
```
