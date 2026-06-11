<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# kernel

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/kernel` · **tier: core**

Foundational GMEOW categories grounded in gUFO, plus the annotation properties used across all modules.

*This is a STUB guide (#325 Tier-2): the slice is modelled, aligned, and
reasoned, but its narrative documentation has not been written yet. The
module-status matrix tracks the gap; term-level documentation (labels,
definitions) lives in `module.ttl` and renders via `gmeow describe`.*

## The domain-free epistemic axes

Four orthogonal, domain-free, non-functional facets may attach to any value,
entity, claim, or carrier. None subsumes another; none bridges to confidence
or standpoint modality (Principle 9). Together with those two statement-layer
properties they form the six-way matrix every projection consults:

| Axis | Property | Question it answers | Kind |
|---|---|---|---|
| Granularity | `gmeow:hasGranularity` | At what resolution is this stated? | resolution |
| Determinacy | `gmeow:hasDeterminacy` | How inherently defined is the value itself? | ontic |
| Sensitivity | `gmeow:hasSensitivity` | What disclosure risk does it carry? | privacy |
| **Aboutness** | `gmeow:hasAboutness` | Does the carrier *describe* its subject or *enact* it? | rhetorical |
| (Confidence) | `gmeow:confidence` | How sure is the asserter? | epistemic |
| (Standpoint modality) | `gmeow:standpointModality` | What belief value does the frame assign? | doxastic |

**Aboutness** (#349) is the mention/use distinction made first-class: a chunk
*defining* a trust framework describes trust; a covenant *demanding* trust
enacts it. Text about deception is not text that deceives. A carrier may
describe one subject while enacting another, and vantages may disagree via
the statement layer. Fiction is the licensed case where enactment co-occurs
with non-assertion — see the deception module's
`veridicalityLicensedFalsehood` (documented bridge; deliberately no axiom
coupling, so enactment never entails assertion).

External alignment is near-empty by survey, not by omission (search trail
from the parked `wip-aboutness-349` branch, whose mapping set lands with the
compiler-arc work): the one settled Wikidata anchor is **Q2577553**
(*use–mention distinction*, analytic philosophy) — a loose `relatedMatch` for
the class, since the QID names the distinction, not a mode vocabulary. IAO's
*is about* (IAO_0000136) is aboutness-as-reference (what a carrier is about),
not aboutness-as-mode (what it does with it) — refused. schema.org, PROV-O,
CIDOC-CRM, DOLCE+DnS, and Web Annotation carry no mention/use mode property;
the seed individuals have no settled QIDs and stay unaligned rather than
force weak matches (Principle 5).
