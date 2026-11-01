<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Fictional identity and source wording

A fictional character is an Entity with an identity local to one narrative
reference frame. `Character` is explicitly disjoint from `Person`; a novel’s
representation of a historical person is a separate entity. An explicitly
supported cross-realm correspondence uses the existing `counterpartOf`,
which is symmetric and deliberately nontransitive. Shared spelling does not
supply that correspondence or merge referents (Principles 4, 5 and 9).

The existing `hasReferenceFrame` in places is the universal, domain-free
frame spine. Character has a class-local required, single
`NarrativeReferenceFrame` on that relation. There is no new frame predicate
or global functional carrier. `arcFrame` would incorrectly cast the character
as a CharacterArc, while `sourceFor` would cast it as a CreativeWork.
`accordingTo` is an annotation on a claim’s context, not the identity frame
property of a fictional entity.

The actual Entity ranges of `arcSubject` and `narrativeRoleBearer` already
accept Character. Canonical formula obligations require an arc’s supplied
frame to agree with its Character subject’s frame, and require agreement for
a role whose scope is explicitly a narrative frame. The role check does not
reinterpret works or segments as frames. Separate continuities use separate
Character identities. These are intrinsic consistency obligations, not a
reasoner run or a verdict on an interpretation’s truth.

`arcPositionLabel` preserves the original arc-phase field on its actual
bearer. Its domain is deliberately open: a Character, a source occurrence or
an information record need not become an ArcSample or CharacterArc. A
canonical formula requires the bearer to retain an explicit narrative frame.
The literal does not construct a NarrativePosition or classify a BeatFunction.
It also does not supply `sampleState` or bypass the existing required fields
of an ArcSample.

The three prose properties use the union of `xsd:string`,
`rdf:langString` and `rdf:dirLangString`. Each is nonfunctional.
A language tag and a direction are preserved when supplied; an untagged
string does not invent a language. `motivation` and `relationshipText`
have Character domains. They describe attributed source wording, rather than
constructing an intentional mode, a resolved contact relation or endorsement
by another character (Principles 2, 9 and 17).

The scene attaches source and viewpoint to exact RDF 1.2 statements.
Documentary `wasDerivedFrom` belongs to those statement carriers or to the
actual source record, not to the fictional character simply because a document
describes it. `wasAttributedTo` names the recorder; `accordingTo` retains
the interpreter’s standpoint. The character’s narrative identity frame
remains a separate coordinate. Conflicting interpretations therefore coexist
without conflating source, viewpoint and referent.

The logic design’s context-indexed semantics distinguishes in-frame claims
from out-of-frame descriptions; its World and AttributedContext machinery
does not identify a fictional participant with a Person or make a mere frame
binding an executed contextual evaluation. No existing fictional-object class
in the inspected canonical logic declarations supplies the Character identity
needed here. The new class uses the existing Entity root without introducing
a competing context model.

The language design distinguishes source surfaces from analyzed forms.
These fields retain original, possibly unanalyzed prose. A language tag alone
does not manufacture a `lang:Form`, denotation, analysis or translation.
When an actual analyzed form is supplied, the existing language structures
remain available with their own sign-system, rendering and anchoring
contracts. No new language or mathematics engine is necessary to preserve
these strings, and no quantified mathematical object is asserted by them
(Principles 11 and 19).

Direct dependencies on entities, coreference and standpoint are explicit.
All new semantics are canonical `logic:` axioms or realized formula trees;
validation shapes remain generated views. The addition contains ordinary
source guidance and a scene, with no tests, fixtures or competency machinery.
