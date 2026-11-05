<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# EPUB source records and title occurrences

Documents owns the source-supplied publication structure represented by
EpubSpineEntry accounts and their TOC paths. An account is an InformationObject
record, not the href's resolved target and not a claim that an EPUB parser
accepted the source or that the package inventory is complete. Distinct coherent
accounts remain distinct even when fields match. The existing source information
object can be associated through the optional, nonfunctional epubSpineEntry;
no extra package, base URI, revision or digest identity is introduced.

The order and depth fields are source-reported integer descriptors. Interpret
them only under the actual source convention. Their exact thirteen-member XSD
integer-family datatype union preserves supplied integer subtype and lexical
identity; it neither coerces strings nor imposes a universal base, bound or
comparison policy. A descriptor is not a computed count, canonical tree depth
or sequence occurrence position, and an arbitrary integer code does not become
interpreted order or depth. These are documentary source descriptors rather
than mathematical Quantities with invented frames or units.

Each EpubTocPathStep specializes the existing StructuredValue as the real ordered
occurrence. Its sole scalar structuredTerm is its complete title, and its depth
belongs on that same occurrence. A second title property or a separate wrapper
would add a lexical authority or identity distinction unsupported by the model.
The full word-literal union preserves blank text, exact spacing, language and
RDF 1.2 direction; untagged input remains xsd:string. The existing scalar branch
and whole-term identity laws remain authoritative.

A path reuses StructuredSequence, structuredItem and the actual zero-based
entryOrdinal occurrence position. Source spine order and reported depth remain
separate fields. Repetition uses distinct occurrence nodes; no identity, tree,
contiguous-index rule, missing prefix or position pairing is inferred. An absent
path and an actually supplied current zero-item path remain distinct, without
reconstructing historical field presence or text lost before the current input.

Ordinary logic all-values restrictions, class-local required-path closures and
total literal maxima require each account's order and href and each typed step's
depth. The path remains optional and locally at most one using the universal
object filler. One realized Formula requires every role-selected path item to
be a step. Another requires a scalar word title on every typed step, including
standalone ones: two independently guarded arms express title presence and
every-term datatype admission without a mixed unscoped existential/universal
projection. No generic payload or sequence law is weakened.

Documents directly declares RecordMetadata because these are real reused
carriers, independent of their mutually dependent core composition. Source
ownership remains documentary. Titles already inherit complete-literal semantics
through structuredTerm, so no duplicate lang:literalValue role or direct Lang
term dependency is needed. No grounding peerage, seam or external correspondence
is changed. These source-record semantics make no EPUB standards-conformance
claim.

The guide's fictional scene retains supplied account, association, membership,
title, depth and position facts with native RDF 1.2 exact-statement provenance.
No context is invented to fill unavailable source identity. These canonical
logic declarations and guidance add no parser, URI resolver or execution engine,
and the scene is not evidence of executed validation (Principles 2, 4, 9, 17
and 19).
