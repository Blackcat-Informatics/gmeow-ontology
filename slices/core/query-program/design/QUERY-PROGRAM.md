<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Query program semantics

The source models a consumer's graph and its attributed evidence. It extends
ontology vocabulary without installing evaluation strategies in a native compiler
or executor. Every required capability of a selected consumer operation remains
mandatory; partial query answers carry explicit losses rather than masquerading
as complete results (Principles 4, 9, 11, 12 and 17).

## Identity and selection

Graph membership is explicit and finite. A node's effect classification and
stratum belong to that operation description; reuse under different classifications
requires a different node description. Reified edge endpoints and binary
consumer-to-producer dependencies agree. Operand closure prevents a branch outside
the graph from evading its effect rules. Multiple terminal sinks do not weaken
the build pipeline's separate single serializer discipline.

A ranked entry belongs to one list occurrence. Candidate identity may recur across
lists, while the entry identity changes. The contribution law is keyed by selected
entry occurrence, so this ownership is essential to preserving per-list evidence.
The output set, contribution coverage and per-candidate score coverage each have
a two-sided law: no input member is omitted and no unrelated output appears.

The reciprocal law is an integer inequality, avoiding inexact recurring decimal
division. Its finite sum is an explicit well-founded fold: every positive index
has its predecessor, indices are unique within a score, the zero step is zero,
and each matching contribution is used exactly once. The final index bounds all
steps and the final total is the score. These conditions jointly express finite
summation without an invented summation predicate or hidden aggregate behavior.

Input ranks remain exactly as supplied, including tied and truncated ranks.
Tie keys order equal-rank input rows; source-stable output ordinals order equal
fusion scores. Neither ordering changes the numerical contributions or promotes
a retrieval score into confidence.

## Loss and verdict separation

A merge's exact unions preserve candidate and loss identities. Loss cause,
affected constraint and acknowledgement remain on the original resources. No
rule collapses two losses merely because their descriptions agree, and none
manufactures an answer-axis value from a loss count.

Verdict choice binds one answer and one axis. Existing answer-axis cardinalities
come from the query-context owner. Known equality selects the left operand;
known inequality selects the right. Absence and the selected axis's unknown
sentinel select the explicit unknown branch. Not-evaluated remains a meaningful
information value, not an absent field or a completeness value.

## Feedback and measured evidence

The feedback resource policy is distinct from the acyclic workflow policy.
Positive dependencies permit equal strata; negative or nonmonotone dependencies
strictly descend. Cycles therefore contain no negative or nonmonotone edge.
Effectful nodes are terminal and cannot participate in those cycles.

The concrete convergence criterion is finite monotone candidate-set equality.
Recorded states stay within a declared finite universe, are uniquely indexed and
include every completed predecessor. Each positive state names the actual body
execution that produced it and that execution's unique previous-state input.
Distinct positive indices require distinct body executions within that run. Body
extensionality excludes a random or ambient-state-sensitive operation from being
silently admitted as a deterministic set transformer under the same description.

For a converged outcome, the last two states have equal membership and at least
one pass was completed. Budget stops and unfinished evaluation have their own
values and make no convergence claim. An equality record is evidence asserted by
the consumer, not proof that GMEOW executed or certified a loop.

## Canonical expression and boundaries

Required paths use `logic:allValuesFrom` plus class-scoped `ClosureEntry` records.
Intrinsic scalar bindings have qualified at-most-one restrictions. Cross-record
obligations use realized `logic:Constraint` Formula trees with quantified variables
and indexed argument carriers. Arithmetic uses the existing finite RDF numeric
relations. No hand-authored OWL, SHACL or ShEx surface is added.

The selection leaf declares a pure SPARQL SELECT language boundary and explicit
text/dataset/context/input/output bindings. Structural obligations establish those
bindings, not grammatical validity of arbitrary text. Parsing and semantic
admission remain the selected consumer's responsibility; this source neither
claims a parser exists here nor silently executes unsupported text. Query and
transaction-program NodeKind values remain individuals, distinct from classes.

An effective enactment principal is recorded authority context. Its presence does
not establish a permission check, identify a tool provider or weaken the existing
Enactment identity contract.

## Fitted class-weight evidence and quantitative ownership

QueryFusionWeightFit is a source-reported information record. It retains exactly
one judged-content identity and at least one supplied class-weight quantity.
QueryFusionClassWeight is an abstract quantitative carrier under math:Quantity;
it is not an InformationObject or a separate model artifact. Mathematical value
and dimension ownership remain with math:quantityValue and math:hasDimension.
The class-weight dimension is fixed to math:dimensionless, with one positive
exact decimal magnitude and one source retrieval-class resource.

Required paths use canonical allValuesFrom restrictions and class-scoped
ClosureEntry records. Qualified at-most-one restrictions bind the judged-set,
class, magnitude and dimension coordinates. The dimension value set contains
only math:dimensionless. These declarations do not infer a missing source
measurement or replace an unavailable fit with a unit coefficient.

FusionClassWeightPositiveLaw compares each retained magnitude with exact decimal
zero through logic:termGreater. The declared decimal range supplies its numeric
datatype. FusionFitClassWeightUniquenessLaw checks that two quantities retained
by one fit and naming the same class are the same selected quantity resource.
Both are realized Constraint/Formula trees with explicit quantified variables
and indexed arguments; generated validation is their projection. No native
marker metadata or alternate constraint surface is authored.

fusionWeightFit is an optional nonfunctional evidence association on an existing
QueryProgramNode. Several source fits can coexist. A quantity may be referenced
by several fit records without identifying those fits or their judged sets.
There is no inverse ownership rule, global class preference, fitting executor,
hash verification law or implicit source authentication.

The evidence association changes none of ReciprocalRankFusion's existing
unweighted contribution, quantization, fold or tie-order laws. Source-reported
measurements do not select a weighted operator or prove that any node used them.
No probability interpretation, held-out validity or consumer adoption follows
from the record. The query-program manifest directly depends on math for its
quantitative carriers; no language-grammar grounding or lexical numeric property
is needed for machine decimal magnitudes.

## Source-preserving candidate unions

QueryCandidateUnion is a SubKind of QueryLedgerMerge, and QueryUnionAnswer is a
SubKind of QueryAnswer. The node narrows mergeAnswerInput to RankedQueryList and
mergeAnswerOutput to QueryUnionAnswer. The existing parent contract retains a
nonempty input set and exactly one output; the answer retains its required
queryAnswerFor and queryLossLedger. No duplicate input/output relation or required
path is introduced.

The inherited QueryMergeCandidateUnionLaw and QueryMergeLossUnionLaw remain the
two-sided authority for candidate and original loss membership. They read the
reused merge relations directly. RankedListMembershipConstraint and the existing
entry-ownership laws preserve each list's candidate/entry correspondence,
exclusive entry occurrence ownership, original positive ranks and local tie keys.
No copied entry or union-wide rank is required to express the result.

UnionInputListRetentionLaw requires the linked answer to retain every selected
input list through unionClassList. UnionRetainedListSourceLaw requires every
retained list of that linked answer to be an original selected input. Together
they define exact list-identity retention without merging lists by class labels,
equal candidates or equal ranks.

rankedListClass has a class-local at-most-one qualified bound on RankedQueryList
and no required-path ClosureEntry. UnionSelectedListClassLaw requires an explicit
class resource only on selected union inputs. This preserves older unselected
list descriptions and allows several lists to name the same retrieval class.
The resource is a supplied retrieval-family or space identity, not an RDF Class.

unionClassList has no standalone required-path closure. When an answer is linked
as the output of a candidate union, the directional laws and inherited nonempty
input contract supply its exact retained-list obligation. A standalone answer
does not acquire an invented producing node, list set or class binding.

All three additional obligations are realized Constraint/Formula trees with
explicit focus variables and indexed arguments. The class-binding obligation
uses a fresh existential resource witness. Their validation surface is generated
from the canonical logic source; no second shape surface, marker metadata,
primitive numerical property or new grounding vocabulary is authored.

The unranked union stage supplies neither an aggregate fusion score nor a global
merit order. Existing class-local score evidence remains source evidence; the
absence of a union score does not mean zero. Original loss identities and their
attribution remain intact. The description does not alter reciprocal-rank
arithmetic, order policy, accepted-reason folding or verdict-choice semantics.
