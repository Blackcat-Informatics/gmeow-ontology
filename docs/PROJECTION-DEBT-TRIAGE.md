<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Projection debt remediation inventory

This dated authoring decision covers every one of the 137 commitment records in
the [canonical registry](../slices/core/slice-quality-rubric/module.ttl) on
2026-10-20. It classifies remediation routes, not live measured residue. A ceiling
is a maximum budget, not proof that any of that debt still exists. Numerical
budgets are deliberately not copied into this inventory.

| Route | Disposition |
|---|---|
| G | Ground the owning slice's assertions or constraints in canonical logic:, then derive the external view. Keep domain terms in place. SHACL retirement additionally requires demonstrated equivalence. |
| O | Ground domain assertions in place and reconcile their external-model boundary with the named grounding owner. Reuse its canonical terms; consolidate or rehome external grounding records there. Do not move domain concepts wholesale. A cell can require both grounding and owner reconciliation. |
| S | The committed budget is already zero. Preserve that seal; it is not a permanent exception and is not a fresh measurement of the source. |

No cell is classified as permanently legitimate ungrounded authoring. Legitimate
validated owner correspondences and supported by-reference links are already
excluded by the counter; they need no standing debt allowance. A positive budget
may now contain unused headroom: measure it before lowering, and never equate the
commitment with its current residue. New commitments need their own classification.

The O route names the semantic ownership boundary, not an automatic relocation.
If a suitable owner correspondence already exists, consume it and replace the
inferior local encoding under equivalence proof. When records genuinely move,
retain their identities, demonstrate source departure and destination arrival,
fund the adjustment with the matching source lowering, and use the existing
CeilingRelocation contract. Never raise an unfunded ceiling or net an axis floor.

For example, domain class stereotypes can become canonical logic: stereotypes
without moving the domain class. A mathematical slice's OBI grounding belongs
to logic:, while quantity-interchange grounding belongs to math: and lexical or
annotation-model grounding belongs to lang:. The owner column below comes from
the registry, rather than inferring ownership from namespace spelling.

The [counting and measurement contract](PROJECTION-VOCABULARY-RATCHET.md) governs
the numeric report. Use the live developer sweep for a whole repository or
`gmeow slice projection-debt <slice-dir>` for explicitly supplied authored slice
sources. Both use the same native counter; this inventory authorizes no deletion.

| Commitment | Route | Grounding owner |
|---|---|---|
| `gmeow:pcc-accessibility-rdfs` | G | `logic:` |
| `gmeow:pcc-accounts-rdfs` | S | `logic:` |
| `gmeow:pcc-affect-mfoem` | O | `logic:` |
| `gmeow:pcc-affect-oa` | O | `lang:` |
| `gmeow:pcc-affect-rdfs` | G | `logic:` |
| `gmeow:pcc-agentic-rdfs` | G | `logic:` |
| `gmeow:pcc-agentic-sh` | G | `logic:` |
| `gmeow:pcc-aggregation-qb` | O | `math:` |
| `gmeow:pcc-aggregation-rdfs` | G | `logic:` |
| `gmeow:pcc-agreements-rdfs` | G | `logic:` |
| `gmeow:pcc-ai-oa` | O | `lang:` |
| `gmeow:pcc-ai-rdfs` | G | `logic:` |
| `gmeow:pcc-ai-sh` | G | `logic:` |
| `gmeow:pcc-archaeological-evidence-oa` | O | `lang:` |
| `gmeow:pcc-archaeological-evidence-rdfs` | G | `logic:` |
| `gmeow:pcc-attestation-rdfs` | G | `logic:` |
| `gmeow:pcc-awareness-rdfs` | G | `logic:` |
| `gmeow:pcc-calendar-rdfs` | G | `logic:` |
| `gmeow:pcc-citations-oa` | O | `lang:` |
| `gmeow:pcc-citations-rdfs` | G | `logic:` |
| `gmeow:pcc-cognition-rdfs` | G | `logic:` |
| `gmeow:pcc-cognition-sumo` | O | `logic:` |
| `gmeow:pcc-concepts-ontolex` | O | `lang:` |
| `gmeow:pcc-concepts-rdfs` | G | `logic:` |
| `gmeow:pcc-concepts-sh` | G | `logic:` |
| `gmeow:pcc-connectivity-rdfs` | G | `logic:` |
| `gmeow:pcc-contacts-rdfs` | G | `logic:` |
| `gmeow:pcc-creative-works-rdfs` | G | `logic:` |
| `gmeow:pcc-deception-rdfs` | G | `logic:` |
| `gmeow:pcc-diagnostics-rdfs` | G | `logic:` |
| `gmeow:pcc-diagnostics-sh` | G | `logic:` |
| `gmeow:pcc-documentation-rdfs` | G | `logic:` |
| `gmeow:pcc-documentation-sh` | G | `logic:` |
| `gmeow:pcc-documents-rdfs` | G | `logic:` |
| `gmeow:pcc-dreaming-rdfs` | G | `logic:` |
| `gmeow:pcc-email-rdfs` | G | `logic:` |
| `gmeow:pcc-employment-rdfs` | G | `logic:` |
| `gmeow:pcc-entities-rdfs` | G | `logic:` |
| `gmeow:pcc-epistemics-iao` | O | `logic:` |
| `gmeow:pcc-epistemics-rdfs` | G | `logic:` |
| `gmeow:pcc-epistemics-sh` | G | `logic:` |
| `gmeow:pcc-epistemics-sumo` | O | `logic:` |
| `gmeow:pcc-events-rdfs` | G | `logic:` |
| `gmeow:pcc-evidence-rdfs` | G | `logic:` |
| `gmeow:pcc-expertise-rdfs` | G | `logic:` |
| `gmeow:pcc-finance-rdfs` | G | `logic:` |
| `gmeow:pcc-gender-rdfs` | G | `logic:` |
| `gmeow:pcc-genealogy-rdfs` | G | `logic:` |
| `gmeow:pcc-graphrag-rdfs` | G | `logic:` |
| `gmeow:pcc-graphrag-sh` | G | `logic:` |
| `gmeow:pcc-gts-rdfs` | G | `logic:` |
| `gmeow:pcc-gts-sh` | G | `logic:` |
| `gmeow:pcc-guides-rdfs` | G | `logic:` |
| `gmeow:pcc-images-oa` | O | `lang:` |
| `gmeow:pcc-images-rdfs` | G | `logic:` |
| `gmeow:pcc-imagination-rdfs` | G | `logic:` |
| `gmeow:pcc-inference-rdfs` | G | `logic:` |
| `gmeow:pcc-inhabitation-rdfs` | G | `logic:` |
| `gmeow:pcc-inhabitation-sh` | G | `logic:` |
| `gmeow:pcc-inquiry-rdfs` | G | `logic:` |
| `gmeow:pcc-kernel-bfo` | O | `logic:` |
| `gmeow:pcc-kernel-gufo` | O | `logic:` |
| `gmeow:pcc-kernel-rdfs` | G | `logic:` |
| `gmeow:pcc-kernel-sh` | G | `logic:` |
| `gmeow:pcc-lang-lexvo` | G | `lang:` |
| `gmeow:pcc-lang-rdfs` | G | `logic:` |
| `gmeow:pcc-lang-sh` | G | `logic:` |
| `gmeow:pcc-language-rdfs` | G | `logic:` |
| `gmeow:pcc-languages-lexvo` | O | `lang:` |
| `gmeow:pcc-languages-rdfs` | G | `logic:` |
| `gmeow:pcc-learning-rdfs` | G | `logic:` |
| `gmeow:pcc-learning-sh` | G | `logic:` |
| `gmeow:pcc-lexicon-ontolex` | O | `lang:` |
| `gmeow:pcc-lexicon-rdfs` | G | `logic:` |
| `gmeow:pcc-lifecycle-rdfs` | G | `logic:` |
| `gmeow:pcc-logic-rdfs` | G | `logic:` |
| `gmeow:pcc-mappings-fno` | O | `logic:` |
| `gmeow:pcc-mappings-oa` | O | `lang:` |
| `gmeow:pcc-mappings-ontolex` | O | `lang:` |
| `gmeow:pcc-mappings-qb` | O | `math:` |
| `gmeow:pcc-math-obi` | O | `logic:` |
| `gmeow:pcc-math-rdfs` | G | `logic:` |
| `gmeow:pcc-mentation-bfo` | O | `logic:` |
| `gmeow:pcc-mentation-rdfs` | G | `logic:` |
| `gmeow:pcc-metacognition-rdfs` | G | `logic:` |
| `gmeow:pcc-model-serving-rdfs` | G | `logic:` |
| `gmeow:pcc-model-serving-sh` | G | `logic:` |
| `gmeow:pcc-music-fno` | O | `logic:` |
| `gmeow:pcc-music-rdfs` | G | `logic:` |
| `gmeow:pcc-music-sh` | G | `logic:` |
| `gmeow:pcc-names-ontolex` | O | `lang:` |
| `gmeow:pcc-names-rdfs` | G | `logic:` |
| `gmeow:pcc-narrative-rdfs` | G | `logic:` |
| `gmeow:pcc-norms-rdfs` | G | `logic:` |
| `gmeow:pcc-notation-fno` | O | `logic:` |
| `gmeow:pcc-notation-rdfs` | G | `logic:` |
| `gmeow:pcc-notation-sh` | G | `logic:` |
| `gmeow:pcc-notes-oa` | O | `lang:` |
| `gmeow:pcc-notes-rdfs` | G | `logic:` |
| `gmeow:pcc-observations-oa` | O | `lang:` |
| `gmeow:pcc-observations-obi` | S | `logic:` |
| `gmeow:pcc-observations-pato` | S | `logic:` |
| `gmeow:pcc-observations-qb` | O | `math:` |
| `gmeow:pcc-observations-qudt` | O | `math:` |
| `gmeow:pcc-observations-rdfs` | G | `logic:` |
| `gmeow:pcc-organization-rdfs` | G | `logic:` |
| `gmeow:pcc-places-bfo` | O | `logic:` |
| `gmeow:pcc-places-qudt` | O | `math:` |
| `gmeow:pcc-places-rdfs` | G | `logic:` |
| `gmeow:pcc-places-so` | O | `logic:` |
| `gmeow:pcc-profiles-rdfs` | G | `logic:` |
| `gmeow:pcc-provenance-rdfs` | G | `logic:` |
| `gmeow:pcc-quality-oa` | O | `lang:` |
| `gmeow:pcc-quality-rdfs` | G | `logic:` |
| `gmeow:pcc-rights-rdfs` | G | `logic:` |
| `gmeow:pcc-rights-sh` | G | `logic:` |
| `gmeow:pcc-risk-rdfs` | G | `logic:` |
| `gmeow:pcc-sensory-environment-mfoem` | O | `logic:` |
| `gmeow:pcc-sensory-environment-rdfs` | G | `logic:` |
| `gmeow:pcc-sensory-rdfs` | G | `logic:` |
| `gmeow:pcc-sexuality-rdfs` | G | `logic:` |
| `gmeow:pcc-slice-quality-rubric-rdfs` | G | `logic:` |
| `gmeow:pcc-software-rdfs` | G | `logic:` |
| `gmeow:pcc-standpoint-dul` | O | `logic:` |
| `gmeow:pcc-standpoint-fno` | O | `logic:` |
| `gmeow:pcc-standpoint-iao` | O | `logic:` |
| `gmeow:pcc-standpoint-oa` | O | `lang:` |
| `gmeow:pcc-standpoint-rdfs` | G | `logic:` |
| `gmeow:pcc-standpoint-sh` | G | `logic:` |
| `gmeow:pcc-tags-oa` | O | `lang:` |
| `gmeow:pcc-tags-rdfs` | G | `logic:` |
| `gmeow:pcc-teleology-rdfs` | G | `logic:` |
| `gmeow:pcc-temporal-qudt` | O | `math:` |
| `gmeow:pcc-temporal-rdfs` | G | `logic:` |
| `gmeow:pcc-temporal-sh` | G | `logic:` |
| `gmeow:pcc-trust-rdfs` | G | `logic:` |
| `gmeow:pcc-versions-rdfs` | G | `logic:` |
