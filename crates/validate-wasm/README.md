<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-validate-wasm

The repo-free GMEOW **Tier-1** validator compiled to `wasm32-unknown-unknown`, so
editor plugins, browsers, and LLM clients can check authored GMEOW RDF against a
`gmeow.gts` bundle **client-side** — no server, no repository, no Docker.

It wraps the wasm-clean [`gmeow-validate`](../validate) core: SHACL against the
bundle's data-graph shape union plus the OntoUML disciplines. The Tier-2 `--deep`
semantic pass reasons through the native DL engine, which does not compile to wasm,
so this surface is Tier-1 only by contract.

## JavaScript API

```js
import { ready, validate, version } from "gmeow-validate-wasm";

await ready();                          // one-time wasm instantiation
const bundle = /* Uint8Array of gmeow.gts */;
const report = JSON.parse(
  validate(
    "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> . ex:a a gmeow:Person .",
    "turtle",
    bundle,
    "https://blackcatinformatics.ca/gmeow/",
    "my-data.ttl",
  ),
);
// report.findings?.filter((f) => f.severity === "error")
```

`validate(data, format, gts, namespace, origin)` returns the canonical diagnostics
`Report` as a JSON string (`findings` omitted when the graph conforms); it throws on
a malformed bundle or unparsable input.

## Build

```sh
make wasm-pkg        # release wasm + wasm-bindgen web bindings → js/pkg/
make wasm-pkg-test   # build + Node real-execution round-trip lane
```
