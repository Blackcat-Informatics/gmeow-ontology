<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# accessibility

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/accessibility` · **tier: extension**

Cross-cutting accessibility layer — whether an entity can reach or use a location under constraints (wheelchair/step-free, sensory, clearance, life-support). Accessibility features, barriers, and needs are modelled as orthogonal, co-equal value facets (Principle 9). The flat shortcuts (hasAccessibilityFeature, hasBarrier, hasAccessibilityNeed) cover the 80 % case; promote to AccessibilityAssertion when provenance, confidence, temporal scope, or suppression matter (Principle 10). Accessible routes are computed by the solver layer, not asserted in OWL (Principle 12). Part of the #42 …

*This is a STUB guide (#325 Tier-2): the slice is modelled, aligned, and
reasoned, but its narrative documentation has not been written yet. The
module-status matrix tracks the gap; term-level documentation (labels,
definitions) lives in `module.ttl` and renders via `gmeow describe`.*
