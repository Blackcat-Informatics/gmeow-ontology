# Metadata repair contract

Issue 1803 requires generic native engines to own mechanisms while GMEOW owns source language and world/provenance policy. The recorded default inventory exposed annotation properties used as OWL roles. PR 1840 corrected the four true music data roles. This increment addresses the five remaining metadata properties without changing their annotation kinds.

| Source owner and path | Preserved validation intent | Replacement |
| --- | --- | --- |
| profiles: rdfs:label, skos:definition, profileDescriptor | At least one value for each descriptive path | Three requiredness constraints; literal labels/definitions are not forced into object classes |
| documentation: docOwnerSlice | Exactly one owning-slice IRI | Requiredness, same-path forbidden distinct resource values and IRI kind constraints |
| logic: enforcesFailureClass | Required single failure-class IRI on flagship scenarios | Existing wiring formula retains existence; new single-value and IRI constraints retain class scope and UnwiredFlagshipScenario failure identity |
| lang: accordingTo | A required resource-valued asserting standpoint on GMN envelopes | Requiredness and resource-kind constraints retain GmnMissingEnvelopeField failure identity |

Same-path comparisons bind both metadata values before comparing them. Distinct IRI annotations remain distinct even when the data also relates them by owl:sameAs. This is a term-bound integrity comparison, not a rewrite to denotational inequality or an inference of unique names. IRI/resource kind is enforced separately; no value-equality shortcut is used to admit literal metadata.

The focused synthetic control uses the actual frontend and generated SHACL, compares against an independent minCount/maxCount/IRI fixture, and requires exact expected focus sets across positive and adversarial inputs. Native reader-view admission, annotation kind and replayable native consistency proof are checked separately from integrity validation; native consistency is not claimed to enforce closed-world metadata requiredness.

Fresh production source/assertion/shape-union acceptance remains hosted-only. This increment does not complete native class delegation, proof+stop identity support, cheap incremental/shared-store boundaries, whole-canon prover preparation or external-corpus competency support. Those remain active work in their existing issues, not scope cuts or newly deferred tickets. No independent review is fabricated when dispatch capability is unavailable.
