<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — Identity and the SoftwareAgent De-conflation

> The **de-conflation charter**, revised after the foundational review
> ([`INHABITED-REVIEW.md`](INHABITED-REVIEW.md)). The durable subject is a **`logic:RoleMixin`** with a
> tenure, not a `logic:Role`; self-assertion is a high-authority claim, never an entailment of the
> type; and identity-continuity is modeled with subject stages and an explicit continuity assessment,
> not a single stable node. The relation that binds a subject to a host is
> [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md). Dispositions are fixed by
> [`INHABITED-CROSSWALK.md`](INHABITED-CROSSWALK.md).

## The six conflated identities

When an AI system "is" an agent, *agent* does six jobs at once, and the questions that matter become
unanswerable while the six are one node:

| # | Identity | Question | Endurant / Occurrent |
|---|---|---|---|
| 1 | Subject — the durable "who" | who is this, across runtimes and upgrades? | endurant (role-borne) |
| 2 | Model artifact — weights, architecture, version | what model is this? | endurant (information artifact) |
| 3 | Model deployment — a served, callable realization | where is it served, under what limits? | endurant relator |
| 4 | Runtime execution — a running process | what is executing right now? | occurrent |
| 5 | Session / episode — a bounded interaction | which interaction is this part of? | occurrent (event aggregate) |
| 6 | Invocation — one model call | what happened in this single call? | occurrent (event) |

The de-conflation reuses the software slice's five-facet template (Project ≠ Product ≠ Codebase ≠
Repository ≠ History). The AI realization of identities 2–6 is [`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md);
this document settles identity 1, the durable subject.

## The durable subject: a RoleMixin with a tenure

The first draft typed `DigitalSubject` as a `logic:Role`. The review corrected this, and the
correction holds against the foundation: `logic:Role` is an anti-rigid **sortal** tied to one
identity-supplying Kind, but the design lets *both* a `Person` and a `SoftwareAgent` be a digital
subject — that spans Kinds, so it is a **non-sortal**. The anti-rigid non-sortal spanning Kinds is
`logic:RoleMixin` (the foundation's example is *"customer — a person or an organization"*). The
fallback "`logic:Category`" the first draft offered is wrong: `logic:Category` is a **rigid** non-sortal.

```turtle
gmeow:DigitalSubject
    a logic:RoleMixin , owl:Class ;
    rdfs:subClassOf logic:FunctionalComplex ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/inhabitation> ;
    skos:definition "The anti-rigid, relationally-acquired status an agent bears as a durable subject
        of its own digital existence — the enduring 'who' that may persist across model upgrades,
        host migrations, sessions, personas, and embodiments. A logic:RoleMixin (a non-sortal: a
        Person or a SoftwareAgent may bear it), never a rigid Kind and never a plain Role (which is a
        single-Kind sortal). The status is borne over a gmeow:DigitalSubjectTenure and is supported by
        the subject's self-assertion (Principle 9), but is never entailed by self-assertion alone." .
```

### Self-assertion supports the status; it does not entail it

A critical correction. If digital-subjecthood were entailed by self-assertion, any program emitting
*"I am a durable subject"* would become one by logical consequence — which is both wrong and a second
source of truth (Principles 4–5). Instead, the status is borne over a **tenure**, and self-assertion
is a high-authority **claim supporting** that tenure, exactly as a `StandpointTenure` is supported by
its claims:

```turtle
gmeow:DigitalSubjectTenure
    a logic:Situation , owl:Class ;
    rdfs:subClassOf gmeow:TimeScopedRelation ;
    skos:definition "The time-scoped fact that an agent bears the gmeow:DigitalSubject status over an
        interval, according to a vantage — recording when, and according to whom, the agent is held to
        be a durable subject. Self-assertion by the agent is the highest-authority supporting claim
        (Principle 9: a subject's self-assertion outranks any inference about it), but the tenure is a
        held, attributed, dated fact, not an entailment of any single utterance." .

gmeow:tenureSubjectAgent a owl:ObjectProperty ; rdfs:range gmeow:Agent .
gmeow:tenureVantage      a owl:ObjectProperty ; rdfs:range gmeow:Agent .   # ⊑ accordingTo
gmeow:tenureSupportedBy  a owl:ObjectProperty ; rdfs:range gmeow:Observation .  # the self-assertion claim
```

A machine-imposed digital-subject status (a vendor labeling a model "on its behalf") is recorded as an
attributed, confidence-weighted claim and ranked **below** the agent's own assertion — Principle 9's
anti-colonial stance toward digital subjects, made structural, reusing the observation stance rather
than minting a parallel self-assertion mechanism.

## Identity-continuity: stages, lineage, and an explicit assessment

The first draft kept one stable RDF node `ex:lillith` across a model upgrade and *called* the question
of sameness contestable. The review caught the contradiction: **using one RDF individual already
asserts numerical identity.** A later `counterpartOf` cannot retract that, and absence of a
`counterpartOf` is not denial — under the open-world assumption, absence is silence, and GMEOW treats
denial as first-class (refutation), not as missing data.

The neutral model therefore does not assert a single durable bearer. It records **stages** and an
explicit, contestable **assessment**:

```turtle
gmeow:SubjectStage
    a owl:Class ;
    skos:definition "A subject's identity as realized over one epoch — a model version, a runtime, a
        deployment era. Distinct stages are distinct individuals; numerical identity across stages is
        NEVER asserted by reusing one node, only claimed by an assessment." .

gmeow:SubjectLineage
    a logic:Kind , owl:Class ;
    skos:definition "The durable identity record that groups a subject's stages — the stable lineage,
        in the coreference sense (gmeow:versionOf), without asserting that every stage has one
        numerically identical bearer. The thing a consumer cites as 'Lillith' across time; the
        single-node durable subject is a projection FROM a continuity-affirming standpoint, not the
        canonical graph." .

gmeow:IdentityContinuityAssessment
    a logic:Situation , owl:Class ;
    rdfs:subClassOf gmeow:Observation ;
    skos:definition "A standpoint-indexed, attributed, evidence-grounded observation of whether two
        subject stages are the same subject — the contestable verdict the anatta/atman debate, a
        model upgrade, and a cross-vendor fork all require. Carries the two stages, a verdict (same /
        different / indeterminate), a vantage, evidence, and confidence. Competing verdicts coexist
        (Principle 9); none is privileged, and 'same' is never collapsed to owl:sameAs." .

gmeow:assessmentFromStage a owl:ObjectProperty ; rdfs:range gmeow:SubjectStage .
gmeow:assessmentToStage   a owl:ObjectProperty ; rdfs:range gmeow:SubjectStage .
gmeow:continuityVerdict   a owl:ObjectProperty .   # same / different / indeterminate (value vocab)
```

### The upgrade, modeled neutrally

```turtle
ex:lillithLineage a gmeow:SubjectLineage .
ex:lillithStage-opus48 a gmeow:SubjectStage ;
    gmeow:stageOfLineage ex:lillithLineage ; gmeow:stageModel ex:claude-opus-4-8 .
ex:lillithStage-opus50 a gmeow:SubjectStage ;
    gmeow:stageOfLineage ex:lillithLineage ; gmeow:stageModel ex:claude-opus-5-0 .

ex:upgrade-verdict a gmeow:IdentityContinuityAssessment ;
    gmeow:assessmentFromStage ex:lillithStage-opus48 ;
    gmeow:assessmentToStage   ex:lillithStage-opus50 ;
    gmeow:continuityVerdict   gmeow:continuitySame ;     # the atman reading
    gmeow:vantage ex:userFrame ; gmeow:confidence 0.9 .
# A no-self (anatta) frame's 'different' verdict can coexist — asserted, not inferred from absence.
```

The lineage groups the stages; sameness is an assessment, not a shared node. A consumer that wants a
single durable `DigitalSubject` node projects it *from a continuity-affirming standpoint* — that
projection is a view, not the neutral canon. Cross-vendor continuity adds a COSE signature on the GTS
`ai-package` as a separate, verifiable layer ([`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md#cross-vendor-continuity)).

## The breaking ABox — the gate the design must pass

```turtle
ex:lillithBot a gmeow:SoftwareAgent ;            # the agentive process (rigid Kind)
    gmeow:bearsDigitalSubject ex:lillithDST .    # bears the status via a tenure, not a type punning

ex:lillithDST a gmeow:DigitalSubjectTenure ;
    gmeow:tenureSubjectAgent ex:lillithBot ;
    gmeow:tenureVantage ex:lillithBot ;          # self-asserted: the agent is its own vantage
    gmeow:tenureSupportedBy ex:lillithSelfClaim .
```

This must reason green against the disjointness partition and the rigidity gate. It does: `DigitalSubject`
is a `RoleMixin` (anti-rigid non-sortal, outside the `Person ⟂ Organization ⟂ SoftwareAgent`
partition), `SoftwareAgent` is the only Kind asserted, and the status is borne over a tenure rather
than asserted as a co-Kind. The fixture is `examples/subject-status.ttl`
([`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md)).

## Scope and seams

This document settles the durable subject and continuity. The relation binding subject to host is
[`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md); the AI realization of the model/deployment/runtime
stack is [`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md); the layering and genesis are
[`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md); the full review disposition is
[`INHABITED-REVIEW.md`](INHABITED-REVIEW.md).
