<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Source apparatus and mathematical content

Documents retains ownership of ContentSegment, its open structural-kind
vocabulary and caption. ContentBlock is a SubKind inheriting ContentSegment
identity. Its atomicity applies to a selected source segmentation and later
retrieval cuts, not to every possible textual or semantic decomposition.
Immediate blockOf specializes segmentOf; only segmentOf is transitive.
A local required container and the existing segment index/type carriers keep
position and ancestor relationships distinct.

This extension owns genuinely new source apparatus. It is an extension for
reference-work consumers, not a mandatory core engine. In particular, lang
already depends on documents, so the language-dependent apparatus consumes
both owners downstream instead of adding a reverse documents dependency.
Math, lang, concepts, entities, provenance, sources and standpoint are explicit
dependencies. No mathematical or linguistic foundation is copied here.

The twelve structural block kinds are paragraph, heading, verse, dialogue,
stage direction, code listing, equation display, table, figure, formal block,
epigraph and quotation. They use ContentSegmentType individuals, including
the existing paragraph value. Volume is another structural value, not a
Manifestation class or physical copy. FormalBlockKind is a separate open
printed-role axis because algorithms, exercises and notes are documentary
roles, while math’s statement roles are theory-scoped mathematical status.

Programming-language wording cannot reuse software’s programmingLanguage:
its actual domain is Project and its range is Language, whereas the source
field is a string. The source spelling and any independently supplied
sign-system identity therefore remain separate. Likewise, sourceNotation
does not manufacture MathematicalNotation identity or a meaning-preservation
judgment.

An exact equation string requires a matching retained language surface.
BlockRendering requires actual content and a retained surface form. These
canonical formula obligations enforce intrinsic supplied-data linkage.
SurfaceForm’s own sign-system and analysis contracts still apply.
UnanalyzedProse explicitly records a raw surface without claiming a parsed
language form; a mathematical interpretation can be supplied separately
through a genuine expression AST and ExpressionRendering.

TableCell is an InformationObject with a table-local pair of integer source
coordinates and an explicit index origin. The origin is part of the source
coordinate contract, not an optional parser fallback or a mathematical AST
slot convention. Every supplied cell must have it. Duplicate coordinate
records are rejected through comparison obligations rather than identity
merging. The laws neither fill absent cells nor execute table layout.
Math’s MatrixEntry would be false here: its actual owner requires a scalar
RationalValue in a Matrix representing a linear or bilinear structure.

A printed proof target remains a source assertion on a ContentBlock.
The actual mathematical proof, when supplied, is a separate math:Proof
derivation reached through lang:renderedContent, with its own
math:provesStatement. Theorem status remains theory-scoped and warranted.
A checker event, a FormalVerificationResult and a held observation are not
fabricated from a printed label or an unexecuted source derivation.

Caption’s canonical domain is one union of MediaObject and ContentSegment;
two domain axioms would instead impose both types. The schema profile reads
raw source types without ontology closure. Its media branch therefore admits
MediaObject and the schema MediaObject, ImageObject, AudioObject and
VideoObject classes explicitly. The structural branch admits ContentSegment,
ContentBlock and the current MusicalSegment and ToneEvent descendants.
Supported equality/disjunction filters dispatch these declared classes
without assuming inferred superclass triples or inventing a new executor.

The structural branch retains text as schema:description and discloses the
loss of caption role and attachment semantics. It does not classify the
structural bearer as a media file or claim exact caption equivalence.
Reference apparatus owns this domain-specific mapping instance’s annotation
coat and translations; the actual profile pattern and binding remain in the
shared mapping DSL. The same individual has one set of annotation facts and
one set of operational profile facts.

All constraints are realized logic:Formula trees with direct property guards.
Required paths and at-most-one fields use canonical class-local restrictions
and ClosureEntries; no global functional carrier, handwritten shape,
parser, producer, proof checker or table executor is introduced.
Source attribution uses RDF 1.2 statements and preserves conflicting readings.
These choices implement Principles 2, 4, 5, 9, 11, 15, 16, 17 and 19.
