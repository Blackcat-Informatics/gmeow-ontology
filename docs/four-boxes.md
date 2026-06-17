<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# ABox, TBox, RBox, CBox in GMEOW

GMEOW uses the four-box vocabulary as an explanatory and tooling layer over its
existing source/projection doctrine. It does not replace the Constitution:
canonical sources still live in the authored ontology, mapping DSL, statement
DSL, shapes, and slice guides; generated artifacts remain projections.

The roles are graph annotations, not hard partitions. A term, shape, module, or
graph artifact may carry more than one `gmeow:graphBoxRole`, and consumers must
not assume exactly one role.

| Role | GMEOW meaning | Common artifacts | Validation/report use |
| --- | --- | --- | --- |
| `gmeow:boxABox` | Asserted instance/data graph | examples, imported data, user payloads, GTS payload graphs | focus/value nodes and data-shape failures |
| `gmeow:boxTBox` | Terminology, schema, shapes, restrictions, profiles, and logic declarations | slices, ontology root, SHACL shapes, generated docs, profile declarations | source shape/schema context |
| `gmeow:boxRBox` | Property and role behavior | object/datatype/annotation properties, inverses, transitive spines, property chains, path constraints | predicate/path and role-axiom context |
| `gmeow:boxCBox` | Contextual metadata about assertions | RDF 1.2 reifiers, statement DSL output, provenance, evidence, confidence, time, standpoint, determinacy, validity, disclosure context | `sh:reifierShape` and contextual assertion diagnostics |

## CBox Scope

In GMEOW, CBox means contextual metadata about an assertion. The core examples
are RDF 1.2 reifiers and statement annotations: who asserted a triple, from
which evidence, with what confidence, at what time, under which standpoint, and
with which disclosure or validity context.

Configuration metadata is related but distinct. Runtime package profiles, solver
profiles, GTS transport profiles, and reference-frame profiles should stay in
the existing Profile and GTS machinery unless they are specifically metadata
about an assertion.

## Validation

The Rust `gmeow_shacl` validator uses these roles only as diagnostics. Existing
result keys remain stable. When shapes or paths carry `gmeow:graphBoxRole`,
Python-facing result dictionaries may also include optional role arrays, and
human-facing validation output may prefix messages with `[CBox]`, `[RBox]`, and
similar labels.

`sh:reifierShape` support is deliberately scoped to the SHACL 1.2 Core Working
Draft dated 2026-06-02. GMEOW implements the draft subset it tests; it does not
claim full SHACL 1.2 conformance from this feature alone.

## Sources

- Kurt Cagle, "A-Box, T-Box, R-Box, C-Box", 2026-03-05.
- W3C, "SHACL 1.2 Core", Working Draft, 2026-06-02.
