<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# purrdf (wasm) — RDF 1.2 in the browser & node, the RDF/JS way

`purrdf` is a `wasm32`, **in-memory** RDF 1.2 engine compiled from the oxigraph-free
[`gmeow-rdf`](../rdf) kernel and surfaced to JavaScript/TypeScript through the
[RDF/JS](https://rdf.js.org/) community spec (`DataFactory`, `DatasetCore`,
`Stream`/`Sink`). It is parcel **P10** of the purrdf program
(EPIC #832, [`docs/design/PURRDF-PLAN.md`](../../docs/design/PURRDF-PLAN.md)).

> **Status:** under construction (issue #846). The crate scaffold + wasm toolchain
> gate land first; the DataFactory / DatasetCore / Stream surface and the npm package
> follow in the subsequent commits of this PR.

## The RDF-1.2 wedge

No incumbent RDF/JS library carries RDF-1.2 **quoted-triple terms** or **directional
literals**. purrdf's `DataFactory` accepts a quoted triple anywhere a term is expected
and round-trips base direction — the deliberate extension to stock RDF/JS.

## Scope

- **In-memory only** — the oxigraph `Store` (RocksDB) and the logic engine do not
  compile to wasm and are excluded by design. SPARQL query is out of scope here.
- Text codecs (Turtle / N-Triples / N-Quads / TriG / RDF-XML) ride the wasm-clean
  `gmeow-gts/rdf-codecs` subset — no Store dependency.

## License

AGPL-3.0-only (the `gmeow-rdf` engine); the `gmeow-rdf-events` ingestion protocol it
depends on is permissive (MIT OR Apache-2.0).
