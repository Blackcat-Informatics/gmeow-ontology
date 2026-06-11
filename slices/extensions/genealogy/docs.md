<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# genealogy

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/genealogy` · **tier: extension**

A comprehensive, evidence-centric genealogy model: reified, typed kinship relationships and kinship convenience properties. Life events themselves are modelled by the universal events module (gmeow:Event + gmeow:eventType value vocabulary + the gmeow:Participation relator); a person's life events are gmeow:LifeEvent occurrences carrying gmeow:eventTypeBirth / …Marriage / …NameChange etc. Structured naming terms (gmeow:PersonName et al.) live in the names module and link to the event that confers them via gmeow:conferredByEvent. Supersedes the unmaintained W3C SWAP gedcom vocabulary; aligned …

*This is a STUB guide (#325 Tier-2): the slice is modelled, aligned, and
reasoned, but its narrative documentation has not been written yet. The
module-status matrix tracks the gap; term-level documentation (labels,
definitions) lives in `module.ttl` and renders via `gmeow describe`.*
