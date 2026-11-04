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

## Scalar regression interpretation

`scoreRegression` is one additional individual of the existing open
`ScoreSemantics` vocabulary, which is grounded in `logic:QualityValue`. It
declares a model-predicted scalar for the named output dimension represented
by the output's actual ClassifierLabel. The existing one-output, one-target,
one-label, one-raw-decimal and one-declared-meaning obligations remain unchanged.
Model identity, exact revision, run record, inputs and source attribution stay
on their existing paths.

The declared model output dimension is not a physical or mathematical dimension.
`classifierScore` remains the established raw decimal carrier, with no
`math:quantityValue` specialization or new quantity subject. Actual measured
quantities and tool/formula measurements retain their own modeling obligations.

No `impliesLabelSetDecision` edge is added. The existing decision-consistency
law is guarded by an actual such edge and does not infer one from the name of
this individual or its model task. The calibration-presence law selects exactly
`scoreCalibratedProbability`, and the hypothesis-template law selects exactly
`scoreEntailment`; neither is selected merely by declaring regression semantics.
Registered label-set requirements and explicit run-label membership remain
unchanged. No probability bound, normalization, decision rule, threshold,
default, calibration or confidence conversion follows (Principles 4, 9, 11,
12, 17 and 19).

The current native affect adapter has a finite supported-semantics capture
model. Its recovery path rejects an unrecognized semantics IRI; it does not
substitute probability, a threshold or another meaning. This source value does
not extend that API. No native admission, producer, generated, installed or
runtime result is established by the ontology declaration.

## Keyword measurement roles

AI owns the extraction-specific `measuredTerm`, `ngramSize` and `termRank`
roles on the existing `Measurement` spine. Lang owns the complete literal
carrier; math owns Quantity, dimension and magnitude; observations owns the
actual unit, observer, method, metric and result roles. This adds no keyword
class, count class, numeric-value alias, ranking or tokenization engine.

Each source row and convention remains on its own actual Measurement.
The three roles are optional and nonfunctional, with no global uniqueness,
tuple pairing or equality rule. Equal terms, targets, magnitudes, ranks or
weights do not collapse records. `measuredTerm` specializes `lang:literalValue`
and admits the canonical plain, language-tagged and directional string kinds;
wording alone has no interpreted Form, concept, referent or occurrence identity.
`termRank` also specializes the complete literal carrier but retains the
source's integer ordering descriptor. An unknown origin, direction, tie scheme
or gap policy remains unknown; an unrelated integer code is not a rank.

The `ngramSize` object role names a separate actual positive integral count
Quantity. Its mathematical dimension is dimensionless; its actual segmentation
and counting convention is an applicable existing ReferenceFrame. Counting
positions preserves repetition. A distinct-word set, character length,
keyword-row count, whitespace split or unqualified integer report cannot
substitute for that meaning. The existing weight remains the separate
`observationResult`; the size role is not its subproperty and introduces no
probability, normalization or confidence interpretation.

Two ordinary Formula constraints specify the selected-count contract.
The context law follows each `ngramSize(this, q)` and requires a magnitude,
the dimensionless dimension and a frame edge. Its guarded universal leaves
reject every different dimension by complete RDF identity and every genuinely
unequal pair of magnitudes by numeric value equality. Different retained
lexical/datatype representations of the same valid numeric magnitude do not
require rewriting. The value law binds every magnitude through the selected
Quantity and conjunctively requires finite numeric admission, the exact
datatype-component disjunction for the thirteen XSD integer-family datatypes
and `logic:termGreater` against typed integer zero. A positive integer is not
retyped to pass, and a valid magnitude cannot hide an additional malformed,
zero, negative, fractional, floating or unknown-typed value.

The Formula trees bind each variable operand through a positive stored
relation. The integer-type disjunction reads the original datatype IRI through
`termSameComponent`; it does not introduce a datatype coercion. Numeric lexical
and derived-type validity remains with the existing shared finite-numeric
admission path. Context and value checks stay scoped to actual size edges;
no global property characteristic or unrelated Quantity restriction is added
(Principles 4, 9, 11, 12, 17 and 19).

The bound greater-than predicate is logic-owned and uses its already
implemented integrity-comparison path. Its canonical declaration supplies
ownership and annotation for that existing predicate; it does not install a
new instruction, ordinary-rule numeric relation or engine. The logical
validation design records its binding, numeric error and projection contract.
AI consumes its existing logic, math, Lang and observations dependencies and
the Places-owned frame role explicitly. No grounding peerage or seam changes
are needed.

The selected enforcement surface is ordinary generated procedural validation.
Neither law carries `enforcesFailureClass`, so native violation-marker
production is unselected. A frame witness must satisfy the existing generic
frame profile, but its graph structure cannot certify actual source
applicability, segmentation execution or historical bytes. A source integer
without its counting convention does not qualify this actual count. The
vocabulary can describe fully specified supplied counts without requiring
consumer adoption, inventing missing context or claiming runtime qualification.
