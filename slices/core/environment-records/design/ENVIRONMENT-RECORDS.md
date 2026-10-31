<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Grounding environment records

The owner is `core/environment-records`. The five domains share provider and
source-evidence boundaries but do not change existing owners. The canonical
source is `module.ttl`; paired French and Chinese coats live only in catalogs.
Principles 4, 9, 10, 12, 17 and 19 apply.

## Kind values have their own hierarchy

`Concept.subsumes` has Concept endpoints; a `WorldObjectKind` is a quality
value. Broadening that unrelated property would coerce the value vocabulary
into social-object semantics. The narrower-to-broader
`worldObjectKindBroader` relation therefore owns the typed hierarchy.
It allows multiple parents and open local extension. Existing generic
`worldObjectKind` classification is extended by the single canonical
`logic:propertyChainAxiom (worldObjectKind worldObjectKindBroader)`.
The recursive head is the first chain member, matching the established
`memberOf / subOrganizationOf` pattern. No second check-only inheritance
Constraint duplicates that inferential source. The acyclicity obligation
uses the existing one-or-more `logic:transitiveReach` relation.

No object taxonomy, external product mapping or physical-material
classification is imported. “Vessel” means a container and “glass” a drinking
object. A query for a broader everyday classification does not become a
capability or permission decision. Attribution must remain with the
selected input description; inference never fabricates self-assertion.

## Identity and state have explicit scope

Provider programs are SoftwareAgents; provider worlds are resources, without
a forced InformationObject type. Opaque codes use `xsd:string` because token
whitespace collapse would lose source identity. This deliberately refines the
issue’s provisional token suggestion. Existing identifierScheme is literal
metadata and cannot serve as an object-property bridge to a provider agent.
Explicit provider/world scope is smaller and preserves the actual meaning.

State claims are InformationObjects whose selected state lives in a separate
artifact. Profiles identify canonical encoding and meaningful-field selection,
rather than pretending every provider shares one encoding. Digest values
follow the existing tagged-string content-identity contract. Source agreement
laws are not hash verifiers. Causal state replacement is local to equal
object/provider/world/profile scope; timestamps cannot supply that ordering.

A WorldStateView is a bounded operational record with an explicit inventory.
Its head law quantifies only that inventory and retains every concurrent
head. It is not an abstract mathematical set, and does not claim complete
knowledge of the outside environment. The canonical reachability laws have
existing shape-projection support; no executed native acyclicity
certification is asserted by authoring them.

## Evidence is not enactment

Request source revisions are InformationObjects, not intrinsic intentional
modes. The actual Desire receives preferredLocation. The attention-request
lifecycle stays with its owner: a source can report a longer object-request
life without an invented shared state machine or a dependency on an
unimplemented planning model.

Conditions and EvaluationVerdict come from observations; ConditionEvaluation,
evaluatedCondition, evaluationVerdict and conditionText come from norms.
A fulfillment binds the exact revision, actual object and separately
attributed condition evaluations. Each evaluation repeats that exact
request/object context. Universal agreement of every evaluation context
binding prevents a reusable Condition from smuggling an old verdict into a
new object or revision. The Condition remains the observed feature; verdict
values do not become observationResult entities. All stated criteria are
covered, with at least one met condition required for a fulfillment report.
The laws check intrinsic records, not the truth of source reports.

Acceptance is an Event/Activity. Its agent and time specialize existing event
relations; acceptanceEvidence names documentary support without a backward
document-generated-event assertion. The created object remains an Entity and
is reached through the accepted fulfillment. The request digest always binds
the source request revision, including when carried on an offered object.
Corrections preserve challenged original records and their history.

## Groups and audiences remain different

DisclosureCircle specializes the existing informal Group, rather than
Organization. It uses circleMember rather than Organization.hasMember.
CircleAudience specializes ProjectionContext, a quality-value vocabulary,
and links to the actual circle via audienceCircle. Internal and named-circle
policies demand explicit owner bindings; named-circle policy also demands
the owner’s declared circle. There is no inferred default owner, membership,
audience, native store policy or release permission.

## Scope of this addition

This is source vocabulary, realized canonical constraints, a classification
axiom, complete paired annotation coats, documentation and ordinary scenes.
It adds no tests, fixture producers, competency cases, executors, engines,
hand-authored shapes or protected existing-module edits. Class-local
qualified cardinalities avoid new global functional-property carriers.
Qualification must distinguish source consistency and frontend/projection
support from any reasoning or provider operation actually executed.

## Whole-file provenance and reader classification

worldFilePath belongs here because its identity boundary is a source world
store. Its InformationObject domain covers any decomposed information node,
without coercing it into WorldObjectRecord or a ledger event. xsd:string
preserves source spelling; xsd:token would collapse significant spaces.
A property-scoped closure validates supplied literals without requiring paths on
all information objects. Paths are nonfunctional: multiple attributed origins
must not collapse files or nodes. The lexical value stays relative to its
source root; no filesystem parser, root resolution or path rewrite is implied.

RetiredWorldProjection is a logic:Role under InformationObject. Being retired
depends on a store's maintenance relation and on a reader's coverage assessment,
rather than supplying an intrinsic artifact identity. It is not a rigid Kind,
a permanent intrinsic Phase or an assertion that content is false.
The original InformationObject remains the same thing (Principles 4 and 9).

RetiredWorldProjectionAttributionConstraint has a supported class-membership
focus guard. Its realized Formula joins all three existing
logic:reifiedStatementSubject, logic:reifiedStatementPredicate and
logic:reifiedStatementObject facets for the exact native type claim, then
requires accordingTo naming an actual reader Agent. Source data authors
rdf:reifies; those join facets are derived, never hand-authored. The actual
recording writer may differ from the reader. This law checks attribution,
without claiming a parser, maintenance detector, ledger migration or proof
engine.

Actual ledger descent remains wasDerivedFrom, and supplied production activity
remains wasGeneratedBy with its actual producer association. Citation is not
substituted for descent. No ledger, producer or store identity is fabricated
when the reader does not know it. The role does not imply that the producer
has ceased to exist or that the ledger omits content; the reader's inability
to prove complete restatement remains an epistemic limit. No new dependency
or external projection vocabulary is needed (Principles 2, 4, 9, 17 and 19).

## Interaction vocabulary and shared ownership

The interaction layer adds source-valued EventType classifications, open
QualityValue vocabularies and one InformationObject catalog kind. It introduces
no parallel event hierarchy, mutable winner field or executable provider.
Event kinds report beginnings, observations, modifications, provider lifecycle
and notice occurrences; their existence is not proof of external effects.
Latest local locus naming preserves old names and exact attribution without a
global preferred-name rule (Principles 4, 9 and 10).

MessageKind and hasMessageKind are shared messaging declarations with unchanged
IRIs, open/nonfunctional meaning and Message domain. Their ownership moves from
email to core messaging; email retains its specialized value individuals and
already consumes messaging. This prevents a core-to-extension dependency.
channelLayout is media metadata owned by documents. Environment records directly
consumes messaging, agentic and documents for Message, ToolCall and MediaObject
semantics; those manifests have no return edge to environment records.

The notice carrier is the canonical union of Message and Activity. Its explicit
binary-guarded law requires either a world-notice Message or a typed delivery
Activity affecting the actual world-notice Message. This is a real subject
admission check: the anonymous domain union alone does not produce such a check
through the generic closed-world domain projection. Pending and uncertain are
explicit reports, never absent-field defaults. Notice topics require the shared
world-notice message kind but assert no verified environmental truth.

Attention endpoints have Event domain and Entity range; state endpoints use
QualityValue range. Activity affectsObject keeps performer and affected entity
separate. Partial source transitions retain only supplied endpoints. No generic
transition graph, success propagation or total chronology is invented.
Details are InformationObjects; stirring occurrences are Events; embedded roles
remain contextual to supplied partOf membership and source attribution.

WorldAffordance specializes InformationObject as an actual catalog-entry kind.
Its class-guarded law requires a catalog owner and supplied tool name, while
incomplete fragments can remain plain InformationObjects. The target law has
both an owner-existence clause and universal agreement over every advertising
owner's hasPart inventory. A convenient owner cannot validate a shared entry's
foreign target. These are source-record laws, not an implementation, purity
proof or access-control mechanism. Tags are checked as affordance metadata and
do not replace ActionPolicy or admit a call.

The prose ranges preserve untagged source wording and actual language/base
direction when supplied. glanceText specifically requires language through the
native termHasLang relation. Machine fields channelLayout, affordanceToolName,
cognitionModeAsRecorded and defeatedWorkDigest are exact xsd:string values,
because token whitespace collapse would destroy source content. The latter
three do not resolve a tool, a cognition category or a work identity.
No range declaration is represented as a parser or cryptographic verifier.

The cognition-mode carrier is InformationObject or Desire, with an explicit
property-targeted disjunctive type law. Desire wording retains actual
intentBearer and intentionGoal relationships independently; no Goal is
manufactured from purpose prose. Correction Activity and correction
InformationObject remain different: defeatedWorkDigest belongs to the former,
while defeatsResponse belongs to the latter.

All nine new integrity formulas have supported binary or class focus guards.
Named domain/range and anonymous range-union closures retain the selected
source type boundaries. Anonymous subject unions have explicit native laws.
No global functionality, functional-carrier ledger change, authored shape,
external OWL/RDFS alignment, engine extension or test producer is introduced.

The six broader-kind edges use the existing canonical value hierarchy and
property-chain authority. Bowl-shaped kintsugi is the narrow sense required
by its bowl parent; the ontology does not claim every repaired vessel is a bowl.
Virtual desktop and the raft's craft meaning deliberately receive no invented
physical-surface or container parent. Inherited kind assertions remain inferred,
with attribution to the original source description (Principles 2, 4, 9, 17 and 19).
