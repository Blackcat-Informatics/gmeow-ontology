<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Query context

A query answer can preserve what it found and the inputs it could not reach.
Separately, a query can describe precisely which contextual interpretation it
uses when combining frames. This slice retains those two kinds of information
without selecting a privileged standpoint or manufacturing a complete answer.

## Keep the loss itself

`QueryAnswer` is an attributed information record, linked to its declared query
resource through `queryAnswerFor` and to one or more `QueryLossLedger` objects
through `queryLossLedger`. It is not automatically a `logic:ReasoningResult`.
The query resource need not have an invented query-language class.

`ledgerLoss` keeps each original `QueryLoss` resource. A loss identifies what it
`lossConcerns`, one or more `lossCause` values, and the affected `lossConstraint`
when the source names it. The constraint can be an ordinary query-condition
resource; it need not be a grounding integrity-rule node.

The cause vocabulary is open and supplies five initial distinctions:

- `lossValueMissing`: an input value was missing.
- `lossVaultLocked`: a relevant protected source was locked to this context.
- `lossCoverageEnds`: available coverage ended before the requested scope.
- `lossFamilyCouldNot`: a participating retrieval family could not evaluate.
- `lossPrecisionCoarser`: available precision was coarser than requested.

These are reports of query-input limitations, not projection-loss codes.
`logic:projectionLoss` describes a different loss surface and is not reused.
A locked source does not grant access; coarse input does not acquire fabricated
detail; missing content does not become false.

`lossAcceptedBy` identifies a reader acknowledging the report. The same loss
resource remains in a ledger after acknowledgement. That relation neither
repairs the cause nor erases it, changes a result status or authorizes access.
Several readers and source reports may coexist (Principle 9).

An empty ledger has no listed losses in its reporting scope. It does not prove
that every input was available, that evaluation completed or that no relevant
facts exist outside that scope.

## Three independent result axes

The local answer fields reuse the existing grounding value vocabularies directly:

| Field | Meaning | Vocabulary |
| --- | --- | --- |
| `queryInformationState` | Verdict about the queried proposition | `logic:InformationState` |
| `queryEvaluationStatus` | What computation was completed or could be performed | `logic:EvaluationStatus` |
| `queryCompletenessStatus` | Completeness relative to declared scope or fragment | `logic:CompletenessStatus` |

Undetermined and not-evaluated remain information states. They are not completeness
values. A supported answer with incomplete coverage remains provisional; a
conclusive neither verdict differs from a verdict never reached. No field is
inferred from another or from reader acknowledgement.

Each supplied axis has at most one value on an individual attributed answer.
Independent source or context reports use separate answer records. Missing values
remain absent, rather than being filled with successful or negative defaults.

These descriptive records do not authorize silent capability degradation.
A selected operation still requires its declared inputs, capabilities and outputs.
The ledger does not relax a pipeline sink contract or permit an executor to
substitute an undeclared weaker operation.

## Declare the interpretation before combining frames

`QueryFrameInterpretation` identifies an explicit `interpretationScope` and domain
of `interpretationMember` evaluation contexts. A context is referenced as a
`logic:World`, without asserting the alethic `logic:PossibleWorld` subtype.

A `QueryFrameExtent` records a frame’s admitted contexts through `extentMember`
in its `extentInterpretation`. `extentFrame` identifies the represented Standpoint
or Scenario when one is supplied. Extents from different interpretations cannot
silently be combined. In an explicitly complete interpretation, separate extent
records naming the same `extentFrame` must have identical memberships.
`ExtentFrameCongruenceLaw` enforces both directions while preserving the
records’ separate identities.

`interpretationComplete` is an explicit source assertion that the domain and
recorded extents form the complete interpretation for the named scope. It is not
inferred from sample size and is not a runtime-verified certificate. A finite
sample of a larger semantic universe does not establish full intersection or
union there. All set equations below are relative to the declared complete model.

## Three different compositions

`FrameComposition` is a binary descriptor with `compositionInterpretation`,
`compositionLeft` and `compositionRight`. Use the concrete kind that matches the
requested meaning:

| Kind | Result extent | Meaning |
| --- | --- | --- |
| `StandpointMeetComposition` | Intersection of admitted precisifications | Contexts admitted by both frames |
| `StandpointTruthUnionComposition` | Union of admitted precisifications | Domain needed for universal truth throughout both frames |
| `ScenarioConjunctionComposition` | Intersection of satisfying scenario contexts | Jointly entertaining both suppositions |

For standpoint extents A and B, the meet admits A ∩ B. Universal truth on that
smaller domain can hold even when truth does not hold throughout A or B.
To require a proposition true throughout both inputs, its universal truth must
cover A ∪ B: truth at every member of the union is equivalent to truth at every
member of A and at every member of B. The union descriptor itself asserts no
proposition true.

Scenario conjunction uses an intersection of satisfying contexts but remains a
separate typed operation. Entertaining the joint supposition neither adopts a
standpoint nor asserts actuality or alethic possibility. There is no generic
cross-type accessibility inference.

## A descriptor is not a realized usable frame

Minting a descriptor supplies no result, compatibility flag or usable frame.
A source can explicitly retain `compositionResult`, `compositionRealized` and
`compositionCompatible`. A realization claim requires a result in the same
source-asserted complete interpretation. The extensional laws require exact
membership in both directions, rather than merely a common witness.

A compatible result is nonempty. An incompatible result is an explicit empty
extent, retained without a usable frame or a vacuous universal-truth promotion.
Only an explicitly asserted `compositionFrame` exposes a represented result,
with realized, compatible and nonempty evidence in that interpretation.

Standpoint inputs must be nonempty represented standpoints. The unspecified
default standpoint stays outside the sharpening order. Usable standpoint results
remain standpoints; scenario results remain scenarios. An empty joint scenario
can still be described and its incompatibility retained without producing a usable
query frame.

The existing `sharpens` poset does not guarantee meet existence. A nonempty exact
intersection is a meet in the admitted-set interpretation when it is represented
by an allowed frame. This source model does not automatically create global
`sharpens` edges or certify a meet across every possible semantic interpretation.
Unindexed statements likewise remain unspecified, not universal.

The [archive scene](examples/archive-query.ttl) retains acknowledged losses and a
small explicitly scoped composition model. The [design rationale](design/QUERY-CONTEXT.md)
states the canonical laws and the limits of their interpretation.
