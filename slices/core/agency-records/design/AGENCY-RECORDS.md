<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Agency records design

The model adds source vocabulary for agency evidence. Existing `Intention`,
`Goal`, `Event`, `Activity`, `ToolCall`, `ModelInvocation` and `Observation`
retain their definitions. Prose attempts are event kinds; they acquire no compiled
prescription, snapshot or journal merely because work continued.

## Independent axes

An attempt stopping, an intention being released, an invocation failing and an
artifact being incomplete are separate claims. Source-attributed alternatives
can coexist with temporal and standpoint metadata. No global functional status
or automatic lifecycle state machine is introduced.

Intention lifecycle events identify the affected mode. They do not make the mode
an event or require a new intention for a decision to do nothing. Paused holding
can remain open until a later withdrawal. Goal evaluation remains a separate
canonical judgment, never a renamed stop code.

`IntentionSalienceObservation` is a particular observation with its own decimal
reading, target, vantage and explicit source frame. It avoids the assumptions of
claim import importance and avoids inventing numeric bounds merely to create a
`ScoreScale`. An opaque source convention is retained as an `InformationObject`;
its unknown arithmetic remains unknown. No scale-conversion engine is implied.

## Canonical obligations

The module authors `logic:` declarations, restrictions, explicit closure policies
and realized integrity formulas (Principles 4 and 17). It authors no separate
OWL/RDFS or validation-shape surface.

| Object or assertion | Authored obligation |
| --- | --- |
| Artifact operation receipt | Required operation, artifact and containing-event paths; an actual attempt or ToolCall supplies the parent. |
| Read receipt | Its `readArtifact` target matches its identified receipt artifact. |
| Budget checkpoint pause | An actual information-object resume point is retained. |
| Planless resumption | The original attempt, resume point and sub-event context are explicit. |
| Intention transition | The affected Intention is identified; supersession names its replacement. |
| Satisfied intention transition | A separate GoalEvaluation is retained. |
| Salience observation | Intention, decimal value, producer frame and vantage are required. |
| Artifact verification observation | Artifact, method and vantage are required; missing verdicts do not become successes. |
| Held read-after-write | Distinct write and later-read receipts identify the same digest-bearing artifact version with explicit before-or-meets evidence. |
| Practical goal conflict | A canonical symmetry assertion, without transitivity or constitutive opposition. |

Presence requirements use universal restrictions with class-scoped closed-world
entries rather than existential witnesses that would manufacture missing source
objects. Fixed datatype obligations preserve standard strings, decimals and
booleans. Digest roles reuse the existing algorithm-labelled literal convention;
no custom digest datatype or provider is introduced.

A receipt is an actual subordinate activity occurrence, not a free-floating claim
of success. `artifactComplete` expresses the originally requested artifact bit.
The ambiguous local name `receiptComplete` is handled by an explicit semantic
migration, not by inventing a second completeness axis.

## Verification and byte identity

The read-after-write method is an observation method, not a verifier agent or a
successful status. The observation’s result reuses existing evaluation verdicts.
It does not use the Attestation-domain `verificationResult` property, which would
incorrectly classify an observation or activity as an attestation.

For a held verdict, the actual read and write are different occurrences. Both
refer to the same exact artifact version, whose digest identifies its bytes. A
mutable filename alone cannot stand for that version. The result returned by a
tool can be a report envelope; its `resultDigest` remains distinct from artifact
identity. A matching report hash alone therefore cannot confirm a write.

Explicit source ordering permits both a strict `before` gap and abutting `meets`.
It is not inferred from absent dates, a call identifier or a receipt’s list order.
The model does not execute a reader, materialize a checkpoint or calculate a goal
verdict from recorded metadata.

## Ownership and correspondence

This core slice consumes other core foundations by reference. It does not relax
ToolCall’s existing SoftwareAgent requirement or borrow enactment-only continuation
kinds for planless work. Minimally described tool agents preserve source evidence
without a fabricated holder or universal name-to-agent identity rule.

The slice-owned native close match from `used` to `prov:used` carries the ordinary
unqualified activity-to-entity relation. It claims no lossless qualified-usage,
role or usage-time projection. Mapping-set annotation coats remain in the module,
with their paired translations, while the linkage and its curation metadata live
in `mappings/equivalences.ttl`.
