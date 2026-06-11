<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# quality

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/quality` · **tier: core**

Cross-cutting data-quality layer refining confidence and provenance across every realm. Quality assessments are reified observations about ISO 19157 dimensions (positional accuracy, temporal accuracy, thematic accuracy, completeness, logical consistency, topological consistency) plus lineage. Aligns W3C DQV and GeoDCAT-AP by reference; quality-metric computation lives in the solver layer (Principle 12). QualityAssessment reuses the universal Observation stack: the assessed entity is the observedFeature, the quality result is the observationResult (typically a ScalarQuantity), and the …

*This is a STUB guide (#325 Tier-2): the slice is modelled, aligned, and
reasoned, but its narrative documentation has not been written yet. The
module-status matrix tracks the gap; term-level documentation (labels,
definitions) lives in `module.ttl` and renders via `gmeow describe`.*
