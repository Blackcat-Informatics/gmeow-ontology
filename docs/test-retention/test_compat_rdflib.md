# Retention: `tests/test_compat_rdflib.py`

**Category:** PyO3 seam

## What it tests

Tests for the purrdf rdflib compat shim (``purrdf.compat.rdflib``).

Retained dynamic tests:

- `test_submodule_import_after_shim_swap` — The native names AND the pure-Python subpackage both resolve in-process.
- `test_terms_are_str_subclasses` — URIRef/BNode/Literal behave as ``str`` subclasses (RDFLib parity).
- `test_literal_value_and_topython` — Literal value-space coercion matches the XSD datatype.
- `test_literal_term_equality_xsd_string_asymmetry` — A plain literal is NOT term-equal to an explicit ``xsd:string`` (RDFLib).
- `test_literal_rewrap_preserves_integer_subtype` — Re-wrapping a typed literal preserves its exact datatype IRI.
- `test_literal_value_space_eq_is_separate_from_term_equality` — ``Literal.
- `test_literal_ordering_uses_value_then_term_fallback` — Value-comparable literals sort by value, then deterministic term fallback.
- `test_graph_add_value_contains_and_xsd_string_provenance` — The shim preserves RDFLib plain-vs-explicit xsd:string term provenance.
- `test_graph_keeps_plain_and_explicit_xsd_string_as_separate_terms` — RDFLib stores plain and explicit xsd:string literals as distinct terms.
- `test_parsed_graph_string_patterns_use_native_value_space` — Parsed/native graphs have no shim provenance, so string lookups stay broad.
- `test_graph_numeric_literal_contains_uses_value_space` — Numeric object patterns keep the RDFLib value-space containment behavior.
- `test_graph_accessors_and_wildcards` — The accessor family projects wildcard patterns correctly.
- `test_remove_and_set` — ``remove`` deletes matching triples; ``set`` replaces an object.
- `test_graph_intersection_symmetric_difference_and_update` — P9 graph algebra and SPARQL UPDATE mutate through the native COW dataset.
- `test_dataset_named_graph_quads_filtering` — Dataset quads distinguish any graph, named graph, and default graph.
- `test_turtle_roundtrip_and_isomorphic` — serialize(turtle) → canonicalize_turtle; reparse is isomorphic.
- `test_private_language_tag_survives_cow_materialization` — Project-private language tags round-trip through the COW graph surface.
- `test_serialize_nt_encoding_contract` — ``encoding=`` returns bytes; absent returns str (RDFLib contract).
- `test_collection_write_read_roundtrip` — A written RDF list reads back in order.
- `test_sparql_select_ask_construct_and_resultrow` — SELECT yields ResultRow (positional + named); ASK/CONSTRUCT work.
- `test_query_initbindings_nonprojected_var` — ``initBindings`` pre-binds a variable that need not be projected.
- `test_to_canonical_graph_and_graph_diff` — Canonicalization + diff over the native RDFC-1.
- `test_guess_format` — Suffix → format detection (RDFLib parity).
- `test_jsonld_star_roundtrip` — serialize(json-ld) emits JSON-LD-star (gmeow-gts) and reparses isomorphic.
- `test_rdfxml_roundtrip` — serialize(xml) emits RDF/XML (gmeow-gts) and reparses isomorphic.
- `test_namespace_attribute_and_item_access` — Namespace attribute and item access mint URIRefs.

## Why it cannot be deleted or moved to Rust today

Tests Python-to-Rust marshalling and error surfacing for the PyO3 binding, which Rust cannot exercise from the inside.
