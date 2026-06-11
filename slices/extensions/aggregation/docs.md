<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# aggregation

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/aggregation` · **tier: extension**

Spatial aggregation and statistical summarisation over places — count, sum, average, density, centroid, and binning. Realises the #42 Location-as-reference-frame epic and the #101 cross-cutting aggregation concern. Every aggregation is a gmeow:Measurement (the universal observation stack), so vantage, observedFeature, observationResult, confidence, granularity, and temporal scope are inherited without duplication (Principle 4). The aggregation region is the observedFeature; the result is a gmeow:ScalarQuantity. The actual arithmetic — counting, density computation, centroid calculation …

*This is a STUB guide (#325 Tier-2): the slice is modelled, aligned, and
reasoned, but its narrative documentation has not been written yet. The
module-status matrix tracks the gap; term-level documentation (labels,
definitions) lives in `module.ttl` and renders via `gmeow describe`.*
