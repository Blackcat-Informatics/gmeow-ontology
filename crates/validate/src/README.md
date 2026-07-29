<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-validate Source Map

This directory hosts Rust validation-path lints. Most modules are PyO3-free
engine code; `py.rs` and `py_dsl.rs` are the binding layer used by the unified
native extension.

## Module Families

| Family | Modules | Role |
| --- | --- | --- |
| Orchestration and cache | `validate_all`, `cache`, `model`, `store` | Shared validation inputs, content-addressed cache keys, native Turtle/GTS loading, and result models. |
| Ontology gates | `constitution`, `coverage`, `crate_layering`, `gufo`, `slice_ownership`, `statement`, `signature` | Repository-level hard gates and ontology-specific structural checks. |
| DSL and mappings | `dsl`, `dsl_shacl`, `mapping_eval`, `data_validate` | Mapping/statement DSL checks and data validation helpers. |
| Reports and diagnostics | `advisory`, `findings`, `lint`, `repo_static`, `crossref`, `language_tags`, `instance` | Finding construction, static repository scans, and specialized lint surfaces. |
| Bindings | `py`, `py_dsl` | Python module registration and conversion from Rust findings to Python-facing results. |

## Boundaries

- Keep engine modules PyO3-free unless the module is explicitly a binding
  surface.
- Use native RDF/GTS loaders in `store` rather than one-off parser code.
- A validation rule that enforces generated-output drift should check canonical
  sources or generator output, not normalize committed generated files by hand.

## Checks

```bash
make validate
make crate-check
make rust-docs
```
