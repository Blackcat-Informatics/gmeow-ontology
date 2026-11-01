<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Reference apparatus

This extension preserves the technical apparatus supplied with reference-work
blocks. Core documents owns ContentBlock, structural segment kinds and caption.
This extension adds documentary cells, source fields, printed formal roles and
explicit language/rendering links. It does not parse a source, execute a listing,
render a table or check a proof (Principles 4, 12, 16 and 19).

## Source fields

`speaker` points to the supplied Entity; it does not require a Person.
`programmingLanguageAsRecorded` keeps an exact string, including meaningful
spaces. Existing software `programmingLanguage` is an ObjectProperty from
Project to Language and is not a substitute for that source field.

`equationSource` and `sourceNotation` preserve original strings before any
normalization or interpretation. Every supplied equation string has an
explicit `blockSurface` with matching `lang:surfaceText`. That language
surface follows its own sign-system, encoding, script, normalization,
collation and analysis-level contracts. Source code is not silently parsed
into an AST. A source notation name alone does not resolve a convention.

`altText` preserves an alternative description for perceiving content.
`caption` remains the visible adjacent description, owned by documents.
A figure block may contain a genuine MediaObject without being that file.
Textual fields retain language and direction when supplied; opaque code
strings remain unchanged. Record the source and interpreter on the exact
RDF 1.2 statement when a field is attributed or contested.

## Documentary cells

`cell` and its inverse `cellOf` connect a table block to TableCell records.
Each cell has one exact table, one integer `cellRow` and one integer
`cellColumn`. The table explicitly supplies a single `tableIndexOrigin`
for both coordinate axes. No implicit zero or one is inserted.

Canonical obligations require a table classification, coordinates at or above
the supplied origin, and unique coordinate pairs within the same table.
Different tables keep distinct scopes. `cellText` preserves actual text,
including an explicitly empty string; missing text is not replaced with an
empty value. The inventory need not be rectangular or globally exhaustive.

Printed cells can contain arbitrary words. They are InformationObjects, not
`math:MatrixEntry` objects, whose actual model requires scalar RationalValue
entries in a mathematical Matrix. A real mathematical interpretation can be
supplied separately; no numerical matrix is inferred from layout.

## Printed formal roles

`FormalBlockKind` is an open quality-value vocabulary containing definition,
theorem, lemma, corollary, proof, algorithm, example, exercise, solution and
note. `formalKind` carries the source’s classification, while `formalLabel`
retains its original heading or identifier. A block classified as
`segmentTypeFormalBlock` must retain at least one printed formal kind.
Additional source kinds can be named without a class per kind.

`defines` names the supplied Concept being defined. `proves` retains the
source’s asserted proof target, which may be a documentary item or an actual
supplied mathematical statement. It means the block presents an argument
for that target; it is not `math:provesStatement`. Neither field creates a
formal definition, proof object or correctness certificate.

## Actual language and mathematics

`blockRendering` points to an actual `lang:Rendering`. Its
`lang:renderedContent` reaches the supplied mathematical expression,
MathematicalStatement, Proof or other content, and its
`lang:renderingForm` reaches a retained `blockSurface`.
These are separate nodes with their actual sorts. The documentary block
does not become a mathematical object or a language form.

For expression notation, reuse `math:ExpressionRendering`, which already
specializes the language rendering model. Name its convention, kind and
preservation judgment. A SymbolReference resolves to its actual mathematical
symbol; an ApplicationExpression carries its actual operator and indexed
argument slots. A display string cannot stand in for those structures.

Mathematical statements retain their theory-scoped statement role, premises
and conclusion. A supplied Proof has its derivation and
`math:provesStatement` link. Formal verification remains a separate checker
activity, result object and attributed observation. Explicit
`logic:Unsupported` rendering preservation records that no semantic
preservation guarantee was supplied; it does not erase the retained source or
substitute a weaker parser. No parser or checker operation is selected here.

[The ordinary source scene](examples/source-apparatus.ttl) extends the
[core block inventory](../../core/documents/examples/source-blocks.ttl).
It includes all ten printed formal kinds, documentary coordinates, a
retained notation surface and separately supplied mathematical content.
These are source records, not tests, fixtures or execution receipts.
[The design](design/REFERENCE-APPARATUS.md) records the semantic boundaries.
