<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Attributed classifier evidence

The AI slice owns `ClassifierOutput`, `ClassifierLabel`, `ClassifierLabelSet`,
and the shared execution-record, score and decision vocabulary. One output is one
target/label/score cell, attributed to a `ModelInferenceRun` with an exact model
identifier and revision. External label identities remain information artifacts;
even a label spelled “genre” or “joy” does not become a canonical category.
Reviewed correspondences are a separate operation (Principles 4, 5, 9 and 17).

`ClassifierOutput` specializes `Observation`; `classifiedTarget` specializes
`observedFeature`. `AffectClassifierOutput`, `AffectClassifierLabel`, and
`AffectLabelSet` specialize the general classes and retain their affect-local
ranges. Affect-only prohibitions and claim laws remain in affect. Shared Formula laws retain stable constraint IRIs, and the five property
characteristic carriers belong to their shared properties. Direct affect restrictions and closure entries preserve
the current projection surface for affect consumers.

## A record is distinct from its execution

`ModelInferenceRun` is an enduring `InformationObject`, hence an `Entity`, and may
serve as the output’s observation vantage. It records an execution; it is not an
`Activity`. `producedBy` names that record and is not a specialization of
`wasGeneratedBy`. If an actual execution activity is supplied, the existing
provenance foundation can link the record to that activity through
`wasGeneratedBy` and the activity to its agent through `wasAssociatedWith`.
The record and record-owned properties do not substitute an activity or
activity-domain relations in a PROV projection. No execution machinery follows from
the vocabulary addition (Principles 11, 15 and 16).

## Label sets and zero-shot identity

Every label belongs to an identified set. `RegisteredClassifierLabelSet` denotes
an admitted static registry and requires its reviewed `labelSetDecision` through
a canonical universal restriction plus closed-world closure entry. The existing
four affect registries are explicitly registered sets. This requirement does not
apply to every `ClassifierLabelSet`: a run-scoped zero-shot candidate set may lack
a reviewed decision rule.

When an output declares `scoreEntailment`, a Formula law follows `producedBy`
to its execution record and requires the exact `hypothesisTemplate`. The free
`modelTask` text does not select this obligation. Candidate inventory, exact label surfaces, template and any
supplied `labelSetRevision` belong to the recorded context; a different template or candidate inventory requires a fresh
execution record. `runLabelSet` may explicitly name the set. When it does, a law
requires each output label to belong to it. Existing affect records can instead
preserve membership through emitted labels and any supplied label-set revision;
they do not acquire a new mandatory edge.

## Scores keep their meanings

`classifierScore` is the raw decimal carrier. `scoreSemantics` declares how to
interpret it. Softmax normalization, sigmoid scores, per-hypothesis entailment,
logits and margins remain distinct. Logits and margins rule out blanket unit
interval bounds. Calibrated-probability semantics requires `scoreCalibration`,
but calibration does not establish `logic:confidence` or the provenance warrant
represented by `logic:evidenceStrength`. A conversion requires an explicit,
separately justified mapping. Declared decision rules must agree with the score
semantics where both are present (Principles 2, 9 and 19).

The grounding reuses the observation and provenance sorts and canonical `logic:`
restrictions, closure entries and Formula laws. `hypothesisTemplate` specializes
the domain-free `lang:literalValue` complete-literal carrier. Its narrower
`xsd:string` contract preserves the exact template source expression, including
placeholders. As with source programs in the literal policy, this is a supplied
source string rather than a localized utterance or a machine identifier. The
affect capture, emission and recovery paths carry that string; no language or
direction is inferred. The score is not a
`math:Quantity`, so it does not specialize `math:quantityValue`; no fake numeric
grounding changes its subject sort or invents probability/confidence conversion.

## Supplied machinery datatypes

`modelVersionTag`, `labelSetRevision`, `modelFramework`, `modelIdentifier`,
`modelRevision`, `modelTask`, `scoreCalibration` and `tokenizerRevision` admit
supplied `xsd:string` and `xsd:token` literals through canonical datatype unions.
Preserve the exact lexical form and datatype; do not normalize whitespace,
retype a string or infer token typing from its spelling. A source recording one
datatype does not authorize a consumer to recover another from lexical text alone.

The model identifier/revision universal restrictions and the label-set revision
universal and qualified maximum use the same union. Required paths, cardinalities,
functionality and source attribution retain their existing scope. Equal strings
from different producer namespaces do not identify the same model or revision.
A model-card version tag remains separate from stable citation identity.

`hypothesisTemplate` remains an exact source expression carried as `xsd:string`.
Task identifiers do not select hypothesis obligations, and calibration-profile
identifiers do not establish epistemic confidence. These datatype declarations
add no lexical checker, model execution or token-preserving capture/recovery
implementation (Principles 2, 4, 7, 9 and 17).

## A reference-work import scene

An importer preserves a zero-shot genre assessment of one reference-work entry.
The following is illustrative ontology usage, not an executable fixture or a
claim that the native affect producer executes genre models. The candidate set
intentionally has no static decision rule.

```turtle
@prefix ex: <https://example.org/classifier-scene/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

ex:candidates a gmeow:ClassifierLabelSet .
ex:encyclopediaLabel a gmeow:ClassifierLabel ;
    rdfs:label "encyclopedia entry"@en ;
    gmeow:memberOfLabelSet ex:candidates .
ex:manualLabel a gmeow:ClassifierLabel ;
    rdfs:label "technical manual"@en ;
    gmeow:memberOfLabelSet ex:candidates .
ex:run a gmeow:ModelInferenceRun ;
    gmeow:modelIdentifier "example/genre-nli" ;
    gmeow:modelRevision "5f4d3c2b1a0987654321fedcba9876543210abcd" ;
    gmeow:modelTask "zero-shot-classification" ;
    gmeow:labelSetRevision "candidate-inventory-v1" ;
    gmeow:hypothesisTemplate "This text is an example of {}." ;
    gmeow:runLabelSet ex:candidates ;
    gmeow:usedInput ex:entry .
ex:output a gmeow:ClassifierOutput ;
    gmeow:vantage ex:run ;
    gmeow:producedBy ex:run ;
    gmeow:classifiedTarget ex:entry ;
    gmeow:emittedLabel ex:encyclopediaLabel ;
    gmeow:classifierScore 0.83 ;
    gmeow:scoreSemantics gmeow:scoreEntailment .
```

The importer can retain competing assessments and later attach a reviewed genre
interpretation without replacing this evidence. The affect producer continues to
emit affect-specialized classes and apply its existing registry checks. EmotionML
continues to enumerate affect categories and axes with its declared losses;
general genre evidence does not broaden that projection.
