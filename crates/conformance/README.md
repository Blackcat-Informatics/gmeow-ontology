<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-conformance

`gmeow-conformance` is the native logic-conformance harness. It discovers cases
under `conformance/logic/cases/`, drives the Rust logic engine directly, and
compares the produced artifacts against committed goldens.

## Source Map

| Module | Responsibility |
| --- | --- |
| `discover` / `paths` | Locate case directories and repo-relative assets. |
| `profile` | Parse `profile.json` and enforce required case metadata, including the `shipped_rules` IRI list. |
| `run` | Execute the native compiler, certifier, materializer, query, and explanation cores; resolve a case's `shipped_rules` against `slices/grounding/logic/module.ttl`. |
| `serialize` | Write N-Quads and canonical JSON artifacts for comparison/reporting. |
| `compare` | Diff RDF by graph isomorphism, JSON canonically, and explanations by cited-IRI skeleton. |
| `external` / `divergence` | Ingest external corpora and emit divergence findings. |

## Checks

```bash
make conformance
make conformance-report
make rust-docs
```
