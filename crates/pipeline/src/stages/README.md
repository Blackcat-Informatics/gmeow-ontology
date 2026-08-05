<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Pipeline Stages

This directory holds the concrete production stages for the dogfooded
`gmeow-pipeline` DAG. Each stage implements `crate::node::Stage` and is
registered from `mod.rs` under the `gmeow:stageImpl` key read from the ontology.

## Stage Families

| Family | Modules | Role |
| --- | --- | --- |
| Ingestion and carrier | `source_load`, `carrier`, `fold_arena`, `gts_compose`, `gts_sink` | Assemble authored inputs into the native RDF carrier and terminal GTS bundle. |
| Canonical compilers | `statements`, `mappings`, `compile_logic`, `correspondence_lower`, `reason`, `validate`, `conformance` | Compile authored source layers and prove drift/conformance invariants. |
| Documentation and reports | `docs_render`, `diag_render`, `matrix`, `metadata`, `provenance_graph`, `references`, `bench`, `evals` | Emit committed documentation, status matrices, diagnostics, metadata, and evidence graphs. |
| Export leaves | `apache`, `catalog`, `profiles`, `frame_shapes`, `result_shapes`, `json_schema`, `lpg`, `schemas`, `research_objects`, `okf`, `export`, `yaml_ld` | Project the terminal carrier into release and consumer-facing formats. |

## Stage Contract

- A stage consumes declared `StageProduct` inputs and emits named artifacts or a
  replacement carrier bundle.
- A stage should not re-read `generated/dist/gmeow.gts` when the needed data is
  already present in the in-memory carrier.
- Generated outputs must be reproducible through `make check`; never patch
  generated files by hand to satisfy a stage change.
- Source/output ownership belongs in the dogfooded DAG plus `register_default`;
  adding an unregistered stage is dead code.

## Checks

```bash
make check-sync
make carrier-purity
make crate-check
```
