<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Environment records

This slice records what a simulated-world provider and its agents report:
everyday object kinds, meaningful state, provider identifiers, requests and
their evidence, and disclosure within an owner’s agency or named circles.
It supplies canonical GMEOW semantics for these records. It does not execute
a provider, construct objects, run criteria, authenticate bytes, or enforce
access. Principles 4, 9, 10, 12 and 17 govern these distinctions.

## Everyday classification and provider identity

`WorldObjectKind` is an open `logic:QualityValue` vocabulary. The nineteen
everyday kinds include cup, mug, drinking glass, goblet, bowl, jar, bottle,
box, basket, table, desk, bed, nightstand, chair, sofa, shelf, mirror, lamp
and book. Six broader values describe container, vessel, drinking vessel,
furniture, supporting surface and seat. These are values attached through
`worldObjectKind`, rather than subclasses of physical objects.

`worldObjectKindBroader` points from narrower to directly broader values.
The canonical property chain on `worldObjectKind` carries classification
through that hierarchy: a mug is also a cup, drinking vessel, vessel and
container. A drinking-vessel query can therefore include a cup. It does not
establish that the cup is usable now, readable, accessible or made of glass.
“Glass” names the drinking vessel; “vessel” does not mean a ship; “surface”
does not mean a mathematical surface; a book value does not cast its bearer
as a CreativeWork. Local kinds may add typed broader links. Cycles violate
the canonical reachability obligation.

Classification belongs to the selected attributed description. A consumer
must retain the source and standpoint of its input and mark derived broader
classification as inferred. An agent’s direct assertion of “mug” is not a
direct assertion of every derived broader value.

A `WorldObjectRecord` names its actual `recordedWorldObject`,
`providerProgram`, `providerWorld`, attribution and original evidence.
The world identity is a resource, not necessarily a document or artifact.
Parts use the existing part relation and their own provider records; placement
uses `onTopOf`, `insideOf` and `besideOf`.

Provider handles, capability codes, unrecognized semantic types and created
world identifiers are exact `xsd:string` values. Spaces are significant.
An identical handle in a different provider or world supplies no identity
bridge. `firstSeenAt` is the provider’s original clock reading, not a global
first or a causal sorting key. `tagReadable` is an attributed tag; it grants
no access. `worldIdentifier` may annotate a reported creation event without
certifying its execution.

## Meaningful state and concurrent heads

A `WorldObjectStateClaim` specializes the provider record. Its
`worldObjectState` is an algorithm-tagged digest of the selected meaningful
state in `stateContent`. The canonical state artifact carries that same
`contentDigest`; it is distinct from the enclosing claim’s own bytes.

`stateEncodingProfile` identifies the original artifact defining meaningful
fields and canonical bytes. It has its own digest. Changing that profile
requires a separate history unless the source explicitly supplies a justified
relationship; this slice invents no equivalence between encodings.

`supersedes` orders causal replacement within one object, provider, world and
encoding profile. Source clocks may reverse. Nonempty self-reachability is
forbidden. A `WorldStateView` declares a bounded inventory in that same scope.
Its `currentStateClaim` values are exactly the inventory members without a
superseding member in that inventory. Concurrent heads coexist, and an empty
inventory has no heads. This operational inventory does not claim global
completeness and is not a `math:Set`.

The digest laws require a tag and nonempty payload and agreement with the
named artifacts. They do not calculate a hash or certify algorithm support,
digest length, canonicalization or byte authenticity.

## Request revisions, fulfillment and acceptance

A `WorldObjectRequest` is an exact attributed source revision with its own
`contentDigest`. It is separate from an attention request and from the
requesting agent’s `Desire`. `requestDesire` links that actual desire, and
`preferredLocation` belongs on the desire. `originFocus`,
`missingCapability`, the two dream-origin fields and `tagImaginedOrigin`
retain source wording rather than inventing events or a dream taxonomy.

`RequestTransition` records a state and source transition time, author and
evidence against `answersRequestRevision`. The seven state values are
requested, acknowledged, planned, fulfilled, withdrawn, declined and
superseded. Missing state stays unknown. Acknowledgement and planning are
not fulfillment. This slice imposes no invented universal transition machine.

`recordsResponse` and `recordsFulfillment` retain separate records for the
same revision. A `RequestFulfillment` names its `fulfilledObject`, cited
work, claimed met conditions, evaluations, author and original source.
`CitedWorkEvidence` is an InformationObject role; its open
`WorkReferenceKind` and `WorkVerifiedStatus` values retain the source’s
classification and verification report. Citation is not genealogical
derivation, and reported verification is not a native certificate.

Each request criterion is a `Condition` with canonical `conditionText`.
A fulfillment must claim at least one met condition and cover the stated
criterion inventory of its exact revision. Each claimed condition needs an
attributed `ConditionEvaluation`, `verdictHeld`, evaluator `vantage` and
original source evidence. The evaluation itself also binds
`answersRequestRevision` and `recordedWorldObject` to the same revision
and object as that fulfillment. Every extra context binding must agree:
a held verdict about another object or revision cannot be reused.
`evaluatedCondition` remains the evaluation’s functional observed feature;
the provided object is never substituted for the Condition. These records
report the evaluator’s finding; validation does not execute the criterion.

A `RequestCorrection` keeps a targeted response or transition via
`defeatsResponse` or `correctsTransition`, the same answered revision,
author and source evidence. It challenges a record without deleting that
record or claiming an event never happened.

`ObjectAcceptance` is an actual Event/Activity, with `acceptedBy` specializing
`wasAssociatedWith` and `acceptedAt` specializing `eventTime`.
Its original words, source evidence and exact accepted fulfillment remain
explicit. Put these fields on the acceptance act. Reach the made object
through `acceptedFulfillment → fulfilledObject`; do not type the object as
an Event to attach the fields. `acceptanceEvidence` documents the act rather
than claiming that a later document generated an earlier event.

Every supplied `requestRevisionDigest`, including one on an offered object,
must name its `answersRequestRevision` and match that revision’s own
`contentDigest`. A response hash or the object’s state hash cannot stand in
for the request revision.

## Owner-scoped disclosure

`policyAgentInternal` requires the explicit `disclosureOwner` whose own
agency bounds the claim. `policyCourtRealm` requires a declared
`disclosureCircle` and its matching owner. A `DisclosureCircle` is a named
informal `Group`, with explicit `circleOwner` and `circleMember` links.
It is not forced into Organization, and ownership alone grants no membership.

`CircleAudience` is a `ProjectionContext` quality value. Its
`audienceCircle` names the actual group, preserving the distinction between
an audience value and member agents. A supplied circle audience on a
named-circle claim must represent one of that claim’s declared circles.
Several circles and conflicting attributed claims can coexist without a
default circle or release permission. These laws preserve intrinsic scope
consistency; an access-control consumer must implement any release policy.

## Source scenes and qualification

[Provider and state](examples/provider-and-state.ttl) illustrates exact opaque
codes, kinds, parts, placement, clock reversal and two concurrent heads.
[Request and disclosure](examples/request-and-disclosure.ttl) illustrates
origin, criteria, corrections, work evidence, a distinct acceptance act and
two named circles. They are ordinary illustrative source data, not fixtures,
tests, competency cases or execution receipts.

[Design](design/ENVIRONMENT-RECORDS.md) records the grounding choices.
All ontology obligations are authored in `logic:`; shape surfaces are derived.
The reachability formulas use existing projection support. This source
addition does not itself establish executed native reasoning, acyclicity
certification, provider operations, hashing or disclosure enforcement.

## Retain decomposed files and retired projections

worldFilePath records the exact original path relative to a particular
world-store root on an InformationObject decomposed from that whole file.
Keep its spaces, case, Unicode and separators. Use wasDerivedFrom for the actual
whole-file source and scope the exact path statement to the source store's
standpoint. A path is neither a global file identity nor a ledger event.
Several source paths can coexist; the property is not required everywhere and
has no functionality or normalization rule.

RetiredWorldProjection is the reader's role classification of an unmaintained
ledger-derived snapshot or rendering. Keep the file's existing identity.
Reify the exact native RDF 1.2 type assertion with accordingTo naming the actual
reader. wasAttributedTo separately names the writer who recorded the claim.
A class-targeted canonical law requires that exact reader attribution.

Preserve an actual ledger with wasDerivedFrom and an actual production activity
with wasGeneratedBy when supplied. Neither link is invented when its identity is
unknown. Failure to prove that the ledger restates all content does not prove
false content, permanent retirement or permission to discard the file. Other
readers and later findings remain independent claims (Principles 2, 4, 9 and 10).

The [whole-file provenance scene](examples/whole-file-provenance.ttl) separates
decomposed nodes, the original file, the producer's ledger and the reader's
classification. It asserts no migration execution or coverage proof.

## Recorded environment interactions

The environment EventType values cover world beginnings, browsing and discovery
exposure; health, attention and locus transitions; place naming; detail creation,
revision and retirement; object offers, materialization, noticing, stirring,
announcement, change and contract upgrades; reference placement, bedside items,
provider updates, degradation and replacement; and notice recording and delivery.
These are values attached to actual Event or Activity occurrences. They neither
execute operations nor make an announced object externally physical.

A rename preserves the actual place identity and earlier names. A provider may
report its latest local name, with source chronology and context on the exact
name statement. This does not establish a global preferred name, delete aliases
or infer a new place. Source clocks alone do not resolve competing names.

attentionFrom and attentionTo retain supplied attention endpoints. Physical or
virtual movement still uses eventFromLocation and eventToLocation.
stateFrom and stateTo retain actual QualityValue endpoints, including the open
EnvironmentStatus values healthy and degraded. Missing endpoints remain unknown;
there is no inferred state machine or winning current state. affectsObject
lists entities touched by an Activity, keeping those entities distinct from its
performing agent and from any claim of globally complete impact.

## Notice artifacts and delivery occurrences

messageKindWorldNotice uses the transport-neutral MessageKind and
hasMessageKind owned by core messaging. noticeKind records the Message's topic:
gift offered, request created, fulfilled or withdrawn, and world degradation or
recovery. A topic reports what the notice says; it proves neither fulfillment
nor an environment transition. Its canonical law requires the world-notice
message-kind assertion.

noticeDeliveryState can annotate the actual notice Message or one delivery
Activity. The native carrier law requires a delivery Activity to have
eventTypeWorldNotificationDelivery and affectsObject pointing to its actual
world-notice Message. Attempts are distinct occurrences. Pending, attempted,
delivered, failed and explicitly uncertain remain different open vocabulary
values. A delivered report says the notice reached its recipient; it does not
prove reading, comprehension or authentication. Missing state is never filled
with uncertain, failed or delivered. Competing attributed reports coexist.

## Details, embedded parts and media

worldDetailKind classifies retained InformationObject descriptions as aesthetic,
identity description or organizational reference. It does not turn a detail
document into an Event or establish resolved identity or organizational membership.

embeddedPartRole retains form, visual representation, mechanism plate, historical
record or held history. Keep the actual partOf relationship and source context
on the precise role claim. Its native law requires a supplied enclosing Entity.
One shared part can play different roles in different attributed contexts.

glanceText preserves the original language-bearing line, including RDF 1.2 base
direction. The native language law rejects a missing language rather than
guessing one. It does not truncate a full description or certify a reader's glance.
channelLayout belongs to core documents and preserves the actual MediaObject's
source channel-layout string, such as RGBA, independently of colour space.
stirringKind belongs on the actual Event and distinguishes beheld and wound
(the mechanism sense), without typing the object as an Event.

## Advertised affordances and reported tool outcomes

WorldAffordance is an InformationObject catalog entry. affordsAction connects its
owning object representation, also an InformationObject. Keep an actual physical
object separate from that representation. A qualified entry has its supplied
affordanceToolName; this exact string is not a resolved endpoint or ToolCall.
affordanceTarget names a supplied part of that owner. The target law requires
an actual owner and checks hasPart for every catalog owner advertising a shared
entry, preventing targets from leaking across object catalogs.

tagReadOnlyAction and tagDreamEligible annotate the catalog entry. Their laws
check WorldAffordance carriers, not effect purity, access permission or runtime
safety. Core ActionPolicy remains the authority for a supplied action theory;
these catalog records neither implement tools nor weaken that theory.

toolCallOutcome belongs to the actual ToolCall with its actual usedTool.
The source values ok, completed, opened and error remain distinct. Completion
is not proof of success; an error does not prove rollback or absence of partial
effects. No outcome is inferred from a missing field.

fulfilsOffering links a reported fulfillment Activity to the actual existing
Offering, distinct from the offered item, a WorldObjectRequest and an acceptance
act. It neither certifies every condition nor infers recipient consent.

## Request wording and additional object kinds

desiredObjectName, desiredExperienceText and requestPurposeText preserve the
actual Desire's words, including supplied language and direction. The requested
name does not rename or instantiate an object; hoped-for experience is not an
experienced Event; purpose prose is not automatically a Goal or commitment.
Use requestDesire only when the actual Desire is known.

cognitionModeAsRecorded preserves the source's mode code on an InformationObject
record or actual Desire. A property-targeted native law checks that union
carrier. The code is not automatically mapped to AwarenessMode.
defeatedWorkDigest is the exact opaque work-reference digest string on a
correction Activity. It is distinct from defeatsResponse, which links an
information correction record to the withdrawn response. No digest algorithm,
length, byte authenticity or matching work identity is inferred.

The additional WorldObjectKind individuals are raft, bookshelf, desktop,
archived desktop, music box, generative music box, kintsugi bowl vessel, written
record, document collection, readable framed artifact and measuring instrument.
The direct broader edges are bookshelf to shelf, archived desktop to desktop,
generative music box to music box, kintsugi bowl vessel to bowl, and document
collection and readable framed artifact to written record. The existing
worldObjectKindBroader chain supplies inherited classifications. These are
quality values, never object subclasses. The kintsugi value uses the bowl-shaped
sense; raft is not linked to the container meaning of vessel, and desktop is
not automatically a physical desk.

[Notices and transitions](examples/notices-and-transitions.ttl) and
[Object catalogs and request words](examples/object-catalogs-and-request-words.ttl)
are ordinary generic source scenes. They make no execution, proof, delivery or
authorization claim beyond their explicit attributed example records.
