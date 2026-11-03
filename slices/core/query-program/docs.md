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

## Complete finite reported rerank stages

QueryRerank describes an operation over one original QueryUnionAnswer and one
reported QueryRerankAnswer. Both explicitly select rerankFiniteFloatProfile
through rerankScoreProfile. The operation's rerankInput and the answer's
rerankedUnion name the same original resource. A description does not assert a
provider invocation or execution occurrence.

The result retains exactly the input candidate identities through queryCandidate
and exactly one QueryRerankEntry per candidate. Every entry belongs to one
answer and retains its original union position through rerankInputPosition and
its supplied output position through entryOrdinal. Positions are nonnegative
and unique within each answer; no contiguous indices or source count are
invented. Original unionClassList resources, class entry occurrences, ranks,
tie keys and associated source evidence remain on the original lists.

Each entry has exactly one outcome branch. A scored entry retains a
QueryRerankScore and its supplied rerankWinningPassage. An unscored entry
retains an actual StructuredValue through rerankUnscoredDetail, with no score
or winning passage. Structured record tags and fields remain data through
structureName, hasFacet and the native payload relations. An unscored detail
does not automatically become an aggregate QueryLoss.

Every scored answer retains rerankAuthority, and each score's direct
rerankScoreAuthority references that same actual authority record. The existing
score contract requires its finite xsd:float magnitude, source-declared
dimension and actual frame to agree with that authority. Equal opaque identity
bytes on different records do not satisfy answer-to-score authority agreement.
Missing context or a rejected numeric value fails the selected profile; it is
never silently dropped, replaced or relabeled as an unscored source outcome.

Every original ledger loss survives by resource identity in an output ledger;
additional stage losses may coexist. An answer with an unscored entry cannot
report logic:CompleteForFragment. Other status evidence remains source supplied,
and an absent status is not replaced with an inferred value.

Canonical order checking has two integer-only obligations: all scored entries
precede all unscored entries, and unscored entries retain their relative input
order. Scored numeric-descending order, equal-score ties and the source's
floating-point total comparator remain reported source behavior outside these
checks. No ordinary numeric comparison is presented as a proof of signed-zero
sorting, lexical rendering or bit identity.

The fictional graph below retains four original candidates and their original
class-list occurrences. The supplied output has two equal finite scores, with
both winning passages retained, followed by two unscored candidates in original
relative order. Its explicit dimensionless scalar frame and provider wording
supply context without inferring a probability or confidence interpretation.
The frame's crisp default is a frame declaration, not a relevance truth claim.

```turtle
@prefix ex: <https://example.org/rerank-stage/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:classList a gmeow:RankedQueryList ;
    gmeow:queryAnswerFor ex:query ;
    gmeow:queryLossLedger ex:inputLedger ;
    gmeow:rankedListClass ex:lexicalClass ;
    gmeow:queryCandidate ex:a, ex:b, ex:c, ex:d ;
    gmeow:rankedQueryEntry ex:classA, ex:classB, ex:classC, ex:classD .
ex:classA a gmeow:RankedQueryEntry ;
    gmeow:rankedEntryCandidate ex:a ;
    gmeow:queryInputRank "1"^^xsd:positiveInteger ;
    gmeow:queryTieKey "a" .
ex:classB a gmeow:RankedQueryEntry ;
    gmeow:rankedEntryCandidate ex:b ;
    gmeow:queryInputRank "1"^^xsd:positiveInteger ;
    gmeow:queryTieKey "b" .
ex:classC a gmeow:RankedQueryEntry ;
    gmeow:rankedEntryCandidate ex:c ;
    gmeow:queryInputRank "3"^^xsd:positiveInteger ;
    gmeow:queryTieKey "c" .
ex:classD a gmeow:RankedQueryEntry ;
    gmeow:rankedEntryCandidate ex:d ;
    gmeow:queryInputRank "4"^^xsd:positiveInteger ;
    gmeow:queryTieKey "d" .

ex:union a gmeow:QueryUnionAnswer ;
    gmeow:queryAnswerFor ex:query ;
    gmeow:queryLossLedger ex:inputLedger ;
    gmeow:unionClassList ex:classList ;
    gmeow:queryCandidate ex:a, ex:b, ex:c, ex:d .
ex:inputLedger a gmeow:QueryLossLedger ;
    gmeow:ledgerLoss ex:originalLoss .
ex:originalLoss a gmeow:QueryLoss ;
    gmeow:lossConcerns ex:sourceGraph ;
    gmeow:lossCause gmeow:lossCoverageEnds .

ex:stage a gmeow:QueryRerank ;
    gmeow:queryNodeEffectful false ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:rerankScoreProfile gmeow:rerankFiniteFloatProfile ;
    gmeow:rerankInput ex:union ;
    gmeow:rerankOutput ex:answer .
ex:answer a gmeow:QueryRerankAnswer ;
    gmeow:queryAnswerFor ex:query ;
    gmeow:queryLossLedger ex:outputLedger ;
    gmeow:queryCompletenessStatus logic:Incomplete ;
    gmeow:rerankScoreProfile gmeow:rerankFiniteFloatProfile ;
    gmeow:rerankedUnion ex:union ;
    gmeow:rerankAuthority ex:authority ;
    gmeow:queryCandidate ex:a, ex:b, ex:c, ex:d ;
    gmeow:rerankEntry ex:outB, ex:outD, ex:outA, ex:outC .
ex:outputLedger a gmeow:QueryLossLedger ;
    gmeow:ledgerLoss ex:originalLoss, ex:stageLoss .
ex:stageLoss a gmeow:QueryLoss ;
    gmeow:lossConcerns ex:a ;
    gmeow:lossCause gmeow:lossFamilyCouldNot ;
    gmeow:lossDetail ex:unscoredA .

ex:scoreFrame a gmeow:ReferenceFrame ;
    gmeow:frameRealm gmeow:frameRealmMathematical ;
    gmeow:frameKind gmeow:frameKindScalar ;
    gmeow:dimensionCount "1"^^xsd:nonNegativeInteger ;
    gmeow:hasAxis ex:scoreAxis ;
    gmeow:determinacyModel gmeow:determinacyCrisp ;
    gmeow:requiresHost false .
ex:scoreAxis a gmeow:Axis .
ex:authority a gmeow:QueryRerankAuthority ;
    gmeow:rerankAuthorityIdentity "00a1"^^xsd:hexBinary ;
    gmeow:rerankScoreKind "relevance-logit" ;
    gmeow:rerankScoreDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scoreFrame .
ex:scoreB a gmeow:QueryRerankScore ;
    math:quantityValue "-1.25"^^xsd:float ;
    math:hasDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scoreFrame ;
    gmeow:rerankScoreAuthority ex:authority .
ex:scoreD a gmeow:QueryRerankScore ;
    math:quantityValue "-1.25"^^xsd:float ;
    math:hasDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scoreFrame ;
    gmeow:rerankScoreAuthority ex:authority .

ex:outB a gmeow:QueryRerankEntry ;
    gmeow:rerankEntryCandidate ex:b ;
    gmeow:rerankInputPosition "1"^^xsd:nonNegativeInteger ;
    gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger ;
    gmeow:rerankScore ex:scoreB ;
    gmeow:rerankWinningPassage "A retained passage for candidate b." .
ex:outD a gmeow:QueryRerankEntry ;
    gmeow:rerankEntryCandidate ex:d ;
    gmeow:rerankInputPosition "3"^^xsd:nonNegativeInteger ;
    gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger ;
    gmeow:rerankScore ex:scoreD ;
    gmeow:rerankWinningPassage "A retained passage for candidate d." .
ex:outA a gmeow:QueryRerankEntry ;
    gmeow:rerankEntryCandidate ex:a ;
    gmeow:rerankInputPosition "0"^^xsd:nonNegativeInteger ;
    gmeow:entryOrdinal "2"^^xsd:nonNegativeInteger ;
    gmeow:rerankUnscoredDetail ex:unscoredA .
ex:outC a gmeow:QueryRerankEntry ;
    gmeow:rerankEntryCandidate ex:c ;
    gmeow:rerankInputPosition "2"^^xsd:nonNegativeInteger ;
    gmeow:entryOrdinal "3"^^xsd:nonNegativeInteger ;
    gmeow:rerankUnscoredDetail ex:unscoredC .

ex:unscoredA a gmeow:StructuredMap ;
    gmeow:structureName "NoWindowScored" ;
    gmeow:hasFacet ex:firstRefusal .
ex:firstRefusal a gmeow:LabelledFacet ;
    gmeow:facetLabel "first" ;
    gmeow:facetStructuredValue ex:wrongCount .
ex:wrongCount a gmeow:StructuredMap ;
    gmeow:structureName "WrongCount" ;
    gmeow:hasFacet ex:scoreCount, ex:passageCount .
ex:scoreCount a gmeow:LabelledFacet ;
    gmeow:facetLabel "scores" ;
    gmeow:facetValue "0"^^xsd:unsignedLong .
ex:passageCount a gmeow:LabelledFacet ;
    gmeow:facetLabel "passages" ;
    gmeow:facetValue "1"^^xsd:unsignedLong .
ex:unscoredC a gmeow:StructuredMap ;
    gmeow:structureName "NoClassMatched" .
```

The equal-score order is retained as reported; it is not proved by the integer
order law. The original list's tied positive ranks and distinct tie keys remain
unchanged. The shared detail resource is explicitly supplied as both an outcome
cause and stage loss detail; no rule infers those identities from similar text.

An all-unscored answer may retain every candidate without an authority. This
compact fictional example supplies its cause and reported incomplete status
explicitly. It does not manufacture an authority from the cause text. Prefixes
are the same as above.

```turtle
ex:unavailableUnion a gmeow:QueryUnionAnswer ;
    gmeow:queryAnswerFor ex:otherQuery ;
    gmeow:queryLossLedger ex:unavailableInputLedger ;
    gmeow:queryCandidate ex:x .
ex:unavailableInputLedger a gmeow:QueryLossLedger .
ex:unavailableStage a gmeow:QueryRerank ;
    gmeow:queryNodeEffectful false ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:rerankScoreProfile gmeow:rerankFiniteFloatProfile ;
    gmeow:rerankInput ex:unavailableUnion ;
    gmeow:rerankOutput ex:unavailableAnswer .
ex:unavailableAnswer a gmeow:QueryRerankAnswer ;
    gmeow:queryAnswerFor ex:otherQuery ;
    gmeow:queryLossLedger ex:unavailableOutputLedger ;
    gmeow:queryCompletenessStatus logic:Incomplete ;
    gmeow:rerankScoreProfile gmeow:rerankFiniteFloatProfile ;
    gmeow:rerankedUnion ex:unavailableUnion ;
    gmeow:queryCandidate ex:x ;
    gmeow:rerankEntry ex:unavailableEntry .
ex:unavailableOutputLedger a gmeow:QueryLossLedger ;
    gmeow:ledgerLoss ex:unavailableLoss .
ex:unavailableLoss a gmeow:QueryLoss ;
    gmeow:lossConcerns ex:x ;
    gmeow:lossCause gmeow:lossFamilyCouldNot ;
    gmeow:lossDetail ex:noReranker .
ex:unavailableEntry a gmeow:QueryRerankEntry ;
    gmeow:rerankEntryCandidate ex:x ;
    gmeow:rerankInputPosition "0"^^xsd:nonNegativeInteger ;
    gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger ;
    gmeow:rerankUnscoredDetail ex:noReranker .
ex:noReranker a gmeow:StructuredMap ;
    gmeow:structureName "NoReranker" ;
    gmeow:hasFacet ex:noRerankerWhy .
ex:noRerankerWhy a gmeow:LabelledFacet ;
    gmeow:facetLabel "why" ;
    gmeow:facetStructuredValue ex:notShared .
ex:notShared a gmeow:StructuredMap ;
    gmeow:structureName "NotShared" .
```

An empty original candidate set also admits an empty result. This final minimal
standalone union supplies no class-list inventory and asserts no status from
emptiness. The node and answer still explicitly select their profile and retain
the required query, original union and ledgers.

```turtle
ex:emptyUnion a gmeow:QueryUnionAnswer ;
    gmeow:queryAnswerFor ex:emptyQuery ;
    gmeow:queryLossLedger ex:emptyInputLedger .
ex:emptyInputLedger a gmeow:QueryLossLedger .
ex:emptyStage a gmeow:QueryRerank ;
    gmeow:queryNodeEffectful false ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:rerankScoreProfile gmeow:rerankFiniteFloatProfile ;
    gmeow:rerankInput ex:emptyUnion ;
    gmeow:rerankOutput ex:emptyAnswer .
ex:emptyAnswer a gmeow:QueryRerankAnswer ;
    gmeow:queryAnswerFor ex:emptyQuery ;
    gmeow:queryLossLedger ex:emptyOutputLedger ;
    gmeow:rerankScoreProfile gmeow:rerankFiniteFloatProfile ;
    gmeow:rerankedUnion ex:emptyUnion .
ex:emptyOutputLedger a gmeow:QueryLossLedger .
```

These records retain supplied evidence without proving source authentication,
provider execution, passage-selection exhaustiveness, current consumer adoption
or a floating-point sorting implementation. The separate content-tier and
activation order policy, accepted-reason folding and verdict-choice semantics
remain governed by their own contracts.

## Explicit query-order definitions

QueryOrderPolicy defines a complete comparison prescription under the selected
unique-predicate and source-standard-direction contract. Its orderCriterion
occurrences retain precedence through entryOrdinal, key kind through
orderCriterionKind and explicit typed boolean direction through orderDescending.
Every policy includes fused score and a final source subject tie. Content tier
and snapshot activation are included explicitly when selected.

The admitted directions are ascending content tier, descending fused score,
descending activation and ascending source subject tie. A missing direction is
invalid rather than inferred from key kind. Positions and key kinds are unique
within the policy, and each criterion occurrence belongs to one policy only.
Positions need not be contiguous. These integers describe control precedence;
they are not measured quantities or linear-map ranks.

A content-tier criterion retains QueryPredicateTierBinding declarations, each
with an actual predicate IRI and one xsd:unsignedInt tierBindingOrdinal. Smaller
declared labels precede larger ones. A candidate uses the minimum label among
the predicates on its matched literal statements; a candidate without a declared
match follows every declared tier. The declaration never filters such a
candidate or examines every predicate on the subject as a substitute for
matching evidence. An explicitly empty mapping collection gives a constant
undeclared key.

Within one content-tier criterion, a predicate selects one binding resource.
Bindings may be shared across policy definitions, and different predicates may
have equal tier labels. This selected contract rejects duplicate predicate
declarations rather than silently applying a native last-wins vector rule.
It does not claim to qualify every input accepted by a broader native API.

The complete fictional graph below describes three policies. The requested
sequence places score before activation; the examined native sequence places
activation before score. Each uses separate owned criterion occurrences while
sharing the reusable predicate-tier declarations. The minimal definition omits
both leading keys and retains score plus terminal tie, with a permitted gap in
its precedence positions. All booleans are explicitly typed data.

```turtle
@prefix ex: <https://example.org/query-order/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:scoreFirst a gmeow:QueryOrderPolicy ;
    gmeow:orderCriterion ex:sfTier, ex:sfScore, ex:sfActivation, ex:sfTie .
ex:sfTier a gmeow:QueryOrderCriterion ;
    gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger ;
    gmeow:orderCriterionKind gmeow:orderContentTier ;
    gmeow:orderDescending "false"^^xsd:boolean ;
    gmeow:contentTierBinding ex:labelTier, ex:bodyTier .
ex:sfScore a gmeow:QueryOrderCriterion ;
    gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger ;
    gmeow:orderCriterionKind gmeow:orderFusedScore ;
    gmeow:orderDescending "true"^^xsd:boolean .
ex:sfActivation a gmeow:QueryOrderCriterion ;
    gmeow:entryOrdinal "2"^^xsd:nonNegativeInteger ;
    gmeow:orderCriterionKind gmeow:orderActivation ;
    gmeow:orderDescending "true"^^xsd:boolean .
ex:sfTie a gmeow:QueryOrderCriterion ;
    gmeow:entryOrdinal "3"^^xsd:nonNegativeInteger ;
    gmeow:orderCriterionKind gmeow:orderSubjectTie ;
    gmeow:orderDescending "false"^^xsd:boolean .

ex:activationFirst a gmeow:QueryOrderPolicy ;
    gmeow:orderCriterion ex:afTier, ex:afActivation, ex:afScore, ex:afTie .
ex:afTier a gmeow:QueryOrderCriterion ;
    gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger ;
    gmeow:orderCriterionKind gmeow:orderContentTier ;
    gmeow:orderDescending "false"^^xsd:boolean ;
    gmeow:contentTierBinding ex:labelTier, ex:bodyTier .
ex:afActivation a gmeow:QueryOrderCriterion ;
    gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger ;
    gmeow:orderCriterionKind gmeow:orderActivation ;
    gmeow:orderDescending "true"^^xsd:boolean .
ex:afScore a gmeow:QueryOrderCriterion ;
    gmeow:entryOrdinal "2"^^xsd:nonNegativeInteger ;
    gmeow:orderCriterionKind gmeow:orderFusedScore ;
    gmeow:orderDescending "true"^^xsd:boolean .
ex:afTie a gmeow:QueryOrderCriterion ;
    gmeow:entryOrdinal "3"^^xsd:nonNegativeInteger ;
    gmeow:orderCriterionKind gmeow:orderSubjectTie ;
    gmeow:orderDescending "false"^^xsd:boolean .

ex:minimal a gmeow:QueryOrderPolicy ;
    gmeow:orderCriterion ex:minimalScore, ex:minimalTie .
ex:minimalScore a gmeow:QueryOrderCriterion ;
    gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger ;
    gmeow:orderCriterionKind gmeow:orderFusedScore ;
    gmeow:orderDescending "true"^^xsd:boolean .
ex:minimalTie a gmeow:QueryOrderCriterion ;
    gmeow:entryOrdinal "4"^^xsd:nonNegativeInteger ;
    gmeow:orderCriterionKind gmeow:orderSubjectTie ;
    gmeow:orderDescending "false"^^xsd:boolean .

ex:labelTier a gmeow:QueryPredicateTierBinding ;
    gmeow:tierBindingPredicate ex:label ;
    gmeow:tierBindingOrdinal "0"^^xsd:unsignedInt .
ex:bodyTier a gmeow:QueryPredicateTierBinding ;
    gmeow:tierBindingPredicate ex:body ;
    gmeow:tierBindingOrdinal "2"^^xsd:unsignedInt .

ex:node a gmeow:QueryProgramNode ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:queryOrderPolicy ex:scoreFirst, ex:activationFirst, ex:minimal .
```

queryOrderPolicy is an optional nonfunctional description association. The
three definitions do not select an executed policy for the node or alter an
existing fusion operator's arithmetic, source ranks or tie contract. In
particular, the ability to describe score-before-activation does not establish
that the examined activation-first native comparator accepts or applies that
sequence. Neither policy is an automatic canonical default.

No candidate key values, activation snapshots, output order or native policy
digest are supplied by these graphs. The source subject-tie kind does not
synthesize tie bytes or substitute canonical RDF tuple order. A native identity
recipe that encodes only predicate tiers and an activation flag cannot identify
a novel criterion sequence without a separate explicit source contract.

An applied ordering account must separately retain the actually selected
policy, original and output candidates, actual key evidence and losses. Measured
score and activation fields require appropriate math:Quantity carriers with
their supplied datatype, dimension, actual reference frame and source profile.
Activation additionally requires its actual vault and snapshot context; a
missing first snapshot differs from a measured zero. A control-tier label or
criterion position does not acquire those measurement obligations merely by
being an integer.

These definitions assert no source authentication, score calibration, truth,
provider execution, consumer adoption or change to accepted-reason or choice
semantics. They preserve an explicit prescription for later evidence to name.
