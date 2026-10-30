<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Query losses and frame composition

This slice owns descriptive query records and composition descriptors. It consumes
the grounding status vocabularies and existing Standpoint/Scenario types by
reference. It introduces no query executor, query grammar, program operator,
convergence engine or sink policy.

## Loss identity and independent status

A loss is a reified information object, not an opaque string on a projection
certificate. Its concerned resource and cause are required; the affected query
constraint is retained when known. A reader acknowledgement keeps the original
loss resource anchored in a ledger. No relation models acknowledgement as repair,
erasure or permission to bypass a selected operation’s requirements.

Query answers use local fields with the existing information, evaluation and
completeness values. Class-local closed value sets prevent information non-results
from being relabelled as completeness. At-most-one bounds apply to one attributed
answer record, not to all source reports. Ledger membership and answer-ledger
links are nonfunctional so identity-preserving composition remains possible.

## Explicit extensional model

The interpretation domain and frame memberships are actual RDF relations. An
interpretation scope identifies what those entries purport to cover. Source
completeness is a model assumption, not evidence that an arbitrary sample covers
a larger universe. Partial or exploratory descriptions do not expose a usable
result merely because their nodes exist.

The model laws use binary operand descriptors to keep the operation explicit.
All operands and results share the same interpretation. Every extent member is
in that interpretation’s domain. Empty extents are representable, because an
incompatible conjunction is information worth retaining. In an explicitly complete
interpretation, two extent records sharing both `extentInterpretation` and
`extentFrame` have the same members. `ExtentFrameCongruenceLaw` universally
quantifies ordered record pairs, so its membership implication applies in both
directions without equating the records themselves.

| Operation | Directional canonical laws |
| --- | --- |
| Standpoint meet | Result membership implies membership in both operands; common membership implies result membership. |
| Both-standpoint truth domain | Result membership implies membership in at least one operand; each operand’s members belong to the result. |
| Scenario conjunction | Result membership implies membership in both scenario operands; common scenario membership implies result membership. |

Together, each pair or triple of laws is an exact set equation in the declared
complete interpretation. They are realized `logic:Constraint` records with
`logic:Formula` trees. Each is an outer universal guarded implication, without
an unprojectable biconditional or positive universal subformula. The distinction
between the operations is in the actual membership conditions and typed operands,
not just their display labels (Principle 17).

Usable-result evidence additionally requires a nonempty realized compatible result
represented by the named frame. Incompatible results are empty and retained. The
interpretation’s completeness and realization flags remain attributed source
assertions, never runtime-verified certificates. No method manufactures a frame,
member or global truth from a declaration.

## Order and truth

`sharpens` is admitted-set containment from the more specific to the broader
standpoint. The intersection, if nonempty and represented, is the greatest lower
bound in the set interpretation: every other represented lower bound is contained
in that intersection. Being a poset alone provides no existence theorem and does
not require the permitted frames to be closed under intersection.

Universal truth reverses the tempting shorthand. Requiring truth throughout both
sets needs their union, not their meet. The union descriptor only constructs that
required domain; it does not establish that any particular proposition is true.
Scenario conjunction concerns entertained suppositions and is separately typed.
No result is inferred through a generic context-accessibility superproperty.

## Source ownership

All declarations, universal restrictions, closure entries, intrinsic cardinalities
and realized integrity laws are authored in `module.ttl`. OWL/RDFS, SHACL and ShEx
remain generated views, with no second constraint surface (Principles 4 and 17).
The manifest declares only core dependencies. Existing large grounding and
standpoint owners remain unchanged.
