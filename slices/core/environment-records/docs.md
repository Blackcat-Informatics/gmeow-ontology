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
