<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-foundation-corpus

`gmeow-foundation-corpus` imports a narrative JSONL corpus into GMEOW RDF
instance data, emits `foundation.ttl`, records the flat-vs-reified budget, and
writes the lossy projection set used by the foundation corpus lane.

## Source Map

| Module | Responsibility |
| --- | --- |
| `model` | Deserialized corpus record shape. |
| `importer` | Record-to-RDF import logic and GMEOW instance construction. |
| `budget` | Flat/reified/tag/projection accounting. |
| `graphview` | Read model used by projection writers. |
| `projections` | DraCor, Syuzhet, schema.org, TEI, Web Annotation, and training-manifest outputs. |
| `reconcile` | Optional N-Quads reconciliation report. |
| `py` | Optional PyO3 binding behind the `python` feature. |

## Checks

```bash
make native-py
make rust-docs
```
