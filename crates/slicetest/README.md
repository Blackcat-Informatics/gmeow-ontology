<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-slicetest

`gmeow-slicetest` executes the slice-resident declarative test DSL under
`cargo-nextest`. Slice tests live as authored ontology data in
`slices/<group>/<name>/tests/*.ttl`.

## Source Map

| Module | Responsibility |
| --- | --- |
| `dsl` | Load a spec Turtle file and extract typed test cells. |
| `stores` | Build asserted and RDFS-closed ontology stores for competency questions. |
| `exec` | Execute competency, structural, and example-conformance cells. |
| `paths` | Resolve repo and manifest-relative paths. |

## Checks

```bash
make slicetest
make rust-docs
```
