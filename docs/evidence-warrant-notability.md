<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Evidence Warrant and Notability Orthogonality

## The two axes are intentionally separate

GMEOW models evidence on a claim through **two orthogonal axes** that travel with the citation / evidence-span link, not with the asserted fact itself:

1. **Evidential warrant** (`gmeow:hasEvidenceClass` → `gmeow:EvidenceClass`) answers: *How strong is the evidence for this fact?*
   Is it a verified legal filing, an independent trade-press article, a family narrative, or an unverified rumour?

2. **Source typing** (`gmeow:sourceIndependence`, `gmeow:sourceTier`, `gmeow:coverageDepth`, `gmeow:supportsNotability`) answers: *Does this source establish encyclopedic significance?*
   Is it independent of the subject? Is it secondary analysis rather than a primary filing? Does it provide significant coverage rather than a passing mention?

These axes are **orthogonal by design** (CONSTITUTION Principle 1: model what should have been written). A source can score highly on one axis and poorly on the other.

## Examples of the orthogonality

| Source | Evidential warrant | Notability support |
|--------|-------------------|-------------------|
| Independent newspaper profile | `evidenceIndependentTradePress` — high | `independent` + `secondary` + `significantCoverage` → `supportsNotability true` |
| Primary legal filing | `evidenceLegalFiling` — high | `selfOrIssuerOriginated` + `primary` + `routineFiling` → `supportsNotability false` |
| Self-published blog post | `evidenceSelfControlledSite` — low-to-medium | `selfOrIssuerOriginated` + `primary` + `significantCoverage` → `supportsNotability false` |
| Family anecdote | `evidenceFamilyNarrative` — low | `independent` + `secondary` + `passingMention` → `supportsNotability false` |
| Rumour on social media | `evidenceRUMOR` — very low | `independent` + `tertiary` + `passingMention` → `supportsNotability false` |

A **primary legal filing** is the canonical example of high warrant + zero notability: it verifies a fact with high authority (it was filed in court), but it does not establish that the subject is encyclopedically significant, because it is self-originated and primary.

## Consequences for projection tooling

- **Fact-checkers** care about **warrant**: they want to know if a claim is backed by verified evidence or only by rumour.
- **Notability-reviewers** care about the **WP:GNG triad** (independent + secondary + significant coverage): they want to know if the subject has been covered in depth by sources unconnected to it.
- **Projection layers** use these axes separately: a biography projection may suppress low-warrant claims entirely, while a Wikipedia-style notability projection may suppress claims whose `supportsNotability` is false regardless of warrant.

## Constitutional grounding

- **Principle 1** (SOTA by being SOTA): No existing vocabulary unifies these axes; GMEOW models them correctly rather than inheriting the weakness of partial solutions.
- **Principle 5** (Maximal bridging by reference): The axes are aligned to CRMinf, PROV-O, schema.org, C2PA, DataCite, nanopublications, and the WP:GNG convention — but the model is not reduced to any of them.
- **Principle 9** (Inclusive without overtyping; co-equal facets): `hasEvidenceClass` is non-functional — a claim may carry multiple evidence classes, and no single classification is privileged.
- **Principle 10** (Suppression, never erasure): A citation whose only evidence is self/private and whose source is self-originated triggers a SHACL **Warning**, prompting projection suppression rather than deletion.
- **Principle 12** (Compute outside the logic): The notability **eligibility decision** (e.g. "does this person meet Wikipedia's notability guideline?") is computed in the solver layer; the ontology delivers only the named vocabulary and constraints.
