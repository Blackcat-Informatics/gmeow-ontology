"""Fast in-process SPARQL execution over the merged ontology via pyoxigraph.

rdflib's pure-Python SPARQL engine and its triple-by-triple graph copy dominate
the non-Docker test runtime. pyoxigraph (Rust) parses the merged ontology ~10x
faster and runs the same SPARQL 1.1 ``SELECT`` / ``ASK`` / ``CONSTRUCT`` ~12x
faster, so this module provides the fast lane the test suite and the query
executors use.

This is a **non-authoritative acceleration path**. Jena remains the canonical
RDF 1.2 writer and reasoner; pyshacl/rdflib remains the canonical SHACL engine.
The :mod:`gmeow_tools.engine_crosscheck` gate proves rdflib and pyoxigraph return
identical answers for every committed query, which is what licenses callers to
trust this engine (CONSTITUTION Principle 7 — verified by construction).

``CONSTRUCT`` results are returned as rdflib :class:`~rdflib.Graph` objects (via a
single N-Triples hand-off) so existing rdflib-based assertions keep working
unchanged.
"""

from __future__ import annotations

from functools import lru_cache
from io import BytesIO
from pathlib import Path
from typing import cast

import pyoxigraph
from rdflib import Graph
from rdflib.term import Identifier

from gmeow_tools.graph import bind_prefixes, iter_source_files

_TURTLE = pyoxigraph.RdfFormat.TURTLE
_NT = pyoxigraph.RdfFormat.N_TRIPLES

#: pyoxigraph does not export a single ``Term`` union, so name the term types a
#: query solution / substitution can hold — the full set ``Store.query`` expects.
#: ``pyoxigraph.Triple`` (a quoted triple, RDF 1.2) is included for completeness,
#: but ``_to_rdflib`` rejects it explicitly: rdflib 7.6 has no quoted-triple *term*
#: type, and the SELECT/CONSTRUCT queries this module serves never project one (the
#: RDF 1.2 statement round-trip uses the dedicated ``rdf12_pyoxigraph`` path).
type _OxTerm = (
    pyoxigraph.NamedNode | pyoxigraph.BlankNode | pyoxigraph.Literal | pyoxigraph.Triple
)


def _load_base(store: pyoxigraph.Store, include_imports: bool) -> None:
    """Load the merged-ontology Turtle sources into *store*.

    The Turtle sources are loaded **directly** (pyoxigraph parses them natively)
    rather than via an rdflib N-Triples hand-off: an rdflib→N-Triples→pyoxigraph
    round-trip silently breaks blank-node RDF collections (``owl:members`` lists),
    whereas a direct Turtle load preserves them.
    """
    for path in iter_source_files(include_imports=include_imports):
        store.load(path=str(path), format=_TURTLE)


@lru_cache(maxsize=2)
def _base_store(include_imports: bool) -> pyoxigraph.Store:
    """A cached read-only pyoxigraph store of the merged ontology.

    Built once per ``include_imports`` flavour. Callers that only query must not
    mutate it; callers that need to add instance data should use
    :func:`store_with`, which returns a fresh store.
    """
    store = pyoxigraph.Store()
    _load_base(store, include_imports)
    return store


def merged_store(*, include_imports: bool = False) -> pyoxigraph.Store:
    """Return the cached, read-only merged-ontology store (query only)."""
    return _base_store(include_imports)


def store_with(
    *sources: Path | bytes,
    include_imports: bool = False,
    extra_triples: Graph | None = None,
) -> pyoxigraph.Store:
    """Build a fresh store of the merged ontology plus extra instance data.

    Args:
        sources: Turtle files (``Path``) or raw N-Triples bytes to load on top of
            the merged ontology.
        include_imports: Whether the merged base includes the vendored imports.
        extra_triples: An rdflib graph whose triples are also loaded (serialized
            to N-Triples once) — for small ad-hoc additions without RDF lists.

    Returns:
        A new pyoxigraph store seeded with the merged ontology plus every supplied
        source — fast (~30 ms) versus an rdflib deep copy (~140 ms).
    """
    store = pyoxigraph.Store()
    _load_base(store, include_imports)
    for source in sources:
        if isinstance(source, bytes):
            store.load(source, format=_NT)
        else:
            store.load(path=str(source), format=_TURTLE)
    if extra_triples is not None and len(extra_triples) > 0:
        store.load(extra_triples.serialize(format="nt", encoding="utf-8"), format=_NT)
    return store


def store_from_graph(graph: Graph) -> pyoxigraph.Store:
    """Load an arbitrary rdflib graph into a fresh pyoxigraph store."""
    store = pyoxigraph.Store()
    store.load(graph.serialize(format="nt", encoding="utf-8"), format=_NT)
    return store


def construct(
    store: pyoxigraph.Store,
    query_text: str,
    *,
    substitutions: dict[str, Identifier] | None = None,
) -> Graph:
    """Run a ``CONSTRUCT`` query and return the result as an rdflib graph.

    Args:
        store: The store to query.
        query_text: A SPARQL ``CONSTRUCT`` query.
        substitutions: Optional variable bindings (``{"focus": URIRef(...)}``)
            applied via pyoxigraph's native substitution — never string-spliced.

    Returns:
        A fresh rdflib graph of the constructed triples, prefixes bound.
    """
    results = store.query(query_text, substitutions=_subs(substitutions))
    assert isinstance(results, pyoxigraph.QueryTriples)
    buffer = BytesIO()
    pyoxigraph.serialize(results, buffer, format=_NT)
    out = Graph()
    out.parse(data=buffer.getvalue(), format="nt")
    bind_prefixes(out)
    return out


def select(
    store: pyoxigraph.Store,
    query_text: str,
    *,
    substitutions: dict[str, Identifier] | None = None,
) -> list[tuple[Identifier | None, ...]]:
    """Run a ``SELECT`` query, returning rows as tuples of rdflib terms.

    Terms are converted back to rdflib (``URIRef`` / ``BNode`` / ``Literal``) so
    callers can use the rows exactly like an rdflib query result — ``str(row[0])``,
    set membership, datatype access — with no engine-specific handling.
    """
    results = store.query(query_text, substitutions=_subs(substitutions))
    assert isinstance(results, pyoxigraph.QuerySolutions)
    variables = list(results.variables)
    return [
        tuple(_to_rdflib(solution[var]) for var in variables) for solution in results
    ]


def ask(store: pyoxigraph.Store, query_text: str) -> bool:
    """Run an ``ASK`` query and return its boolean answer."""
    result = store.query(query_text)
    assert isinstance(result, pyoxigraph.QueryBoolean)
    return bool(result)


def _subs(
    substitutions: dict[str, Identifier] | None,
) -> dict[pyoxigraph.Variable, _OxTerm] | None:
    """Translate ``{name: rdflib term}`` to pyoxigraph substitution form."""
    if not substitutions:
        return None
    return {
        pyoxigraph.Variable(name): _to_ox_term(term)
        for name, term in substitutions.items()
    }


def _to_ox_term(term: Identifier) -> _OxTerm:
    """Convert a single rdflib term to its pyoxigraph counterpart via N-Triples."""
    quad = next(
        iter(pyoxigraph.parse(f"<urn:x> <urn:p> {term.n3()} .".encode(), format=_NT))
    )
    return cast("_OxTerm", quad.object)


def _to_rdflib(value: _OxTerm | None) -> Identifier | None:
    """Convert one pyoxigraph result term back to its rdflib counterpart."""
    from rdflib import BNode, Literal, URIRef

    if value is None:
        return None
    if isinstance(value, pyoxigraph.NamedNode):
        return URIRef(value.value)
    if isinstance(value, pyoxigraph.BlankNode):
        return BNode(value.value)
    if isinstance(value, pyoxigraph.Literal):
        if value.language is not None:
            return Literal(value.value, lang=value.language)
        datatype = value.datatype.value
        # rdflib renders a plain literal with no datatype; pyoxigraph tags it
        # xsd:string. Drop xsd:string so the rdflib terms match either origin.
        if datatype == "http://www.w3.org/2001/XMLSchema#string":
            return Literal(value.value)
        return Literal(value.value, datatype=URIRef(datatype))
    # A quoted triple term (RDF 1.2) has no rdflib term-type counterpart in
    # rdflib 7.6 and never appears in the queries this helper serves; surface it
    # explicitly rather than silently mishandling it.
    raise NotImplementedError(
        f"quoted-triple SELECT result is not representable as an rdflib term: {value!r}"
    )
