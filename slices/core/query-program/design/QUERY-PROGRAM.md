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

Verdict choice binds one attributed answer and one axis on an outer binary
logic:Choice knownness gate. Its supported guard selects one owned internal
QueryVerdictComparison on the left; unsupported knownness selects its explicit
unknown right target. The comparison's supported matching guard selects its
matching left target, otherwise its known-nonmatching right target. Existing
answer-axis cardinalities remain owned by query-context; InfoBoth and
InfoNotEvaluated remain known comparable information values.

Reported outer and inner evaluations supply their own choiceEvaluationState.
Both Situation support bits must agree with that same actual answer context,
and querySelectedBranch retains the resolved target. Inner-only evaluations
join their unique distinct structural outer owner and require knownness at
that inner State without fabricating an outer execution. Missing structure or
State is invalid; absent actual axis evidence follows the explicit unknown
contract. Declared support does not authenticate source history or establish
native runnable query bodies, and no native start instruction is introduced.

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

## Complete query-order policy definitions

QueryOrderPolicy is an InformationObject selecting a complete unique-predicate
and source-standard-direction definition contract. QueryOrderCriterion is an
owned sequence occurrence, while QueryPredicateTierBinding is a reusable
predicate-to-control-tier declaration. The prescription stays distinct from
an applied order, mathematical quantity, rubric judgment and provider execution
(Principles 4, 7, 9, 11, 12, 17 and 19).

The policy's orderCriterion collection has unique entryOrdinal positions and
unique orderCriterionKind values. The admitted kind set contains orderContentTier,
orderFusedScore, orderActivation and orderSubjectTie. Each occurrence requires
one kind, one nonnegative position and one explicitly supplied boolean
orderDescending. Five required class/property coordinate pairs use
allValuesFrom, ClosureEntry and qualified maximum one: three on criteria and
predicate/tier coordinates on bindings. Collection paths retain range
restrictions without maximum or closure. No unqualified cardinality, global
functionality or source-count requirement is introduced.

QueryOrderCriterionOwnerLaw requires an existing typed policy owner and equal
owner identities for every pair of links to a criterion occurrence. Its single
outer universal binds occurrence and owner-pair variables. A typed-occurrence
guard implies a conjunction containing a fresh joined owner existential and
a guarded two-owner-link equality implication. There is no positive nested
universal. Bindings remain shareable and acquire no inverse-owner restriction.

QueryOrderCriterionPositionLaw compares positively bound integer ordinals with
logic:termEqual and requires equal occurrence identities when positions match.
QueryOrderCriterionKindLaw requires equal occurrence identities for the same
selected kind within one policy. Neither rule compares across policies or
requires contiguous positions.

QueryOrderRequiredKeysLaw supplies two separate existence obligations: a typed
and linked fused-score occurrence and a typed and linked source-subject-tie
occurrence. Distinct fresh existential names prevent accidental correlation in
projection. Kind uniqueness makes each required key single within the policy.
QueryOrderTerminalTieLaw requires every other criterion position to precede the
tie position through logic:termLess. Both occurrence identities and positions
are positively bound before their intrinsic comparisons.

QueryOrderDirectionLaw binds a criterion's stored kind and stored typed boolean
direction. Its two guarded implications require false for content tier or
subject tie and true for fused score or activation. Boolean literal arguments
carry the actual xsd:boolean datatype. This validates the explicit direction;
it does not derive a missing value. Reverse directions are outside this selected
contract even if some broader comparator could support them.

Content-tier selection means the minimum declared label among matched-literal
predicates, with undeclared candidates after every declared tier. It is an order,
never a filter. QueryOrderTierBindingScopeLaw allows bindings only on that key
kind; an empty mapping collection gives a constant undeclared key. The mapping
is not a rule over all predicates on a subject, and a policy definition supplies
no evidence that a candidate matched a particular literal.

QueryOrderTierPredicateLaw applies unary logic:termIsIri to a positively bound
predicate coordinate. QueryOrderTierUniquenessLaw requires equal selected
binding identities for the same predicate within one content-tier criterion.
Each binding requires one xsd:unsignedInt control-tier label. Equal labels for
different predicates are legal. The selected profile rejects duplicate predicate
records instead of adopting a native last-wins mapping vector, and it makes no
claim to cover every broader native policy input or identity encoding.

All nine laws are ordinary Constraint/Formula trees with formalizes, messages,
indexed arguments and distinct fresh existential binders. Only termEqual,
termLess and termIsIri are needed as intrinsics. Every variable operand is
bound through positive stored edges before comparison. No new parser, generic
sort engine, reflected numeric relation or hand-authored validation shape is
introduced; generated validation is the projection of these canonical laws.

queryOrderPolicy optionally associates supplied definitions with an existing
QueryProgramNode. It is nonfunctional descriptive evidence, without an executed
selection, default policy or modification to the node's existing operator
contract. A score-before-activation definition and an activation-before-score
definition remain separate prescriptions with separate occurrence resources.
Shared mappings do not merge their policy identities. No native policy digest
is invented from either graph.

The definition layer carries no measured key magnitude, snapshot or output
sequence. Its tier labels and precedence indices are control coordinates,
not math:Quantity or the linear-map math:Rank. A later applied account must
retain selected policy identity, actual candidate key evidence, source snapshots
and vault scope, quantitative datatype/dimension/frame/profile context, original
and output occurrences and loss identities. Existing fusion arithmetic,
source-ranked lists, rerank outcomes, accepted-reason folding and choice
contracts remain independently owned.

## Complete selected accepted-reason folds

QueryAcceptedReasonFold specializes QueryProgramNode; QueryAcceptedReasonAnswer
specializes QueryAnswer; QueryOrderedEvidenceRow specializes QuerySolutionRow;
QueryRowAcceptance is an InformationObject. Operation descriptions, attributed
answers, original row occurrences and answer-local acceptance annotations remain
distinct (Principles 4, 7, 9, 11, 12, 17 and 19). The operation does not inherit
QueryLedgerMerge or use its domain-bearing merge roles, whose exact candidate
union contract would conflict with excluding unaccepted undecided rows.

The selected operation declares complete original established/undecided
partitions, accepted reasons and the known inventory of its named original
workspace. The ordered-row subtype declares the complete supplied original
solutionReason collection. Within that profile an empty edge collection is a
declared empty source collection, not an unavailable-input fallback. An importer
must fail before asserting the profile if any selected collection is unavailable
or truncated. Additional set wrappers would not authenticate that trust boundary.

Eight required class/property pairs use allValuesFrom and explicit ClosureEntry:
operation input/output/workspace, answer original, row position/evidence, and
annotation original row/accepted reasons. Seven have qualified maximum one;
acceptanceReason is required-many without a maximum. Six further local collection
ranges cover both partitions, accepted and known sets, answer annotations and
the folded answer's ordered solution rows. No global functionality, inverse
row ownership, required original reason or exclusive-frame rule is added.

QueryAcceptedFoldInputAgreementLaw requires the output's single original answer
to be the same resource as the operation input. QueryAcceptedFoldPartitionLaw
requires exact input-row partition coverage in both directions, typed original
rows and disjoint established/undecided membership. Source partitions are supplied
declarations, not inferred from missing status fields.

QueryAcceptedFoldPositionLaw focuses on every typed QueryOrderedEvidenceRow.
One outer universal binds this, position, operation, input, other and
otherPosition. Its consequent contains two guarded implications: a bound
entryOrdinal must satisfy termLessEqual against the exact typed
xsd:nonNegativeInteger maximum 18446744073709551615; selected-operation input
membership, two original row links and two bound equal positions require equal
row identities. No positive nested universal is introduced. Local allValuesFrom
and maximum onDataRange retain the existing xsd:nonNegativeInteger carrier.
The exact upper bound preserves the source u64 numeric value domain without
a literal-datatype or byte-encoding identity claim.

Selected output order is interpreted from retained original positions, with
no contiguous-index or renumbering requirement. Native partition order and
iterator emission order remain source/import requirements. QueryAcceptedFoldKnownReasonLaw
requires accepted identities in the same operation's declared workspace-known
inventory. It permits a known accepted reason unused by any selected original row.

QueryAcceptedFoldEstablishedLaw retains every established row unconditionally.
QueryAcceptedFoldUndecidedLaw realizes both ANY-overlap directions: a bound
original reason in the selected set requires emission, and every emitted
undecided row requires a fresh joined overlap witness. QueryAcceptedFoldOutputLaw
rejects every extra solutionRow identity by requiring established membership or
undecided membership with another fresh joined overlap witness. No all-reasons
substitution or generic payload reconstruction is introduced.

QueryAcceptedFoldAcceptanceLaw supplies a fresh typed-and-linked annotation
witness for each admitted undecided row and requires every output annotation
to name a retained undecided row. Required annotation coordinates prevent a
missing-row escape; partition disjointness forbids established-row annotations
even when their reasons overlap the accepted set.

QueryRowAcceptanceOwnerLaw uses one outer universal for annotation and owner
pair, a fresh existing typed answer-owner existential and a guarded owner-pair
termEqual implication. It has no positive nested universal. Original rows remain
reusable; folded answers receive no inverse operation uniqueness rule.
QueryRowAcceptanceUniquenessLaw compares annotation identities only within one
answer for one original row. QueryRowAcceptanceReasonsLaw realizes both exact
intersection directions against original reasons and each incoming operation's
accepted selection, preserving the original full reason collection separately.

QueryAcceptedFoldContextLaw uses four guarded implications: original-to-output
and output-to-original queryAnswerFor, plus both directions for queryLossLedger.
Those existing required paths are nonfunctional. No single witness, new maximum,
aggregate status default or immutable ledger-content proof replaces collection
equality. QueryAcceptedFoldAnswerContextLaw requires a fresh existing typed fold
operation linked to every folded answer. Multiple descriptions can share an
answer only when all operation-relative membership and intersection obligations
agree; sets differing only in unused known reasons can remain consistent.

All thirteen laws are ordinary Constraint/Formula ASTs with formalizes, message,
indexed arguments and one outer universal. Every existential binder is fresh
relative to outer and sibling binders. Stored positive joins bind comparison
operands; only termEqual and termLessEqual intrinsics are needed. Validation
is derived from these canonical laws rather than authored
as shapes; no new parser, comparison engine or execution path is introduced.

Graph laws constrain supplied identities, membership, intersections and typed
coordinates. They do not authenticate immutable source contents, validate arbitrary
candidate edges or impose a binding/score schema. Reusing the original row and
evidence resource retains declared identity, but changed payload under the same
name or source-reason deletion followed by consistent graph edits can evade
graph-only validation. Authentic complete original payload/evidence preservation
is an importer obligation and must not be advertised as a blanket deletion gate.

Native source admission additionally checks sorted unique accepted IDs,
workspace-knownness, evidence owner access and original row order. A terminal
source error ends the lazy merge; a returned prefix is not a complete successful
fold and cannot be imported as this selected complete profile. Source identity,
canonical vector encoding and native accepted-row digests are not synthesized
from these graph names. The original evidence information, incompleteness and
frame remain separate from visible acceptance; no aggregate QueryAnswer status
projection is supplied by that row API. Missing statuses stay absent and any
additional host-supplied status needs its own source evidence.

The model preserves the declared fold behavior without asserting consumer
adoption, access authority, execution, repaired completeness, arithmetic changes
or a redesign of other operators. Applied ordering evidence and binary Choice
placement retain their independent ownership.

## Binary choice grounding and reported evaluations

QueryVerdictChoice and QueryVerdictComparison both specialize QueryProgramNode
and logic:Choice. The outer knownness guard selects the owned comparison on
the left or the explicit unknown query target on the right. The comparison's
matching guard selects its matching left target or known-nonmatching right
target. Answer, axis and expected value are bound once on the outer node;
the internal occurrence borrows that structural context (Principles 4, 7, 9,
11, 12, 17 and 19).

The two classes have nine required-one coordinates: outer answer, axis,
expected value, comparison left operand, unknown right operand and knownness
guard, plus inner matching/nonmatching operands and matching guard. Each uses
allValuesFrom, a class-scoped ClosureEntry and qualified maximum one. Existing
node effect/stratum requirements remain. No unqualified cardinality or global
functionality is added.

QueryVerdictComparisonOwnerLaw requires a typed joined outer owner witness and
equal identities for typed outer-owner pairs. Its separate non-self implication
forbids a typed outer owner from being the comparison itself. The existential
contains only positive stored joins; pair and non-self variables are bound in
separate universal guarded branches. There is one outer universal and no
positive nested universal, blanket class disjointness or target distinctness.
States, Situations and downstream targets acquire no inverse-owner restriction.

choiceEvaluationState is optional on generic QueryNodeExecution, with State
range and qualified maximum one and no ClosureEntry. QueryChoiceEvaluationStateLaw
conditionally requires a fresh typed State witness for an outer or inner
choice execution. Conversely, use of the role requires a fresh executed-node
witness typed as an outer choice or internal comparison. These witness names
are distinct. Nonchoice executions without the role retain their existing
contract and require neither a choice State nor selected branch.

The state/guard laws join the reported execution, executed node, typed State,
typed outer owner and comparison, bound answer/axis and outer knownness guard.
The executed node must equal that outer or its comparison. Matching and
selection additionally join expected value and matching guard; selection joins
all three supplied query targets. Every comparison variable is bound through
stored edges before termEqual, and no dynamic predicate is interpreted.

Knownness is expanded as two explicit alternatives: the information axis has an
actual value other than InfoUndetermined, or the completeness axis has an actual
value other than CompletenessUnknown. Known equality additionally compares the
bound actual with expected. Each information/completeness witness has a fresh
name, distinct across forward/reverse knownness and matching directions.
InfoBoth and InfoNotEvaluated remain known comparable information values;
different attributed answers coexist and existing answer-axis maxima remain.

QueryChoiceKnownnessLaw requires knownness support exactly when that actual
known-value condition holds. It additionally requires knownness support for a
reported inner execution at the inner execution's own State. The unique outer
context is structural; no outer execution occurrence is required or fabricated.
QueryChoiceMatchLaw requires matching support exactly for known equality. It
applies during outer unknown evaluations too, preventing coincident target
identities from masking a contradictory binary guard.

QueryChoiceAxisConstraint admits exactly two expected-value alternatives:
typed InformationState excluding InfoUndetermined on the information axis, or
typed CompletenessStatus excluding CompletenessUnknown on the completeness
axis. No unknown expected value, third axis or cross-axis coercion is admitted.
Knownness applies to the actual value; an absent actual value remains distinct
from a missing required structural field.

QueryChoiceSelectionLaw retains querySelectedBranch as the resolved target of
the two-level router. An outer evaluation with unsupported knownness selects
outer right; supported knownness resolves the comparison's matching or
nonmatching target according to matching support. A reported inner evaluation
selects its own left or right target with knownness already required. A fresh
joined selected-target witness makes reporting conditional but mandatory for
these evaluations. Existing maximum one rejects conflicting reported targets.
This does not recursively evaluate arbitrary downstream query programs.

The seven ordinary Constraint/Formula trees use one outer universal, indexed
arguments, explicit stored joins and distinct existential binders. Equivalences
are realized by two guarded implications, without Iff or positive nested
universal nodes. Generic left/right operand scope traverses the internal node
and its targets through the existing graph membership rule; iteration-body
scope and all unrelated effect/stratum admission remain unchanged.

State is owned by core/standpoint, declared as a direct dependency. Situation,
Choice, guardSituation and situationObtains remain logic-owned. guardSituation
serves conditional goals or binary Choice programs without classifying Choice
as GoalExpression: a false goal guard prescribes nothing, while a false Choice
guard selects its right program. situationObtains retains the State-to-Situation
relationship and nonmutating Path context, with goal evaluation and Choice
dispatch among its existing consumers. No logic axiom or evaluator change is
implied by those shared owner descriptions.

The inspected answer and its query/loss provenance remain the same resources.
Routing neither deletes losses nor derives status from loss count, treats
acknowledgement as repair or selects a preferred attributed truth. These laws
check supplied support and structure; they do not authenticate State history,
run a provider or establish a query-leaf adapter. A reported evaluation State
is not a native transaction start, and generic Choice's existing recursive
execution requirements and success/failure/pending semantics remain separate.

## Reported fused magnitudes and source interpretation

QueryReportedFusionScore specializes math:Quantity as a reported exact-decimal
carrier, not a fixed-point datatype or arithmetic-conformant result class.
QueryFusionScoreProfile is the declared interpretation and source-profile
context selected directly by reportedFusionScoreProfile. It is not a separate
source-call resource, full application profile or call/result association
(Principles 4, 7, 11, 12, 17 and 19).

The ten data terms preserve one canonical magnitude door: math:quantityValue.
Source identity literals use lang:literalValue specializations; dimension uses
math:Dimension and applicable frame uses the existing places ReferenceFrame
spine. Generic mathematical concepts, scalar operations and frame semantics
remain with those owners. No old unweighted fusion role, integer score/fold
record or measured-fit association is broadened into weighted application.

The two explicit convention individuals describe the observed arithmetic
families, without independent parameter defaults or arbitrary field combinations.
The weighted scale-18 claim uses k=60, u128 raw units, a single numerator division
per present class and class-order checked summation. Source ranks and tied ranks
are retained; absent class contribution is zero and final overflow refuses.
The pinned unit scale-12 claim uses k=60, signed i128 raw units and reciprocal-first
truncation before unit checked multiplication and checked sums. Source family
ranks count distinct subjects from one, with defocus as an admitted component.
Current positive unit contributions yield nonnegative scores. Neither selection
certifies these calculations, fits, component availability or normalization.

The actual selected full source FusionProfile supplies its component set.
Recorded name/version alone neither determines nor validates that set, which
this foundation does not serialize. Supplied native profile bytes likewise do
not reveal all weights/policy content or encode arbitrary canonical order.
Equal metadata does not merge RDF interpretation records or authenticate a
source. Actual source call, complete profile and result association remains
application-owned rather than invented through a new wrapper or identifier.

Seven class-local required-one coordinates use allValuesFrom, ClosureEntry and
qualified maximum one: score magnitude, dimension, frame and selected profile,
plus profile convention, dimension and frame. Convention is restricted to the
two explicit values. The three native identity roles have local datatype ranges
and maxima but no unconditional ClosureEntry. Together there are seven closures
and ten qualified maxima. No global functionality or inverse context-owner rule
is added, and no existing quantity/frame property is globally narrowed.

FusionScoreProfileSourceFormLaw uses one outer universal with positively bound
profile/convention. Its two guarded consequences require digest only under the
weighted convention and actual name/version only under the unit convention.
The three required and three forbidden existence occurrences have six distinct
fresh binders. Optional local ranges/maxima handle datatype and multiplicity;
the Formula handles applicable conditional presence and exclusion. No universal
native-byte requirement or guessed source version is introduced.

The inspected unit-source result returns its fusion name; version is supplied
by the actual selected public FusionProfile input. It is not the crate version,
a returned candidate field or a default. The byte-identified source requires
actual returned profile bytes without manufacturing a name/version. This law
checks representation form only, not source authenticity, native byte width,
hash validity, component identity or application. Missing actual required
metadata must fail selected import rather than borrow fictional example data.

FusionScoreContextAgreementLaw joins each score to its declared profile record
and compares positively bound dimension and frame pairs through termEqual.
Required coordinates are independently enforced by local class restrictions.
Agreement is exact resource identity, not equal labels or equal-looking frames.
The profile record itself is the declared context; no separate call/result link
or authenticated call-context resource is represented by this law.

FusionScoreCarrierRangeLaw uses a typed score guard, selected profile/convention
and bound decimal magnitude. It requires nonnegative value and the appropriate
exact carrier maximum: (2^128-1)/10^18 or (2^127-1)/10^12. termGreaterEqual and
termLessEqual grade representable carrier range only. They do not establish
the complete numeric domain, quantum membership, bit encoding or arithmetic
conformity. An in-range finer-quantum decimal can pass, as documentation shows.
xsd:decimal already excludes nonfinite values, so no redundant finite law exists.

All three constraints are ordinary Formula ASTs with formalizes, message,
indexed arguments and one outer universal. Only supported termEqual,
termGreaterEqual and termLessEqual comparisons are used, with variable operands
positively joined before comparison. Existential names are distinct; there is
no Iff, positive nested universal, reflected arithmetic relation or hand-authored
shape surface. Validation derives from the canonical structural/range laws.

The native APIs serialize no mathematical dimension/frame object. Required
coordinates are an explicit declared interpretation with actual source/adapter
justification, not metadata inferred from score words or native IDs. No automatic
dimensionless or frame default is introduced. The complete scalar frame example
uses the places owner's actual realm, kind, dimension count, Axis, determinacy
and host declarations; frame crispness does not confer relevance truth.

This foundation adds no multiplication, division, floor or complete-score
computation path. The existing ordinary shape comparison support and separate
native numeric mechanism are not a certificate of source quantization. Full
arithmetic qualification must address the exact selected grid, truncation
placement and checked summation rather than silently replacing these with
exact no-rounding division or a rounded rational sum. Unsupported projections
must remain honest; no fallback, omitted law or shape exception is authorized.

The reported values and source claims can later be referenced by complete
weighted application and applied ordering accounts. Original union/result
linkage, actual coefficients and fits, all contributions, complete candidate
and loss/query retention, source authentication and final integration retain
their independent acceptance obligations. These records assert no provider
execution, consumer adoption, calibration, confidence or arithmetic change.
