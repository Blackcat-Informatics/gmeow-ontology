<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# language

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/language` · **tier: core**

The CORE half of the languages split (#287 dependency surgery): gmeow:Language and gmeow:WritingSystem as first-class, registry-independent classes; gmeow:languageTag / gmeow:bcp47Tag / gmeow:languageCode (the x-gmeow-\* private-use anchor machinery); and the three seed individuals — English, Mandarin, French — that back the framework's own language-tagged literals (Canadian + US + European + Chinese coverage). The rich sociolinguistic machinery (proficiency, writing-system usage, varieties, diachronic states, version lineage, conlangs) lives in the languages extension slice, which extends …

*This is a STUB guide (#325 Tier-2): the slice is modelled, aligned, and
reasoned, but its narrative documentation has not been written yet. The
module-status matrix tracks the gap; term-level documentation (labels,
definitions) lives in `module.ttl` and renders via `gmeow describe`.*
