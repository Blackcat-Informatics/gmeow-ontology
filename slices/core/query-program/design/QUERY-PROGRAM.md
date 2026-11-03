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

## Finite rerank score and source authority context

QueryRerankScore is an abstract quantitative carrier under math:Quantity.
QueryRerankAuthority is a distinct InformationObject. The former retains a
reported magnitude; the latter retains opaque source identity, provider wording
and declared score context. This division gives math sole magnitude and
dimension ownership without treating source authority as a provider agent,
model, execution, observation or fitted artifact (Principles 4, 11, 12, 17
and 19).

The score class explicitly selects a finite IEEE 754 single precision carrier.
Its four required single coordinates are math:quantityValue with xsd:float,
math:hasDimension with math:Dimension, hasReferenceFrame with ReferenceFrame and
rerankScoreAuthority with QueryRerankAuthority. Authority also has four required
single coordinates: rerankAuthorityIdentity with xsd:hexBinary, rerankScoreKind
with xsd:string, rerankScoreDimension with math:Dimension and hasReferenceFrame
with ReferenceFrame. Each class/property pair has allValuesFrom, an explicit
ClosureEntry and qualified maximum one. There is no unqualified cardinality,
global functionality, inverse ownership or authority identity-key rule.

rerankAuthorityIdentity and rerankScoreKind specialize lang:literalValue while
retaining their domain roles and complete typed literals. Opaque identity bytes
impose no universal length, hash algorithm or authentication scheme. Several
source records may retain equal bytes without becoming the same record. The
open score-kind wording supplies no inferred probability, logit transform,
calibration, dimension or rescaling rule.

The actual hasReferenceFrame spine applies directly to both authority and
quantity. No duplicate frame relation or frame class is introduced. A selected
frame retains the places owner's required realm, kind, axis count, axes,
determinacy default and host declaration. Its dimensionCount describes its axis
structure; it is not math:hasDimension. Mathematical dimension and actual frame
remain distinct coordinates. Source-declared context is mandatory, without a
silent dimensionless or frame default.

QueryRerankFiniteScoreLaw binds every typed score's math:quantityValue and
requires the canonical unary logic:termIsFiniteNumeric intrinsic. The xsd:float
restriction separately fixes datatype, and the closure and qualified bound fix
presence and one value. No comparison with infinity, self-equality trick,
exact-decimal coercion or provider invocation is part of this law.

QueryRerankScoreContextLaw positively binds score, linked authority, both
dimension resources and both frame resources. Its consequent conjoins
logic:termEqual for the dimension pair and logic:termEqual for the frame pair.
Required coordinate restrictions handle absence separately. Both laws are real
Constraint/Formula trees with indexed quantified variables and argument
carriers; generated validation is their projection. They use no special native
marker selection or second authored shape surface.

The selected contract rejects nonfinite values, missing context and datatype
disagreement. It never replaces a rejected magnitude, drops a candidate or
reclassifies an existing source score as unscored. Generic provider interfaces
may be broader than this finite profile; no universal finiteness claim is made.
xsd:float retains reported numeric precision without asserting lexical, byte or
bit identity, an exact real target or a justified approximation error.

The manifest directly declares lang and places dependencies for the reused
literal and frame spines; math is already direct. No extension dependency or
record-metadata carrier is needed. This value-context contract introduces no
rerank operation, result membership, ordering, outcome or loss semantics and
does not establish consumer adoption. A complete rerank stage must separately
retain the original union and all scored and unscored candidates with its
source positions, passages, causes and losses.

## Finite reported rerank stage and outcome accounting

QueryRerank is a QueryProgramNode description; QueryRerankAnswer is an attributed
QueryAnswer. rerankInput binds the declared original QueryUnionAnswer,
rerankOutput binds the reported result and rerankedUnion retains the actual
original evidence directly on that answer. QueryRerankInputAgreementLaw requires
the two input resources to agree when an operation names that answer. This
distinguishes declaration from reported evidence without inventing an execution
occurrence (Principles 4, 7, 11, 12, 17 and 19).

Operation and answer each require rerankScoreProfile with the single admitted
value rerankFiniteFloatProfile. An explicit oneOf restriction and ClosureEntry
make selection mandatory; no score presence, provider wording or class name
inserts a default. The selected finite QueryRerankScore and QueryRerankAuthority
contract is reused with its existing magnitude, dimension, actual frame, opaque
identity and provider-wording obligations. No finite-value or score-context law
is duplicated here.

QueryRerankCandidateAccountingLaw has four guarded directions: each original
candidate has a retained entry, each entry names an original candidate, each
original candidate remains an output candidate and every output candidate is
original. QueryRerankEntryUniquenessLaw identifies two entries for the same
candidate within one answer. These obligations give exact identity coverage
without a declared count and admit empty collections when the input is empty.
The same original union resource preserves its original class-list resources,
entry occurrences, class-local ranks, tie keys and associated evidence.

QueryRerankEntry is an answer-local InformationObject. Its required coordinates
are one candidate, one original union position and one reported output
entryOrdinal. QueryRerankEntryOwnerLaw requires one existing typed answer owner
and equal owner identities for any two links to the occurrence. Its Formula has
one outer universal over entry and the owner pair. The typed-entry guard implies
a conjunction of a fresh-owner joined existential and a guarded two-owner-link
implication. It contains no positive nested universal. Only entry occurrences
have this inverse-owner obligation; candidates, scores, authorities and losses
remain shareable resources.

QueryRerankPositionUniquenessLaw separately compares original input positions
and output positions within one answer through logic:termEqual. Matching a
position requires equal entry identity. The coordinates are source-supplied
nonnegative integers; there is no contiguous-index requirement, successor law,
renumbering or global sequence identity.

QueryRerankOutcomeLaw requires a score or an explicit structured unscored cause
and forbids both. Its two passage implications require a winning passage exactly
when a score exists. Branch existence uses distinct fresh existential variables
in every sibling or nested binder, preserving independent scope in validation
projection. A typed StructuredValue retains its own native branch and field
contracts. No missing payload is replaced with a null, zero or empty container.
solutionReason and lossDetail retain their existing subject domains and are not
generic entry payload properties.

QueryRerankAuthorityRequiredLaw requires an answer authority for any scored
entry, while allowing all-unscored results with no authority or a supplied one.
QueryRerankAuthorityAgreementLaw requires each retained score's direct authority
to be the same actual record as the answer's authority. Equal opaque bytes on
different records are insufficient. The existing score laws enforce finite
typed magnitude and dimension/frame agreement through that direct authority
link; this stage adds no observer, model, authentication or calibration claim.

QueryRerankReportedOrderLaw has exactly two integer-only guarded implications.
Every scored entry's output ordinal is less than every unscored entry's output
ordinal. For two unscored entries, a lesser original input position requires a
lesser output ordinal. logic:termLess compares the bound integer coordinates.
Canonical checking is limited to group placement and tail preservation. Scored
numeric-descending order, equal-score tie order and floating-point total-order
signed-zero behavior remain observed source behavior outside these checks.
No score magnitude comparison or bit/lexical identity proof is authored.

QueryRerankLossRetentionLaw requires every original ledger loss to appear by
the same resource identity in an output ledger and permits additional stage
losses. QueryRerankUnscoredCompletenessLaw forbids CompleteForFragment when any
entry is unscored. It does not infer a missing status, make an empty ledger prove
completeness or waive a limitation after acknowledgement.

There are eight new required class/property coordinate pairs: three on the
operation, two on the answer and three on entries. Each has allValuesFrom, an
explicit ClosureEntry and qualified maximum one. Four optional-one coordinates
have range and qualified maximum restrictions without ClosureEntry: answer
authority and entry score, passage and unscored detail. Result entry and
candidate collections have range restrictions, with full accounting supplied
by the laws. No unqualified cardinality, global property functionality or
authored validation shape is introduced.

All eleven laws are ordinary Constraint/Formula trees with explicit quantified
variables, indexed arguments, formalizes and messages. Generated validation is
their projection; no special native marker lane is selected. Direct
record-metadata dependency supplies entryOrdinal and the structured-value
carriers. Existing direct lang, math and places dependencies supply literal,
quantitative and actual-frame ownership without an extension dependency.

A selected score with a nonfinite magnitude, wrong datatype or missing context
fails its existing carrier contract. It cannot be silently removed, replaced
or reclassified as a source unscored outcome. The stage retains the actual
source branches and reported evidence without claiming provider execution or
consumer adoption. Content-tier/activation order, accepted-reason folding,
verdict-choice semantics and weighted-fusion behavior are separate contracts.
