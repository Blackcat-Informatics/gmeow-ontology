<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-logic Source Map

This directory is the native runtime reasoning surface. It consumes compiler IR
from `gmeow-logic-compile`, runs rule/query/reasoning engines, and exposes the
PyO3 module registered by `gmeow-native`.

## Module Families

| Family | Modules | Role |
| --- | --- | --- |
| Engine and dispatch | `physical`, `rule_ir`, `materialize`, `dispatch` | Drive typed native rule and query execution. |
| Reasoning and verification | `reason`, `slme`, `verify`, `certificate`, `certify`, `profile_gate`, `dag_profile` | Entailment, consistency, module extraction, static profile checks, and coherence certificates. |
| Results and provenance | `result`, `result_rdf`, `derivation_graph`, `provenance`, `explain`, `seam` | Stable result contracts, RDF projections, derivation metadata, and Python seam shapes. |
| Logic features | `counterfactual`, `entrenchment`, `foundation`, `obligations`, `probabilistic`, `stablemodel`, `teleology`, `transaction`, `transition`, `versioning`, `wellfounded` | Domain-specific logic surfaces layered on the core runtime. |
| Support | `encode`, `store`, `dense`, `lower`, `query_ir`, `reference_resolver`, `relational_core`, `logic_diagnostics`, `py` | Data encoding, graph storage, compiler-runtime bridging, diagnostics, and bindings. |

## Boundaries

- Compiler-only data structures belong in `gmeow-logic-compile`; this crate is
  native-only and consumes the compiler's typed IR directly.
- Python-visible result shapes must stay byte-compatible with the conformance
  goldens and the documented seam contract.
- Runtime budget behavior and static certification must remain explicit; do not
  hide incomplete or engine-dependent behavior behind successful results.

## Checks

```bash
make rust-test
make conformance
make rust-docs
```
