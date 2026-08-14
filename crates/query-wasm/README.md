<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-query-wasm

The RDF 1.2 / SPARQL engine behind the documentation site's **offline** query
playground and bundle explorer, compiled to `wasm32-unknown-unknown`. No server,
no network, no repository checkout.

## Why this crate exists

The playground engine used to be a **prebuilt blob vendored from the sibling
`purrdf` repository** (`crates/docs/assets/purrdf/`), pinned only by BLAKE3 of its
bytes — no version, no revision — and refreshed by a Make target that **did not
exist**. Its provenance file pointed at `crates/rdf-wasm`, a path that is not in
this repository. It therefore could not be rebuilt here, and nothing detected it
drifting from the `purrdf` the workspace actually pins.

Building the engine here removes that drift class outright: the shipped engine is
compiled from the workspace pin, by a real target, with a real parity lane.

## Surface

| JS | Role |
|---|---|
| `ready(bytesOrUrl?)` | One-time async wasm instantiation (the `web` target needs it) |
| `Dataset.parse(text, format)` | Parse any format `purrdf` classifies (`turtle`, `trig`, `nquads`, `jsonld`, …) |
| `Dataset.fromGts(bytes)` | Read **every named graph** of a `gmeow.gts` bundle |
| `dataset.size` | Quad count across every graph |
| `dataset.query(sparql, base?)` | SPARQL Results JSON for SELECT/ASK; Turtle for CONSTRUCT/DESCRIBE |
| `dataset.serialize(format)` | Re-encode; dataset-capable formats keep every named graph |
| `version()` | Crate SemVer — the liveness probe that the module instantiated |

`fromGts` is what lets the playground query the **shipped bundle** rather than a
flattened core extract, so named graphs and the RDF 1.2 statement layer stay
addressable — the `.goals` requirement that RDF 1.2 is first-class and that
information is only trimmed at exit gates.

## Refusals are refusals

A malformed document, an unknown format, an unevaluable query, or a
`SERVICE` / `LOAD` clause the browser cannot resolve **throws**. None of them
returns an empty result that reads like "no matches".

## Build and parity

```sh
make query-wasm-pkg        # release wasm32 + wasm-bindgen (web) + required wasm-opt -Oz
make query-wasm-pkg-test   # the above, then the Node parity lane
make maint-refresh-query-asset   # re-vendor into crates/docs/assets/query/ and re-pin digests
```

Parity is a **corpus**, not a single golden pair: `js/tests/corpus.trig` plus
`js/tests/queries.json` exercise SELECT, ASK, CONSTRUCT, DESCRIBE, named-graph
selection, typed and language-tagged literals, and a quoted-triple annotation. The
native half (`tests/witness_query.rs`) and the wasm half read the *same* corpus and
query file and compare against the *same* attestation, so the two cannot drift in
what they ask. The corpus is committed and self-contained rather than bundle-scoped:
engine parity is a property of the engine, and a bundle-scoped witness would red on
every ontology edit.
