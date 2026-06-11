<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# deception

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/deception` · **tier: core**

Cross-cutting deception, falsehood, and misinformation facility. Builds on the standpoint module (#43) and the event module (#41) to model deceptive acts as standpoint-indexed divergences between what a party holds true and what they project. Part of the Deception EPIC (#212). DOCTRINE. \* No isFalse / isDeceptive axiom and no boolean truth datatype property (Principles 1, 12). A falsehood is a frame-relative gmeow:StandpointClaim whose claimModality = gmeow:refuted (settled-false per a designated reference frame). Deceptiveness is asserted as a standpoint-indexed, attributed …

*This is a STUB guide (#325 Tier-2): the slice is modelled, aligned, and
reasoned, but its narrative documentation has not been written yet. The
module-status matrix tracks the gap; term-level documentation (labels,
definitions) lives in `module.ttl` and renders via `gmeow describe`.*

## Bridge: aboutness (kernel, #349)

`gmeow:veridicalityLicensedFalsehood` (fiction, satire, sarcasm) is the
special case where the kernel's aboutness axis meets veridicality: a fictional
carrier *enacts* its content (`gmeow:hasAboutness gmeow:aboutnessEnacts`)
while asserting nothing — enactment without assertion is licensed, not
deceptive. The bridge is documentation only, deliberately: no axiom couples
`hasAboutness` to veridicality or standpoint modality, so enactment never
entails assertion (and text *about* deception is never inferred to deceive).
