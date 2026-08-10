<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-reason-wasm

The native GMEOW **structured-DL reasoner** compiled to `wasm32-unknown-unknown`, so
editor plugins, browsers, and LLM clients can run the reasoning chase over authored
GMEOW RDF **client-side** — no server, no repository, no Docker.

It wraps the wasm-clean [`gmeow-logic`](../logic) chase: it runs the same structured-DL
reasoning as the native engine (serially on wasm — the parallel scheduler degrades to
sequential where threads are unavailable) and returns the reasoned closure, the inferred
triples. Byte-identity to the native reasoner is proven by the Node parity witness lane
(`make reason-wasm-pkg-test`, gate-enforced on every pull request via `wasm-parity` in
the required CI `make heavy` lane).

## JavaScript API

```js
import { ready, reason, version } from "gmeow-reason-wasm";

await ready();                          // one-time wasm instantiation
const closure = reason(
  "@prefix ex: <https://example.org/> . ex:a a ex:Cat .",
  "turtle",
);
// closure: N-Quads text of the inferred triples
```

`reason(data, format)` runs the chase over `data` (RDF text in `format` — `turtle`,
`n-triples`, `n-quads`, `trig`, `rdf+xml`, `json-ld`) and returns the reasoned closure as
N-Quads text; it throws on unparsable input, a reasoning failure, or a serialization error.

## Build

```sh
make reason-wasm-pkg        # release wasm + wasm-bindgen web bindings → js/pkg/
make reason-wasm-pkg-test   # build + Node native↔wasm parity witness lane
```
