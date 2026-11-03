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

## Measured class-weight evidence

QueryFusionWeightFit retains source-reported fitted class weights against one
exact judged-content resource. weightFitJudgedSet identifies that content;
fittedClassWeight retains a nonempty collection of supplied quantitative values.
Each QueryFusionClassWeight is a math:Quantity, not an information record,
probability, confidence value or fitted-model artifact. It names one source
retrieval-class resource through weightForClass and carries its magnitude through
math:quantityValue with math:hasDimension math:dimensionless.

The magnitude is one positive exact xsd:decimal value. The class and dimension
coordinates are also required and single-valued; another dimension, a missing
value, zero or a negative magnitude does not satisfy the selected description.
No parallel numeric weight property or lexical-number field is introduced.
Equal values for different classes do not identify the classes or quantities.
Within a fit, one class selects one quantity identity.

fusionWeightFit associates a QueryProgramNode with supplied evidence. It is
optional and nonfunctional: one operation can carry several source fit records.
It does not assert that the node applied those values, choose a default fit or
change ReciprocalRankFusion's unweighted integer-quantization contract.
Unmeasured unit coefficients are not fabricated fitted measurements.

Fits retain their own identities and judged sets even when they share a supplied
quantity carrier. No unique fit owner is imposed on a quantity. Preserve source
attribution through existing provenance relations when actually supplied; the
fit record is not thereby an Observation, proof of fitting, authentication of
the judged contents or a claim of held-out generalization.

This fictional graph records supplied evidence. Both fits explicitly report
the same two quantities against different judged-content identities; neither
fit is inferred from the other or from equal numeric values. The node records
an evidence association only, with no execution or weighted-fusion assertion.

```turtle
@prefix ex: <https://example.org/fit/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:node a gmeow:QueryProgramNode ;
    gmeow:queryNodeEffectful false ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:fusionWeightFit ex:fitA, ex:fitB .

ex:fitA a gmeow:QueryFusionWeightFit ;
    gmeow:weightFitJudgedSet ex:judgedA ;
    gmeow:fittedClassWeight ex:lexicalWeight, ex:spaceWeight .
ex:fitB a gmeow:QueryFusionWeightFit ;
    gmeow:weightFitJudgedSet ex:judgedB ;
    gmeow:fittedClassWeight ex:lexicalWeight, ex:spaceWeight .
ex:judgedA a gmeow:InformationObject .
ex:judgedB a gmeow:InformationObject .

ex:lexicalWeight a gmeow:QueryFusionClassWeight ;
    gmeow:weightForClass ex:lexicalClass ;
    math:quantityValue "1.00"^^xsd:decimal ;
    math:hasDimension math:dimensionless .
ex:spaceWeight a gmeow:QueryFusionClassWeight ;
    gmeow:weightForClass ex:spaceClass ;
    math:quantityValue "2.00"^^xsd:decimal ;
    math:hasDimension math:dimensionless .
```

The class resources identify retrieval families or spaces and are not typed as
ontology classes. The supplied exact decimals are coefficients, not normalized
probabilities. This account specifies neither a fitting algorithm nor a claim
that any consumer implemented the representation or applied the weights.

## Candidate union with original class lists

QueryCandidateUnion specializes QueryLedgerMerge. Use its existing
mergeAnswerInput and mergeAnswerOutput relations: selected inputs are original
RankedQueryList resources, and the single output is a QueryUnionAnswer. The
inherited contract requires at least one input and retains the exact candidate
and original loss identity unions. Empty evaluated input lists and empty ledgers
remain valid; an unavailable retrieval family is not replaced by an invented
successful empty answer. There is no zero-input union contract.

unionClassList retains exactly those original input list resources on the linked
output. Their original rankedQueryEntry occurrences, rankedEntryCandidate values,
queryInputRank values and source queryTieKey values remain attached to the same
lists. A candidate may recur across lists with different ranks and separate
entry identities; it occurs once in the union's candidate set. Neither RDF
serialization order nor the candidate set supplies a global merit order.

rankedListClass retains one supplied retrieval-family or space resource, without
typing it as an ontology class or deriving identity from a display label.
The property is optional on ordinary RankedQueryList records and has an
at-most-one bound there. Only inputs selected by a QueryCandidateUnion require
the binding. Different lists can name the same class, retaining their separate
source contexts and identities; no one-list-per-class rule is imposed.

The union stage supplies no aggregate fusion score, global rank or later
ordering policy. Class-local native scores retained as source evidence remain
distinct from an aggregate score; they are not deleted or rewritten to zero.
The original loss nodes retain their causes, affected resources and any supplied
acceptance attribution. Union membership changes no answer verdict or
completeness axis, repairs no loss and selects no weighted arithmetic.

This fictional graph supplies two distinct lists for the same class and an
explicitly empty evaluated list for another class. The shared candidate keeps
two original entry occurrences with different ranks and tie keys. The same loss
resource appears in both contributing ledgers and the output ledger.

```turtle
@prefix ex: <https://example.org/union/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:union a gmeow:QueryCandidateUnion ;
    gmeow:queryNodeEffectful false ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:mergeAnswerInput ex:listA, ex:listB, ex:emptyList ;
    gmeow:mergeAnswerOutput ex:answer .

ex:listA a gmeow:RankedQueryList ;
    gmeow:queryAnswerFor ex:queryA ;
    gmeow:queryLossLedger ex:ledgerA ;
    gmeow:rankedListClass ex:classOne ;
    gmeow:queryCandidate ex:shared, ex:extra ;
    gmeow:rankedQueryEntry ex:entryA1, ex:entryA2 .
ex:listB a gmeow:RankedQueryList ;
    gmeow:queryAnswerFor ex:queryB ;
    gmeow:queryLossLedger ex:ledgerB ;
    gmeow:rankedListClass ex:classOne ;
    gmeow:queryCandidate ex:shared ;
    gmeow:rankedQueryEntry ex:entryB1 .
ex:emptyList a gmeow:RankedQueryList ;
    gmeow:queryAnswerFor ex:queryC ;
    gmeow:queryLossLedger ex:emptyLedger ;
    gmeow:rankedListClass ex:classTwo ;
    gmeow:queryEvaluationStatus logic:EvaluationCompleted ;
    gmeow:queryCompletenessStatus logic:CompleteForFragment .

ex:entryA1 a gmeow:RankedQueryEntry ;
    gmeow:rankedEntryCandidate ex:shared ;
    gmeow:queryInputRank "1"^^xsd:positiveInteger ;
    gmeow:queryTieKey "a-shared" .
ex:entryA2 a gmeow:RankedQueryEntry ;
    gmeow:rankedEntryCandidate ex:extra ;
    gmeow:queryInputRank "3"^^xsd:positiveInteger ;
    gmeow:queryTieKey "a-extra" .
ex:entryB1 a gmeow:RankedQueryEntry ;
    gmeow:rankedEntryCandidate ex:shared ;
    gmeow:queryInputRank "4"^^xsd:positiveInteger ;
    gmeow:queryTieKey "b-shared" .

ex:ledgerA a gmeow:QueryLossLedger ; gmeow:ledgerLoss ex:loss .
ex:ledgerB a gmeow:QueryLossLedger ; gmeow:ledgerLoss ex:loss .
ex:emptyLedger a gmeow:QueryLossLedger .
ex:loss a gmeow:QueryLoss ;
    gmeow:lossConcerns ex:sourceInput ;
    gmeow:lossCause gmeow:lossValueMissing .

ex:answer a gmeow:QueryUnionAnswer ;
    gmeow:queryAnswerFor ex:union ;
    gmeow:queryLossLedger ex:outputLedger ;
    gmeow:queryCandidate ex:shared, ex:extra ;
    gmeow:unionClassList ex:listA, ex:listB, ex:emptyList .
ex:outputLedger a gmeow:QueryLossLedger ; gmeow:ledgerLoss ex:loss .
```

The source ranks remain 1, 3 and 4; they are not renumbered into a union-wide
ranking. The explicit empty list remains one of the selected inputs, with its
supplied evaluation/completeness report. This description installs no executor,
certifies no source outcome and claims no consumer adoption of the graph.

## Finite reported rerank scores

QueryRerankScore retains a supplied finite IEEE 754 single precision score as a
math:Quantity. Its magnitude is one xsd:float math:quantityValue. It also requires
one math:hasDimension, one actual hasReferenceFrame and one rerankScoreAuthority.
Finite negative values and zero are legal; this is not a positive-weight or
probability profile. Selecting this carrier requires every coordinate. A missing
coordinate, NaN, infinity or a different numeric datatype fails the selected
profile rather than producing a replacement value or an unscored outcome.

QueryRerankAuthority is the separately identified information record for the
supplied scoring context. rerankAuthorityIdentity preserves opaque source bytes
as xsd:hexBinary; rerankScoreKind retains exact provider wording as xsd:string.
Both specialize lang:literalValue without interpreting their lexical forms as
quantitative magnitudes. No identity algorithm, universal byte length, model
identity, authentication promise or fabricated semantic IRI follows from the
opaque bytes. Distinct authority records can carry the same bytes.

The authority explicitly supplies rerankScoreDimension and hasReferenceFrame.
Every associated score retains that same dimension through math:hasDimension
and that same frame through hasReferenceFrame. A bare score-kind word or numeric
range supplies neither coordinate. The declared frame is an actual
ReferenceFrame with its own realm, kind, dimension count, axes, determinacy
default and host requirement. Those frame declarations retain their existing
owner contracts; no context string substitutes for the frame resource.

This fictional graph supplies a dimensionless scalar context explicitly. Its
provider wording does not turn either score into a probability or a log-odds
value. The frame's crisp default describes the declared value frame; it is not
a claim that relevance is true or that the scores express confidence. Each of
the two authority records retains the same opaque bytes without a rule merging
their identities.

```turtle
@prefix ex: <https://example.org/rerank-score/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:scoreFrame a gmeow:ReferenceFrame ;
    gmeow:frameRealm gmeow:frameRealmMathematical ;
    gmeow:frameKind gmeow:frameKindScalar ;
    gmeow:dimensionCount "1"^^xsd:nonNegativeInteger ;
    gmeow:hasAxis ex:scoreAxis ;
    gmeow:determinacyModel gmeow:determinacyCrisp ;
    gmeow:requiresHost false .
ex:scoreAxis a gmeow:Axis .

ex:authorityA a gmeow:QueryRerankAuthority ;
    gmeow:rerankAuthorityIdentity "00a1"^^xsd:hexBinary ;
    gmeow:rerankScoreKind "relevance-logit" ;
    gmeow:rerankScoreDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scoreFrame .
ex:authorityB a gmeow:QueryRerankAuthority ;
    gmeow:rerankAuthorityIdentity "00a1"^^xsd:hexBinary ;
    gmeow:rerankScoreKind "relevance-logit" ;
    gmeow:rerankScoreDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scoreFrame .

ex:scoreA a gmeow:QueryRerankScore ;
    math:quantityValue "-1.25"^^xsd:float ;
    math:hasDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scoreFrame ;
    gmeow:rerankScoreAuthority ex:authorityA .
ex:scoreB a gmeow:QueryRerankScore ;
    math:quantityValue "0"^^xsd:float ;
    math:hasDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scoreFrame ;
    gmeow:rerankScoreAuthority ex:authorityB .
```

The score literal retains the reported numeric precision, not a claim about
source lexical spelling, byte identity or an exact real target. ApproximateValue
would require an identified approximation target and error account; neither is
implied by a reported floating-point score. A declared source dimension and frame
do not prove cross-provider comparability, model quality or calibration.

This selected finite carrier does not certify every provider interface: a
broader interface may admit values outside it. The record itself claims no
provider invocation or current consumer adoption. It also supplies no rerank
node, candidate coverage, scored/unscored result, source order or loss-retention
contract. Those stage obligations require their own explicit result model.
