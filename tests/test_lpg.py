"""Tests for the LPG (Labeled Property Graph) export.

Covers: node/edge extraction, statement-metadata-as-edge-properties,
standpoint parallel multi-edges, reifier exclusion, format validity,
round-trip information preservation.
"""

from __future__ import annotations

import csv
from pathlib import Path

import pyoxigraph
import pytest

from gmeow_tools.lpg import (
    LPGEdge,
    LPGGraph,
    LPGNode,
    _curie,
    _short_key,
    _value_from_term,
    build_lpg,
    serialize_cypher,
    serialize_generic_csv,
    serialize_graphml,
    serialize_neo4j_csv,
)


def _make_store_from_ttl(ttl: str) -> pyoxigraph.Store:
    """Load a Turtle string into a fresh pyoxigraph Store.

    Note: pyoxigraph's Turtle parser does not support RDF-star ``<<(...)>>``
    syntax for synthetic strings — it only works when Jena produces the file.
    This helper is therefore limited to plain Turtle.
    """
    import tempfile

    store = pyoxigraph.Store()
    with tempfile.NamedTemporaryFile(mode="w", suffix=".ttl", delete=False) as f:
        f.write(ttl)
        path = f.name
    try:
        store.load(path=path, format=pyoxigraph.RdfFormat.TURTLE)
        return store
    finally:
        Path(path).unlink(missing_ok=True)


class TestCurieShortening:
    def test_curie_known_prefix(self) -> None:
        assert _curie("https://blackcatinformatics.ca/gmeow/Person") == "gmeow:Person"

    def test_curie_unknown_returns_full(self) -> None:
        assert _curie("https://example.org/Thing") == "https://example.org/Thing"

    def test_short_key_from_curie(self) -> None:
        assert (
            _short_key("https://blackcatinformatics.ca/gmeow/familyName")
            == "familyName"
        )


class TestValueFromTerm:
    def test_literal_integer(self) -> None:
        lit = pyoxigraph.Literal(
            "42",
            datatype=pyoxigraph.NamedNode("http://www.w3.org/2001/XMLSchema#integer"),
        )
        assert _value_from_term(lit) == 42

    def test_literal_decimal(self) -> None:
        lit = pyoxigraph.Literal(
            "3.14",
            datatype=pyoxigraph.NamedNode("http://www.w3.org/2001/XMLSchema#decimal"),
        )
        assert _value_from_term(lit) == 3.14

    def test_literal_boolean(self) -> None:
        lit = pyoxigraph.Literal(
            "true",
            datatype=pyoxigraph.NamedNode("http://www.w3.org/2001/XMLSchema#boolean"),
        )
        assert _value_from_term(lit) is True

    def test_literal_datetime(self) -> None:
        lit = pyoxigraph.Literal(
            "2026-01-01T00:00:00Z",
            datatype=pyoxigraph.NamedNode("http://www.w3.org/2001/XMLSchema#dateTime"),
        )
        assert _value_from_term(lit) == "2026-01-01T00:00:00Z"

    def test_named_node(self) -> None:
        node = pyoxigraph.NamedNode("https://blackcatinformatics.ca/gmeow/Person")
        assert _value_from_term(node) == "gmeow:Person"

    def test_language_tagged(self) -> None:
        lit = pyoxigraph.Literal("hello", language="en")
        assert _value_from_term(lit) == {"value": "hello", "lang": "en"}


class TestLPGGraph:
    def test_add_node(self) -> None:
        g = LPGGraph()
        g.add_node(LPGNode(id="a", labels=("Person",), properties={"name": "Alice"}))
        assert len(g.nodes) == 1

    def test_merge_node_properties(self) -> None:
        g = LPGGraph()
        g.add_node(LPGNode(id="a", labels=("Person",), properties={"name": "Alice"}))
        g.add_node(LPGNode(id="a", labels=("Agent",), properties={"age": 30}))
        node = g.nodes[0]
        assert set(node.labels) == {"Person", "Agent"}
        assert node.properties["name"] == "Alice"
        assert node.properties["age"] == 30

    def test_add_edge(self) -> None:
        g = LPGGraph()
        g.add_edge(
            LPGEdge(id="e1", source="a", target="b", type="knows", properties={})
        )
        assert len(g.edges) == 1

    def test_edges_for_key(self) -> None:
        g = LPGGraph()
        g.add_edge(
            LPGEdge(id="e1", source="a", target="b", type="knows", properties={"c": 1})
        )
        g.add_edge(
            LPGEdge(id="e2", source="a", target="b", type="knows", properties={"c": 2})
        )
        assert len(g.edges_for("a", "b", "knows")) == 2

    def test_deterministic_ordering(self) -> None:
        g = LPGGraph()
        g.add_node(LPGNode(id="b", labels=(), properties={}))
        g.add_node(LPGNode(id="a", labels=(), properties={}))
        ids = [n.id for n in g.nodes]
        assert ids == ["a", "b"]


class TestBuildLPG:
    def test_nodes_created_from_plain_turtle(self) -> None:
        ttl = """
        @prefix ex: <http://example.org/> .
        ex:Alice a ex:Person ; ex:name "Alice" .
        ex:Bob a ex:Person ; ex:name "Bob" .
        """
        store = _make_store_from_ttl(ttl)
        lpg = build_lpg(store)
        ids = {n.id for n in lpg.nodes}
        # ex: prefix is not in PREFIXES, so full IRIs are used
        assert "http://example.org/Alice" in ids
        assert "http://example.org/Bob" in ids

    def test_reifiers_excluded(self) -> None:
        """Reifiers must NOT become nodes.

        Uses ``rdf:reifies`` (the RDF-star reifier predicate) so the LPG builder
        recognises ``ex:Reifier1`` as a reifier and excludes it from the node
        set.  The quoted triple itself cannot be expressed in plain Turtle, so
        the object of ``rdf:reifies`` here is a plain IRI; the builder still
        adds the subject to ``reifier_iris`` and skips it.
        """
        ttl = """
        @prefix ex: <http://example.org/> .
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        ex:Alice ex:knows ex:Bob .
        ex:Reifier1 rdf:reifies ex:Alice ;
                    <https://blackcatinformatics.ca/gmeow/confidence> 0.9 .
        """
        store = _make_store_from_ttl(ttl)
        lpg = build_lpg(store)
        ids = {n.id for n in lpg.nodes}
        assert "http://example.org/Reifier1" not in ids

    def test_no_tbox_edges(self) -> None:
        """OWL TBox predicates must not create edges."""
        ttl = """
        @prefix owl: <http://www.w3.org/2002/07/owl#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        @prefix ex: <http://example.org/> .
        ex:Child rdfs:subClassOf ex:Person .
        ex:knows rdfs:domain ex:Person ; rdfs:range ex:Person .
        """
        store = _make_store_from_ttl(ttl)
        lpg = build_lpg(store)
        edge_types = {e.type for e in lpg.edges}
        assert "subClassOf" not in edge_types
        assert "domain" not in edge_types
        assert "range" not in edge_types

    def test_blank_nodes_skipped(self) -> None:
        """OWL restriction blank nodes must not appear as LPG nodes."""
        ttl = """
        @prefix owl: <http://www.w3.org/2002/07/owl#> .
        @prefix ex: <http://example.org/> .
        ex:Person a owl:Class ;
            owl:equivalentClass [
                a owl:Restriction ;
                owl:onProperty ex:age ;
                owl:minCardinality "0"^^<http://www.w3.org/2001/XMLSchema#integer>
            ] .
        """
        store = _make_store_from_ttl(ttl)
        lpg = build_lpg(store)
        assert all(not n.id.startswith("_bnode:") for n in lpg.nodes)

    def test_statement_metadata_on_canonical_rdf12(self) -> None:
        """Load the canonical Jena-produced RDF 1.2 artifact and verify
        statement metadata lands as edge properties."""
        from gmeow_tools.config import STATEMENT_RDF12_FILE

        if not STATEMENT_RDF12_FILE.exists():
            pytest.fail(f"canonical RDF 1.2 artifact missing: {STATEMENT_RDF12_FILE}")

        store = pyoxigraph.Store()
        store.load(path=str(STATEMENT_RDF12_FILE), format=pyoxigraph.RdfFormat.TURTLE)
        lpg = build_lpg(store)

        # Find the Crimea edges — the headline example
        crimea_edges = [e for e in lpg.edges if e.source == "gmeow:examples/crimea"]
        assert len(crimea_edges) >= 1

        # At least one edge should have metadata
        edges_with_meta = [e for e in crimea_edges if e.properties]
        assert len(edges_with_meta) >= 1

        # Verify metadata fields
        for e in edges_with_meta:
            assert "confidence" in e.properties

    def test_standpoint_parallel_edges_on_canonical_rdf12(self) -> None:
        """The Crimea example has two standpoint claims → parallel edges."""
        from gmeow_tools.config import STATEMENT_RDF12_FILE

        if not STATEMENT_RDF12_FILE.exists():
            pytest.fail(f"canonical RDF 1.2 artifact missing: {STATEMENT_RDF12_FILE}")

        store = pyoxigraph.Store()
        store.load(path=str(STATEMENT_RDF12_FILE), format=pyoxigraph.RdfFormat.TURTLE)
        lpg = build_lpg(store)

        crimea_edges = [e for e in lpg.edges if e.source == "gmeow:examples/crimea"]
        # Two standpoints = two parallel edges for containedInPlace
        contained = [e for e in crimea_edges if e.type == "containedInPlace"]
        assert len(contained) == 2
        standpoints = {e.properties.get("accordingTo") for e in contained}
        assert "gmeow:examples/standpoint-un" in standpoints
        assert "gmeow:examples/standpoint-ru" in standpoints


class TestSerializers:
    def _sample_lpg(self) -> LPGGraph:
        """Build a small LPG by hand for serializer tests."""
        lpg = LPGGraph()
        lpg.add_node(
            LPGNode(
                id="ex:Alice",
                labels=("Person",),
                properties={"name": "Alice", "uri": "http://example.org/Alice"},
            )
        )
        lpg.add_node(
            LPGNode(
                id="ex:Bob",
                labels=("Person",),
                properties={"name": "Bob", "uri": "http://example.org/Bob"},
            )
        )
        lpg.add_edge(
            LPGEdge(
                id="e1",
                source="ex:Alice",
                target="ex:Bob",
                type="knows",
                properties={"confidence": 0.95, "since": "2020"},
            )
        )
        return lpg

    def test_generic_csv_roundtrip(self, tmp_path: Path) -> None:
        lpg = self._sample_lpg()
        nodes_path, edges_path = serialize_generic_csv(lpg, tmp_path)

        with nodes_path.open(encoding="utf-8") as f:
            node_rows = list(csv.DictReader(f))
        assert any(r["id"] == "ex:Alice" for r in node_rows)

        with edges_path.open(encoding="utf-8") as f:
            edge_rows = list(csv.DictReader(f))
        knows = [r for r in edge_rows if r["type"] == "knows"]
        assert len(knows) == 1
        assert knows[0]["confidence"] == "0.95"

    def test_neo4j_csv_structure(self, tmp_path: Path) -> None:
        lpg = self._sample_lpg()
        paths = serialize_neo4j_csv(lpg, tmp_path)
        assert any(p.name == "nodes.csv" for p in paths)

    def test_cypher_syntax(self, tmp_path: Path) -> None:
        lpg = self._sample_lpg()
        path = serialize_cypher(lpg, tmp_path / "test.cypher")
        text = path.read_text(encoding="utf-8")
        assert "CREATE" in text
        assert "MATCH" in text
        assert "ex:Alice" in text

    def test_graphml_wellformed(self, tmp_path: Path) -> None:
        lpg = self._sample_lpg()
        path = serialize_graphml(lpg, tmp_path / "test.graphml")
        text = path.read_text(encoding="utf-8")
        assert "<graphml" in text
        assert "<graph" in text
        assert "<node" in text
        assert "<edge" in text


class TestRoundTrip:
    def test_base_triples_recoverable(self, tmp_path: Path) -> None:
        """Serialize to generic CSV and reload; base triples must be recoverable."""
        lpg = LPGGraph()
        lpg.add_node(
            LPGNode(
                id="ex:Alice",
                labels=("Person",),
                properties={"uri": "http://example.org/Alice"},
            )
        )
        lpg.add_node(
            LPGNode(
                id="ex:Bob",
                labels=("Person",),
                properties={"uri": "http://example.org/Bob"},
            )
        )
        lpg.add_edge(
            LPGEdge(
                id="e1",
                source="ex:Alice",
                target="ex:Bob",
                type="knows",
                properties={"confidence": 0.95},
            )
        )

        # Serialize to CSV
        nodes_path, edges_path = serialize_generic_csv(lpg, tmp_path)

        # Reload into a fresh LPGGraph
        reloaded = LPGGraph()
        with nodes_path.open(encoding="utf-8") as f:
            for row in csv.DictReader(f):
                labels = tuple(row["labels"].split(";")) if row["labels"] else ()
                props: dict[str, object] = {"uri": row["uri"]}
                for k, v in row.items():
                    if k not in ("id", "labels", "uri") and v:
                        props[k] = v
                reloaded.add_node(
                    LPGNode(id=row["id"], labels=labels, properties=props)
                )

        with edges_path.open(encoding="utf-8") as f:
            for row in csv.DictReader(f):
                edge_props: dict[str, object] = {}
                for k, v in row.items():
                    if k not in ("id", "source", "target", "type") and v:
                        edge_props[k] = v
                reloaded.add_edge(
                    LPGEdge(
                        id=row["id"],
                        source=row["source"],
                        target=row["target"],
                        type=row["type"],
                        properties=edge_props,
                    )
                )

        # Collect base triples from reloaded edges
        base_triples: set[tuple[str, str, str]] = set()
        for edge in reloaded.edges:
            source_uri = next(
                (n.properties["uri"] for n in reloaded.nodes if n.id == edge.source),
                edge.source,
            )
            target_uri = next(
                (n.properties["uri"] for n in reloaded.nodes if n.id == edge.target),
                edge.target,
            )
            base_triples.add((str(source_uri), edge.type, str(target_uri)))

        assert (
            "http://example.org/Alice",
            "knows",
            "http://example.org/Bob",
        ) in base_triples
