<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-sparql-results

`gmeow-sparql-results` serializes and reads the SPARQL result model used by the
native query stack. It supports SPARQL Results JSON, XML, CSV, and TSV plus an
additive GMEOW provenance extension where the format can carry one.

## Source Map

| Module | Responsibility |
| --- | --- |
| `model` | Provenance structures carried with result documents. |
| `json` / `json_read` | SPARQL Results JSON writer and reader. |
| `xml` / `xml_read` | SPARQL Results XML writer and reader. |
| `csv` / `tsv` | Value-only CSV/TSV writers. |
| `graph` / `term` | CONSTRUCT graph and RDF term lexicalization helpers. |
| `error` | Format and serialization errors. |

## Checks

```bash
make rdf-core-hygiene
make rust-docs
```
