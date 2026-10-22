<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Norm and source-contract observation JSON

The validation norm-claims channel, terminal shipped-claim census, and conformance
superset source-contract channel use the pinned native JSON record owner. Readers
still authenticate the selected action or terminal artifact before decoding;
decoding does not run a closure, parse a source document, or manufacture a verdict.

Compact UTF-8 has no trailing newline. Records retain declaration order:

* Shipped census: `graph`, `graph_digest`, `asserted_quads`, `advisory_assessments`.
* Closure: `source_path`, `source_digest`, `source_quads`, `closure_triples`.
* Norm envelope: `claims`, `closure`.
* Source contract: `source_path`, `source_digest`, `fanout_rules`,
  `expected_outputs`, `definitions`.
* Definition: `text`, `values`.
* Rule: `path`, `match_prefix`, `suffix`, `family`, `form`.
* Recorded rejection: `root`, `causes`, using the existing native diagnostic-node
  payload without recalculating fingerprints or discarding causal nodes.

Vectors preserve input order; sets and string-keyed maps are sorted. Duplicate
map keys remain last-wins after each value is admitted. Census and closure counts
require unsigned integer tokens that fit `usize`; negative zero, negative values,
fractions, exponents, and overflow refuse with the field's native error pointer.

`closure` and definition `values` retain externally tagged `{"Ok":payload}` or
`{"Err":recorded-rejection}` objects with exactly one member. Family unit tags
are `RdfFanout`, `Edoal`, `Opaque`, `HeaderDict`; form unit tags are `Turtle`,
`NTriples`, `NQuadsSelf`, `Blob`, `HeaderDict`. Units write as Pascal-case strings
and also admit their exact single-member null-object forms. `NQuads` has a string
payload, for example `{"NQuads":"urn:graph"}`. Decoding retains that IRI;
actual hydration still asserts the one supported native dispatch graph.

Records require named objects. Incidental positional record arrays are not part
of this producer-owned transport. Known duplicate fields refuse and unknown
fields remain ignored. All fields are required except rule `suffix`, whose
absence or explicit null decodes as `None`; writers retain explicit null. An
empty suffix string remains present. Null required collections refuse; empty
collections remain explicit. Malformed or trailing JSON refuses.

Other observation leaves and their Serde-bound execution caches remain separate.
The shared recorded-rejection type retains Serde for those genuine embeddings.
RDF commitments, selector checks, graph dispatch, source identities and grading
are unchanged by this transport.
