Integrity consumers holding native RDF values could not use SPARQL equality without copying its semantics or constructing queries. The existing comparator is now public as `equal_values`, with its language-string profile and the caller's workspace capability. Internal expression callers use the same function, and the obsolete private name is removed.

Native and WASM tests match the query operator across 14 representative pairs, including scoped blank nodes, numeric values, language and direction, triple terms, composites, and expression errors. A bounded-account test preserves the original allocation refusal and verifies that charges are released. Strict affected Clippy and normal hooks passed. The hosted native and WASM comparison tests also passed.

The downstream integrity implementation is still in progress. The separate imported-closure proof replay defect remains open; this change does not resolve it or establish that the whole workflow passed.

Refs #590.
