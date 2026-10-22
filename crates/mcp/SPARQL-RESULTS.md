<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# MCP query result transport

`query_docs` and `query_local` return one JSON document in the MCP text content.
The `ok` field reports query success and `form` identifies the result:

- `bindings`: SPARQL JSON `head.vars` and `results.bindings`, with absent members
  for unbound cells. Empty results retain their variable headers.
- `boolean`: the ASK `boolean` value.
- `graph`: native RDF 1.2 `graph_nquads` and `quad_count` for CONSTRUCT/DESCRIBE.

PurRDF's native SPARQL result writer owns binding syntax. Language-tagged
literals carry `xml:lang`; directional literals also carry `its:dir`. Triple
terms retain recursive subject/predicate/object records. Scoped blank nodes use
the native injective label encoding, so different native identities remain
distinct. Literal lexical forms remain strings, including typed numeric forms.

The byte-only native result writer feeds the native JSON document reader for
envelope composition. Native values remain native until compact text emission;
the enclosing MCP/JSON-RPC transport carries that text unchanged. The graph arm
uses the native RDF dataset serializer. No RDF term parser or serializer is
implemented at the MCP boundary. Native writer/reader errors, duplicate result
headers and malformed row dimensions fail rather than omit bound cells. Native
JSON document limits apply to envelope composition.

The query API returns a `SparqlResult`, with no `ResultProvenance` carrier.
Serialization supplies an explicitly empty carrier and checks that no provenance
was dropped; it invents no proof, producer identity or query hash. This transport
does not claim proof-carrying query results.

Documentation search and fixed documentation panels intentionally select lexical
strings from native results for display. Generic query responses keep complete
term records; the dedicated competency tool keeps its plural full-literal
`rationales` contract.

Missing tool arguments retain the MCP tool-error response. Query, format and
serialization failures retain the selected query's `ok:false` error envelope.
The local-query scope and external-provenance placement remain unchanged.
