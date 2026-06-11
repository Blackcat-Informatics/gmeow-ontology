<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# tags

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/tags` · **tier: core**

Universal tagging building block — open folksonomy, aboutness, and typing as three distinct axes (Principle 9). A gmeow:Tag is an information-object value (like a skos:Concept); a gmeow:TagScheme is its namespace bucket; and gmeow:Tagging is a gufo:Relator that reifies the act of tagging with provenance, confidence, temporal scope and suppression (Principle 10). The flat gmeow:hasTag shortcut covers the 80% case; promote to Tagging when the act itself must be a node. Tags align (lossily) to SKOS, schema.org, W3C Web Annotation and MOAT.

*This is a STUB guide (#325 Tier-2): the slice is modelled, aligned, and
reasoned, but its narrative documentation has not been written yet. The
module-status matrix tracks the gap; term-level documentation (labels,
definitions) lives in `module.ttl` and renders via `gmeow describe`.*
