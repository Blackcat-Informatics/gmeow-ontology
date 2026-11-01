<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Query program graphs

A `QueryProgramGraph` describes a consumer-owned query operation. It records
operators, their admission contract and attributed execution evidence. Declaring
one installs no executor, SPARQL parser or native certification procedure.
Katamari and memory retrieval adapters can describe their existing execution with
these terms while retaining GMEOW's canonical meanings (Principles 4, 11, 12 and 17).

## Graph scope and effects

`queryGraphNode` bounds the graph. Nodes have an explicit boolean
`queryNodeEffectful` and nonnegative `queryNodeStratum`; absent classification
never means purity. `queryGraphEdge` references `logic:DataFlowEdge` records:
`logic:flowFrom` is producer, `logic:flowTo` is consumer. The corresponding binary
edge is `logic:dataflowConsumes` from consumer to producer. Every declared node
dependency needs its classified graph edge. Branch operands and loop bodies stay
inside the same graph.

Positive monotone edges (`queryEdgeMonotone true`) may have a producer at the
same or a lower stratum. Negative or nonmonotone edges require a strictly lower
producer stratum. The selected `queryStratifiedFeedbackContract` names these
admission obligations. Strata classify dependency semantics, not clocks.

Every effectful node must be a designated `queryGraphSink`. Every sink belongs
to the graph and has no downstream consumer inside it. Answer rows, a loss ledger
and an audit record can therefore have distinct sinks. A source can also designate
a pure terminal output. No rule changes the build pipeline's single serialization
exit or its `sinkCapability`.

## Reciprocal rank fusion

A `RankedQueryList` is an attributed `QueryAnswer`. It keeps its required
`queryAnswerFor` and `queryLossLedger`. Each `RankedQueryEntry` belongs to exactly
one list occurrence, names its `rankedEntryCandidate`, retains the positive
`queryInputRank` reported by the source, and has a unique list-local `queryTieKey`.
A candidate occurring in two lists has two entry occurrences. This prevents
per-list evidence from collapsing merely because candidate identities match.

Ranks are inputs, not positions reconstructed from RDF serialization. Equal ranks
stay tied. Truncated lists keep the original retained ranks. Codepoint ordering of
the source strings supplies stable presentation among equal input ranks without
renumbering them. Duplicate candidates and duplicate tie keys within one list are
invalid.

A `ReciprocalRankFusion` selects lists through `fusionInput`, an answer through
`fusionOutput`, a positive integer `fusionScale` S and a nonnegative
`fusionRankOffset` k. For every selected entry of rank r, exactly one
`FusionContribution` retains the entry and its nonnegative integer q:

```text
q × (k + r) ≤ S < (q + 1) × (k + r)
```

Thus q is floor(S / (k + r)). Quantize each contribution before adding; rounding
the final rational sum instead would define a different operation. Integer
addition, multiplication and comparison use the existing finite RDF numeric
relations. No recurring decimal division, floating approximation, weight or
confidence interpretation is introduced. An unavailable family is an explicit
loss, not an invented empty list that seems to have run successfully.

Output candidates are exactly the union of selected input candidates. Each has
one `QueryFusionScore`. Its `fusionScoreValue` is the finite sum of all matching
per-list contributions. The canonical finite fold retains `scoreFoldStep`
records: step zero has index and total zero, each positive step has the previous
index plus one and adds one contribution, and `scoreFinalStep` bounds the steps
and supplies the final total. Contribution coverage and uniqueness prevent
omitted, repeated or unrelated additions.

Equal final scores remain tied in relevance. The unique, source-stable
`fusionTieOrder` gives their deterministic ascending display order without changing
scores. A producer keeps this ordinal stable for the same selected input and
candidate identities; it is not a global ranking or a preferred truth claim.

## Loss-preserving merge

`QueryLedgerMerge` names its input answers with `mergeAnswerInput` and output
with `mergeAnswerOutput`. Candidate membership is their exact identity union.
Loss membership follows `queryLossLedger / ledgerLoss`, also as an exact union.
The output refers to the original `QueryLoss` resources; it does not copy just
cause labels into newly minted records or merge distinct reports sharing a cause.
Their concerned resources, affected constraints and acceptance attribution remain
attached to the same identities. Acknowledgement never repairs or waives a loss.
Empty input ledgers remain valid. No merged completeness or information verdict
is silently synthesized from candidate or ledger membership.

## Verdict choice

`QueryVerdictChoice` binds `choiceAnswer`, `choiceAxis` and
`choiceExpectedValue`. The axis is the actual `queryInformationState` or
`queryCompletenessStatus` predicate. The expected value belongs to that axis's
existing closed vocabulary. There is no cross-axis coercion.

The left operand is the matching branch; the right operand is the nonmatching
branch. `choiceUnknownBranch` is explicit and mandatory. An absent value,
`InfoUndetermined`, or `CompletenessUnknown` selects that unknown branch.
`InfoNotEvaluated` is an explicit comparable value: an expected not-evaluated
verdict can select the lexical branch in the ordinary matching case. The unknown
sentinels themselves are not accepted as expected values.

A `QueryNodeExecution` names the choice through `executesQueryNode` and retains
`querySelectedBranch`. The selected-branch law checks the corresponding case.
This is a new answer-verdict operator, not a changed meaning of situation-guarded
`logic:Choice`. Different attributed answers remain different observations.

## Finite monotone feedback

`ConvergentQueryIteration` specializes `logic:Iteration` and `QueryProgramNode`.
It retains `logic:iterationBody`, its situation-valued `logic:iterationCondition`,
`iterationUniverse`, `queryConvergenceCriterion queryFiniteSetEquality` and the
selected admission contract. The condition describes continuing expansion while
a fixed point has not been established. The universe's `queryCandidate` membership
is the complete, finite bound selected for this operation. The body and loop are
explicitly pure. The body is an extensional candidate-set transformer: equal
candidate input sets produce equal candidate output sets under this description.
Changes of dataset or context therefore require a different body description.

The execution record retains seed state zero, every completed intermediate state,
`queryIterationCount`, `queryIterationFinalState` and
`queryIterationTermination`. Count means completed body executions. A partially
executed next pass does not increment it. Each positive-index state is the output
of an actual body `QueryNodeExecution` via `wasGeneratedBy`, with exactly the previous
state retained as `queryExecutionInput`. One body execution cannot witness two
positive state indices in the same run. State membership stays inside the finite
universe and never shrinks.

`queryIterationConverged` requires at least one completed pass and equality of the
last two candidate sets. `queryIterationBudgetStopped` and
`queryIterationUnfinished` retain the last completed state without certifying a
fixed point. State zero can be final for these unfinished cases. An initially
empty seed is still compared through a body pass before a converged claim.
These records describe the consumer's evidence; declarations alone prove no run
terminated and carry no fabricated native certificate.

## SPARQL selection and effective principal

`SparqlSelectionLeaf` retains verbatim `selectionText`,
`selectionLanguage querySparqlSelect`, the selected `selectionDataset` and
`selectionContext`, and explicit `selectionInput` and `selectionOutput`
descriptors. Empty inputs and outputs have descriptors too. The pure SPARQL 1.2
SELECT language contract excludes UPDATE and SERVICE; PREFIX and BASE declarations
in the text fix name resolution. Relative names need the text's BASE declaration.
Input bindings and output row descriptors describe the operation, not a claim
that any rows have already been produced.

`selectionNodeKind logic:Query` references the existing NodeKind individual.
It does not type the leaf as an instance of a nonexistent Query class and does
not repurpose Formula-valued `logic:programQuery`. The consumer must parse and
admit the text against the selected language, dataset and context before executing
it. The ontology supplies this explicit boundary; its structural constraints do
not certify arbitrary text as valid SPARQL. No regex is presented as a parser.

`effectiveEnactmentAgent` records the Agent under whose effective authority an
existing `logic:Enactment` ran. It is not a default from recorder, provider or tool
identity. It supplies an auditable principal assertion, not an authorization
certificate, and changes none of Enactment's prescription or snapshot obligations.

The [retrieval scene](examples/retrieval.ttl) combines ranked evidence and a loss
without erasing either. The [feedback scene](examples/feedback.ttl) retains actual
body passes and a measured fixed point. See the [design rationale](design/QUERY-PROGRAM.md)
for the canonical laws and source limits.
