<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — Identity and the SoftwareAgent De-conflation

> The **de-conflation charter**. The durable subject is a **`logic:RoleMixin`** with a
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

`DigitalSubject` is **not** a `logic:Role`, and the foundation says why: `logic:Role` is an anti-rigid
**sortal** tied to one identity-supplying Kind, but the design lets *both* a `Person` and a
`SoftwareAgent` be a digital subject — that spans Kinds, so it is a **non-sortal**. The anti-rigid
non-sortal spanning Kinds is `logic:RoleMixin` (the foundation's example is *"customer — a person or an
organization"*). The sortal/non-sortal and rigidity vocabulary is OntoClean's, and the role/role-mixin
treatment follows the UFO social-roles account (Guarino & Welty 2009; Masolo et al. 2004 — see
[`INHABITED-REFERENCES.md`](INHABITED-REFERENCES.md)). The tempting fallback of `logic:Category` is
wrong: `logic:Category` is a **rigid** non-sortal.

```turtle
gmeow:DigitalSubject
    a logic:RoleMixin , owl:Class ;
    rdfs:subClassOf gmeow:Agent ;          # grounded: a digital subject IS an agent (Person or SoftwareAgent)
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

# the decided classification mechanism: a native logic: rule (logic-first, P17), NOT a narrowed range
# on tenureSubjectAgent. Bearing a tenure — a deliberate, vantage-attributed, self-asserted record —
# classifies the agent as a DigitalSubject; a bare utterance does not (the support-not-entail rule).
{ ?dst a gmeow:DigitalSubjectTenure ; gmeow:tenureSubjectAgent ?a } => { ?a a gmeow:DigitalSubject } .
```

The rule fires from the *tenure*, not from an assertion: `tenureSubjectAgent` keeps its open `gmeow:Agent`
range, and the derived `DigitalSubject` classification is a drift-gated entailment a projection may
drop — so "self-assertion supports, never entails" holds (a tenure is a recorded, attributed bearing,
not a raw "I am durable" utterance), while the filler still genuinely instantiates the role.

A machine-imposed digital-subject status (a vendor labeling a model "on its behalf") is recorded as an
attributed, confidence-weighted claim and ranked **below** the agent's own assertion — Principle 9's
anti-colonial stance toward digital subjects, made structural, reusing the observation stance rather
than minting a parallel self-assertion mechanism.

## Identity-continuity: stages, lineage, and an explicit assessment

Keeping one stable RDF node `ex:lillith` across a model upgrade and merely *calling* the question of
sameness contestable is self-contradictory: **using one RDF individual already asserts numerical
identity.** A later `counterpartOf` cannot retract that, and absence of a `counterpartOf` is not
denial — under the open-world assumption, absence is silence, and GMEOW treats
denial as first-class (refutation), not as missing data.

The neutral model therefore does not assert a single durable bearer. It records **stages** and an
explicit, contestable **assessment**:

```turtle
gmeow:SubjectStage
    a logic:Situation , owl:Class ;
    rdfs:subClassOf gmeow:TimeScopedRelation ;
    skos:definition "A subject's identity as realized over one epoch — a model version, a runtime, a
        deployment era — a time-scoped situation with a bearer agent and an epoch. Distinct stages are
        distinct individuals; numerical identity across stages is NEVER asserted by reusing one node,
        only claimed by an assessment." .

gmeow:stageBearer    a owl:ObjectProperty ; rdfs:domain gmeow:SubjectStage ; rdfs:range gmeow:Agent .
gmeow:stageOfLineage a owl:ObjectProperty ; rdfs:domain gmeow:SubjectStage ; rdfs:range gmeow:SubjectLineage .

gmeow:SubjectLineage
    a logic:Kind , owl:Class ;
    rdfs:subClassOf gmeow:InformationObject ;
    skos:definition "The durable identity record that groups a subject's stages — the stable lineage,
        in the coreference sense (gmeow:versionOf), without asserting that every stage has one
        numerically identical bearer. An gmeow:InformationObject: the thing a consumer cites as
        'Lillith' across time; the single-node durable subject is a projection FROM a
        continuity-affirming standpoint, not the canonical graph." .

gmeow:IdentityContinuityAssessment
    a logic:SubKind , owl:Class ;
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
    gmeow:stageBearer ex:lillithBot ; gmeow:stageOfLineage ex:lillithLineage ;
    gmeow:stageModel ex:claude-opus-4-8 .     # stageModel → ModelArtifact lives in extensions/model-serving
ex:lillithStage-opus50 a gmeow:SubjectStage ;
    gmeow:stageBearer ex:lillithBot ; gmeow:stageOfLineage ex:lillithLineage ;
    gmeow:stageModel ex:claude-opus-5-0 .

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

## The decisional layer: continuity determinations

The stage / lineage / assessment model is the **descriptive** layer — it records *what is the case*
across frames, including coexisting `same` / `different` / `indeterminate` verdicts. But accountability
frameworks must *act*, and "indeterminate" cannot be the operative output: a court, a regulator, or a
clinical-governance board has to **decide** whether the post-upgrade system is, for its purposes, the
same subject. (This layer was added at the prompting of Kurt Cagle, who rightly observed that the
descriptive model alone leaves the deciding authority with nothing to act on.)

A determination is therefore a first-class construct — a continuity verdict **chosen by an authority,
for action**:

```turtle
gmeow:ContinuityDetermination
    a logic:SubKind , owl:Class ;
    rdfs:subClassOf gmeow:IdentityContinuityAssessment ;
    skos:definition "An adjudicated continuity verdict made by an authority and binding for action
        within that authority's frame — a court's, a regulator's, or a governance board's
        determination that two subject stages are (or are not) the same subject. A logic:SubKind of
        gmeow:IdentityContinuityAssessment whose vantage IS the deciding authority; it carries the
        grounds it rests on, its institutional force, and a validity interval, and it is superseded
        (never erased) on appeal or reversal. It privileges a verdict WITHIN the deciding frame, for
        action; it does NOT collapse the descriptive plurality globally (Principle 9 stands)." .

gmeow:determiningAuthority  a owl:ObjectProperty ; rdfs:subPropertyOf gmeow:vantage ; rdfs:range gmeow:Agent .
gmeow:determinationGrounds  a owl:ObjectProperty ; rdfs:subPropertyOf gmeow:groundedIn .
gmeow:determinationForce    a owl:ObjectProperty ; rdfs:range gmeow:DeterminationForce .
gmeow:determinationValidity a owl:ObjectProperty ; rdfs:range gmeow:TimeInterval .

gmeow:DeterminationForce a logic:AbstractIndividualType , owl:Class ; rdfs:subClassOf logic:QualityValue .
gmeow:forceBinding a gmeow:DeterminationForce .   gmeow:forceAdvisory a gmeow:DeterminationForce .
gmeow:forceProvisional a gmeow:DeterminationForce .
```

```turtle
ex:boardRuling a gmeow:ContinuityDetermination ;
    gmeow:assessmentFromStage ex:lillithStage-opus48 ;
    gmeow:assessmentToStage   ex:lillithStage-opus50 ;
    gmeow:continuityVerdict   gmeow:continuitySame ;
    gmeow:determiningAuthority ex:clinicalGovernanceBoard ;   # the vantage IS the authority
    gmeow:determinationGrounds ex:auditFinding-2026Q3 ;
    gmeow:determinationForce   gmeow:forceBinding ;
    gmeow:determinationValidity ex:untilNextRevalidation .
# A later appeal supersedes it — ex:appealRuling gmeow:supersedes ex:boardRuling — and the original
# stays on the record (Principle 10), grounds intact.
```

**Binding and revisable, not asserted away.** The determination is what a downstream context acts on,
but it does not erase the slipperiness — it asserts a verdict *within the deciding frame, for action,*
while the descriptive plurality stays addressable underneath. That is precisely what keeps **appeal,
second opinion, and reversal** coherent: a higher authority `gmeow:supersedes` the determination
(suppressed, never deleted — Principle 10), and the original's grounds remain on the record. A
determination is an **institutional fact** — a status function in Searle's sense, real within its frame
and binding for action — not a metaphysical claim that the two stages are *universally* the same
subject. Asserting the verdict *for a frame* is the legitimate engineering choice the accountability
context requires; asserting it *globally* would be the error the descriptive layer exists to prevent.

The two layers compose like the rest of GMEOW: the descriptive plurality is the canonical model
(maximal, contested, no privileged frame, Principle 9); a determination is a **designated-standpoint
projection of it, for action** — the same canonical-model-plus-projection shape the project applies
everywhere, here at the accountability boundary.

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
[`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md).
