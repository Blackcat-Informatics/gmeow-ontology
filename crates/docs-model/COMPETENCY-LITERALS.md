<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Competency rationale literals

`gmeow:cqRationale` is nonfunctional prose. A competency question may carry
untagged `xsd:string`, `rdf:langString` and RDF 1.2 `rdf:dirLangString` values
together. The test vocabulary states their datatype union; its existing
requiredness constraint requires at least one rationale. No domain, preferred
language or single-value restriction is inferred.

The authored-source DSL resolver derives the selected class/path obligation and
the named rationale requiredness constraint through the shared logic frontend
and SHACL projectors. It composes those products with residual DSL shapes, leaving
untouched paths on their existing validation surface. Missing or unprojectable
selected obligations fail. Vocabulary source bytes, composed shapes and the
admitted implementation participate in the validation cache identity.

DocsModel and the slice-test harness retain all rationales as native `RdfLiteral`
values, sorted by lexical form, datatype, language and direction. Identical RDF
terms are deduplicated; language or direction differences remain distinct. The
model's existing typed serialization carries each literal's `lexical_form`,
nullable `datatype`, nullable `language` and nullable `direction`. Native literal
component and absolute datatype-IRI admission apply on both reads and writes.
An empty collection means no rationale; a collection containing an empty string
remains distinct. Nonliteral values fail instead of disappearing during extraction.

Coverage considers every rationale. Documentation displays every lexical value
without selecting a preferred language. The RDF projection writes each complete
literal through the native term writer, including language and base direction.
Private-use tags remain private on this source surface. The existing public
projection boundary applies its explicitly supplied canonical language-tag map;
untagged values stay untagged and base direction is retained.

The MCP `competency_questions` tool reads native SELECT bindings directly. Each
question returns `rationales`, an array of complete `lexical_form`, effective
`datatype`, nullable `language` and nullable `direction` records. The array is
sorted by those RDF term fields and deduplicates identical terms; it has no
preferred value. Missing rationales produce an empty array. Query text and the
optional signed 64-bit row count and boolean exact-row expectation retain their
existing output fields. Nonliteral rationales, malformed typed expectations and
conflicting scalar fields fail the selected operation.

The test DSL has its own explicit ontology/catalog owner. Shared translation
lint, merge, export, glossary and reviewed-input consumers use that owner rather
than a slice identity. The French and Chinese term batch covers the rationale
and its requiredness record; other untranslated test-vocabulary fields remain
visible to coverage and are not claimed translated by this batch.

The registered DSL owner inventory also selects fixture and pipeline cache
inputs. Vocabulary sources and each selected PO catalog bind the model fixture,
documentation source and mapping-stage identities. Fully absent owners are
outside that operation; partially present or malformed selected owners fail.
DocsModel schema version 20 identifies the plural native rationale records.
