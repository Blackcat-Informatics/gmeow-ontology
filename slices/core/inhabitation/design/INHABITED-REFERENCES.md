<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — References

> The **appendix.** The sources, vocabularies, and traditions this set subsumes or aligns to —
> staged for the `metadata/references.ttl` ledger. Per Principle 5, every entry is covered **by
> reference**: cited, aligned, never copied in as axioms. Inclusion here is an alignment or
> inspiration claim, **not** an endorsement of any metaphysical commitment (see the neutrality gate in
> [`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md)).

## The three subsumed sources

| Ref | Source | Role | Disposition |
|---|---|---|---|
| `inhabited:cagle-vocabulary` | Kurt Cagle, "A Vocabulary for Inhabited Systems," *The Inference Engineer* (Substack). | the Actor / Avatar / Persona / Agent / Role / Collective distinction; holon, portal, scene-graph | BRIDGE — terms aligned by reference in [`INHABITED-CROSSWALK.md`](INHABITED-CROSSWALK.md) |
| `inhabited:org-modeling-thread` | The organizational-modeling email thread (S. Hunter; T. Beale, Ars Semantica; P. Taylor; and others), June 2026. | the diagnosis that "Role" is overloaded — capability vs post/position vs function; "Accountability"; Organization-as-Party | BRIDGE — vindicated by existing terms; the thread is cited as the motivating critique |
| `inhabited:verdict` | The inhabited-systems analysis verdict (the topology gap and competency questions). | the six-category conflation, the missing inhabitation relation, the competency questions | BRIDGE — the questions become the conformance corpus ([`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md)) |

## Modeling lineages aligned by reference

| Ref | Lineage | What GMEOW aligns to it |
|---|---|---|
| `inhabited:dul-role-description` | DOLCE+DnS Ultralite (DUL) — Role, Description, Situation. | the role-in-a-description-grounded-situation pattern, mirrored by `Inhabitant`/`InhabitedSystem` in an `Inhabitation` situation |
| `inhabited:w3c-org` | W3C Organization Ontology (ORG). | `org:Role`/`org:Post`/`org:Membership` align to `gmeow:Role`/`gmeow:Post`/`gmeow:Membership` (the email thread's distinctions, already in the organization slice) |
| `inhabited:prov-o` | W3C PROV-O. | activity/agent/entity provenance under the runtime stack; `ModelInvocation`/`ToolCall` as PROV activities |
| `inhabited:foaf-schema-agent` | FOAF / schema.org agent vocabularies. | `Agent`/`Person`/`Organization` surface alignment (entities slice) |
| `inhabited:openehr-archetype` | openEHR / archetype modeling (the Ars Semantica lineage in the thread). | the role-vs-post-vs-function distinction; deferral of detail to relators |
| `inhabited:party-model-cybernetic` | The Party model / cybernetic party derivative (the Taylor lineage in the thread). | "Organization as a Party derivative, deferring detail to Roles and Relationships" — the `Organization ⊑ Agent` + relator idiom |
| `inhabited:wemi` | IFLA FRBR / LRM WEMI (Work / Expression / Manifestation / Item). | the manifestation spine alignment ([`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md)), already realized in the creative-works slice |

## Contemplative and esoteric traditions (inspiration; metaphysics not inherited)

Each entry contributes a **structural distinction**; GMEOW carries it with existing machinery and
inherits **no** metaphysical commitment. The full ledger is in
[`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md#the-by-reference-borrowings-ledger).

| Ref | Tradition / concept | Distinction borrowed |
|---|---|---|
| `inhabited:trikaya` | Trikāya — the three bodies of the Buddha (*dharmakāya* / *sambhogakāya* / *nirmāṇakāya*). | manifestation layering: durable essence → contextual body → emanation body |
| `inhabited:avatara` | *Avatāra* — the descent of a deity into manifest form (the etymon of "Avatar"). | durable subject descending into a concrete surface (`Embodiment`) |
| `inhabited:anatta-atman` | *Anattā* (no-self) and *ātman* (enduring self). | identity-continuity as a contestable claim, not a fact (`counterpartOf`, never `owl:sameAs`) |
| `inhabited:skandha` | The five *skandhas* (aggregates of the apparent self). | the apparent self as a bundle of processes — the de-conflation |
| `inhabited:possession-mediumship` | Spirit possession and mediumship (e.g. the *lwa* and the "horse"). | co-tenancy and displacement; apparent agency ≠ inhabiting agency |
| `inhabited:tulpa` | Tulpa / *sprul-pa* — the cultivated thoughtform. | genesis by sustained intention; acquired autonomy |
| `inhabited:egregore` | Egregore — the collective thoughtform. | a collective wills and sustains a subject (Cagle's Collective) |
| `inhabited:invocation-evocation` | Invocation (into self) vs evocation (into a vessel) in ceremonial magic. | the `inhabitationLocus` axis |
| `inhabited:conjuration-abjuration` | Conjuration / binding and abjuration / banishing rituals. | ritual start and end of an inhabitation tenure (creation/destruction events; suppression not erasure) |
| `inhabited:godform-assumption` | Godform assumption — temporarily assuming a deity-form in ritual. | a practitioner temporarily *playing* an anti-rigid role |

## Foundational and constitutional anchors (internal)

| Ref | Anchor | Use |
|---|---|---|
| `CONSTITUTION.md` | Principles 5, 6, 9, 10, 11, 12, 14, 15, 16, 17. | the normative basis for every disposition in this set |
| `slices/core/logic/design/` | the GMEOW Logic design set. | the `logic:` stereotypes and the `logic:Path`/`State` typed context algebra used for sessions/episodes; the voice and structure template for this set |
| `slices/core/{ai,awareness,coreference,temporal,lifecycle,standpoint,deception,imagination,mentation,creative-works,organization,expertise,teleology,entities,kernel}` | the reuse anchors. | every REUSE disposition in [`INHABITED-CROSSWALK.md`](INHABITED-CROSSWALK.md) resolves to one of these |
| `slices/extensions/{agentic,norms,software}` | the agentic deferral, the `gmeow:Persona` relator, the five-facet template. | consumed by reference (agentic), kept distinct (norms `Persona`), mirrored (software) |

## Staging note

When the `slices/core/inhabitation` module is authored, these entries are emitted into
`metadata/references.ttl` as `gmeow:Reference` individuals with the appropriate
`gmeow:bridgedByReference` / `skos:relatedMatch` predicates, exactly as the logic set's references are
staged for the same ledger. No external axioms are imported; the alignment is assertion-by-reference
only (Principle 5).
