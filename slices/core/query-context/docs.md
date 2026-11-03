<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Query context

A query answer can preserve what it found and the inputs it could not reach.
Separately, a query can describe precisely which contextual interpretation it
uses when combining frames. This slice retains those two kinds of information
without selecting a privileged standpoint or manufacturing a complete answer.

## Keep the loss itself

`QueryAnswer` is an attributed information record, linked to its declared query
resource through `queryAnswerFor` and to one or more `QueryLossLedger` objects
through `queryLossLedger`. It is not automatically a `logic:ReasoningResult`.
The query resource need not have an invented query-language class.

`ledgerLoss` keeps each original `QueryLoss` resource. A loss identifies what it
`lossConcerns`, one or more `lossCause` values, and the affected `lossConstraint`
when the source names it. The constraint can be an ordinary query-condition
resource; it need not be a grounding integrity-rule node.

The cause vocabulary is open and supplies these distinctions:

- `lossValueMissing`: an input value was missing.
- `lossVaultLocked`: a relevant protected source was locked to this context.
- `lossCoverageEnds`: available coverage ended before the requested scope.
- `lossFamilyCouldNot`: a participating retrieval family could not evaluate.
- `lossPrecisionCoarser`: available precision was coarser than requested.
- `lossNotEvaluated`: work on which the answer depends has not been performed.

These are reports of query-input limitations, not projection-loss codes.
`logic:projectionLoss` describes a different loss surface and is not reused.
A locked source does not grant access; coarse input does not acquire fabricated
detail; missing content does not become false.

`lossAcceptedBy` identifies a reader acknowledging the report. The same loss
resource remains in a ledger after acknowledgement. That relation neither
repairs the cause nor erases it, changes a result status or authorizes access.
Several readers and source reports may coexist (Principle 9).

An empty ledger has no listed losses in its reporting scope. It does not prove
that every input was available, that evaluation completed or that no relevant
facts exist outside that scope.

## Three independent result axes

The local answer fields reuse the existing grounding value vocabularies directly:

| Field | Meaning | Vocabulary |
| --- | --- | --- |
| `queryInformationState` | Verdict about the queried proposition | `logic:InformationState` |
| `queryEvaluationStatus` | What computation was completed or could be performed | `logic:EvaluationStatus` |
| `queryCompletenessStatus` | Completeness relative to declared scope or fragment | `logic:CompletenessStatus` |

On the information axis, undetermined and not-evaluated remain information states,
not completeness values. Separately, lossNotEvaluated reports unperformed dependent
input work without assigning any result-axis value. A supported answer with incomplete coverage remains provisional; a
conclusive neither verdict differs from a verdict never reached. No field is
inferred from another or from reader acknowledgement.

Each supplied axis has at most one value on an individual attributed answer.
Independent source or context reports use separate answer records. Missing values
remain absent, rather than being filled with successful or negative defaults.

These descriptive records do not authorize silent capability degradation.
A selected operation still requires its declared inputs, capabilities and outputs.
The ledger does not relax a pipeline sink contract or permit an executor to
substitute an undeclared weaker operation.

## Declare the interpretation before combining frames

`QueryFrameInterpretation` identifies an explicit `interpretationScope` and domain
of `interpretationMember` evaluation contexts. A context is referenced as a
`logic:World`, without asserting the alethic `logic:PossibleWorld` subtype.
The current profile requires at least one interpretationMember. Empty result
extents remain permitted.

A `QueryFrameExtent` records a frame’s admitted contexts through `extentMember`
in its `extentInterpretation`. `extentFrame` identifies the represented Standpoint,
Scenario or QueryRestrictionFrame when one is supplied. Extents from different interpretations cannot
silently be combined. In an explicitly complete interpretation, separate extent
records naming the same `extentFrame` must have identical memberships.
`ExtentFrameCongruenceLaw` enforces both directions while preserving the
records’ separate identities.

`interpretationComplete` is an explicit source assertion that the domain and
recorded extents form the complete interpretation for the named scope. It is not
inferred from sample size and is not a runtime-verified certificate. A finite
sample of a larger semantic universe does not establish full intersection or
union there. All set equations below are relative to the declared complete model.

## Three different compositions

`FrameComposition` is a binary descriptor with `compositionInterpretation`,
`compositionLeft` and `compositionRight`. Use the concrete kind that matches the
requested meaning:

| Kind | Result extent | Meaning |
| --- | --- | --- |
| `StandpointMeetComposition` | Intersection of admitted precisifications | Contexts admitted by both frames |
| `StandpointTruthUnionComposition` | Union of admitted precisifications | Domain needed for universal truth throughout both frames |
| `ScenarioConjunctionComposition` | Intersection of satisfying scenario contexts | Jointly entertaining both suppositions |

For standpoint extents A and B, the meet admits A ∩ B. Universal truth on that
smaller domain can hold even when truth does not hold throughout A or B.
To require a proposition true throughout both inputs, its universal truth must
cover A ∪ B: truth at every member of the union is equivalent to truth at every
member of A and at every member of B. The union descriptor itself asserts no
proposition true.

Scenario conjunction uses an intersection of satisfying contexts but remains a
separate typed operation. Entertaining the joint supposition neither adopts a
standpoint nor asserts actuality or alethic possibility. There is no generic
cross-type accessibility inference.

## A descriptor is not a realized usable frame

For the binary families, minting a descriptor supplies no result, compatibility
flag or usable frame.
A source can explicitly retain `compositionResult`, `compositionRealized` and
`compositionCompatible`. A realization claim requires a result in the same
source-asserted complete interpretation. The extensional laws require exact
membership in both directions, rather than merely a common witness.

A compatible result is nonempty. An incompatible result is an explicit empty
extent, retained without a usable frame or a vacuous universal-truth promotion.
Only an explicitly asserted `compositionFrame` exposes a represented result,
with realized, compatible and nonempty evidence in that interpretation.

In the binary families, standpoint inputs must be nonempty represented standpoints. The unspecified
default standpoint stays outside the sharpening order. Usable standpoint results
remain standpoints; scenario results remain scenarios. An empty joint scenario
can still be described and its incompatibility retained without producing a usable
query frame.

The existing `sharpens` poset does not guarantee meet existence. A nonempty exact
intersection is a meet in the admitted-set interpretation when it is represented
by an allowed frame. This source model does not automatically create global
`sharpens` edges or certify a meet across every possible semantic interpretation.
Unindexed statements likewise remain unspecified, not universal.

The [archive scene](examples/archive-query.ttl) retains acknowledged losses and a
small explicitly scoped composition model. The [design rationale](design/QUERY-CONTEXT.md)
states the canonical laws and the limits of their interpretation.

## Row reasons and scoped loss observations

A QuerySolutionRow identifies an occurrence in an attributed QueryAnswer, linked
by solutionRow. Equal bindings do not merge occurrences or their evidence.
solutionReason names the supplied grounds or explanations by which the row is
reported to hold in its solutionFrame. Positive support, derivations and other
resource-valued reasons are admissible. A referenced QueryLoss remains a
limitation in that account; its presence does not make it positive support.
Neither relation certifies truth, a valid proof or completeness.

Keep the source's content-identified reason IRI as the original reason resource.
Rows and ledgers can then reference the same QueryLoss without per-row copies.
The ontology does not compute the identifier, prescribe its preimage or verify
that it identifies its content. Acknowledgement still preserves the loss and
does not repair it or grant permission to omit required work.

A loss can also be the native rdf:reifies resource of the exact triple term it
concerns. Keep lossConcerns and lossCause; resource-level and whole-answer
losses do not acquire invented triples. Reifying a triple does not assert it.
lossNotEvaluated reports dependent work not performed and assigns no value to
the independent information, evaluation or completeness axes.

lossVault and lossWatermark retain an explicitly supplied single-vault commit
coordinate. The direct watermark is the nonnegative xsd:integer position for
that one identified vault, not a global frontier or completeness certificate.
Do not pair independent sets of vaults and watermarks. Multiple independent
positions require source-supplied scoped occurrences or statement reification;
neither invent scopes nor split a content-identified loss merely to fit the
representation. Missing positions remain unknown, without a default zero.

lossVantage and lossObservedAt retain the supplied vantage and observation
instant on the information object. They do not specialize the Observation-domain
properties vantage and observedAt. The observation instant is distinct from
fact valid time, source assertion time, execution time and commit position.
Several attributed accounts may coexist; no observer or current time is guessed.

lossDetail reuses record-metadata's StructuredValue branches and labelled fields.
Preserve exact keys, repeated field occurrences, native datatypes, supplied order
and nesting. A StructuredMap can retain its producer-supplied record-type tag
through structureName. An omitted detail, explicit null and an empty map remain
different; none is replaced by opaque JSON text.

The graph below is a fictional supplied account. Both rows have the same complete
single binding, illustrated by ex:item, yet remain distinct occurrences. Their
positive ground and shared content-identified loss have different roles. The
reason IRI, frame, vault position, type tags, vantage and time are fictional
inputs, not computed identifiers or defaults. The ex:item predicate illustrates
the supplied binding only; it introduces no GMEOW binding schema.

```turtle
@prefix ex: <https://example.org/query/> .
@prefix reason: <urn:example:reason:> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:answer a gmeow:QueryAnswer ;
    gmeow:queryAnswerFor ex:query ;
    gmeow:queryLossLedger ex:ledger ;
    gmeow:solutionRow ex:row1, ex:row2 .
ex:ledger a gmeow:QueryLossLedger ;
    gmeow:ledgerLoss reason:content-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef .

ex:row1 a gmeow:QuerySolutionRow ;
    ex:item ex:item1 ;
    gmeow:solutionReason ex:positiveGround,
        reason:content-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef ;
    gmeow:solutionFrame ex:frame .
ex:row2 a gmeow:QuerySolutionRow ;
    ex:item ex:item1 ;
    gmeow:solutionReason ex:positiveGround,
        reason:content-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef ;
    gmeow:solutionFrame ex:frame .
ex:positiveGround a gmeow:InformationObject .
ex:frame a gmeow:Standpoint .
ex:reader a gmeow:Agent .

reason:content-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
    a gmeow:QueryLoss ;
    rdf:reifies <<( ex:input ex:needs ex:embedding )>> ;
    gmeow:lossConcerns ex:input ;
    gmeow:lossCause gmeow:lossNotEvaluated ;
    gmeow:lossVault ex:vault ;
    gmeow:lossWatermark "42"^^xsd:integer ;
    gmeow:lossVantage ex:reader ;
    gmeow:lossObservedAt "2026-11-03T12:00:00Z"^^xsd:dateTime ;
    gmeow:lossDetail ex:detail .

ex:detail a gmeow:StructuredMap ;
    gmeow:structureName "PendingInput" ;
    gmeow:hasFacet ex:scopeField, ex:countsField .
ex:scopeField a gmeow:LabelledFacet ;
    gmeow:facetLabel "vault" ;
    gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger ;
    gmeow:facetStructuredValue ex:vaultValue .
ex:vaultValue a gmeow:StructuredValue ;
    gmeow:structuredTerm ex:vault .
ex:countsField a gmeow:LabelledFacet ;
    gmeow:facetLabel "counts" ;
    gmeow:entryOrdinal "1"^^xsd:nonNegativeInteger ;
    gmeow:facetStructuredValue ex:counts .
ex:counts a gmeow:StructuredMap ;
    gmeow:structureName "PendingCounts"^^xsd:token ;
    gmeow:hasFacet ex:waitingField .
ex:waitingField a gmeow:LabelledFacet ;
    gmeow:facetLabel "waiting" ;
    gmeow:entryOrdinal "0"^^xsd:nonNegativeInteger ;
    gmeow:facetValue "3"^^xsd:integer .
```

This graph documents vocabulary use, not current consumer output. The
Katamari writer emits typed loss detail and joins loss identities to row reason
IDs, while row evidence still travels through a typed API and exposes an actual
boolean rather than named row-frame RDF. Its current watermarked loss
constructors each supply one vault; its public Loss type does not enforce that
invariant. The writer currently places concerned triple terms directly in
lossConcerns rather than emitting this resource-plus-reifier pattern;
conformance of that representation has not been established here. Whole-answer
losses with no emitted lossConcerns remain a gap against the existing required
field. These differences neither weaken the vocabulary contract nor establish
consumer adoption of this graph.

## Restriction syntax and n-ary meets

A QueryRestrictionFrame records conjunctive restriction syntax independently of
an interpretation. Its five explicit atom roles preserve standpoint,
counterfactual, imagined, assumed and licensed-falsehood restrictions. A role
does not assign a class, observer identity, truth or access authority to its
target. In particular, Scenario typing alone chooses none of these roles.
An explicitly assumed scenario remains a restriction, not an actual fact.

restrictionComponentsComplete is an explicit assertion that the supplied finite
atom inventory is complete. Missing atoms or a missing flag never mean actual.
The fixed actualQueryFrame individual has an explicitly complete empty inventory
and contributes no atom to a conjunction. It is not logic:actualWorld, a top
standpoint, a preferred observer or a proposition that holds everywhere.

QueryFrameMeetExtent is a QueryFrameExtent whose meetOperand values retain an
unordered finite set of operand extents. Use its inherited extentInterpretation,
extentFrame and extentMember rather than binary compositionLeft/compositionRight.
Every operand has the same interpretation. A complete operand inventory requires
one selected complete QueryRestrictionFrame identity per operand; unrelated raw
Standpoint or Scenario aliases do not select an atom kind.

meetOperandsComplete closes the supplied operand inventory explicitly.
meetRealized retains a source realization assertion in an explicitly complete
interpretation. Under those guards the result contains exactly the declared
domain members admitted by every operand. With zero operands it is the whole
declared domain; with one it equals that operand's extent. The existing profile
requires a nonempty interpretation domain. Empty result extents still represent
incompatibility and do not expose a usable frame.

The complete result syntax contains exactly the typed atom-set union of the
operand syntax inventories, independent of semantic realization. Intersection
of admitted contexts and union of restriction atoms describe different aspects
of the same conjunction. Neither operation is the binary universal-truth union
of standpoint contexts or the union of alternative witnesses.

meetUsableFrame names the same complete syntax frame already represented by
extentFrame, with explicit realization, complete model and inventories, and at
least one result member. A syntax name, descriptor or realization flag alone
supplies none of that evidence. Source completeness and realization assertions
remain source assertions; the ontology does not certify their authenticity or
infer global truth, consistency across all models or new sharpening relations.

## Shared IRI-key frame identity

The IRI-key profile names normalized conjunctive restriction syntax. Identical
complete typed atom sets obtain the same name regardless of insertion order,
repetition, meet grouping or explicit actual operands. A singleton wrapper keeps
its own syntax identity, distinct from its original standpoint or scenario.
Semantic equivalence under different RDF names does not identify their atoms.

The profile requires one explicit true restrictionComponentsComplete value and
the complete finite inventory for every participating syntax frame. A complete
meet inventory uses the supplied operand syntax descriptions; flatten nested
grouping by taking the union of their typed atoms. Actual contributes none.
Do not fetch descriptions, infer atoms from a frame name or replace an original
atom by a composite frame IRI. Partial inventories, conflicting completeness
claims, disjunctive alternative-witness frames, blank nodes, literals, triple
terms, unresolved relative references and undeclared kind encodings refuse
profile naming. They are never silently repaired into a weaker description.

Let F(x) be the eight-byte unsigned little-endian length of byte string x,
followed by x. Let H be unkeyed BLAKE3 with its first 32 output bytes. For each
atomic resource r, I is its exact absolute RDF IRI string after syntax and base
resolution. Use its UTF-8 bytes without Turtle brackets, prefix spelling,
serialization escaping, case folding, percent decoding, Unicode normalization,
URI-equivalence rewriting, RDF canonicalization or sameAs traversal:

```text
K(r) = H(F(UTF8("gmeow/query-frame-component-iri/v1")) || F(UTF8(I)))
```

Create native FrameComponent values from the explicit role and K(r):

| Atom role | Native kind | Kind byte |
| --- | --- | --- |
| restrictionStandpoint | Standpoint | 0 |
| restrictionCounterfactual | Counterfactual | 1 |
| restrictionImagined | Imagined | 2 |
| restrictionAssumed | Assumed | 3 |
| restrictionLicensedFalsehood | LicensedFalsehood | 4 |

Sort first by native kind order, then by unsigned lexicographic key bytes, and
remove identical typed tuples. Different roles on the same resource remain
different atoms. Nonempty input constructs one conjunctive branch, not one
branch per component. Empty input uses Frame::actual with zero branches,
not one empty branch. Keep native Frame identity unchanged:

```text
D = H(F(UTF8("purrdf-core/evidence-frame/v1"))
      || F(u64le(branch_count))
      || for each branch:
           F(u64le(component_count))
           || for each component: F(kind_byte) || F(key_32_bytes))
```

Counts are eight-byte values that are themselves length-framed. The kind is one
byte and each key is exactly 32 bytes. There is no terminator or extra NUL.
The component-key domain excludes the kind because native Frame identity
already includes its explicit kind byte.

Every framed length and count must be representable as an unsigned 64-bit
integer and by the selected target's native length type. Refuse an input or
allocation that cannot be represented or admitted; never wrap, truncate a
count, omit an atom or change the encoding. This conjunction profile has zero
or one branch and does not perform alternative-branch expansion.

Render the empty normalized atom set as the fixed vocabulary IRI
https://blackcatinformatics.ca/gmeow/actualQueryFrame. Render nonempty input as
https://blackcatinformatics.ca/id/gmeow/query-frame/iri-key-v1/
followed immediately by exactly 64 lowercase hexadecimal characters encoding
native digest D in byte order. The displayed line break is not an IRI byte.
Actual retains native zero-branch digest semantics without a second actual hash
IRI or equality axiom. Nonempty computed names belong to a data namespace;
they are not additional vocabulary individuals. This naming contract asserts
no resolver, deployment, availability or dereference behavior.

Arbitrary caller-supplied component keys are outside this IRI-key profile.
An identifier or digest alone does not establish conformity: require the full
inputs and reject a name/description conflict. A collision or dishonest claim
never merges RDF resources through an ontology rule. Formula laws constrain
declared structure and extents; they do not execute BLAKE3, verify a digest or
authenticate source descriptions. The protocol specifies a shared construction,
not a claim that a consumer has implemented or qualified it.

Interpretation is absent from the syntax identity. The same syntax carrier can
have distinct extent records in different interpretations; within one complete
interpretation the existing extent congruence law requires identical membership.
Raw scenario identity alone does not equate assumed, imagined and counterfactual
denotations, and actual premises do not automatically populate revising scenarios.

## Worked meet descriptions

All ex: resources below are fictional supplied descriptions. Their syntax names
are descriptive IRIs, not computed canonical IRI-key names or hash test vectors.
Each frame's typed inventory, operand inventory, membership and completeness or
realization claim is explicitly supplied. The nonempty domain is shared by all
extents. No type or admitted member is inferred from a descriptive name.

```turtle
@prefix ex: <https://example.org/frame/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

ex:model a gmeow:QueryFrameInterpretation ;
    gmeow:interpretationScope ex:scope ;
    gmeow:interpretationMember ex:w1, ex:w2, ex:w3 ;
    gmeow:interpretationComplete true .

gmeow:actualQueryFrame a gmeow:QueryRestrictionFrame ;
    gmeow:restrictionComponentsComplete true .
ex:standpointSyntax a gmeow:QueryRestrictionFrame ;
    gmeow:restrictionStandpoint ex:readingStandpoint ;
    gmeow:restrictionComponentsComplete true .
ex:assumedSyntax a gmeow:QueryRestrictionFrame ;
    gmeow:restrictionAssumed ex:supposition ;
    gmeow:restrictionComponentsComplete true .
ex:imaginedSyntax a gmeow:QueryRestrictionFrame ;
    gmeow:restrictionImagined ex:imaginedAccount ;
    gmeow:restrictionComponentsComplete true .
ex:mixedSyntax a gmeow:QueryRestrictionFrame ;
    gmeow:restrictionStandpoint ex:readingStandpoint ;
    gmeow:restrictionAssumed ex:supposition ;
    gmeow:restrictionImagined ex:imaginedAccount ;
    gmeow:restrictionComponentsComplete true .

ex:actualExtent a gmeow:QueryFrameExtent ;
    gmeow:extentInterpretation ex:model ;
    gmeow:extentFrame gmeow:actualQueryFrame ;
    gmeow:extentMember ex:w1, ex:w2, ex:w3 .
ex:standpointExtent a gmeow:QueryFrameExtent ;
    gmeow:extentInterpretation ex:model ;
    gmeow:extentFrame ex:standpointSyntax ;
    gmeow:extentMember ex:w1, ex:w2 .
ex:assumedExtent a gmeow:QueryFrameExtent ;
    gmeow:extentInterpretation ex:model ;
    gmeow:extentFrame ex:assumedSyntax ;
    gmeow:extentMember ex:w2, ex:w3 .
ex:imaginedExtent a gmeow:QueryFrameExtent ;
    gmeow:extentInterpretation ex:model ;
    gmeow:extentFrame ex:imaginedSyntax ;
    gmeow:extentMember ex:w2 .

ex:mixedMeet a gmeow:QueryFrameMeetExtent ;
    gmeow:extentInterpretation ex:model ;
    gmeow:extentFrame ex:mixedSyntax ;
    gmeow:meetOperand ex:actualExtent, ex:standpointExtent,
        ex:assumedExtent, ex:imaginedExtent ;
    gmeow:meetOperandsComplete true ;
    gmeow:meetRealized true ;
    gmeow:extentMember ex:w2 ;
    gmeow:meetUsableFrame ex:mixedSyntax .

ex:zeroMeet a gmeow:QueryFrameMeetExtent ;
    gmeow:extentInterpretation ex:model ;
    gmeow:extentFrame gmeow:actualQueryFrame ;
    gmeow:meetOperandsComplete true ;
    gmeow:meetRealized true ;
    gmeow:extentMember ex:w1, ex:w2, ex:w3 ;
    gmeow:meetUsableFrame gmeow:actualQueryFrame .

ex:singletonMeet a gmeow:QueryFrameMeetExtent ;
    gmeow:extentInterpretation ex:model ;
    gmeow:extentFrame ex:assumedSyntax ;
    gmeow:meetOperand ex:assumedExtent ;
    gmeow:meetOperandsComplete true ;
    gmeow:meetRealized true ;
    gmeow:extentMember ex:w2, ex:w3 ;
    gmeow:meetUsableFrame ex:assumedSyntax .

ex:counterfactualSyntax a gmeow:QueryRestrictionFrame ;
    gmeow:restrictionCounterfactual ex:alternativeAccount ;
    gmeow:restrictionComponentsComplete true .
ex:counterfactualExtent a gmeow:QueryFrameExtent ;
    gmeow:extentInterpretation ex:model ;
    gmeow:extentFrame ex:counterfactualSyntax ;
    gmeow:extentMember ex:w3 .
ex:emptySyntax a gmeow:QueryRestrictionFrame ;
    gmeow:restrictionStandpoint ex:readingStandpoint ;
    gmeow:restrictionCounterfactual ex:alternativeAccount ;
    gmeow:restrictionComponentsComplete true .
ex:emptyMeet a gmeow:QueryFrameMeetExtent ;
    gmeow:extentInterpretation ex:model ;
    gmeow:extentFrame ex:emptySyntax ;
    gmeow:meetOperand ex:standpointExtent, ex:counterfactualExtent ;
    gmeow:meetOperandsComplete true ;
    gmeow:meetRealized true .
```

The mixed meet admits only ex:w2 and actual contributes no restriction. The
explicit zero-operand meet admits the whole declared domain, and the singleton
preserves the assumed operand's extent. The last meet has an explicitly complete
operand inventory with no common member and retains an empty result without
meetUsableFrame. The named empty syntax remains a description, not a usable
frame or a proof of alethic impossibility.
