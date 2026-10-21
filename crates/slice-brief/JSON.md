<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Authoring packet JSON

`AuthoringPacket::to_json()` returns two-space pretty JSON without a terminal
newline. Its public return type remains `String`. The native
`purrdf_lex::json::record::{ToJson, FromJson}` contracts also encode and decode
the packet model directly. Records emit members in declaration order and vectors
retain their supplied order; assembly owns the packet's deterministic ordering.
Semantic content digests and Turtle rendering have their own unchanged owners.

Optional fields always appear as JSON null when absent. Decoding accepts either
an absent member or null as `None`. Known duplicate record members are refused
with their JSON location, while unknown record members are ignored. Counts and
batch indices require unsigned integer lexemes within their Rust widths;
fraction and exponent spellings do not satisfy integer members.

Grounding attributes use the JSON names `En`, `Fr`, `Zh`, `ExternalMapped` and
`Exemplar`. These names are distinct from their RDF tag and IRI spellings.
Object terms have exactly one externally tagged member:

```json
{"Iri":"urn:example"}
{"Blank":"b0"}
{"Literal":{"lexical":"é","datatype":"urn:datatype","language":null}}
```

Native JSON admission requires object records and string grounding attributes.
Positional record arrays and singleton-map unit enums incidentally admitted by
the former serde decoder are refused. The object-term variant wrapper remains
an object with exactly one of `Iri`, `Blank` or `Literal`; it does not ignore
additional variant members.

Confidence decodes through the native correctly rounded finite binary64 reader,
including difficult decimal rounding cases. Valid numeric input normalizes to
the native owner's shortest round-trip binary64 spelling, such as `1e0` becoming
`1.0`; numeric lexemes are not retained in this typed model. Overflow such as
`1e400` is refused. A manually constructed NaN or infinity also fails explicitly
on native export, including through the public string renderer, rather than
becoming null. The existing infallible string API reports this invalid export
with an actionable panic naming the grounding cell. Wire decoding adds no
confidence range or ontology constraints; those remain with their domain owners.
