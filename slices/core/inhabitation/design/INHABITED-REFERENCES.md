<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — References and Citations

> The **appendix.** The sources this set subsumes, the ontological and formal-methods literature it
> builds on, and the scholarship grounding each borrowed structural distinction — staged for the
> `metadata/references.ttl` ledger. Per Principle 5, every entry is covered **by reference**: cited,
> aligned, never copied in as axioms. For the contemplative and esoteric traditions, the citations are
> to **scholarly studies** of those traditions, included as the source of a *structural distinction* —
> **not** as an endorsement of any metaphysical claim (the neutrality gate,
> [`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md)).

## How to read these citations

Three rules govern this appendix:

1. **By reference, never by import (Principle 5).** A citation asserts an alignment or an intellectual
   debt; it copies no axioms. External vocabularies are bridged with SSSOM / `skos:*Match`; external
   theory is acknowledged.
2. **Traditions are cited for structure, not truth.** An entry under "contemplative and esoteric
   scholarship" records *which distinction GMEOW borrowed* and *the academic study it came from*. GMEOW
   asserts none of the metaphysics; a possession or incarnation is modeled as a frame-indexed
   `InhabitationClaim`, never an asserted fact.
3. **Internal sources are dated, attributed records.** The three originating sources (a public post, an
   email thread, an analysis) are cited with provenance, not as peer-reviewed literature.

## The originating sources

| Key | Source | Provenance |
|---|---|---|
| `cagle-inhabited` | Cagle, K. (2026). *A Vocabulary for Inhabited Systems.* The Inference Engineer (Substack). | Public post; accessed 2026-06. Contributes the Actor / Avatar / Persona / Agent / Role / Collective distinction and the holon / portal / scene-graph framing. |
| `org-modeling-thread` | Hunter, S.; Beale, T. (Ars Semantica); Taylor, P.; et al. (2026). *Organizational modelling — the overloading of "Role"* (email correspondence, June 2026). | Internal correspondence, cited with the participants' attribution and date; not a public citation. Contributes the Role / Post / Function / Accountability diagnosis and the "Organization as a Party derivative" pattern. |
| `inhabited-verdict` | *Inhabited-systems analysis verdict* (2026). | Internal analysis, dated and attributed. Contributes the six-category conflation diagnosis and the competency questions that became the conformance corpus. |

## Ontological and formal-methods foundations

The literature behind the `logic:` stereotypes, the situation/relator/role distinctions, and the
reasoning discipline this set relies on.

| Key | Citation |
|---|---|
| `ufo-guizzardi-2005` | Guizzardi, G. (2005). *Ontological Foundations for Structural Conceptual Models.* PhD thesis, University of Twente. Telematica Instituut Fundamental Research Series. |
| `ufo-2022` | Guizzardi, G., Botti Benevides, A., Fonseca, C. M., Porello, D., Almeida, J. P. A., & Prince Sales, T. (2022). UFO: Unified Foundational Ontology. *Applied Ontology*, 17(1), 167–210. DOI: 10.3233/AO-210256. |
| `gufo-2019` | Almeida, J. P. A., Falbo, R. A., & Guizzardi, G. (2019). *gUFO: A Lightweight Implementation of the Unified Foundational Ontology (UFO).* NEMO, Federal University of Espírito Santo. |
| `ontoclean-2009` | Guarino, N., & Welty, C. A. (2009). An Overview of OntoClean. In *Handbook on Ontologies* (2nd ed., pp. 201–220). Springer. DOI: 10.1007/978-3-540-92673-3_9. |
| `social-roles-2004` | Masolo, C., Vieu, L., Bottazzi, E., Catenacci, C., Ferrario, R., Gangemi, A., & Guarino, N. (2004). Social Roles and their Descriptions. In *Proc. KR 2004* (pp. 267–277). AAAI Press. |
| `dolce-2003` | Masolo, C., Borgo, S., Gangemi, A., Guarino, N., & Oltramari, A. (2003). *WonderWeb Deliverable D18: Ontology Library (DOLCE).* ISTC-CNR. |
| `dns-2008` | Gangemi, A., & Mika, P. (2003). Understanding the Semantic Web through Descriptions and Situations. In *OTM 2003*, LNCS 2888 (pp. 689–706). Springer. DOI: 10.1007/978-3-540-39964-3_44. |
| `mereology-simons-1987` | Simons, P. (1987). *Parts: A Study in Ontology.* Oxford University Press. |
| `mereology-sep` | Varzi, A. (2019). Mereology. *The Stanford Encyclopedia of Philosophy* (Spring 2019), E. N. Zalta (ed.). |
| `holon-koestler-1967` | Koestler, A. (1967). *The Ghost in the Machine.* Hutchinson. (Origin of "holon" and "holarchy".) |
| `standpoint-logic-2021` | Gómez Álvarez, L., & Rudolph, S. (2021). Standpoint Logic: Multi-Perspective Knowledge Representation. In *Proc. FOIS 2021*, IOS Press. DOI: 10.3233/FAIA210367. |
| `transaction-logic-1993` | Bonner, A. J., & Kifer, M. (1993). Transaction Logic Programming. In *Proc. ICLP 1993* (pp. 257–279). MIT Press. |
| `prov-o-2013` | Lebo, T., Sahoo, S., & McGuinness, D. (eds.) (2013). *PROV-O: The PROV Ontology.* W3C Recommendation, 30 April 2013. |

## Identity, persistence, and personal-identity philosophy

The continuity-as-contested-claim model (`SubjectStage` / `SubjectLineage` /
`IdentityContinuityAssessment`) draws on the personal-identity literature, where continuity-over-change
is exactly the contested question.

| Key | Citation |
|---|---|
| `parfit-1984` | Parfit, D. (1984). *Reasons and Persons.* Oxford University Press. (Identity as a matter of degree and relation, not an all-or-nothing further fact.) |
| `personal-identity-sep` | Olson, E. T. (2023). Personal Identity. *The Stanford Encyclopedia of Philosophy*, E. N. Zalta & U. Nodelman (eds.). |
| `coreference-no-sameas` | (Internal) the GMEOW coreference doctrine — `gmeow:counterpartOf` over `owl:sameAs`; see `slices/core/coreference/` and Principle 5. |

## Modeling lineages aligned by reference

| Key | Citation |
|---|---|
| `frbr-1998` | IFLA Study Group on the FRBR (1998). *Functional Requirements for Bibliographic Records.* K. G. Saur. (The Work / Expression / Manifestation / Item spine.) |
| `ifla-lrm-2017` | Riva, P., Le Bœuf, P., & Žumer, M. (2017). *IFLA Library Reference Model (LRM).* IFLA. (The consolidated FRBR/WEMI model.) |
| `w3c-org-2014` | Reynolds, D. (ed.) (2014). *The Organization Ontology.* W3C Recommendation, 16 January 2014. (`org:Role` / `org:Post` / `org:Membership`, aligning the email thread's distinctions.) |
| `foaf-2014` | Brickley, D., & Miller, L. (2014). *FOAF Vocabulary Specification 0.99.* (Agent / Person alignment.) |
| `openehr-archetypes` | Beale, T. (2002). Archetypes: Constraint-based Domain Models for Future-proof Information Systems. In *OOPSLA 2002 Workshop on Behavioural Semantics.* (The Ars Semantica lineage in the email thread; role-vs-post-vs-function discipline.) |
| `dul` | Gangemi, A., et al. (2010). *DOLCE+DnS Ultralite (DUL).* (Role / Description / Situation pattern mirrored by `Inhabitant` / `InhabitedSystem` in an `Inhabitation` situation.) |

## Contemplative and esoteric scholarship (structure borrowed; metaphysics not inherited)

Each entry pairs a **distinction GMEOW borrowed** with an **academic study** of the tradition. The
citation grounds the structural claim; it asserts nothing about the tradition's truth.

| Key | Distinction borrowed | Scholarly citation |
|---|---|---|
| `trikaya-williams-2009` | manifestation layering (durable essence → contextual body → emanation body) | Williams, P. (2009). *Mahāyāna Buddhism: The Doctrinal Foundations* (2nd ed.). Routledge. — and Makransky, J. (1997). *Buddhahood Embodied: Sources of Controversy in India and Tibet.* SUNY Press. |
| `avatara-parrinder-1970` | the durable subject *descends* into a manifest form (the etymon of "Avatar") | Parrinder, G. (1970). *Avatar and Incarnation.* Faber & Faber. — and Sheth, N. (2002). Hindu Avatāra and Christian Incarnation: A Comparison. *Philosophy East and West*, 52(1), 98–125. |
| `anatta-collins-1982` | identity-continuity is a contestable claim, not a given (no-self vs enduring-self) | Collins, S. (1982). *Selfless Persons: Imagery and Thought in Theravāda Buddhism.* Cambridge University Press. — and Siderits, M. (2003). *Personal Identity and Buddhist Philosophy: Empty Persons.* Ashgate. |
| `skandha-gethin-1986` | the apparent self is a bundle of processes (the de-conflation) | Gethin, R. (1986). The Five Khandhas: Their Treatment in the Nikāyas and Early Abhidhamma. *Journal of Indian Philosophy*, 14(1), 35–53. |
| `atman-ganeri-2007` | the enduring-self pole of the continuity contest | Ganeri, J. (2007). *The Concealed Art of the Soul: Theories of Self and Practices of Truth in Indian Ethics and Epistemology.* Oxford University Press. |
| `possession-bourguignon-1976` | co-tenancy and displacement; control is attributed, not given | Bourguignon, E. (1976). *Possession.* Chandler & Sharp. |
| `vodou-deren-metraux` | the "horse"/`chwal` mounted by a *lwa* (host/inhabitant; control attribution) | Deren, M. (1953). *Divine Horsemen: The Living Gods of Haiti.* Thames & Hudson. — and Métraux, A. (1959). *Voodoo in Haiti* (trans. H. Charteris). Oxford University Press. |
| `tulpa-mikles-laycock-2015` | genesis by sustained intention; acquired autonomy | Mikles, N. L., & Laycock, J. P. (2015). Tracking the Tulpa: Exploring the "Tibetan" Origins of a Contemporary Paranormal Idea. *Nova Religio*, 19(1), 87–97. DOI: 10.1525/nr.2015.19.1.87. — and David-Néel, A. (1929). *Magic and Mystery in Tibet.* (Primary travelogue, cited as the term's modern conduit.) |
| `egregore-occult` | a collective wills and sustains a subject (Cagle's Collective) | Stavish, M. (2018). *Egregores: The Occult Entities That Watch Over Human Destiny.* Inner Traditions. (Cited for the concept's structure; a practitioner source, not an academic one.) |
| `esotericism-hanegraaff-2013` | invocation vs evocation (locus); godform assumption (playing a role); conjuration/abjuration (ritual start/end of a tenure) | Hanegraaff, W. J. (2013). *Western Esotericism: A Guide for the Perplexed.* Bloomsbury. — and Asprem, E. (2017). Theurgy. In *The Occult World* (C. Partridge, ed.), Routledge. |

## Staging for `metadata/references.ttl`

When the `slices/core/inhabitation` module is authored, each entry above is emitted as a
`gmeow:Reference` individual with the appropriate predicate — `gmeow:bridgedByReference` /
`skos:relatedMatch` for the modeling lineages, a citation record for the literature, and a
`gmeow:borrowsDistinctionFrom` annotation (paired with `gmeow:assertsNoMetaphysicsOf`) for the
tradition studies. No external axioms are imported; the alignment is assertion-by-reference only
(Principle 5), exactly as the logic set's references are staged for the same ledger.

## Works cited (consolidated)

- Almeida, J. P. A., Falbo, R. A., & Guizzardi, G. (2019). *gUFO: A Lightweight Implementation of UFO.* NEMO, UFES.
- Asprem, E. (2017). Theurgy. In C. Partridge (ed.), *The Occult World.* Routledge.
- Beale, T. (2002). Archetypes: Constraint-based Domain Models for Future-proof Information Systems. *OOPSLA 2002 Workshop on Behavioural Semantics.*
- Bonner, A. J., & Kifer, M. (1993). Transaction Logic Programming. *Proc. ICLP 1993*, 257–279. MIT Press.
- Bourguignon, E. (1976). *Possession.* Chandler & Sharp.
- Brickley, D., & Miller, L. (2014). *FOAF Vocabulary Specification 0.99.*
- Cagle, K. (2026). *A Vocabulary for Inhabited Systems.* The Inference Engineer (Substack).
- Collins, S. (1982). *Selfless Persons.* Cambridge University Press.
- David-Néel, A. (1929). *Magic and Mystery in Tibet.*
- Deren, M. (1953). *Divine Horsemen: The Living Gods of Haiti.* Thames & Hudson.
- Gangemi, A., & Mika, P. (2003). Understanding the Semantic Web through Descriptions and Situations. *OTM 2003*, LNCS 2888, 689–706. Springer.
- Ganeri, J. (2007). *The Concealed Art of the Soul.* Oxford University Press.
- Gethin, R. (1986). The Five Khandhas. *Journal of Indian Philosophy*, 14(1), 35–53.
- Gómez Álvarez, L., & Rudolph, S. (2021). Standpoint Logic. *Proc. FOIS 2021.* IOS Press.
- Guarino, N., & Welty, C. A. (2009). An Overview of OntoClean. *Handbook on Ontologies* (2nd ed.), 201–220. Springer.
- Guizzardi, G. (2005). *Ontological Foundations for Structural Conceptual Models.* PhD thesis, University of Twente.
- Guizzardi, G., et al. (2022). UFO: Unified Foundational Ontology. *Applied Ontology*, 17(1), 167–210.
- Hanegraaff, W. J. (2013). *Western Esotericism: A Guide for the Perplexed.* Bloomsbury.
- IFLA Study Group on the FRBR (1998). *Functional Requirements for Bibliographic Records.* K. G. Saur.
- Koestler, A. (1967). *The Ghost in the Machine.* Hutchinson.
- Lebo, T., Sahoo, S., & McGuinness, D. (eds.) (2013). *PROV-O: The PROV Ontology.* W3C Recommendation.
- Makransky, J. (1997). *Buddhahood Embodied.* SUNY Press.
- Masolo, C., et al. (2003). *WonderWeb Deliverable D18: DOLCE.* ISTC-CNR.
- Masolo, C., et al. (2004). Social Roles and their Descriptions. *Proc. KR 2004*, 267–277. AAAI Press.
- Métraux, A. (1959). *Voodoo in Haiti.* Oxford University Press.
- Mikles, N. L., & Laycock, J. P. (2015). Tracking the Tulpa. *Nova Religio*, 19(1), 87–97.
- Olson, E. T. (2023). Personal Identity. *The Stanford Encyclopedia of Philosophy.*
- Parfit, D. (1984). *Reasons and Persons.* Oxford University Press.
- Parrinder, G. (1970). *Avatar and Incarnation.* Faber & Faber.
- Reynolds, D. (ed.) (2014). *The Organization Ontology.* W3C Recommendation.
- Riva, P., Le Bœuf, P., & Žumer, M. (2017). *IFLA Library Reference Model.* IFLA.
- Sheth, N. (2002). Hindu Avatāra and Christian Incarnation. *Philosophy East and West*, 52(1), 98–125.
- Siderits, M. (2003). *Personal Identity and Buddhist Philosophy.* Ashgate.
- Simons, P. (1987). *Parts: A Study in Ontology.* Oxford University Press.
- Stavish, M. (2018). *Egregores.* Inner Traditions.
- Varzi, A. (2019). Mereology. *The Stanford Encyclopedia of Philosophy.*
- Williams, P. (2009). *Mahāyāna Buddhism: The Doctrinal Foundations* (2nd ed.). Routledge.
