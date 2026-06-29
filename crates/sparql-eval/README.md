<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-sparql-eval

`gmeow-sparql-eval` is the native RDF 1.2 SPARQL evaluator. It consumes
`gmeow-sparql-algebra`, evaluates over `gmeow-rdf-core`'s `DatasetView`, and
keeps the hot path in interned `TermId` space.

## Source Map

| Module | Responsibility |
| --- | --- |
| `engine` / `eval` | Public engine, prepared-query, and evaluation entry points. |
| `bgp`, `path`, `modifier`, `construct`, `template` | Graph-pattern, property-path, solution-modifier, and CONSTRUCT execution. |
| `expr`, `binop`, `list_fn` | FILTER/BIND expression evaluation and built-ins. |
| `scratch` / `solution` | Scratch interner, solution terms, bags, and variable schemas. |
| `remote` / `remote_http` | SERVICE source abstraction and native HTTP transport. |
| `update` | Graph update execution surface. |

## Checks

```bash
make rdf-core-hygiene
make rust-test
make rust-docs
```
