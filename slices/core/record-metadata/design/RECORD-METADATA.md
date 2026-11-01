<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Record metadata design

The shared abstraction is metadata about a record and its content occurrences.
Domain membership is unnecessary for recording time, modification, order or
creation-context exposure. Only the facet-specific predicates imply
`LabelledFacet`. This keeps the metadata useful across record types without
turning every record into a note, document or observation.

## Canonical obligations

Principles 4 and 17 place the vocabulary and its obligations in `module.ttl`.
The slice consumes the grounding kernel’s `logic:` vocabulary. OWL, SHACL and
ShEx remain generated projections; this slice authors none of those surfaces.

| Surface | Canonical obligation |
| --- | --- |
| Recording and modification literals | Anonymous data-range union of four standard XSD temporal datatypes; explicit property-global closed-world range reading. |
| Period references | Range union of the existing `TimeInterval` and `NamedPeriod` classes. |
| Ordinal | Standard `xsd:nonNegativeInteger` range with explicit property-global closed-world reading. |
| Facet label | Universal `xsd:string` / `xsd:token` union restriction, class-scoped closed-world required path and canonical functionality assertion. |
| Facet label wording | Optional, nonfunctional literal relation with property range and class-local universal restriction over `xsd:string`, `rdf:langString` and `rdf:dirLangString`. |
| Facet payload | Literal restrictions, separate functionality assertions and realized integrity formulas excluding simultaneous text, literal and structured channels. |
| Concept interpretation | Nonfunctional relation to existing `Concept`; no preferred interpretation. |
| Context identity | Realized guarded-existence formula requiring `contentDigest` on every referenced context artifact. |

The required label uses a universal restriction plus explicit closure rather
than an existential that could create a witness. Its functionality record sets
at most one label. The original payload exclusion formula applies to subjects of `facetText`; the
structured-channel integrity formula excludes both literal channels and bounds
its own payload by RDF identity. A label-only facet remains permitted. Together
these obligations bound the total payload to one without turning absence into
null.

The context identity formula applies to objects of `contextIncluded`.
The release and projection properties specialize that relation through canonical
`logic:subPropertyOf` axioms. The input role distinguishes the referenced bytes
without inventing a new artifact class or claiming that every compiled artifact
is a published manifestation. Multiple artifacts and multiple digest algorithms
remain representable. Digest lexical validation belongs to the existing digest
contract, not a provider invented by this slice.

The temporal range is anonymous: literals retain their actual standard datatype.
A new named datatype would imply a different literal type and lose that contract.
Coarse values carry no inferred period-start timestamp or timezone. A calendar
year literal and a named historical period remain distinct representations.
The model does not impose an ordering comparison between coarse, potentially
incomparable creation and modification values.

## Bridges follow meaning

`noteModifiedAt` and `dateModified` specialize `updatedAt`: both date the last
modification of their subject resource. Their owners retain the existing
narrower domains and timestamp ranges. The email-owned `participantOrdinal`
specializes `entryOrdinal` in the email module, preserving the extension-to-core
dependency direction.

No unconditional recording bridge is made from `assertedAt` (assertion witness),
`atTime` (occurrence), `sentAt` (dispatch), `dateCreated` (creative-work creation),
`ingestedAt` (recording imported claims in an import activity) or `noteCreatedAt`
(note creation rather than an independently established transaction clock).
The recording time of the import activity’s own record is a separate fact.

`segmentIndex` and `hopOrdinal` are one-based; `positionOrdinal` depends on a
frame. They do not specialize the zero-based ordinal. An importer must interpret
and, where appropriate, convert its source sequence convention explicitly.
Neither a recording value nor an ordinal is declared globally functional: the
model does not select a privileged clock, sequence or standpoint.

Context inclusion has no subproperty bridge to use, derivation or grounding.
Exposure is the entire assertion. A consumer may record stronger evidence
separately without obtaining it from mere co-presence in a context.

## Content fidelity

A label is a literal, including any producer-supplied relative-path notation.
Both `facetLabel` and its `malformedField` specialization admit the supplied
`xsd:string` or `xsd:token` datatype through explicit unions. Retain the original
lexical form without string conversion or whitespace normalization. The
malformed-field universal and qualified maximum use the same union. Position
names prevent key spelling from determining vocabulary or identity. No reversible
object reconstruction is promised from ambiguous path labels alone. A consumer
requiring the original serialization retains that artifact separately.

`facetLabelAsWritten` distinguishes supplied label wording from the required
machine key or relative path. Its anonymous datatype union and the
`LabelledFacet` all-values restriction preserve untagged, language and
directional language strings. Neither a required path nor a cardinality bound
is added. Several source wordings may coexist with their own statement
provenance; missing language or direction is not guessed.

The property specializes the existing `lang:literalValue` carrier, whose range
is `rdfs:Literal` and whose subject has no domain restriction. This preserves
the complete label literal without coercing the facet into a different subject
class. The manifest declares the direct dependency on the language grounding
owner. This is a semantic specialization, not property equivalence or a
literal conversion.

No subproperty bridge to `facetLabel` is asserted: that key's machine-datatype
contract must not erase language or direction from wording. No bridge to
`facetText` is asserted either: label wording is metadata about the label,
not the field's payload. The required key closure and functionality, both
literal payload functionality records and their mutual exclusion remain intact.
The structured channel adds a separate exclusion obligation against those literal
channels.
Retaining wording does not synthesize a key, select a preferred interpretation
or promise source serialization reconstruction (Principles 2, 4, 9 and 17).

Typed scalars keep their source datatype and lexical form. Text keeps source
language and direction when known. No language, missing value or digest is
fabricated. Label-only facets preserve the existence of the key without inventing
a universal null datatype. These choices apply Principle 4 to imported content
and Principles 9 and 10 to reviewed concept interpretations.

## Intake fidelity and source ownership

Reception metadata describes a holder-specific occurrence rather than a shared
proposition. The open channel vocabulary and a guarded per-record uniqueness
formula preserve that scope without a globally functional property identifying
different channels. Missing metadata has no inferred default.

Rejected field occurrences reuse the facet identity and payload contracts.
Only the supplied field and raw representation become required on
`MalformedValue`; unrelated records acquire no mandatory fields. Universal
restrictions, typed local maxima and class-scoped `ClosedWorldClosure` entries
provide these requirements. Existing subproperty inference carries their
meaning into the generic facet path. No source path syntax, repair strategy or
contract interpreter is added.

Verbatim producer references admit supplied `xsd:string` and `xsd:token`
literals through anonymous canonical datatype unions. `originatingTrace`,
`originatingPacket` and `evidenceReference` retain their domain-free provided-value
closures: they declare the admissible datatypes of supplied references without
requiring references on unrelated records. The union retains the exact lexical form and datatype,
without whitespace normalization, inferred token typing, reference resolution
or producer-namespace equivalence. It adds no lexical-checking implementation.
Human wording retains its supplied language and direction, including plain
strings where language was not stated. Attribution and uncertainty remain on
the existing provenance and statement layers. Description subproperties record
stated reasons without turning those reasons into an execution receipt.

The suppression law follows one reason reifier to the already defined lowered
statement components. Its false boolean value and displayable predicate concern
that suppression statement alone. It relies on completed statement-metadata
lowering and does not broaden that mechanism's documented supported fragment.

Held-record terms remain beside the existing span owner in AI, which already
depends on record metadata. Tags and note purpose wording stay with their
respective owners; encoded byte length stays with the source artifact owner.
These directions avoid an AI/record-metadata dependency cycle. Every new
obligation is authored in canonical logic; no shape surface is authored
(Principles 4, 7, 9, 10 and 17).

## General stated rationale

statedRationale follows the existing domain-independent metadata design.
Its canonical range is the anonymous union of xsd:string, rdf:langString and
rdf:dirLangString, with property-global ClosedWorldClosure validating supplied
values. The closure demands no rationale on unrelated records. No functional
characteristic, preferred reason or computed-cause bridge is introduced.

resolutionRationale remains agentic-owned and retains its AttentionResolution
domain; only its semantic specialization points here. The generic property
does not specialize description, whose Entity domain would coerce an Event
bearing a rationale into an endurant. Other narrower reason fields retain their own
meanings and are not generalized incidentally.

Exact authored statements can carry native writer and standpoint attribution.
No new integrity law requires a reifier for every general rationale: a generic
edge inferred from a specialized statement is not newly authored source evidence.
The original specialized statement and its provenance remain canonical
(Principles 2, 4, 9 and 17).

## Structured payload occurrences

`StructuredValue` is an `InformationObject` subkind. Map, sequence and null
subkinds are pairwise disjoint. A realized branch formula requires exactly one
of scalar-term presence, map, sequence or null. Only maps admit `hasFacet` and
only sequences admit `structuredItem`; null and scalar branches admit neither.
Typed empty containers therefore remain distinct from explicit null, an empty
literal and an absent relation. Maps reuse labelled facets; sequence values are
occurrences with no fabricated source key.

The scalar carrier `structuredTerm` is an annotation property. No OWL role
restriction or SHACL node-kind approximation narrows its object to resources or
literals. The realized integrity formula requires its subject to be a
`StructuredValue` and limits it to one original RDF term using the grounding
kernel's `termSameComponent` with `componentWhole`. This comparison preserves
RDF identity, including literal lexical form, datatype, language and direction,
and does not compare scalar payloads by datatype value equality.

The container ordering formulas use numeric `termEqual` and `termDistinct` for
bound, valid `entryOrdinal` literals. They preserve lexical aliases of a numeric
position, reject multiple numeric positions on one entry and reject a shared
position between distinct entries within one owner. Sequence position is required;
map position is optional, so unordered source objects need no invented order.
The existing property-global ordinal closure supplies the nonnegative datatype
obligation. Repeated map labels and repeated sequence values remain legal on
distinct occurrence nodes. No contiguous-index or finite-tree law is asserted.

`facetStructuredValue` complements the literal channels; it does not reinterpret
`facetConcept` or the label-only state. Agentic consumes this slice through its
existing dependency for separate arguments and result roles. Opaque media typing
uses existing `mediaType` and `charset` on the role-bound payload occurrence,
never an ambiguous format on the containing call. Neither representation implies
that another representation is equivalent. These are canonical vocabulary and
integrity laws, not a converter or a storage DTO contract (Principles 2, 4, 17
and 19).
