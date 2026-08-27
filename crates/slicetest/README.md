<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-slicetest

`gmeow-slicetest` executes the slice-resident declarative test DSL once at the
explicit pre-test producer boundary. Slice tests live as authored ontology data in
`slices/<group>/<name>/tests/*.ttl`; every spec is an independently cache-keyed DAG
node and the complete verdict authenticates their receipt identities. Competency
misses share one isolated repository store, while structural and conformance workers
release their memory on exit. Nextest runs only focused synthetic engine checks and
cannot discover or rebuild the repository corpus.

## Source Map

| Module | Responsibility |
| --- | --- |
| `dsl` | Load a spec Turtle file and extract typed test cells. |
| `stores` | Build asserted and RDFS-closed ontology stores for competency questions. |
| `exec` | Execute competency, structural, and example-conformance cells. |
| `paths` | Resolve repo and manifest-relative paths. |
| `repository` | Discover exact inputs, execute only missing per-spec actions in isolated workers, bind their receipts, and authenticate the aggregate read-only. |

## Checks

```bash
make produce-test-fixtures
make verify-test-fixtures
make slicetest
make rust-docs
```
