<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Vendored W3C SPARQL 1.1 conformance fixtures

This tree vendors a **curated subset** of the official W3C SPARQL 1.1 test suite,
exercising the exotic-aggregation and deep-subquery surface delivered by S6b
(#928). It is consumed by the native conformance harness
(`crates/sparql-conformance`).

## Source

- Upstream: **W3C `rdf-tests`** — <https://github.com/w3c/rdf-tests>,
  path `sparql/sparql11/`.
- Mirror of the W3C DAWG/SPARQL-WG test suite at
  <https://www.w3.org/2009/sparql/docs/tests/>.
- Fetched from the `main` branch on **2026-06-26**.

## License

The W3C test files are published under the **W3C Test Suite License** / **W3C
Software and Document License** — see
<https://www.w3.org/Consortium/Legal/2015/copyright-software-and-document>.
They are vendored verbatim (query + data) and are **not** relicensed; each carries
a `.license` SPDX sidecar (`SPDX-License-Identifier: LicenseRef-W3C-Test-Suite`).
The selector `manifest.ttl` files and this document are GMEOW-authored
(AGPL-3.0-only).

## Vendored files & fidelity

| Group | Query / Data | Fidelity |
|-------|--------------|----------|
| aggregates | `agg-numeric.ttl`, `agg-group-builtin.rq`, `agg-sum-01.rq`, `agg-multiple-having.rq` | **verbatim** from `sparql/sparql11/aggregates/` |
| subquery | `sq13.rq`, `sq13.ttl` | **verbatim** from `sparql/sparql11/subquery/` |

The expected-result files (`*.srx`) are **reconstructed to a semantically
equivalent** SPARQL Results XML document: the harness compares SELECT results as a
W3C *solution-set multiset* (via the native `from_xml` reader), so the exact bytes
of the upstream `.srx` are immaterial — only the solution content is, and that is
reproduced faithfully from the upstream expected results.

## Curation rationale

- `agg-group-builtin` — `GROUP BY (DATATYPE(?o) AS ?d)` directly exercises the
  expression-valued `GROUP BY` added in #928 Task 3.
- `agg-multiple-having` — `HAVING (COUNT(*) > 1) (COUNT(*) < 3)` exercises
  multi-condition `HAVING`.
- `agg-sum-01` — `SUM` over the XSD decimal value space.
- `subquery13` ("Subqueries don't inject bindings") — a nested `SELECT` whose
  inner variable scope is independent of the outer query; it also exercises
  blank-node property lists (`[ rdfs:label ?L ]`).

## Not vendored: the W3C federated `service` group

The W3C `sparql11/service` tests require **live HTTP SPARQL endpoints** and cannot
run offline. Federated `SERVICE` is instead covered here by:

- the deterministic in-memory `SERVICE` case in `suite/gmeow-smoke` (the
  `LocalRemoteQuerySource` dog-foods the native engine), and
- the maintainer network-lane live test (`crates/sparql-eval/tests/service_live.rs`,
  #928 Task 8), which drives the real `HttpRemoteQuerySource`.
