<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Literal language and direction

Natural-language values can carry a public BCP 47 language tag, a canonical
private-use carrier tag, or RDF 1.2 language and direction together. Untagged
`xsd:string` is an explicit unspecified-language value. No language is guessed
from the characters, and no missing language is replaced with English.

The word-property range is `rdfs:Literal`; its canonical `logic:allValuesFrom`
restriction admits `xsd:string`, `rdf:langString` and `rdf:dirLangString`. A field
with no declared domain has the same explicit datatype union as its range and
an explicit closed validation reading. Class-scoped restrictions preserve the
subject-domain inference contract: word validation does not additionally demand
that every subject have an explicitly asserted domain type.

Canonical GMEOW-authored prose retains its private-use carrier tags. Consumer
records can retain public tags such as `@en` and directional tags such as
`@ar--rtl`. A vector family still admits only its declared languages; unspecified
language is not permission to infer one or to embed a machine value as prose.

Identifiers, wire keys, source expressions, paths and byte payloads are distinct
from natural-language values even when their lexical forms contain letters.
Use each property's declared datatype and retain its exact source semantics.
A user-supplied sentence is not a machine identifier, and a source program is
not made localizable merely by being stored as a string.
