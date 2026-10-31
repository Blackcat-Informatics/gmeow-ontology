<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Goals, conditions and source evidence

A Goal can carry ordered prose Condition occurrences through
`hasSatisfactionCondition`. Each occurrence has its own `conditionText`,
zero-based `entryOrdinal` and exact source `verifierKey`. Reused content in
different sequences uses different occurrence nodes. A source key is local to
one goal; equal keys across goals do not merge conditions. Each linked condition
requires a configured verifier of that same goal with the identical key, even
before any evaluation exists. Conditions are independent of executable
`logic:GoalExpression` programs (Principles 4 and 17).

A `ConditionVerifier` is an InformationObject configuring a condition key, a
method and any source expectation. It is not a Criterion: no rubric, score,
reward pole or penalty pole is fabricated. `hasConditionVerifier` links it to
the goal; its `verifierKey` joins the actual condition of that same goal.
The key and `conditionVerifierKind` are required under explicit class-scoped
closure. Key and position uniqueness apply within the containing goal, not
globally. Unknown source fields remain unknown rather than receiving defaults.

## Configuration and observations

`conditionVerifierKind` selects a configured method.
Existing `verifierKind` remains on actual ArtifactVerificationObservation
records and continues to specialize `observationMethod`. Applying that
existing selector to configuration would incorrectly type configuration as
an artifact observation. The method vocabulary now includes artifact existence,
external observation, user assertion, retained-memory evidence, reflective
appraisal, simulated-world existence and foreground yield. Selecting any kind
neither executes a verifier nor proves satisfaction.

`verifierTitle` and `expectedText` preserve source wording. `expectedStatement`
contains the actual native RDF 1.2 triple term, with the identical explicit
`rdf:reifies` binding on the descriptor. The expected proposition is quoted,
not asserted. The binding law joins supplied graph terms; it does not implement
a generic triple-term validator. `expectedWorldObject` is an unresolved exact
source identifier, not an object IRI or global identity. A world verifier uses
existing `providerProgram` and `providerWorld` to retain its actual resolution
scope, distinct from the handle itself.

## Positive judgments require structured cited evidence

Evidence belongs to the actual `logic:GoalEvaluation`. Its `conditionKey`
joins a condition and configured verifier within `logic:evaluatesGoal`.
The completed-positive coverage law requires every source condition's key and
matching verifier on the evaluation; choosing an easy condition cannot evade
the others. A conclusive positive judgment has both `logic:Satisfied` and
`logic:GoalEvaluationCompleted`. Neither axis follows from an attempt stop.

The kind-specific integrity formulas require actual derivation sources through
the domain-free `wasDerivedFrom`, used when the evaluation genuinely derives
from that evidence. Each exact derivation statement has a native RDF 1.2 citation
reifier carrying the same `verifierKey`. Existing statement lowering joins its
subject, predicate and object to that exact evaluation and evidence source.
The evaluation node retains its actual goal, world, evaluator, time and criterion;
no extra evaluator or scope is introduced. Evidence for another condition key or
another evaluation cannot silently satisfy this citation. This is not a generic
reference or authorship link. World evidence matches the evaluation's explicit
`providerWorld` environment context. `logic:evaluatedAgainst` may identify a
distinct planning or knowledge state; that state is not equated with the
environment it describes. A foreground-yield event identifies the actual
intention targeting this goal.

| Selected kind | Required cited source for a completed positive evaluation |
| --- | --- |
| Artifact existence | A held ArtifactVerificationObservation naming the actual artifact, selected method and vantage. |
| External observation | A held Observation with actual vantage, method, feature and the exact expected statement reification. |
| User assertion | A held assertion Observation with the asserting user's Agent vantage, method, feature and exact expected statement reification. |
| Memory evidence | A retained MemoryItem with its actual memoryText and agent attribution. |
| Reflective appraisal | A Note with actual noteContent, attributed to the evaluating agent. |
| World-object existence | A provider-owned WorldObjectRecord with actual object, provider and world, and the exact expected handle in the same configured provider/world scope. |
| Foreground yield | An actual attempt-finish event explicitly carrying stopYieldedToForeground and its associated actor. |

`evidenceItem` is fallback source wording when no span is available. It
cannot replace these structured citations. Use actual `groundedIn` evidence
spans where supplied. A citation with content is not a private-store lookup
receipt, authenticity proof or automatic proof that the cited text entails the
condition. The ontology preserves the attributed evaluation and constrains
its evidence structure; it does not execute semantic comparison, retrieval,
environment access or the verifier itself. Existing read-after-write evidence
laws and their distinct versioned read/write receipts remain unchanged. A
configured read-after-write method has its own positive-evidence law requiring
the actual cited held observation, so configuration cannot bypass those receipts.

## Prose attempts and intention history

`followedPlan` on an attempt start points to an InformationObject carrying
`planText`. It requires no executable `logic:Plan`, prescription version or
compiled AST. `attemptSummary`, `bodyCause` and `nextAction` belong to attempt
finishes. The summary does not specialize Entity-domain `description`: an
Event remains an occurrent. A next-action proposal is not an execution.

`adoptedFrom` specializes the domain-free `wasDerivedFrom` from Intention to
Desire. `commitmentText` and `commitmentOrigin` retain self-commitment wording,
without manufacturing a social contractual Commitment. `releaseReason`
belongs to the closed IntentionTenure; its actual interval has an explicit
`endedAtTime` or `hasEndInstant`. The reason neither erases the intention
nor invents the closure date. No description bridge is imposed on that tenure.

`intentionEventSequence` retains the writer's original nonnegative numbering.
It is not a subproperty of zero-based `entryOrdinal`. Every numbered event
identifies its actual intention through `intentionTransitionOf`. The exact
numbering statement is reified with native RDF 1.2, attributed to its actual
writer by `wasAttributedTo` and scoped by `accordingTo`. The formulas read
the existing generated `logic:reifiedStatementSubject`,
`logic:reifiedStatementPredicate` and `logic:reifiedStatementObject` lowering;
never author those components. A number is unique only within the same
intention, writer and source context. Performers do not imply writers.
Duplicate timestamps, backward clocks and other writers' numbering do not
override this source order. No clock ordering or global numbering is inferred.

`deadlineAt` and `nextReviewAt` retain source dateTime values. Preserve source
precision, timezone and frame evidence through the actual source record or
span; do not pad a partial date or invent a timezone. Richer temporal source
values remain on their original temporal records rather than becoming invented
dateTime values. `eventReason` retains the source explanation.
`engineContract` preserves the exact source identifier as a string, including
spaces and case. `auditText` retains an unparsed ToolCall audit line; it is
neither contract execution nor automatically a MalformedValue.

Prose accepts xsd:string, rdf:langString and rdf:dirLangString. Supplied
language and base direction survive verbatim; missing language is not guessed.
Opaque keys, handles and engine codes use exact xsd:string rather than token
normalization. These fidelity choices preserve source information without
creating a parser or executor (Principles 2, 3, 9 and 17).

The ordinary composition scene is
[`goal-attempt-satisfaction.ttl`](../../../extensions/dreaming/examples/goal-attempt-satisfaction.ttl).
It illustrates configured methods, cited evidence and a separate dream-derived
attention request. It is source example data, not an executed acceptance claim.
