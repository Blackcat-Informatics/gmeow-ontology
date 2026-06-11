<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# images

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/images` · **tier: extension**

The Images super-ontology — a layered model for contextual depiction, region encoding, scene graphs, and technical metadata (issue #22). Builds on the WEMI spine (#208): a visual work is a gmeow:Work with workTypeVisual / workTypePhotographic; the digital file is a gmeow:Manifestation (gmeow:MediaObject) plus gmeow:Item / hasCarrier. Colourspace is carried via gmeow:hasReferenceFrame (#70). Rights reuse the #21 facility directly. LAYERS. 1. Contextual depiction — the DepictionUsage relator (mirrors NameUsage) and the flat gmeow:depicts shortcut (⊑ gmeow:isAbout). 2. Region encoding — …

*This is a STUB guide (#325 Tier-2): the slice is modelled, aligned, and
reasoned, but its narrative documentation has not been written yet. The
module-status matrix tracks the gap; term-level documentation (labels,
definitions) lives in `module.ttl` and renders via `gmeow describe`.*
