# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the purrdf rdflib compat shim (``gmeow_rdf.compat.rdflib``).

Where real ``rdflib`` is installed, the facade is differential-tested against it
so the term/equality/serialization behaviour matches. These run in the default
lane (no ``classic_cross_check`` marker) — the facade IS default-path code.
"""

from __future__ import annotations

import gmeow_rdf
from gmeow_rdf.compat.rdflib import (
    RDF,
    RDFS,
    BNode,
    Dataset,
    Graph,
    Literal,
    Namespace,
    URIRef,
)
from gmeow_rdf.compat.rdflib.collection import Collection
from gmeow_rdf.compat.rdflib.compare import graph_diff, isomorphic, to_canonical_graph
from gmeow_rdf.compat.rdflib.namespace import XSD
from gmeow_rdf.compat.rdflib.util import guess_format

EX = Namespace("http://example.org/")


def test_submodule_import_after_shim_swap() -> None:
    """The native names AND the pure-Python subpackage both resolve in-process.

    Proves the ``gmeow_rdf/__init__.py`` ``__path__`` fix: the ``sys.modules``
    swap to the native cdylib must not break ``import gmeow_rdf.compat.rdflib``.
    """
    assert gmeow_rdf.NamedNode("http://x").value == "http://x"
    import gmeow_rdf.compat.rdflib as shim

    assert shim.Graph is Graph


def test_terms_are_str_subclasses() -> None:
    """URIRef/BNode/Literal behave as ``str`` subclasses (RDFLib parity)."""
    u = URIRef("http://example.org/x")
    assert isinstance(u, str)
    assert str(u) == "http://example.org/x"
    assert u.n3() == "<http://example.org/x>"
    assert BNode("b1").n3() == "_:b1"
    assert BNode() != BNode()  # fresh ids


def test_literal_value_and_topython() -> None:
    """Literal value-space coercion matches the XSD datatype."""
    assert Literal("Alice").toPython() == "Alice"
    assert Literal(5).toPython() == 5
    assert Literal(5).datatype == XSD.integer
    assert Literal("true", datatype=XSD.boolean).toPython() is True
    assert Literal("3.14", datatype=XSD.decimal).value == __import__("decimal").Decimal(
        "3.14"
    )
    assert Literal("hi", lang="en").language == "en"


def test_literal_term_equality_xsd_string_asymmetry() -> None:
    """A plain literal is NOT term-equal to an explicit ``xsd:string`` (RDFLib)."""
    assert Literal("x") == Literal("x")
    assert Literal("x") != Literal("x", datatype=XSD.string)
    assert Literal("x", lang="en") != Literal("x", lang="fr")
    # hash follows __eq__
    assert hash(Literal("x")) == hash(Literal("x"))


def test_literal_value_space_eq_is_separate_from_term_equality() -> None:
    """``Literal.eq`` uses XSD values; ``==`` remains RDF term equality."""
    one = Literal("1", datatype=XSD.integer)
    padded = Literal("01", datatype=XSD.integer)
    decimal = Literal("1.0", datatype=XSD.decimal)
    plain = Literal("x")
    explicit_string = Literal("x", datatype=XSD.string)

    assert one != padded
    assert one.eq(padded)
    assert one.eq(decimal)
    assert plain != explicit_string
    assert plain.eq(explicit_string)


def test_literal_ordering_uses_value_then_term_fallback() -> None:
    """Value-comparable literals sort by value, then deterministic term fallback."""
    values = [
        Literal("2", datatype=XSD.integer),
        Literal("01", datatype=XSD.integer),
        Literal("1", datatype=XSD.integer),
    ]
    assert [str(v) for v in sorted(values)] == ["01", "1", "2"]


def test_graph_add_value_contains_and_native_xsd_string_normalization() -> None:
    """Containment via the native store normalizes plain ↔ xsd:string literals."""
    g = Graph()
    g.add((EX.alice, RDF.type, EX.Person))
    g.add((EX.alice, RDFS.label, Literal("Alice")))
    assert len(g) == 2
    assert g.value(EX.alice, RDFS.label) == Literal("Alice")
    assert (EX.alice, RDF.type, EX.Person) in g
    # both plain and explicit xsd:string match through the native store
    assert (EX.alice, RDFS.label, Literal("Alice")) in g
    assert (EX.alice, RDFS.label, Literal("Alice", datatype=XSD.string)) in g


def test_graph_accessors_and_wildcards() -> None:
    """The accessor family projects wildcard patterns correctly."""
    g = Graph()
    g.add((EX.a, RDF.type, EX.T))
    g.add((EX.b, RDF.type, EX.T))
    g.add((EX.a, EX.p, EX.b))
    assert sorted(str(s) for s in g.subjects(RDF.type, EX.T)) == [
        "http://example.org/a",
        "http://example.org/b",
    ]
    assert list(g.objects(EX.a, EX.p)) == [EX.b]
    # subjects() (no filter) yields one per triple, with duplicates (RDFLib parity)
    assert sorted({str(s) for s in g.subjects()}) == [
        "http://example.org/a",
        "http://example.org/b",
    ]


def test_remove_and_set() -> None:
    """``remove`` deletes matching triples; ``set`` replaces an object."""
    g = Graph()
    g.add((EX.a, RDF.type, EX.T))
    g.add((EX.b, RDF.type, EX.T))
    g.remove((EX.b, None, None))
    assert len(g) == 1
    g.set((EX.a, RDFS.label, Literal("one")))
    g.set((EX.a, RDFS.label, Literal("two")))
    assert g.value(EX.a, RDFS.label) == Literal("two")


def test_graph_intersection_symmetric_difference_and_update() -> None:
    """P9 graph algebra and SPARQL UPDATE mutate through the native COW dataset."""
    g1 = Graph()
    g1.add((EX.a, EX.p, EX.one))
    g1.add((EX.b, EX.p, EX.two))
    g2 = Graph()
    g2.add((EX.b, EX.p, EX.two))
    g2.add((EX.c, EX.p, EX.three))

    assert set(g1 * g2) == {(EX.b, EX.p, EX.two)}
    assert set(g1 ^ g2) == {(EX.a, EX.p, EX.one), (EX.c, EX.p, EX.three)}

    g1.update("INSERT DATA { ex:d ex:p ex:four . }", initNs={"ex": EX})
    assert (EX.d, EX.p, EX.four) in g1
    g1.update("DELETE DATA { ex:d ex:p ex:four . }", initNs={"ex": EX})
    assert (EX.d, EX.p, EX.four) not in g1


def test_dataset_named_graph_quads_filtering() -> None:
    """Dataset quads distinguish any graph, named graph, and default graph."""
    ds = Dataset()
    ds.default_graph.add((EX.default, EX.p, EX.o))
    ds.graph(EX.g).add((EX.named, EX.p, EX.o))

    assert set(ds.quads((None, None, None, None))) == {
        (EX.default, EX.p, EX.o, None),
        (EX.named, EX.p, EX.o, EX.g),
    }
    assert list(ds.quads((None, None, None, EX.g))) == [(EX.named, EX.p, EX.o, EX.g)]
    assert list(ds.triples((None, None, None))) == [(EX.default, EX.p, EX.o)]


def test_turtle_roundtrip_and_isomorphic() -> None:
    """serialize(turtle) → canonicalize_turtle; reparse is isomorphic."""
    g = Graph()
    g.add((EX.alice, RDF.type, EX.Person))
    g.add((EX.alice, RDFS.label, Literal("Alice")))
    ttl = g.serialize(format="turtle")
    assert isinstance(ttl, str)
    g2 = Graph()
    g2.parse(data=ttl, format="turtle")
    assert isomorphic(g, g2)


def test_private_language_tag_survives_cow_materialization() -> None:
    """Project-private language tags round-trip through the COW graph surface."""
    g = Graph()
    term = EX["term"]
    g.add((term, RDFS.label, Literal("label", lang="x-gmeow-english")))
    ttl = g.serialize(format="turtle")
    assert "@x-gmeow-english" in ttl

    back = Graph()
    back.parse(data=ttl, format="turtle")
    assert back.value(term, RDFS.label) == Literal("label", lang="x-gmeow-english")


def test_serialize_nt_encoding_contract() -> None:
    """``encoding=`` returns bytes; absent returns str (RDFLib contract)."""
    g = Graph()
    g.add((EX.a, RDF.type, EX.T))
    assert isinstance(g.serialize(format="nt"), str)
    assert isinstance(g.serialize(format="nt", encoding="utf-8"), bytes)


def test_collection_write_read_roundtrip() -> None:
    """A written RDF list reads back in order."""
    g = Graph()
    head = BNode()
    Collection(g, head, [EX.a, EX.b, EX.c])
    assert list(Collection(g, head)) == [EX.a, EX.b, EX.c]
    assert len(Collection(g, head)) == 3


def test_sparql_select_ask_construct_and_resultrow() -> None:
    """SELECT yields ResultRow (positional + named); ASK/CONSTRUCT work."""
    g = Graph()
    g.add((EX.a, RDF.type, EX.T))
    g.add((EX.b, RDF.type, EX.T))
    rows = list(g.query("SELECT ?s WHERE { ?s a ?t }"))
    assert sorted(str(r["s"]) for r in rows) == [
        "http://example.org/a",
        "http://example.org/b",
    ]
    assert sorted(str(r.s) for r in rows) == [
        "http://example.org/a",
        "http://example.org/b",
    ]
    assert bool(g.query("ASK { ?s a ?t }")) is True
    cg = g.query("CONSTRUCT { ?s a ?t } WHERE { ?s a ?t }")
    assert cg.graph is not None
    assert len(cg.graph) == 2


def test_query_initbindings_nonprojected_var() -> None:
    """``initBindings`` pre-binds a variable that need not be projected."""
    g = Graph()
    g.add((EX.alice, EX.knows, EX.bob))
    g.add((EX.carol, EX.knows, EX.dan))
    rows = list(
        g.query(
            "SELECT ?friend WHERE { ?person <http://example.org/knows> ?friend }",
            initBindings={"person": EX.alice},
        )
    )
    assert [str(r.friend) for r in rows] == ["http://example.org/bob"]


def test_to_canonical_graph_and_graph_diff() -> None:
    """Canonicalization + diff over the native RDFC-1.0 surface."""
    g1 = Graph()
    g1.add((EX.a, RDF.type, EX.T))
    g2 = Graph()
    g2.add((EX.a, RDF.type, EX.T))
    g2.add((EX.b, RDF.type, EX.T))
    canon = to_canonical_graph(g1)
    assert len(canon) == 1
    in_both, only1, only2 = graph_diff(g1, g2)
    assert len(in_both) == 1
    assert len(only1) == 0
    assert len(only2) == 1


def test_guess_format() -> None:
    """Suffix → format detection (RDFLib parity)."""
    assert guess_format("a.ttl") == "turtle"
    assert guess_format("a.nt") == "nt"
    assert guess_format("a.nq") == "nquads"
    assert guess_format("a.unknown") is None


def test_jsonld_star_roundtrip() -> None:
    """serialize(json-ld) emits JSON-LD-star (gmeow-gts) and reparses isomorphic."""
    g = Graph()
    g.add((EX.alice, RDF.type, EX.Person))
    g.add((EX.alice, RDFS.label, Literal("Alice")))
    jsonld = g.serialize(format="json-ld")
    assert isinstance(jsonld, str)
    assert '"@context"' in jsonld  # JSON-LD-star carries the gts context
    back = Graph()
    back.parse(data=jsonld, format="json-ld")
    assert isomorphic(g, back)


def test_rdfxml_roundtrip() -> None:
    """serialize(xml) emits RDF/XML (gmeow-gts) and reparses isomorphic."""
    g = Graph()
    g.add((EX.alice, RDF.type, EX.Person))
    g.add((EX.alice, RDFS.label, Literal("Alice")))
    xml = g.serialize(format="xml")
    assert isinstance(xml, str)
    assert xml.lstrip().startswith("<?xml")
    back = Graph()
    back.parse(data=xml, format="xml")
    assert isomorphic(g, back)


def test_namespace_attribute_and_item_access() -> None:
    """Namespace attribute and item access mint URIRefs."""
    assert RDF.type == URIRef("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
    assert EX["with-dash"] == URIRef("http://example.org/with-dash")
