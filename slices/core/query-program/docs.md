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

`QueryVerdictChoice` is the outer binary `logic:Choice` knownness gate. It binds
`choiceAnswer`, `choiceAxis` and `choiceExpectedValue` once, with a Situation
`logic:guardSituation`. Its left operand is one owned `QueryVerdictComparison`;
its right operand is the explicit unknown query target. The comparison is also
a binary `logic:Choice`, selecting matching left or known-nonmatching right
according to its own matching guard.

The selected axis is the actual `queryInformationState` or
`queryCompletenessStatus` predicate. Expected values belong to that axis's
existing closed vocabulary and exclude its unknown sentinel. `InfoBoth` and
`InfoNotEvaluated` are known comparable information values. An absent actual
value, `InfoUndetermined` or `CompletenessUnknown` makes knownness unsupported;
no cross-axis coercion or preferred attributed answer is introduced.

A reported outer or inner `QueryNodeExecution` supplies `choiceEvaluationState`.
At that same State the knownness and matching Situation support must agree in
both directions with the bound actual answer context. `querySelectedBranch`
records the resolved matching, known-nonmatching or outer unknown target, not
the outer's immediate left comparison node. Inner-only reports use their
unique structural outer context and their own State, with knownness required;
no outer execution occurrence is fabricated.

Generic nonchoice executions without this selected role retain their existing
contract. Required structure and State are not unknown-value defaults, and
coincident route targets do not permit contradictory guard support. The graph
records declared support without authenticating execution history or making
query nodes native runnable transactions. Native Choice dispatch continues to
forward its selected branch's success, failure or pending outcome unchanged.

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

## Selected accepted-reason folding

QueryAcceptedReasonFold describes a complete selected fold over an original
QueryAnswer. It declares the original established and undecided row partitions,
the accepted reason set and the complete known-reason inventory of a named
original evidence workspace. Established rows always remain. An undecided row
remains exactly when **any** original solutionReason is selected. Acceptance
does not require all its reasons, rewrite its evidence or make it complete.

Every output QueryAcceptedReasonAnswer names its acceptedFoldOriginal and has
an incoming typed fold operation. Its solutionRow collection contains exactly
the declared eligible original occurrences. The original queryAnswerFor and
queryLossLedger collections are retained in both directions, including multiple
targets. A declined row does not erase a limitation from the original ledger.
These are resource-collection laws, without a new single-query or single-ledger
maximum or an aggregate status inference.

QueryOrderedEvidenceRow retains an original row occurrence, its original
entryOrdinal and its original orderedRowEvidence resource. Output order is
interpreted by retained original positions; filtering leaves gaps rather than
assigning new ranks. Original rows may be referenced by original and folded
answers. Equal bindings do not identify occurrences, and no inverse row-owner
rule is introduced.

Positions use the existing xsd:nonNegativeInteger carrier and must be no greater
than 18446744073709551615. The row-focused position law checks that exact bound
on every typed ordered row, independently of selected-input uniqueness. This
retains the original unsigned 64-bit numeric value domain without claiming
source literal-datatype or byte-encoding identity.

QueryRowAcceptance is a separate answer-local annotation for each admitted
undecided row. Its nonempty acceptanceReason collection is exactly the
intersection of the row's complete original reasons and the operation's
accepted set. Established rows receive no annotation even when their reasons
overlap that set. Each annotation has one answer owner; different answers use
different annotations for a shared original row.

The complete fictional graph below declares two established and two undecided
rows at original positions 0, 1, 2 and 4. Accepting a admits the undecided row
with reasons a and b while retaining both original reasons. The other undecided
row is omitted. The established row at 4 has reason a but receives no override.
Known reason z is accepted without occurring on any selected original row:
workspace-wide knownness is broader than the selected-row reason union.

```turtle
@prefix ex: <https://example.org/accepted-fold/mixed/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:fold a gmeow:QueryAcceptedReasonFold ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:acceptedFoldInput ex:original ;
    gmeow:acceptedFoldOutput ex:folded ;
    gmeow:foldEvidenceWorkspace ex:workspace ;
    gmeow:foldEstablishedRow ex:established0, ex:established4 ;
    gmeow:foldUndecidedRow ex:undecided1, ex:undecided2 ;
    gmeow:foldKnownReason ex:a, ex:b, ex:c, ex:z ;
    gmeow:foldAcceptedReason ex:a, ex:z .

ex:original a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:query1, ex:query2 ;
    gmeow:queryLossLedger ex:ledger1, ex:ledger2 ;
    gmeow:solutionRow ex:established0, ex:undecided1,
        ex:undecided2, ex:established4 .
ex:folded a gmeow:QueryAcceptedReasonAnswer ;
    gmeow:acceptedFoldOriginal ex:original ;
    gmeow:queryAnswerFor ex:query1, ex:query2 ;
    gmeow:queryLossLedger ex:ledger1, ex:ledger2 ;
    gmeow:solutionRow ex:established0, ex:undecided1, ex:established4 ;
    gmeow:rowAcceptance ex:acceptance1 .

ex:established0 a gmeow:QueryOrderedEvidenceRow ;
    gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger ;
    gmeow:orderedRowEvidence ex:evidence0 .
ex:undecided1 a gmeow:QueryOrderedEvidenceRow ;
    gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger ;
    gmeow:orderedRowEvidence ex:evidence1 ;
    gmeow:solutionReason ex:a, ex:b ;
    gmeow:solutionFrame ex:frame .
ex:undecided2 a gmeow:QueryOrderedEvidenceRow ;
    gmeow:entryOrdinal "2"^^xsd:nonNegativeInteger ;
    gmeow:orderedRowEvidence ex:evidence2 ;
    gmeow:solutionReason ex:c .
ex:established4 a gmeow:QueryOrderedEvidenceRow ;
    gmeow:entryOrdinal "4"^^xsd:nonNegativeInteger ;
    gmeow:orderedRowEvidence ex:evidence4 ;
    gmeow:solutionReason ex:a .
ex:acceptance1 a gmeow:QueryRowAcceptance ;
    gmeow:acceptanceOriginalRow ex:undecided1 ;
    gmeow:acceptanceReason ex:a .

ex:ledger1 a gmeow:QueryLossLedger ; gmeow:ledgerLoss ex:loss .
ex:ledger2 a gmeow:QueryLossLedger .
ex:loss a gmeow:QueryLoss ;
    gmeow:lossConcerns ex:undecided2 ;
    gmeow:lossCause gmeow:lossValueMissing .
ex:query1 a logic:Thing .
ex:query2 a logic:Thing .
ex:workspace a logic:Thing .
ex:evidence0 a logic:Thing .
ex:evidence1 a logic:Thing .
ex:evidence2 a logic:Thing .
ex:evidence4 a logic:Thing .
ex:frame a logic:Thing .
ex:a a logic:Thing .
ex:b a logic:Thing .
ex:c a logic:Thing .
ex:z a logic:Thing .
```

Class membership selects complete supplied partitions, known and accepted sets,
and original row-reason collections. In that declared profile, no membership
edges means an explicitly empty collection. It does not prove that an importer
possessed the source list. Unavailable or truncated partitions, accepted lists,
workspace inventories or original row reasons must fail import before asserting
the selected profile. No missing capability becomes an empty-source default.

The next complete graph declares an empty accepted selection. The undecided row
still has a known original reason, but it is not admitted. The established row
remains. There are no acceptance annotations.

```turtle
@prefix ex: <https://example.org/accepted-fold/empty-selection/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:fold a gmeow:QueryAcceptedReasonFold ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:acceptedFoldInput ex:original ;
    gmeow:acceptedFoldOutput ex:folded ;
    gmeow:foldEvidenceWorkspace ex:workspace ;
    gmeow:foldEstablishedRow ex:established ;
    gmeow:foldUndecidedRow ex:undecided ;
    gmeow:foldKnownReason ex:reason .
ex:original a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:query ; gmeow:queryLossLedger ex:ledger ;
    gmeow:solutionRow ex:established, ex:undecided .
ex:folded a gmeow:QueryAcceptedReasonAnswer ;
    gmeow:acceptedFoldOriginal ex:original ;
    gmeow:queryAnswerFor ex:query ; gmeow:queryLossLedger ex:ledger ;
    gmeow:solutionRow ex:established .
ex:established a gmeow:QueryOrderedEvidenceRow ;
    gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger ;
    gmeow:orderedRowEvidence ex:establishedEvidence .
ex:undecided a gmeow:QueryOrderedEvidenceRow ;
    gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger ;
    gmeow:orderedRowEvidence ex:undecidedEvidence ;
    gmeow:solutionReason ex:reason .
ex:ledger a gmeow:QueryLossLedger .
ex:query a logic:Thing .
ex:workspace a logic:Thing .
ex:establishedEvidence a logic:Thing .
ex:undecidedEvidence a logic:Thing .
ex:reason a logic:Thing .
```

Empty original input is also valid. This complete graph declares empty source
partitions, accepted selection, known inventory, output rows and annotations.
Required query, ledger, original-answer, workspace and operation contexts remain.

```turtle
@prefix ex: <https://example.org/accepted-fold/empty-input/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:fold a gmeow:QueryAcceptedReasonFold ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:acceptedFoldInput ex:original ;
    gmeow:acceptedFoldOutput ex:folded ;
    gmeow:foldEvidenceWorkspace ex:workspace .
ex:original a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:query ; gmeow:queryLossLedger ex:ledger .
ex:folded a gmeow:QueryAcceptedReasonAnswer ;
    gmeow:acceptedFoldOriginal ex:original ;
    gmeow:queryAnswerFor ex:query ; gmeow:queryLossLedger ex:ledger .
ex:ledger a gmeow:QueryLossLedger .
ex:query a logic:Thing .
ex:workspace a logic:Thing .
```

The answer has no unique inverse operation owner. In the next complete graph,
two selected descriptions share one result. Their accepted sets differ only by
known unused reason z, so their admitted rows and visible intersections agree.
Both descriptions satisfy the same operation-relative obligations. If their
selections changed an emitted row or visible accepted intersection, the shared
answer would fail the corresponding exactness law.

```turtle
@prefix ex: <https://example.org/accepted-fold/shared-output/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:fold1 a gmeow:QueryAcceptedReasonFold ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:acceptedFoldInput ex:original ;
    gmeow:acceptedFoldOutput ex:folded ;
    gmeow:foldEvidenceWorkspace ex:workspace ;
    gmeow:foldUndecidedRow ex:row ;
    gmeow:foldKnownReason ex:a, ex:z ;
    gmeow:foldAcceptedReason ex:a .
ex:fold2 a gmeow:QueryAcceptedReasonFold ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:acceptedFoldInput ex:original ;
    gmeow:acceptedFoldOutput ex:folded ;
    gmeow:foldEvidenceWorkspace ex:workspace ;
    gmeow:foldUndecidedRow ex:row ;
    gmeow:foldKnownReason ex:a, ex:z ;
    gmeow:foldAcceptedReason ex:a, ex:z .
ex:original a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:query ; gmeow:queryLossLedger ex:ledger ;
    gmeow:solutionRow ex:row .
ex:folded a gmeow:QueryAcceptedReasonAnswer ;
    gmeow:acceptedFoldOriginal ex:original ;
    gmeow:queryAnswerFor ex:query ; gmeow:queryLossLedger ex:ledger ;
    gmeow:solutionRow ex:row ; gmeow:rowAcceptance ex:annotation .
ex:row a gmeow:QueryOrderedEvidenceRow ;
    gmeow:entryOrdinal "4"^^xsd:nonNegativeInteger ;
    gmeow:orderedRowEvidence ex:evidence ; gmeow:solutionReason ex:a .
ex:annotation a gmeow:QueryRowAcceptance ;
    gmeow:acceptanceOriginalRow ex:row ; gmeow:acceptanceReason ex:a .
ex:ledger a gmeow:QueryLossLedger .
ex:query a logic:Thing .
ex:workspace a logic:Thing .
ex:evidence a logic:Thing .
ex:a a logic:Thing .
ex:z a logic:Thing .
```

The graph laws reject unknown accepted reasons relative to the declared known
inventory, missing or overlapping partition membership, duplicate original
positions, omitted eligible rows, emitted no-overlap undecided rows, replacement
output row identities, missing or extra annotations, established-row annotations,
incorrect accepted intersections, shared annotation owners, duplicate annotations
for one answer/row, added or dropped query/ledger targets, missing required
position/evidence links and typed folded answers without an operation context.

Those checks concern declared graph structure. Retaining an original resource
name does not prove its contents were unchanged. Deleting an authentic accepted
reason and consistently editing the output can leave a graph satisfying the
laws. Replacing evidence contents under the same name can do likewise. An
importer must preserve authentic original bindings, ranks, scores, reasons,
information, completeness and frame, and validate the selected source identity,
canonical accepted-ID encoding, evidence-owner admission and successful iterator
completion. A lazy prefix before a terminal source error cannot become a complete
successful fold. RDF membership does not certify native emission order.

These definitions provide no generic payload schema, arbitrary queryCandidate
validation, native hash recipe or aggregate status projection. Original row
evidence remains separate from visible acceptance. Missing aggregate statuses
stay absent; any additional host-supplied statuses require their own source
evidence. The vocabulary neither authorizes access nor asserts execution,
consumer adoption, repaired truth, altered arithmetic or a change to Choice.

## State-grounded nested verdict choice

QueryVerdictChoice is an outer binary logic:Choice and a QueryProgramNode. Its
knownness guard selects its owned QueryVerdictComparison on the left when the
selected answer axis is known; otherwise its right operand is the explicit
unknown query target. The internal comparison is another binary logic:Choice:
its matching guard selects its matching left target, and otherwise its
known-nonmatching right target. Answer, axis and expected value are bound once
on the outer node. The comparison borrows that unique structural context.

A reported outer or inner QueryNodeExecution supplies choiceEvaluationState.
Both guards' situationObtains support at that State must agree with the actual
bound answer context. Knownness holds exactly for a present selected-axis value
other than its explicit unknown sentinel. Matching holds exactly for a known
value equal to expected. InfoBoth and InfoNotEvaluated are known comparable
information values; InfoUndetermined and CompletenessUnknown are the respective
unknown sentinels. Expected sentinels and cross-axis expected values are invalid.
Independent attributed answers coexist, and each answer's existing axis remains
single-valued; no preferred truth claim or cross-axis conversion is introduced.

querySelectedBranch records the resolved query target of this two-level router.
For an outer evaluation it is the unknown right target, the comparison's matching
left target or its known-nonmatching right target. It is not the immediate left
comparison node itself. A separately reported inner execution uses its own State
and its unique structural outer context, with knownness required at that State.
It does not require an invented outer execution occurrence. Inner evaluation
under absent or explicit unknown actual evidence is invalid.

The following complete fictional graph covers missing, explicit unknown,
known-equal and known-unequal actual values on both admitted axes. It also covers
InfoBoth, InfoNotEvaluated, an inner-only report and coincident targets. The
common query and loss ledger remain attached to each inspected answer; guard
support does not rewrite their provenance or infer a status from the loss.
Each comparison has its own distinct outer owner. Generic nonchoice execution
is shown without a choice-state role or selected branch.

```turtle
@prefix ex: <https://example.org/query-choice/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:matchingTarget a gmeow:QueryProgramNode ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger .
ex:nonmatchingTarget a gmeow:QueryProgramNode ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger .
ex:unknownTarget a gmeow:QueryProgramNode ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger .
ex:sharedTarget a gmeow:QueryProgramNode ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger .
ex:sourceQuery a logic:Thing .
ex:sourceInput a logic:Thing .
ex:ledger a gmeow:QueryLossLedger ; gmeow:ledgerLoss ex:loss .
ex:loss a gmeow:QueryLoss ;
    gmeow:lossConcerns ex:sourceInput ;
    gmeow:lossCause gmeow:lossValueMissing .

ex:informationMissingOuter a gmeow:QueryVerdictChoice ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:choiceAnswer ex:informationMissingAnswer ;
    gmeow:choiceAxis gmeow:queryInformationState ;
    gmeow:choiceExpectedValue logic:InfoSupported ;
    logic:guardSituation ex:informationMissingKnown ;
    logic:leftOperand ex:informationMissingComparison ;
    logic:rightOperand ex:unknownTarget .
ex:informationMissingComparison a gmeow:QueryVerdictComparison ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    logic:guardSituation ex:informationMissingMatches ;
    logic:leftOperand ex:matchingTarget ;
    logic:rightOperand ex:nonmatchingTarget .
ex:informationMissingAnswer a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:sourceQuery ;
    gmeow:queryLossLedger ex:ledger .
ex:informationMissingKnown a logic:Situation .
ex:informationMissingMatches a logic:Situation .
ex:informationMissingState a gmeow:State .
ex:informationMissingExecution a gmeow:QueryNodeExecution ;
    gmeow:executesQueryNode ex:informationMissingOuter ;
    gmeow:choiceEvaluationState ex:informationMissingState ;
    gmeow:querySelectedBranch ex:unknownTarget .

ex:informationUnknownOuter a gmeow:QueryVerdictChoice ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:choiceAnswer ex:informationUnknownAnswer ;
    gmeow:choiceAxis gmeow:queryInformationState ;
    gmeow:choiceExpectedValue logic:InfoSupported ;
    logic:guardSituation ex:informationUnknownKnown ;
    logic:leftOperand ex:informationUnknownComparison ;
    logic:rightOperand ex:unknownTarget .
ex:informationUnknownComparison a gmeow:QueryVerdictComparison ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    logic:guardSituation ex:informationUnknownMatches ;
    logic:leftOperand ex:matchingTarget ;
    logic:rightOperand ex:nonmatchingTarget .
ex:informationUnknownAnswer a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:sourceQuery ;
    gmeow:queryLossLedger ex:ledger ;
    gmeow:queryInformationState logic:InfoUndetermined .
ex:informationUnknownKnown a logic:Situation .
ex:informationUnknownMatches a logic:Situation .
ex:informationUnknownState a gmeow:State .
ex:informationUnknownExecution a gmeow:QueryNodeExecution ;
    gmeow:executesQueryNode ex:informationUnknownOuter ;
    gmeow:choiceEvaluationState ex:informationUnknownState ;
    gmeow:querySelectedBranch ex:unknownTarget .

ex:informationBothEqualOuter a gmeow:QueryVerdictChoice ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:choiceAnswer ex:informationBothEqualAnswer ;
    gmeow:choiceAxis gmeow:queryInformationState ;
    gmeow:choiceExpectedValue logic:InfoBoth ;
    logic:guardSituation ex:informationBothEqualKnown ;
    logic:leftOperand ex:informationBothEqualComparison ;
    logic:rightOperand ex:unknownTarget .
ex:informationBothEqualComparison a gmeow:QueryVerdictComparison ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    logic:guardSituation ex:informationBothEqualMatches ;
    logic:leftOperand ex:matchingTarget ;
    logic:rightOperand ex:nonmatchingTarget .
ex:informationBothEqualAnswer a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:sourceQuery ;
    gmeow:queryLossLedger ex:ledger ;
    gmeow:queryInformationState logic:InfoBoth .
ex:informationBothEqualKnown a logic:Situation .
ex:informationBothEqualMatches a logic:Situation .
ex:informationBothEqualState a gmeow:State ;
    logic:situationObtains ex:informationBothEqualKnown, ex:informationBothEqualMatches .
ex:informationBothEqualExecution a gmeow:QueryNodeExecution ;
    gmeow:executesQueryNode ex:informationBothEqualOuter ;
    gmeow:choiceEvaluationState ex:informationBothEqualState ;
    gmeow:querySelectedBranch ex:matchingTarget .

ex:informationNotEvaluatedDifferentOuter a gmeow:QueryVerdictChoice ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:choiceAnswer ex:informationNotEvaluatedDifferentAnswer ;
    gmeow:choiceAxis gmeow:queryInformationState ;
    gmeow:choiceExpectedValue logic:InfoSupported ;
    logic:guardSituation ex:informationNotEvaluatedDifferentKnown ;
    logic:leftOperand ex:informationNotEvaluatedDifferentComparison ;
    logic:rightOperand ex:unknownTarget .
ex:informationNotEvaluatedDifferentComparison a gmeow:QueryVerdictComparison ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    logic:guardSituation ex:informationNotEvaluatedDifferentMatches ;
    logic:leftOperand ex:matchingTarget ;
    logic:rightOperand ex:nonmatchingTarget .
ex:informationNotEvaluatedDifferentAnswer a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:sourceQuery ;
    gmeow:queryLossLedger ex:ledger ;
    gmeow:queryInformationState logic:InfoNotEvaluated .
ex:informationNotEvaluatedDifferentKnown a logic:Situation .
ex:informationNotEvaluatedDifferentMatches a logic:Situation .
ex:informationNotEvaluatedDifferentState a gmeow:State ;
    logic:situationObtains ex:informationNotEvaluatedDifferentKnown .
ex:informationNotEvaluatedDifferentExecution a gmeow:QueryNodeExecution ;
    gmeow:executesQueryNode ex:informationNotEvaluatedDifferentOuter ;
    gmeow:choiceEvaluationState ex:informationNotEvaluatedDifferentState ;
    gmeow:querySelectedBranch ex:nonmatchingTarget .

ex:completenessMissingOuter a gmeow:QueryVerdictChoice ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:choiceAnswer ex:completenessMissingAnswer ;
    gmeow:choiceAxis gmeow:queryCompletenessStatus ;
    gmeow:choiceExpectedValue logic:CompleteForFragment ;
    logic:guardSituation ex:completenessMissingKnown ;
    logic:leftOperand ex:completenessMissingComparison ;
    logic:rightOperand ex:unknownTarget .
ex:completenessMissingComparison a gmeow:QueryVerdictComparison ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    logic:guardSituation ex:completenessMissingMatches ;
    logic:leftOperand ex:matchingTarget ;
    logic:rightOperand ex:nonmatchingTarget .
ex:completenessMissingAnswer a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:sourceQuery ;
    gmeow:queryLossLedger ex:ledger .
ex:completenessMissingKnown a logic:Situation .
ex:completenessMissingMatches a logic:Situation .
ex:completenessMissingState a gmeow:State .
ex:completenessMissingExecution a gmeow:QueryNodeExecution ;
    gmeow:executesQueryNode ex:completenessMissingOuter ;
    gmeow:choiceEvaluationState ex:completenessMissingState ;
    gmeow:querySelectedBranch ex:unknownTarget .

ex:completenessUnknownOuter a gmeow:QueryVerdictChoice ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:choiceAnswer ex:completenessUnknownAnswer ;
    gmeow:choiceAxis gmeow:queryCompletenessStatus ;
    gmeow:choiceExpectedValue logic:CompleteForFragment ;
    logic:guardSituation ex:completenessUnknownKnown ;
    logic:leftOperand ex:completenessUnknownComparison ;
    logic:rightOperand ex:unknownTarget .
ex:completenessUnknownComparison a gmeow:QueryVerdictComparison ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    logic:guardSituation ex:completenessUnknownMatches ;
    logic:leftOperand ex:matchingTarget ;
    logic:rightOperand ex:nonmatchingTarget .
ex:completenessUnknownAnswer a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:sourceQuery ;
    gmeow:queryLossLedger ex:ledger ;
    gmeow:queryCompletenessStatus logic:CompletenessUnknown .
ex:completenessUnknownKnown a logic:Situation .
ex:completenessUnknownMatches a logic:Situation .
ex:completenessUnknownState a gmeow:State .
ex:completenessUnknownExecution a gmeow:QueryNodeExecution ;
    gmeow:executesQueryNode ex:completenessUnknownOuter ;
    gmeow:choiceEvaluationState ex:completenessUnknownState ;
    gmeow:querySelectedBranch ex:unknownTarget .

ex:completenessEqualOuter a gmeow:QueryVerdictChoice ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:choiceAnswer ex:completenessEqualAnswer ;
    gmeow:choiceAxis gmeow:queryCompletenessStatus ;
    gmeow:choiceExpectedValue logic:Incomplete ;
    logic:guardSituation ex:completenessEqualKnown ;
    logic:leftOperand ex:completenessEqualComparison ;
    logic:rightOperand ex:unknownTarget .
ex:completenessEqualComparison a gmeow:QueryVerdictComparison ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    logic:guardSituation ex:completenessEqualMatches ;
    logic:leftOperand ex:matchingTarget ;
    logic:rightOperand ex:nonmatchingTarget .
ex:completenessEqualAnswer a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:sourceQuery ;
    gmeow:queryLossLedger ex:ledger ;
    gmeow:queryCompletenessStatus logic:Incomplete .
ex:completenessEqualKnown a logic:Situation .
ex:completenessEqualMatches a logic:Situation .
ex:completenessEqualState a gmeow:State ;
    logic:situationObtains ex:completenessEqualKnown, ex:completenessEqualMatches .
ex:completenessEqualExecution a gmeow:QueryNodeExecution ;
    gmeow:executesQueryNode ex:completenessEqualOuter ;
    gmeow:choiceEvaluationState ex:completenessEqualState ;
    gmeow:querySelectedBranch ex:matchingTarget .

ex:completenessDifferentOuter a gmeow:QueryVerdictChoice ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:choiceAnswer ex:completenessDifferentAnswer ;
    gmeow:choiceAxis gmeow:queryCompletenessStatus ;
    gmeow:choiceExpectedValue logic:CompleteForFragment ;
    logic:guardSituation ex:completenessDifferentKnown ;
    logic:leftOperand ex:completenessDifferentComparison ;
    logic:rightOperand ex:unknownTarget .
ex:completenessDifferentComparison a gmeow:QueryVerdictComparison ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    logic:guardSituation ex:completenessDifferentMatches ;
    logic:leftOperand ex:matchingTarget ;
    logic:rightOperand ex:nonmatchingTarget .
ex:completenessDifferentAnswer a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:sourceQuery ;
    gmeow:queryLossLedger ex:ledger ;
    gmeow:queryCompletenessStatus logic:Incomplete .
ex:completenessDifferentKnown a logic:Situation .
ex:completenessDifferentMatches a logic:Situation .
ex:completenessDifferentState a gmeow:State ;
    logic:situationObtains ex:completenessDifferentKnown .
ex:completenessDifferentExecution a gmeow:QueryNodeExecution ;
    gmeow:executesQueryNode ex:completenessDifferentOuter ;
    gmeow:choiceEvaluationState ex:completenessDifferentState ;
    gmeow:querySelectedBranch ex:nonmatchingTarget .

ex:innerOnlyNotEvaluatedOuter a gmeow:QueryVerdictChoice ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:choiceAnswer ex:innerOnlyNotEvaluatedAnswer ;
    gmeow:choiceAxis gmeow:queryInformationState ;
    gmeow:choiceExpectedValue logic:InfoNotEvaluated ;
    logic:guardSituation ex:innerOnlyNotEvaluatedKnown ;
    logic:leftOperand ex:innerOnlyNotEvaluatedComparison ;
    logic:rightOperand ex:unknownTarget .
ex:innerOnlyNotEvaluatedComparison a gmeow:QueryVerdictComparison ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    logic:guardSituation ex:innerOnlyNotEvaluatedMatches ;
    logic:leftOperand ex:matchingTarget ;
    logic:rightOperand ex:nonmatchingTarget .
ex:innerOnlyNotEvaluatedAnswer a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:sourceQuery ;
    gmeow:queryLossLedger ex:ledger ;
    gmeow:queryInformationState logic:InfoNotEvaluated .
ex:innerOnlyNotEvaluatedKnown a logic:Situation .
ex:innerOnlyNotEvaluatedMatches a logic:Situation .
ex:innerOnlyNotEvaluatedState a gmeow:State ;
    logic:situationObtains ex:innerOnlyNotEvaluatedKnown, ex:innerOnlyNotEvaluatedMatches .
ex:innerOnlyNotEvaluatedExecution a gmeow:QueryNodeExecution ;
    gmeow:executesQueryNode ex:innerOnlyNotEvaluatedComparison ;
    gmeow:choiceEvaluationState ex:innerOnlyNotEvaluatedState ;
    gmeow:querySelectedBranch ex:matchingTarget .

ex:coincidentUnknownOuter a gmeow:QueryVerdictChoice ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    gmeow:choiceAnswer ex:coincidentUnknownAnswer ;
    gmeow:choiceAxis gmeow:queryInformationState ;
    gmeow:choiceExpectedValue logic:InfoSupported ;
    logic:guardSituation ex:coincidentUnknownKnown ;
    logic:leftOperand ex:coincidentUnknownComparison ;
    logic:rightOperand ex:sharedTarget .
ex:coincidentUnknownComparison a gmeow:QueryVerdictComparison ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger ;
    logic:guardSituation ex:coincidentUnknownMatches ;
    logic:leftOperand ex:sharedTarget ;
    logic:rightOperand ex:sharedTarget .
ex:coincidentUnknownAnswer a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:sourceQuery ;
    gmeow:queryLossLedger ex:ledger ;
    gmeow:queryInformationState logic:InfoUndetermined .
ex:coincidentUnknownKnown a logic:Situation .
ex:coincidentUnknownMatches a logic:Situation .
ex:coincidentUnknownState a gmeow:State .
ex:coincidentUnknownExecution a gmeow:QueryNodeExecution ;
    gmeow:executesQueryNode ex:coincidentUnknownOuter ;
    gmeow:choiceEvaluationState ex:coincidentUnknownState ;
    gmeow:querySelectedBranch ex:sharedTarget .

ex:genericNode a gmeow:QueryProgramNode ;
    gmeow:queryNodeEffectful "false"^^xsd:boolean ;
    gmeow:queryNodeStratum "0"^^xsd:nonNegativeInteger .
ex:genericExecution a gmeow:QueryNodeExecution ;
    gmeow:executesQueryNode ex:genericNode .
```

In the coincident-target case, the actual value is unknown and neither guard
is supported. Adding matching support would be invalid even though the resolved
query target's identity would stay the same. The independent guard laws validate
generic Choice semantics rather than trusting the reported target alone.

Each typed comparison requires an existing typed outer owner, and two typed
outer owners of that comparison must be the same resource. The owner is not the
comparison itself. This scopes borrowed answer context without imposing
ownership on downstream targets, States or Situations, or requiring different
targets for the three routes. When a choice belongs to a QueryProgramGraph, its
internal comparison and supplied targets must also be graph members, with the
existing effect and stratum admission obligations.

Missing structural guards, operands, inspected answer, expected value or
selected evaluation State is invalid. So are an invalid expected sentinel,
cross-axis expected value, wrong resolved target, contradictory guard support,
orphan or shared comparison owners, diagonal self-ownership, inner execution
under unknown evidence and choice-state use on a generic nonchoice execution.
A generic execution without this selected role requires no State. No missing
structural capability becomes an unknown-value default.

These constraints grade declared State support, not authentic execution history.
The reported-state role is not a native transaction start instruction and makes
no query node automatically runnable. Existing native binary Choice still
requires two recursively executable operands and dispatches on guard support,
forwarding the selected branch's success, failure or pending outcome unchanged.
A branch failure is not a verdict-unknown route or a fallback operation. No
state-support producer, native query adapter or consumer adoption is asserted.

## Reported exact fused scores and declared source profiles

QueryReportedFusionScore is a reported exact-decimal math:Quantity. Its sole
magnitude is math:quantityValue with xsd:decimal datatype. It requires one
declared math:hasDimension, one applicable hasReferenceFrame and one
reportedFusionScoreProfile. The selected QueryFusionScoreProfile record itself
is the declared interpretation and source-profile context; it carries its own
dimension and frame, which must be the exact same resources used by the score.
Several score carriers may share that declared record without becoming the
same score or proving an application.

The two explicit convention values preserve different source claims:

| Convention | Source arithmetic claim |
| --- | --- |
| `gmeow:fusionWeightedScale18Convention` | Positive weighted reciprocal ranks, k=60, u128 units divided by 10^18. Divide each present-class coefficient once by 60 plus retained source class rank, truncate before class-order checked summation; an absent class contributes zero. Retain source tied ranks and the candidate's minimum rank per class. Final overflow refuses rather than saturates. |
| `gmeow:fusionUnitScale12Convention` | Pinned unit-weight reciprocal-first arithmetic, k=60, signed i128 units divided by 10^12. Truncate the reciprocal first, then checked-multiply by the unit coefficient and checked-sum contributions. Family ranks count distinct subjects from one, with defocus ranked as an admitted source component. Positive unit contributions yield nonnegative scores. |

Selection is explicit; neither convention is a default or interchangeable with
the other. No sum-to-one normalization, generic weighted algorithm, custom k,
new rank convention or calibration is inferred. The actual selected full source
FusionProfile supplies its component set. Name/version alone neither determines
nor validates that set, which this foundation does not serialize.

The source identity forms differ. Under the weighted scale-18 convention,
fusionProfileDigest retains actual returned native bytes as xsd:hexBinary;
name and version are absent. Under the pinned unit scale-12 convention,
fusionProfileName and fusionProfileVersion retain actual supplied text and
the selected public profile's u32 version; digest is absent. The inspected
result returns a fusion name, while its version comes from the actual selected
full profile input. It must not be guessed from a crate version, an assumed
default or a nonexistent returned candidate-version field. There is no universal
native-byte requirement.

These fields preserve supplied metadata. They are not unique keys for RDF
records or a content-unique complete profile. Equal native bytes, names or
versions do not merge interpretation records, authenticate components/weights,
recompute a source hash or describe an arbitrary canonical order prescription.
The foundation has no separate retained source-call resource or call/result
association. Authentic association with the actual call, full source profile
and result remains application-owned.

The APIs examined do not serialize mathematical dimension or reference-frame
objects. Those coordinates explicitly declare an interpretation with actual
source or adapter justification. No default frame or dimension is manufactured
from score text, source identity or a display label. If the required actual
quantitative interpretation or identity metadata is unavailable, import must
fail this selected contract rather than fabricate it.

The following complete fictional input report illustrates both identity forms,
an explicit dimensionless scalar frame, shared interpretation records, zero,
equal values on distinct carriers, both capacity boundaries and finer-quantum
values. Its digest, name and version are supplied synthetic example data,
not identities observed from either live consumer or assigned to a real result.
The graph contains no source-call/result association or application proof.

```turtle
@prefix ex: <https://example.org/reported-fused-score/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:scalarFrame a gmeow:ReferenceFrame ;
    gmeow:frameRealm gmeow:frameRealmMathematical ;
    gmeow:frameKind gmeow:frameKindScalar ;
    gmeow:dimensionCount "1"^^xsd:nonNegativeInteger ;
    gmeow:hasAxis ex:scoreAxis ;
    gmeow:determinacyModel gmeow:determinacyCrisp ;
    gmeow:requiresHost "false"^^xsd:boolean .
ex:scoreAxis a gmeow:Axis .

ex:profile18 a gmeow:QueryFusionScoreProfile ;
    gmeow:fusionScoreConvention gmeow:fusionWeightedScale18Convention ;
    gmeow:fusionScoreDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scalarFrame ;
    gmeow:fusionProfileDigest "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"^^xsd:hexBinary .
ex:profile12 a gmeow:QueryFusionScoreProfile ;
    gmeow:fusionScoreConvention gmeow:fusionUnitScale12Convention ;
    gmeow:fusionScoreDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scalarFrame ;
    gmeow:fusionProfileName "example-profile"^^xsd:string ;
    gmeow:fusionProfileVersion "3"^^xsd:unsignedInt .

ex:score18 a gmeow:QueryReportedFusionScore ;
    math:quantityValue "0.016393442622950819"^^xsd:decimal ;
    math:hasDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scalarFrame ;
    gmeow:reportedFusionScoreProfile ex:profile18 .
ex:equalScore18 a gmeow:QueryReportedFusionScore ;
    math:quantityValue "0.016393442622950819"^^xsd:decimal ;
    math:hasDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scalarFrame ;
    gmeow:reportedFusionScoreProfile ex:profile18 .
ex:score12 a gmeow:QueryReportedFusionScore ;
    math:quantityValue "0.016393442622"^^xsd:decimal ;
    math:hasDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scalarFrame ;
    gmeow:reportedFusionScoreProfile ex:profile12 .
ex:zero a gmeow:QueryReportedFusionScore ;
    math:quantityValue "0"^^xsd:decimal ;
    math:hasDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scalarFrame ;
    gmeow:reportedFusionScoreProfile ex:profile18 .
ex:capacity18 a gmeow:QueryReportedFusionScore ;
    math:quantityValue "340282366920938463463.374607431768211455"^^xsd:decimal ;
    math:hasDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scalarFrame ;
    gmeow:reportedFusionScoreProfile ex:profile18 .
ex:capacity12 a gmeow:QueryReportedFusionScore ;
    math:quantityValue "170141183460469231731687303.715884105727"^^xsd:decimal ;
    math:hasDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scalarFrame ;
    gmeow:reportedFusionScoreProfile ex:profile12 .
ex:finerQuantum18 a gmeow:QueryReportedFusionScore ;
    math:quantityValue "0.0000000000000000001"^^xsd:decimal ;
    math:hasDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scalarFrame ;
    gmeow:reportedFusionScoreProfile ex:profile18 .
ex:finerQuantum12 a gmeow:QueryReportedFusionScore ;
    math:quantityValue "0.0000000000001"^^xsd:decimal ;
    math:hasDimension math:dimensionless ;
    gmeow:hasReferenceFrame ex:scalarFrame ;
    gmeow:reportedFusionScoreProfile ex:profile12 .
```

Both capacity values are legal **carrier-range boundaries** under this
foundation. This does not claim either could be produced by a particular full
application with its actual class count and coefficients. The finer-quantum
values also pass these structural/range laws, while failing the named source's
fixed-point quantum. Their inclusion makes the reporting boundary explicit:
QueryReportedFusionScore is neither a fixed-point datatype nor an
arithmetic-conformant result class. Decimal datatype already excludes infinity
and NaN; no additional finiteness test is needed.

The exact capacity limits are (2^128-1)/10^18 and (2^127-1)/10^12. The carrier
law uses nonnegative and upper-bound comparisons over exact decimals. It does
not certify full numeric-domain membership, source quantum, truncation,
normalization, complete contribution summation, overflow-path behavior or native
admission. Zero is legal: positive source coefficients can yield a truncated
zero contribution. Equal decimals or equal frames do not identify carriers or
make distinct source profiles comparable in meaning.

Frame completeness follows the places owner. The example's scalar mathematical
realm, dimension count, actual Axis, crisp frame declaration and explicit
host requirement are supplied together. Crispness is a frame declaration,
not a relevance truth or confidence claim. Real alternative dimensions and
frames need actual justification; dimensionless is not an inferred fallback.

Within this foundation the graph laws reject a missing or unknown convention,
missing or duplicate required coordinates, wrong datatypes, different dimension
or frame resources on score and profile, missing or wrong native identity form,
a negative magnitude or a value over the selected carrier maximum. For example,
adding name/version to the weighted byte form or supplying a digest to the unit
name/version form violates the source-form law. A byte-width or cryptographic
identity check is not claimed; those source obligations are not replaced by a
well-typed literal.

The magnitude stays in math:quantityValue, and actual identity literals use
lang:literalValue specializations. No second numeric carrier, numeric-string
parser or hash implementation is introduced. The profile is declared evidence,
without source authentication or a source-call/result link. No query operator,
candidate accounting, actual coefficient/fit selection, contribution graph,
ordering result or execution is asserted by these records. Existing unweighted
integer fusion, measured fit evidence and rerank float contracts keep their
own meanings. Full application, complete arithmetic qualification and final
integration remain separate requirements.

## Reported weighted application and exact declared sum

`QueryWeightedFusion` describes the selected weighted operation separately from
the old unweighted integer operator. A `QueryWeightedFusionApplication` is one
consumer-reported successful occurrence under `QueryNodeExecution`. It names
the actual node through `executesQueryNode`, the original `QueryUnionAnswer`
through `weightedFusionInput`, one `QueryWeightedFusionAnswer` through
`weightedFusionOutput`, and one declared profile through `weightedFusionProfile`.
This selected account uses `fusionWeightedScale18Convention` and an explicitly
justified dimensionless interpretation. Native APIs do not supply mathematical
dimension or reference-frame objects; those coordinates require actual source
or adapter justification.

The original union, lists, entries, candidates, classes, queries and loss ledgers
keep their original resource identities. The answer retains exactly the original
query, candidate and loss links. Its `weightedOriginalUnion` agrees with the
application input. The selected union's candidate membership agrees with its
retained lists, whose existing entry-membership and ownership laws still apply.
Several lists may name the same original retrieval class. Each distinct class
has one applied coefficient, and source class ordinals remain distinct within
the application without inventing an ordinal origin or a digest recipe.

An applied coefficient is a `QueryAppliedFusionCoefficient`, with a positive
decimal magnitude, explicit dimensionless dimension, applicable frame, original
`appliedCoefficientClass` and `entryOrdinal` typed as `xsd:nonNegativeInteger`.
It is an applied Quantity, including in unmeasured mode; it is not automatically
the measured-fit Quantity `QueryFusionClassWeight`.

`weightedFusionMeasured` retains the actual typed Boolean source selection.
True requires every actual selected full `QueryFusionWeightFit`, including its
original judged set and fitted quantities. A fit-in-force summary identifier
does not supply those fields. Missing full evidence must fail this selected
measured import rather than become false or an invented fit. Each full fit has
exactly the applied class set and matching numeric coefficients. Every actually
supplied fitted-weight frame must equal the selected profile frame; absent fit
frames remain legal. Supplied unit metadata must retain a compatible native
raw-scalar interpretation without silent conversion. The graph does not supply
generic unit equivalence or conversion. False forbids fits and requires every
applied coefficient to equal exact decimal one. Measured fits may also report
unit values, so values alone do not determine the Boolean.

Every original candidate has one `QueryAppliedFusionScore`, and each score has
one `QueryWeightedFusionContribution` for every applied coefficient. Contributions
retain their actual coefficient links and exact decimal magnitudes. A delivered
class/candidate pair requires an original minimum-rank entry witness; all matching
original lists remain retained. Ties do not merge entries, and ranks are neither
renumbered nor allowed beyond the source's positive unsigned rank capacity.
An absent pair has no witness and contributes exact zero. A witnessed contribution
may also be zero: zero alone does not establish absence.

The declared decimal sum is explicit. `QueryWeightedScoreFoldStep` is an
InformationObject carrying a witness index and `weightedStepTotal`, whose value
is a `QueryWeightedPartialSum` Quantity. A zero base, immediate predecessors,
unique bounded indices, exact decimal recurrence, complete contribution bijection
and final equality account for every component exactly once. The total carries
its own decimal magnitude, dimension and frame. Its context follows the owning
score and application; witness use alone adds no `reportedFusionScoreProfile`
link or source-reporting claim. An actually source-reported Quantity may also
serve as a compatible final witness total. Quantity identity between total and
score is neither required nor forbidden. Compatible steps may share total
Quantities, while control steps have unique score owners.

The following fictional scoped fragment illustrates two equal contributions and
their sum. It deliberately omits original lists and rank witnesses, coefficient
coordinates, the complete profile and frame, inherited node/answer obligations
and other application bindings. It is not a complete valid account or a report
of an actual native call. In the imagined original input, both classes deliver
this candidate at rank forty with unit coefficients.

```turtle
ex:call a gmeow:QueryWeightedFusionApplication ;
    gmeow:weightedFusionMeasured false ;
    gmeow:weightedFusionScore ex:score .
ex:score a gmeow:QueryAppliedFusionScore ;
    math:quantityValue "0.02"^^xsd:decimal ;
    gmeow:weightedScoreContribution ex:leftPart, ex:rightPart ;
    gmeow:weightedScoreFoldStep ex:base, ex:firstStep, ex:lastStep ;
    gmeow:weightedScoreFinalStep ex:lastStep .
ex:leftPart a gmeow:QueryWeightedFusionContribution ;
    math:quantityValue "0.01"^^xsd:decimal .
ex:rightPart a gmeow:QueryWeightedFusionContribution ;
    math:quantityValue "0.01"^^xsd:decimal .
ex:base a gmeow:QueryWeightedScoreFoldStep ;
    gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger ;
    gmeow:weightedStepTotal ex:zeroTotal .
ex:firstStep a gmeow:QueryWeightedScoreFoldStep ;
    gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger ;
    gmeow:weightedPreviousStep ex:base ;
    gmeow:weightedStepContribution ex:leftPart ;
    gmeow:weightedStepTotal ex:firstTotal .
ex:lastStep a gmeow:QueryWeightedScoreFoldStep ;
    gmeow:entryOrdinal "2"^^xsd:nonNegativeInteger ;
    gmeow:weightedPreviousStep ex:firstStep ;
    gmeow:weightedStepContribution ex:rightPart ;
    gmeow:weightedStepTotal ex:finalTotal .
ex:zeroTotal a gmeow:QueryWeightedPartialSum ;
    math:quantityValue "0.0"^^xsd:decimal .
ex:firstTotal a gmeow:QueryWeightedPartialSum ;
    math:quantityValue "0.01"^^xsd:decimal .
ex:finalTotal a gmeow:QueryWeightedPartialSum ;
    math:quantityValue "0.02"^^xsd:decimal .
```

Equal component values retain two contribution identities and two positive steps;
they are not reduced with distinct-value summation. A successful empty union has
no candidates, scores, contributions or fold steps, but still retains its actual
nonempty declared class set, coefficients, mode, profile and answer occurrence.
An all-zero nonempty score still has its base and one positive step per class.
A failed call is not an empty successful occurrence, and a budget-cut union keeps
its original incomplete recall evidence.

The account enforces the exact sum of reported decimal components. It does not
certify fixed-point grid membership, the source's floor-division contribution
equation, native admission, digest recomputation or source authenticity. Native
weighted scoring and ordering arrive in one result: any applied-order enrichment
must retain this same application and answer occurrence, with its actual policy
and comparison evidence. It must not fabricate a second source execution or
claim complete integration from the reported sum alone.

## Reported activation and qualified call-start snapshots

`QueryFusionActivationAccount` records complete reported activation evidence on
an existing `QueryWeightedFusionApplication`. `fusionActivationAccount` enriches
that actual occurrence and its existing answer; it does not create another call,
execution or native result. Each `QueryCandidateActivationRecord` selects the
existing `QueryAppliedFusionScore` through `activationAppliedScore`. The original
candidate follows that score's `appliedScoreCandidate`; the weighted account
continues to retain original query, candidate and loss identities.

The account requires the actual call-input `activationEnabled` Boolean and the
complete nonempty `activationVaultRecord` roster. Every original applied score
has exactly one candidate record, whose `activationFoundVault` collection is
also nonempty. These found-vault records belong to that same account roster and
must equal the actual original candidate's supplied found-vault set at import.
Other selected vaults may be in the roster without entering that candidate's
aggregate scope. Use the actual captured list that fed scoring and the returned
union; read versions may omit selected vaults and cannot reconstruct this list.

A successful empty original union has no candidate records. It still retains
the actual mode and nonempty selected-call roster, alongside the existing
weighted account's nonempty declared classes and coefficients. Neither an empty
roster nor an empty found-vault set represents an admitted native account.
Unavailable collections fail this complete selected import instead of becoming
empty.

When activation is enabled, every candidate record has one
`reportedActivationValue` pointing to a `QueryReportedActivation` Quantity. When
disabled, that edge is forbidden: the source's inactive constant zero is not a
measured observation. A genuine enabled zero retains its Quantity and context.
Each magnitude is `math:quantityValue` typed `xsd:unsignedLong`, with an explicit
`math:hasDimension` and complete applicable `hasReferenceFrame`. The source API
does not provide those context objects; actual source or adapter interpretation
must justify them. Aggregate Quantities in one account share the exact dimension
and frame resources. No default dimensionless interpretation, probability,
confidence bound or error certificate follows from an unsigned value.

Each `QueryVaultSnapshotRecord` has a supplied `activationVaultIdentity` and an
explicit `activationSnapshotPresent` Boolean. True requires both
`activationSnapshotIdentity` and `activationSnapshotHeld`. The latter references
a `QuerySnapshotHeldCount` Quantity with its own unsigned-long magnitude,
dimension and complete applicable frame. An actual present held count may be
zero. Held count describes activations held by that snapshot; it is neither a
candidate's component estimate nor a graph version, timestamp or candidate count.
Its interpretation is justified independently of aggregate activation and fusion
scores, with no common held-count frame imposed across vaults.

False means evidenced absence of a first snapshot in a live service at the
original capture. It forbids both conditional snapshot links. **Wire None alone
does not establish that state**: native capture can also return None for a
retired service, and recall can budget-skip that service while returning a
successful partial union. This selected account requires actual live-at-capture
association and absence-reason evidence. Missing proof fails import; it cannot
become false, measured zero, a fabricated snapshot or a later current read.
Existing loss evidence remains intact. No service lifecycle action is implied.

Vault identities retain the supplied sixteen bytes as unspaced, either-case
`xsd:hexBinary` with thirty-two hex digits. Present snapshot identities retain
their supplied thirty-two bytes with sixty-four digits. The ordinary
`logic:termRegex` checks use `^[0-9A-Fa-f]{32}$` and `^[0-9A-Fa-f]{64}$` on
positively bound literals. This explicit representation profile is not
XSD-canonical normalization; regex projection dialect residue remains explicit.
Roster uniqueness compares decoded binary values, so case variants do not name
distinct vaults within one account. No hash is computed or authenticated by
these checks.

The following fictional scoped fragment distinguishes candidate scope from the
whole roster. All resource names and identity bytes are fictional. It deliberately
omits the actual source association, complete application, account mode,
candidate-score linkage, typed record declarations, full Quantity dimensions
and complete frames. In this imagined enabled account, separate actual capture
evidence establishes that vault A's live service had no first snapshot; the false
literal alone does not supply that proof. This fragment is not a complete valid
account or a report of a native call.

```turtle
ex:account gmeow:activationVaultRecord ex:vaultA, ex:vaultB ;
    gmeow:activationCandidateRecord ex:row .
ex:row gmeow:activationFoundVault ex:vaultA ;
    gmeow:reportedActivationValue ex:zeroActivation .
ex:vaultA gmeow:activationVaultIdentity
        "00112233445566778899aabbccddeeff"^^xsd:hexBinary ;
    gmeow:activationSnapshotPresent false .
ex:vaultB gmeow:activationVaultIdentity
        "ffeeddccbbaa99887766554433221100"^^xsd:hexBinary ;
    gmeow:activationSnapshotPresent true ;
    gmeow:activationSnapshotIdentity
        "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"^^xsd:hexBinary ;
    gmeow:activationSnapshotHeld ex:held .
ex:zeroActivation a gmeow:QueryReportedActivation ;
    math:quantityValue "0"^^xsd:unsignedLong .
ex:held a gmeow:QuerySnapshotHeldCount ;
    math:quantityValue "0"^^xsd:unsignedLong .
```

Only A belongs to this candidate's found-vault scope, so its confirmed absence
requires the aggregate numeric zero; B's present snapshot does not enter that
scope. B's actual zero held count remains a present snapshot. Another candidate
found in both A and B retains both found-vault records. The graph does not
fabricate B's unexported candidate component or recompute the aggregate.

| Fictional reported case | Required treatment |
| --- | --- |
| Enabled, present snapshot, actual aggregate zero | Retain the observed zero Quantity; do not infer absence. |
| Enabled, all nonempty found-vault records have qualified no-first-snapshot absence | Require numeric aggregate zero. This is one direction only. |
| Disabled, nonempty original candidate set | Retain the complete candidate and snapshot evidence, with no aggregate Quantity edges. |
| Empty original candidate union | No candidate records; keep actual mode and nonempty selected-call roster. |
| Wire None without capture-liveness and absence-reason evidence | Fail this selected import, even for an otherwise successful budget-cut response. |
| Present snapshot missing identity or held count; absent snapshot carrying either | Reject incoherent snapshot fields. |
| Wrong-width or spaced hex; duplicate roster records with opposite-case spellings of identical bytes | Reject the representation or roster identity conflict. |
| Missing full quantitative context, orphan typed record, unrelated score or wrong owner roster join | Reject the incomplete or incoherent account. |

Compatible records may share fully coordinated activation or held-count
Quantities. Control records retain unique account owners; Quantities have
incoming-owner presence without unique inverse ownership. Aggregate activation,
held-count and fusion-score frames need not be the same resource even when all
are explicitly dimensionless and their values happen to agree.

The account retains source-reported aggregates and provenance scope. It does not
certify native saturating arithmetic, expose per-vault candidate components,
recover historical snapshots, authenticate source bytes or establish consumer
adoption. Full applied order still requires actual policy and output occurrences,
original candidate-array coordinates, complete matched-tier evidence and supplied
subject tie identity. Native activation-before-score and requested
score-before-activation remain distinct policies on the same actual application
and answer. The weighted account's exact declared sum is not a substitute for
native fixed-point admission, truncation or authenticated execution evidence.

## Complete reported native order on the weighted result

`QueryAppliedOrderAccount` enriches one existing `QueryWeightedFusionApplication`
and its existing `QueryWeightedFusionAnswer`. It retains actual selected policy,
complete output occurrences, original candidate-array positions and the keys
used by the inspected native weighted order. It creates no second execution,
RPC, native answer, candidate or score. Existing original union, ranked-list
evidence, query links and attributed loss resources remain unchanged.

The application selects the account through `appliedOrderAccount`. The account
requires one `appliedOrderPolicy`, one `appliedOrderActivationAccount` belonging
to that same application, the actual supplied `appliedOrderNativePolicyDigest`
and `appliedOrderNativePolicyDeclared`, and the complete `appliedOrderEntry`
collection. Each `QueryAppliedOrderEntry` selects the existing
`QueryAppliedFusionScore` through `appliedOrderScore`. Its original candidate
follows the score's existing `appliedScoreCandidate`; its activation magnitude
is found through the existing activation account's candidate record for that
same score. There is no new scalar key or duplicate Quantity.

Native precedence is content tier ascending, activation descending, score
descending, then whole source subject tie bytes ascending. The existing
`QueryOrderPolicy` can describe requested score-before-activation too, but this
native-qualified actual account cannot claim that the inspected source executed
it. An optional empty content criterion is adapter interpretation of an inert
native tier key, not a source-serialized criterion occurrence or a new selection
flag. Actual nonempty source mappings select tier probing; native declared means
nonempty mappings or selected activation. The mandatory score and tie criteria
alone do not make that flag true. Native duplicate last-wins predicate mappings
cannot be silently normalized into the existing unique-predicate policy profile.
The native digest does not identify arbitrary criterion precedence.

`appliedOrderInputPosition` retains the actual original zero-based `u32` index
as `xsd:unsignedInt`. The separate `entryOrdinal` retains actual zero-based
output position as `xsd:nonNegativeInteger`. Each coordinate is value-unique
and gap-free from zero within the account. Every original score has exactly
one entry and every entry selects a score from the owner application. These
checks preserve complete array structure without authenticating an external
candidate-to-slot association: import must still match both coordinates to the
actual source arrays. Original class ranks are never renumbered into either
coordinate.

`appliedOrderTierDeclared` retains the actual typed Boolean. True requires one
`appliedOrderTier` typed `xsd:unsignedInt`; false forbids that edge. Declared
4294967295 remains a real label and precedes every undeclared candidate. There
is no fake missing-label sentinel. A declared label equals a selected binding
on some actual retained matched predicate and is no greater than every such
matched selected label. Equal labels on distinct predicates and tied minimum
witnesses remain legal.

The complete applicable matching inventory is retained through
`appliedOrderMatchedLiteral`. Each `QueryMatchedLiteralWitness` requires its
actual `matchedLiteralVault`, `matchedLiteralPredicate` IRI and exact
`matchedLiteralLexicalForm` string. Vault scope is the whole selected roster,
including vaults outside that candidate's found-vault activation subset.
The roster link identifies a vault only: tier graph reads are not activation
snapshot contents. Actual same-call/read association remains required source
evidence, with no later re-query or read-version reconstruction as a substitute.

Matching follows the actual short circuits. Empty mappings skip the tier probe.
With nonempty mappings, a pair equal to an actual nonlexical rank literal is
matched independently. Otherwise a false lexical-hit condition needs no lexical
decision. Only a remaining applicable lexical-hit branch requires successful
query analysis and a successful match decision, whose result can be true or
false. An actual applicable parse or match failure rejects this complete import;
unused parsing need not succeed for vector-only or already matched cases, or
when no pair is applicable. A source undeclared label alone does not prove complete evidence or
failure-free applicable matching.

The witness's `xsd:string` is the native lexical text itself, retained through a
specialization of `lang:literalValue`. Preserve surrounding whitespace and an
actually supplied empty string. Do not reconstruct original RDF datatype,
language, base direction or claim identity. Native probes include framed,
superseded and denied held claims; a match does not become affirmative support.
Preserve separately supplied repeated occurrences even when vault, predicate
and lexical text agree. These are descriptive witness nodes, not fabricated
native claim identifiers, and there is no pair-uniqueness or witness-count law.

`appliedOrderSubjectTie` retains actual separately supplied thirty-two-byte
subject tie identity. The original candidate must be an IRI. Policy and subject
identities use the explicit unspaced either-case `xsd:hexBinary` profile
`^[0-9A-Fa-f]{64}$`; the same width is checked locally on the existing source
fusion-profile digest. These are representation checks, not XSD-canonical
normalization, hashing or authentication, and regex projection dialect residue
remains explicit. Public Fused does not export subject tie bytes or the complete
matching inventory. Full import requires authenticated additional evidence;
missing input cannot become an empty list, inferred tie, undeclared tier or zero.

The following worked fictional comparison preserves one original candidate set
under two different prescriptions. The reported score and activation numbers
are illustrative existing Quantity magnitudes, not a native execution report,
floor-division certificate or recomputed activation sum. Assume both policies
select tier, activation and score with the same predicate mappings and complete
source contexts, differing only in score/activation precedence.

| Candidate | Original input position | Declared tier | Existing score magnitude | Existing activation magnitude | Actual native output ordinal | Requested score-first output ordinal |
| --- | --- | --- | --- | --- | --- | --- |
| A | 0 | 0 | 0.02 | 1 | 1 | 0 |
| B | 1 | 0 | 0.01 | 9 | 0 | 1 |
| C | 2 | 4294967295 | 0.005 | 5 | 2 | 2 |
| D | 3 | undeclared | 0.025 | 99 | 3 | 3 |

B precedes A in the inspected native order because their tiers agree and B's
activation is greater. The requested score-first prescription would place A
before B. C precedes D because every declared tier, including the maximum `u32`,
precedes undeclared; D's larger score and activation cannot override that tier
decision. Only the native column is this actual-source account. The requested
column is a descriptive comparison, not another native answer or claimed source
execution.

This fictional scoped fragment retains A's original input position and different
native output position on the same application and answer. All names and any
identity bytes used in the examples are fictional. The fragment deliberately
omits actual call association, complete policies, supplied native identities,
complete entry collections, typed account declarations, actual matching proof,
full Quantity contexts and complete reference frames. It is not a complete valid
account or an authenticated source report.

```turtle
ex:call a gmeow:QueryWeightedFusionApplication ;
    gmeow:weightedFusionOutput ex:answer ;
    gmeow:weightedFusionScore ex:scoreA, ex:scoreB, ex:scoreC, ex:scoreD ;
    gmeow:fusionActivationAccount ex:activationAccount ;
    gmeow:appliedOrderAccount ex:nativeOrder .
ex:nativeOrder gmeow:appliedOrderPolicy ex:activationFirstPolicy ;
    gmeow:appliedOrderActivationAccount ex:activationAccount ;
    gmeow:appliedOrderNativePolicyDeclared true ;
    gmeow:appliedOrderEntry ex:entryA, ex:entryB, ex:entryC, ex:entryD .
ex:entryA a gmeow:QueryAppliedOrderEntry ;
    gmeow:appliedOrderScore ex:scoreA ;
    gmeow:appliedOrderInputPosition "0"^^xsd:unsignedInt ;
    gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger ;
    gmeow:appliedOrderTierDeclared true ;
    gmeow:appliedOrderTier "0"^^xsd:unsignedInt .
ex:entryB gmeow:appliedOrderScore ex:scoreB ;
    gmeow:appliedOrderInputPosition "1"^^xsd:unsignedInt ;
    gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger .
ex:entryC gmeow:appliedOrderScore ex:scoreC ;
    gmeow:appliedOrderInputPosition "2"^^xsd:unsignedInt ;
    gmeow:entryOrdinal "2"^^xsd:nonNegativeInteger ;
    gmeow:appliedOrderTierDeclared true ;
    gmeow:appliedOrderTier "4294967295"^^xsd:unsignedInt .
ex:entryD gmeow:appliedOrderScore ex:scoreD ;
    gmeow:appliedOrderInputPosition "3"^^xsd:unsignedInt ;
    gmeow:entryOrdinal "3"^^xsd:nonNegativeInteger ;
    gmeow:appliedOrderTierDeclared false .
```

For a separate fictional matching case, suppose A was found only in selected
vault A, while the selected roster contains both A and B. A qualified matched
pair from B can justify A's tier. The following partial graph retains two
actually supplied repeated lexical occurrences without inventing claim IDs:

```turtle
ex:activationAccount gmeow:activationVaultRecord ex:vaultA, ex:vaultB .
ex:activationRowA gmeow:activationAppliedScore ex:scoreA ;
    gmeow:activationFoundVault ex:vaultA .
ex:entryA gmeow:appliedOrderMatchedLiteral ex:witnessOne, ex:witnessTwo .
ex:witnessOne a gmeow:QueryMatchedLiteralWitness ;
    gmeow:matchedLiteralVault ex:vaultB ;
    gmeow:matchedLiteralPredicate ex:label ;
    gmeow:matchedLiteralLexicalForm "  retained text  " .
ex:witnessTwo a gmeow:QueryMatchedLiteralWitness ;
    gmeow:matchedLiteralVault ex:vaultB ;
    gmeow:matchedLiteralPredicate ex:label ;
    gmeow:matchedLiteralLexicalForm "  retained text  " .
```

The source predicate `ex:label` must have an actual selected binding to justify
a declared tier. The source matching context must justify the retained text;
its whitespace is not normalized away. B need not join A's activation subset,
and B's activation snapshot proves nothing about those graph-read statements.
The original literal's datatype, language and claim identity remain unknown from
these native lexical pairs. The records' distinct identities preserve supplied
occurrence evidence, not invented native claim identities.

If A also has an actual matched predicate with selected label two while another
actual matched predicate selects label zero, its stored tier must be zero.
Two different predicates can both select zero without merging bindings or
witnesses. A subject's unrelated predicate cannot replace an actual matched
pair. A complete actual inventory with no selected mapped match is undeclared;
an unavailable inventory cannot be imported as that empty result.

| Additional fictional case | Required treatment |
| --- | --- |
| Selected activation, present snapshot, actually reported aggregate zero | Retain the existing observed zero Quantity; do not infer snapshot absence. |
| All nonempty actual found-vault records have qualified first-snapshot absence | Retain the existing activation account's numeric-zero implication, never its converse. |
| Disabled activation | No aggregate activation Quantity or default zero; policy has no activation criterion. |
| Successful empty original candidates | No order entries or matching witnesses; retain actual policy and nonempty selected activation roster. |
| Missing original input slot or output slot between zero and a positive position | Reject the coordinate gap even if remaining ordinals are unique. |
| Raw snapshot None without actual capture-liveness and absence-reason evidence | Fail the selected complete account; a retired or budget-skipped service cannot select qualified false. |
| Nonempty mappings, vector match already sufficient | Retain the pair without demanding success of unused lexical analysis. |
| Remaining applicable lexical branch fails parsing or match evaluation | Reject full import, even if native fallback would report undeclared. |
| Missing actual tie bytes, whole-byte ordering proof, subject association or matching inventory | Fail full import; do not fabricate hashes, empty witnesses or a successful order certificate. |

All score, activation and held-count magnitudes remain existing
`math:Quantity` carriers with explicit dimensions and complete applicable
ReferenceFrames. The API supplies no default frame. Category-local context
agreement does not force score, aggregate activation and held-count frames to be
the same resource, even when all are explicitly dimensionless. Native fixed-point
admission, truncation, checked sums, saturation and source authentication are
separate evidence requirements.

The ordinary laws check retained identity, array completeness, tier evidence and
decisive numeric order. They do not implement or certify the terminal native
byte comparator. Exact subject/tie association, complete applicable matching,
actual whole-byte order and authenticated source execution remain strict import
requirements. Executable whole-sort qualification, requested-policy adoption and
end-to-end integration need their own evidence; this reported account is not a
Boolean proof field, generic sort engine or runtime parity claim.

## Canonical selection digests and named recipes

`selectionCanonicalDigest` records a supplied algorithm-typed digest of the
canonical bytes defined by one actual `SparqlSelectionCanonicalization` recipe.
A supplied digest requires its actual named companion through
`selectionCanonicalization`. The companion is optional when no digest is
supplied, and at most one recipe belongs to each source- and context-specific
`SparqlSelectionLeaf` description. Every selected companion is an IRI and carries
exactly one actual supplied `selectionCanonicalizationVersion`, retained as its
original `xsd:string` or `xsd:token`. Never guess a version or mint a synthetic
recipe to complete an incomplete source record (Principles 4, 7, 9 and 17).

The recipe specifies the actual parsed-query normalization and canonical
serialization, its byte encoding and its PREFIX, BASE and relative-IRI resolution
rules. Retain the original `selectionText`, `selectionLanguage`,
`selectionDataset`, `selectionContext`, `selectionInput` and `selectionOutput`.
The recipe is an information object describing the source’s rules; these
assertions install no canonicalizer, produce no canonical bytes and prove no
query parsing or execution. The existing pure SELECT language and consumer
admission obligations continue to apply.

The recipe must produce the same canonical byte preimage for comment-only,
insignificant-whitespace and prefix-name-only variants when equivalent PREFIX,
BASE and relative-IRI resolution yields the same parsed bindings. Significant
whitespace and content inside literals and the actual parsed bindings must be
preserved. This is the recipe’s required contract, not a promise of general
algebraic or semantic query equivalence. The vocabulary specifies this contract
without executing or verifying canonicalization.

Digest literals use the existing [Math algorithm datatypes](../../grounding/math/docs.md):
`gmeow:blake3_256Digest` for plain unkeyed default-length BLAKE3,
`gmeow:sha256Digest` for SHA-256 and `gmeow:md5Digest` for MD5. The first two have
exactly 64 bare lowercase hexadecimal digits; MD5 has 32. Keep the supplied
literal’s exact lexical form and datatype. Invalid case, prefixes, widths or
generic datatypes fail admission rather than being rewritten. Several algorithm
digests may coexist under the same one selected recipe-defined preimage; the
role is nonfunctional. Matching hexadecimal spellings under different algorithm
datatypes remain distinct terms.

The comparison key contains the actual recipe IRI, exact supplied version RDF
term and complete algorithm-typed digest RDF term. Compare all three coordinates;
a matching digest alone does not identify a shared canonicalization contract.
Even matching complete keys do not prove semantic query equivalence,
`owl:sameAs`, equal execution, equal datasets, equal effective authority or
collision-free byte identity. A digest assertion alone certifies neither hash
computation nor agreement with actual bytes. This role is not a subproperty of
`contentDigest`: its preimage is not automatically the verbatim query text or
an artifact’s exact original bytes.

Different coequal recipes belong to distinct actually sourced leaf descriptions,
with each description’s actual source and context retained. Never fabricate leaf
copies or aliases just to attach another recipe, and never select a globally
preferred recipe. The flat shape supports several digests for one recipe;
it cannot associate several recipes and separate digests on the same leaf.
If the source supplies only such an unpaired multischeme description, this shape
does not encode it as a complete comparison key.

Canonical restrictions validate supplied endpoints, require exactly one version
on each recipe and bound the optional leaf companion. Two realized `logic:`
Formulas validate exact digest datatype/lexical alternatives and the actual
named companion. These source obligations do not inspect recipe content for
algorithmic completeness or establish that a consumer executed it correctly.
The source and its actual normalization/serialization/encoding rules remain
necessary evidence. Unicode surface profiles, mathematical expression
normalization and effect-argument canonicalization keep their own distinct
owner contracts; none is silently substituted for this parsed-query recipe.
