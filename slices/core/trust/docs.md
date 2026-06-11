<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# trust

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/trust` · **tier: core**

A cross-cutting trust facility: cryptographic keys, certifications (key↔identity attestations) and perspectival owner-trust — the Web-of-Trust superset layer (OpenPGP RFC 4880/9580, X.509, SSH, Nostr; aligned to the WOT schema). Reused wherever identity must be vouched for: contacts, accounts, and message signatures (the messaging-trust module builds on this). Trust here is asserted and perspectival; trust METRICS (transitive validity propagation) are deliberately left outside the logical core — represent inputs and outputs, never compute them in OWL. STANDPOINT DOCTRINE (#51). The trust …

*This is a STUB guide (#325 Tier-2): the slice is modelled, aligned, and
reasoned, but its narrative documentation has not been written yet. The
module-status matrix tracks the gap; term-level documentation (labels,
definitions) lives in `module.ttl` and renders via `gmeow describe`.*
