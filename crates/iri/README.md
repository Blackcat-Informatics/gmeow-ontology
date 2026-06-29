<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-iri

`gmeow-iri` is the zero-dependency IRI/URI value-space crate for the native RDF
and SPARQL stack. It replaces the oxigraph-family `oxiri` dependency with
project-owned RFC 3987/3986 parsing, validation, reference resolution, syntax
normalization, and CURIE/prefix helpers.

## Source Map

| Module | Responsibility |
| --- | --- |
| `parse` | RFC 3987 IRI and RFC 3986 URI parsing with component spans. |
| `resolve` | Strict reference resolution. |
| `normalize` | Case, percent-encoding, and dot-segment syntax normalization. |
| `curie` | Prefix map, CURIE expansion, contraction, and resolution. |
| `error` | Typed parse/validation failures. |

## Checks

```bash
make rdf-core-hygiene
make rust-docs
```
