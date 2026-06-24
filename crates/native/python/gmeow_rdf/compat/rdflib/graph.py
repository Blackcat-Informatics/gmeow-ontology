# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The mutable ``Graph`` facade for the purrdf rdflib compat shim.

Backed by a native :class:`gmeow_rdf.MutableDataset` COW dataset. Presents the RDFLib
``Graph`` surface the internal toolchain uses — ``parse``/``serialize``, ``add``/
``remove``, wildcard ``triples``/``value`` + the accessor family, ``query``,
``bind``/``namespace_manager``, and set algebra. ``serialize(format="turtle")``
routes through the native ``canonicalize_turtle`` (deterministic, dogfooded).

``Dataset``/``ConjunctiveGraph`` subclass ``Graph`` (mirroring RDFLib's
``Dataset is-a Graph``) so both ``isinstance(x, Dataset)`` and
``isinstance(x, Graph)`` dispatch correctly; they default to the N-Quads format.
"""

from __future__ import annotations

import builtins
from collections.abc import Iterable, Iterator
from pathlib import Path
from typing import IO, Any, overload

import gmeow_rdf

from .namespace import RDF, NamespaceManager
from .query import Result, ResultRow
from .term import (
    Identifier,
    Literal,
    URIRef,
    from_native,
    to_native,
)

_TURTLE = gmeow_rdf.RdfFormat.TURTLE
_NT = gmeow_rdf.RdfFormat.N_TRIPLES
_NQ = gmeow_rdf.RdfFormat.N_QUADS
_TRIG = gmeow_rdf.RdfFormat.TRIG

_JSON_LD_FORMATS = frozenset(("json-ld", "jsonld", "application/ld+json"))
_XML_FORMATS = frozenset(("xml", "application/rdf+xml", "pretty-xml"))

#: A graph triple of compat terms.
_Triple = tuple[Identifier, Identifier, Identifier]
#: A wildcard triple pattern (``None`` = any).
_Pattern = tuple[Identifier | None, Identifier | None, Identifier | None]


def _rdf_format(fmt: str | None) -> gmeow_rdf.RdfFormat:
    """Map an RDFLib format string to an oxigraph-native :class:`gmeow_rdf.RdfFormat`.

    JSON-LD-star and RDF/XML are NOT oxigraph-native; ``parse``/``serialize`` route
    those through the gmeow-gts codecs before reaching this mapper.
    """
    f = (fmt or "turtle").lower()
    if f in ("turtle", "ttl", "longturtle", "n3"):
        return _TURTLE
    if f in ("nt", "ntriples", "nt11", "ntriples11", "application/n-triples"):
        return _NT
    if f in ("nquads", "nq", "application/n-quads"):
        return _NQ
    if f in ("trig", "application/trig"):
        return _TRIG
    raise ValueError(f"unsupported RDF format: {fmt!r}")


def _native_subject(term: Identifier) -> gmeow_rdf.NamedNode | gmeow_rdf.BlankNode:
    """Convert a term to a native subject (IRI or blank node)."""
    native = to_native(term)
    if isinstance(native, gmeow_rdf.Literal):
        raise TypeError(f"a literal cannot be a subject: {term!r}")
    return native


def _native_predicate(term: Identifier) -> gmeow_rdf.NamedNode:
    """Convert a term to a native predicate (must be an IRI)."""
    native = to_native(term)
    if not isinstance(native, gmeow_rdf.NamedNode):
        raise TypeError(f"a predicate must be an IRI: {term!r}")
    return native


def _require(value: object) -> Identifier:
    """Assert a converted term is bound (non-``None``) and return it."""
    assert isinstance(value, Identifier)
    return value


def _rebuild_graph(
    cls: type[Graph], nquads: bytes, prefixes: list[tuple[str, str]]
) -> Graph:
    """Reconstruct a graph from its N-Quads content (the pickle restore hook)."""
    graph = cls()
    if nquads.strip():
        graph._store.load(nquads, format=_NQ)
    for prefix, namespace in prefixes:
        graph.bind(prefix, namespace)
    return graph


def _term_n3(term: Any) -> str:
    """Return a term's SPARQL/N3 form (delegating to its ``n3`` method).

    Raw Python values (int/str/bool/float) are coerced to a typed ``Literal``
    so they serialize as proper RDF literals rather than ``<bare>`` IRIs.
    """
    if not isinstance(term, Identifier):
        from .term import Literal

        term = Literal(term)
    n3 = getattr(term, "n3", None)
    if callable(n3):
        result = n3()
        assert isinstance(result, str)
        return result
    return f"<{term}>"


def _inject_bindings(query_text: str, bindings: dict[str, Identifier]) -> str:
    """Inject a ``VALUES`` row binding ``bindings`` into the query's WHERE group."""
    names = " ".join(f"?{str(name).lstrip('?$')}" for name in bindings)
    values = " ".join(_term_n3(term) for term in bindings.values())
    clause = f" VALUES ({names}) {{ ({values}) }} "
    lowered = query_text.lower()
    where = lowered.find("where")
    # SELECT / CONSTRUCT-WHERE: inject after the WHERE keyword's opening brace.
    # ASK / DESCRIBE / WHERE-less SELECT: no WHERE keyword — first `{` IS the
    # group graph pattern.  (A CONSTRUCT always carries an explicit WHERE, so
    # its template brace is never reached by the else branch.)
    brace = query_text.find("{", where) if where != -1 else query_text.find("{")
    if brace == -1:
        return query_text
    return query_text[: brace + 1] + clause + query_text[brace + 1 :]


def _literal_matches(candidate: Identifier, pattern: Literal) -> bool:
    """Return whether a stored literal satisfies an object literal pattern."""
    if not isinstance(candidate, Literal):
        return False
    return bool(candidate == pattern or candidate.eq(pattern))


class Graph:
    """An RDFLib-shaped mutable RDF graph over a native COW dataset."""

    def __init__(
        self,
        store: gmeow_rdf.Store | gmeow_rdf.MutableDataset | None = None,
        identifier: object | None = None,
        *,
        namespace_manager: NamespaceManager | None = None,
        base: str | None = None,
    ) -> None:
        """Create an empty graph (or import an existing native store/dataset)."""
        if isinstance(store, gmeow_rdf.MutableDataset):
            self._store = store
        else:
            self._store = gmeow_rdf.MutableDataset()
            if isinstance(store, gmeow_rdf.Store):
                nquads = store.dump(format=_NQ)
                if nquads.strip():
                    self._store.load(nquads, format=_NQ)
        self._nsm = (
            namespace_manager if namespace_manager is not None else (NamespaceManager())
        )
        self.identifier = identifier
        self.base = base

    def __reduce__(
        self,
    ) -> tuple[object, tuple[type[Graph], bytes, list[tuple[str, str]]]]:
        """Pickle by content (N-Quads) — the native ``Store`` is not picklable.

        RDFLib's ``Graph`` is picklable, and the parallel generator/test runners
        ship graphs across process boundaries, so the compat ``Graph`` must be too.
        """
        return (
            _rebuild_graph,
            (type(self), self._store.dump(format=_NQ), self._nsm.namespaces()),
        )

    # ── prefixes ────────────────────────────────────────────────────────────────

    @property
    def namespace_manager(self) -> NamespaceManager:
        """The prefix registry feeding Turtle serialization."""
        return self._nsm

    def bind(
        self,
        prefix: str | None,
        namespace: object,
        *,
        override: bool = True,
        replace: bool = False,
    ) -> None:
        """Bind ``prefix`` → ``namespace`` for serialization."""
        self._nsm.bind(prefix, namespace, override=override, replace=replace)

    def namespaces(self) -> Iterator[tuple[str, URIRef]]:
        """Yield bound ``(prefix, namespace_iri)`` pairs."""
        for prefix, ns in self._nsm.namespaces():
            yield (prefix, URIRef(ns))

    # ── mutation ────────────────────────────────────────────────────────────────

    def add(self, triple: tuple[Identifier, Identifier, Identifier]) -> None:
        """Add a ``(subject, predicate, object)`` triple."""
        s, p, o = triple
        self._store.add(
            gmeow_rdf.Quad(_native_subject(s), _native_predicate(p), to_native(o))
        )

    def remove(self, triple: _Pattern) -> None:
        """Remove every triple matching the (possibly wildcard) pattern."""
        s, p, o = triple
        # Snapshot matches first — deleting while iterating the store is unsafe.
        matched = list(self.triples((s, p, o)))
        for ms, mp, mo in matched:
            self._store.remove(
                gmeow_rdf.Quad(
                    _native_subject(ms), _native_predicate(mp), to_native(mo)
                )
            )

    def set(self, triple: tuple[Identifier, Identifier, Identifier]) -> None:
        """Replace all ``(s, p, *)`` objects with this single triple's object."""
        s, p, o = triple
        self.remove((s, p, None))
        self.add((s, p, o))

    # ── pattern access ────────────────────────────────────────────────────────────

    def triples(self, pattern: _Pattern) -> Iterator[_Triple]:
        """Yield triples matching the wildcard pattern (``None`` = any)."""
        s, p, o = pattern
        native_object = None if o is None else to_native(o)
        object_value_filter = isinstance(o, Literal)
        quads = self._store.quads_for_pattern(
            None if s is None else _native_subject(s),
            None if p is None else _native_predicate(p),
            None if object_value_filter else native_object,
            None,
            any_graph=False,
        )
        for quad in quads:
            rs = s if s is not None else _require(from_native(quad.subject))
            rp = p if p is not None else _require(from_native(quad.predicate))
            candidate = _require(from_native(quad.object))
            if object_value_filter and not _literal_matches(candidate, o):
                continue
            ro = o if o is not None else candidate
            yield (rs, rp, ro)

    def __iter__(self) -> Iterator[_Triple]:
        """Iterate every triple as ``(subject, predicate, object)``."""
        yield from self.triples((None, None, None))

    def __len__(self) -> int:
        """Return the triple count."""
        return len(self._store)

    def __contains__(self, triple: _Pattern) -> bool:
        """Return whether any triple matches the pattern."""
        for _ in self.triples(triple):
            return True
        return False

    def value(
        self,
        subject: Identifier | None = None,
        predicate: Identifier | None = None,
        object: Identifier | None = None,
        default: Identifier | None = None,
        any: bool = True,
    ) -> Identifier | None:
        """Return the single unspecified term of the first matching triple."""
        for s, p, o in self.triples((subject, predicate, object)):
            if object is None:
                return o
            if subject is None:
                return s
            return p
        return default

    def subjects(
        self, predicate: Identifier | None = None, object: Identifier | None = None
    ) -> Iterator[Identifier]:
        """Yield subjects of triples matching ``(*, predicate, object)``."""
        for s, _p, _o in self.triples((None, predicate, object)):
            yield s

    def predicates(
        self, subject: Identifier | None = None, object: Identifier | None = None
    ) -> Iterator[Identifier]:
        """Yield predicates of triples matching ``(subject, *, object)``."""
        for _s, p, _o in self.triples((subject, None, object)):
            yield p

    def objects(
        self, subject: Identifier | None = None, predicate: Identifier | None = None
    ) -> Iterator[Identifier]:
        """Yield objects of triples matching ``(subject, predicate, *)``."""
        for _s, _p, o in self.triples((subject, predicate, None)):
            yield o

    def subject_objects(
        self, predicate: Identifier | None = None
    ) -> Iterator[tuple[Identifier, Identifier]]:
        """Yield ``(subject, object)`` pairs for ``(*, predicate, *)``."""
        for s, _p, o in self.triples((None, predicate, None)):
            yield (s, o)

    def subject_predicates(
        self, object: Identifier | None = None
    ) -> Iterator[tuple[Identifier, Identifier]]:
        """Yield ``(subject, predicate)`` pairs for ``(*, *, object)``."""
        for s, p, _o in self.triples((None, None, object)):
            yield (s, p)

    def predicate_objects(
        self, subject: Identifier | None = None
    ) -> Iterator[tuple[Identifier, Identifier]]:
        """Yield ``(predicate, object)`` pairs for ``(subject, *, *)``."""
        for _s, p, o in self.triples((subject, None, None)):
            yield (p, o)

    def items(self, list_node: Identifier) -> Iterator[Identifier]:
        """Yield the members of the ``rdf:List`` anchored at ``list_node``."""
        node: Identifier | None = list_node
        while node is not None and node != RDF.nil:
            first = self.value(node, RDF.first)
            if first is not None:
                yield first
            node = self.value(node, RDF.rest)

    def transitive_objects(
        self,
        subject: Identifier,
        predicate: Identifier,
        remember: builtins.set[Identifier] | None = None,
    ) -> Iterator[Identifier]:
        """Yield ``subject`` and every object reachable via ``predicate`` (RDFLib)."""
        if remember is None:
            remember = set[Identifier]()
        if subject in remember:
            return
        remember.add(subject)
        yield subject
        for obj in self.objects(subject, predicate):
            yield from self.transitive_objects(obj, predicate, remember)

    def transitive_subjects(
        self,
        predicate: Identifier,
        object: Identifier,
        remember: builtins.set[Identifier] | None = None,
    ) -> Iterator[Identifier]:
        """Yield ``object`` and every subject reaching it via ``predicate`` (RDFLib)."""
        if remember is None:
            remember = set[Identifier]()
        if object in remember:
            return
        remember.add(object)
        yield object
        for subj in self.subjects(predicate, object):
            yield from self.transitive_subjects(predicate, subj, remember)

    def isomorphic(self, other: Graph) -> bool:
        """Return whether this graph is isomorphic to ``other`` (RDFC-1.0)."""
        from .compare import isomorphic

        return isomorphic(self, other)

    # ── parse / serialize ─────────────────────────────────────────────────────────

    def parse(
        self,
        source: object | None = None,
        publicID: str | None = None,  # noqa: N803 - RDFLib API name
        format: str | None = None,
        location: str | None = None,
        file: IO[bytes] | None = None,
        data: str | bytes | None = None,
        **kwargs: object,
    ) -> Graph:
        """Parse RDF from a path/file/``data`` into this graph (any compat format).

        JSON-LD-star and RDF/XML route through the gmeow-gts codecs (text → native
        ``from_json_ld``/``from_rdf_xml`` → N-Quads → store); the oxigraph-native
        formats load directly (a path source is handed straight to the loader).
        """
        f = (format or "turtle").lower()
        is_codec = f in _JSON_LD_FORMATS or f in _XML_FORMATS
        payload: bytes
        if data is not None:
            payload = data.encode("utf-8") if isinstance(data, str) else data
        else:
            src: object | None = source if source is not None else location
            if src is None and file is not None:
                src = file
            if src is None:
                raise ValueError("parse requires one of: source, data, location, file")
            reader = getattr(src, "read", None)
            if callable(reader):
                raw = reader()
                payload = raw.encode("utf-8") if isinstance(raw, str) else raw
            elif is_codec:
                payload = Path(str(src)).read_bytes()
            else:
                self._store.load(path=str(src), format=_rdf_format(format))
                return self
        if f in _JSON_LD_FORMATS:
            self._store.load(
                gmeow_rdf.from_json_ld(payload.decode("utf-8")), format=_NQ
            )
        elif f in _XML_FORMATS:
            self._store.load(
                gmeow_rdf.from_rdf_xml(payload.decode("utf-8")), format=_NQ
            )
        else:
            self._store.load(payload, format=_rdf_format(format))
        return self

    def _dump_bytes(self, fmt: str | None) -> bytes:
        """Serialize the store to bytes in the requested format.

        Turtle routes through the native ``canonicalize_turtle``; JSON-LD-star and
        RDF/XML route through the gmeow-gts codecs (store → N-Quads → native
        ``to_json_ld``/``to_rdf_xml``); the rest dump directly via oxigraph.
        """
        f = (fmt or "turtle").lower()
        if f in ("turtle", "ttl", "longturtle", "n3"):
            nt = self._store.dump(format=_NT)
            return gmeow_rdf.canonicalize_turtle(nt, self._nsm.namespaces())
        if f in _JSON_LD_FORMATS:
            nquads = self._store.dump(format=_NQ)
            return gmeow_rdf.to_json_ld(nquads, format=_NQ).encode("utf-8")
        if f in _XML_FORMATS:
            nquads = self._store.dump(format=_NQ)
            return gmeow_rdf.to_rdf_xml(nquads, format=_NQ).encode("utf-8")
        return self._store.dump(format=_rdf_format(fmt))

    @overload
    def serialize(
        self,
        destination: None = ...,
        *,
        format: str = ...,
        encoding: None = ...,
        **kwargs: object,
    ) -> str: ...

    @overload
    def serialize(
        self,
        destination: None = ...,
        *,
        format: str = ...,
        encoding: str,
        **kwargs: object,
    ) -> bytes: ...

    @overload
    def serialize(
        self,
        destination: str | Path | IO[bytes],
        *,
        format: str = ...,
        encoding: str | None = ...,
        **kwargs: object,
    ) -> None: ...

    def serialize(
        self,
        destination: str | Path | IO[bytes] | None = None,
        *,
        format: str = "turtle",
        encoding: str | None = None,
        **kwargs: object,
    ) -> str | bytes | None:
        """Serialize the graph; return ``str``/``bytes`` or write to ``destination``."""
        out = self._dump_bytes(format)
        if destination is None:
            return out if encoding is not None else out.decode("utf-8")
        writer = getattr(destination, "write", None)
        if callable(writer):
            writer(out)
        elif isinstance(destination, str | Path):
            Path(destination).write_bytes(out)
        else:
            raise TypeError(f"unsupported serialize destination: {destination!r}")
        return None

    # ── query ─────────────────────────────────────────────────────────────────────

    def query(
        self,
        query_object: str,
        *,
        initBindings: dict[str, Identifier] | None = None,  # noqa: N803 - RDFLib API
        initNs: dict[str, object] | None = None,  # noqa: N803 - RDFLib API
        **kwargs: object,
    ) -> Result:
        """Run a SPARQL query; return a :class:`~.query.Result`.

        ``initBindings`` are applied as a ``VALUES`` row injected into the WHERE
        group (each value via its safe ``n3()`` form), matching RDFLib's
        pre-binding semantics for variables that need not be projected.
        """
        if initNs:
            prefixes = "".join(
                f"PREFIX {prefix}: <{namespace}>\n"
                for prefix, namespace in initNs.items()
            )
            query_object = prefixes + query_object
        if initBindings:
            query_object = _inject_bindings(query_object, initBindings)
        res = self._store.query(query_object)
        if isinstance(res, gmeow_rdf.QueryBoolean):
            return Result("ASK", ask=bool(res))
        if isinstance(res, gmeow_rdf.QueryTriples):
            constructed = Graph()
            nt = res.serialize(_NT)
            if nt:
                constructed._store.load(nt, format=_NT)
            return Result("CONSTRUCT", graph=constructed)
        variables = list(res.variables)
        var_names = tuple(v.value for v in variables)
        rows = [
            ResultRow(tuple(from_native(sol[v]) for v in variables), var_names)
            for sol in res
        ]
        return Result("SELECT", rows=rows, variables=var_names)

    def update(
        self,
        update_object: str,
        *,
        initNs: dict[str, object] | None = None,  # noqa: N803 - RDFLib API
        **kwargs: object,
    ) -> None:
        """Run a SPARQL UPDATE against this graph."""
        if initNs:
            prefixes = "".join(
                f"PREFIX {prefix}: <{namespace}>\n"
                for prefix, namespace in initNs.items()
            )
            update_object = prefixes + update_object
        self._store.update(update_object)

    # ── set algebra ───────────────────────────────────────────────────────────────

    def __iadd__(self, other: Iterable[_Triple]) -> Graph:
        """Add every triple from ``other`` (the ``+=`` operator)."""
        for triple in other:
            self.add(triple)
        return self

    def __isub__(self, other: Iterable[_Pattern]) -> Graph:
        """Remove every triple in ``other`` (the ``-=`` operator)."""
        for triple in other:
            self.remove(triple)
        return self

    def __add__(self, other: Iterable[_Triple]) -> Graph:
        """Return a new graph = the union of this graph and ``other``."""
        result = Graph()
        for triple in self:
            result.add(triple)
        for triple in other:
            result.add(triple)
        return result

    def __sub__(self, other: Iterable[_Triple]) -> Graph:
        """Return a new graph = this graph minus the triples in ``other``."""
        result = Graph()
        removed = set(other)
        for triple in self:
            if triple not in removed:
                result.add(triple)
        return result

    def __mul__(self, other: Iterable[_Triple]) -> Graph:
        """Return a new graph = intersection of this graph and ``other``."""
        result = Graph()
        other_set = set(other)
        for triple in self:
            if triple in other_set:
                result.add(triple)
        return result

    def __xor__(self, other: Iterable[_Triple]) -> Graph:
        """Return a new graph = symmetric difference of this graph and ``other``."""
        result = Graph()
        self_set = set(self)
        other_set = set(other)
        for triple in self_set ^ other_set:
            result.add(triple)
        return result


class _GraphView:
    """An add target for one graph slot of a :class:`Dataset` (``Dataset.graph``)."""

    def __init__(
        self,
        store: gmeow_rdf.MutableDataset,
        graph_name: gmeow_rdf.NamedNode | gmeow_rdf.BlankNode | gmeow_rdf.DefaultGraph,
    ) -> None:
        """Bind to ``store`` and the graph slot ``graph_name``."""
        self._store = store
        self._graph_name = graph_name
        self.identifier = graph_name

    def add(self, triple: tuple[Identifier, Identifier, Identifier]) -> None:
        """Add a triple into this view's graph slot."""
        s, p, o = triple
        self._store.add(
            gmeow_rdf.Quad(
                _native_subject(s), _native_predicate(p), to_native(o), self._graph_name
            )
        )


class Dataset(Graph):
    """A quad-capable graph facade (RDFLib ``Dataset``); defaults to N-Quads."""

    def __init__(
        self,
        store: gmeow_rdf.Store | gmeow_rdf.MutableDataset | None = None,
        default_union: bool = False,
        **kwargs: object,
    ) -> None:
        """Create an empty dataset."""
        super().__init__(store)
        self.default_union = default_union

    def graph(self, identifier: Identifier) -> _GraphView:
        """Return an add target for the named graph ``identifier``."""
        return _GraphView(self._store, _native_subject(identifier))

    @property
    def default_graph(self) -> _GraphView:
        """Return an add target for the (unnamed) default graph."""
        return _GraphView(self._store, gmeow_rdf.DefaultGraph())

    def quads(
        self, pattern: tuple[object, object, object, object] | None = None
    ) -> Iterator[tuple[Identifier, Identifier, Identifier, Identifier | None]]:
        """Yield ``(s, p, o, graph_name)`` quads matching ``pattern``.

        Each ``pattern`` slot is a wildcard when ``None`` (RDFLib quads() semantics);
        ``graph_name`` is ``None`` for the default graph.
        """
        ps, pp, po, pg = pattern if pattern is not None else (None, None, None, None)
        graph_name = None if pg is None else _native_subject(pg)
        native_object = None if po is None else to_native(po)
        object_value_filter = isinstance(po, Literal)
        for quad in self._store.quads_for_pattern(
            None if ps is None else _native_subject(ps),
            None if pp is None else _native_predicate(pp),
            None if object_value_filter else native_object,
            graph_name,
            any_graph=pg is None,
        ):
            graph_name = quad.graph_name
            gname = (
                None
                if isinstance(graph_name, gmeow_rdf.DefaultGraph)
                else from_native(graph_name)
            )
            s = _require(from_native(quad.subject))
            p = _require(from_native(quad.predicate))
            o = _require(from_native(quad.object))
            if object_value_filter and not _literal_matches(o, po):
                continue
            if ps is not None:
                s = ps
            if pp is not None:
                p = pp
            if po is not None:
                o = po
            if pg is not None:
                gname = pg
            yield (s, p, o, gname)

    def parse(
        self,
        source: object | None = None,
        publicID: str | None = None,  # noqa: N803 - RDFLib API name
        format: str | None = None,
        location: str | None = None,
        file: IO[bytes] | None = None,
        data: str | bytes | None = None,
        **kwargs: object,
    ) -> Dataset:
        """Parse RDF (default N-Quads) into the dataset."""
        super().parse(
            source,
            publicID,
            format if format is not None else "nquads",
            location,
            file,
            data,
            **kwargs,
        )
        return self

    def serialize(  # type: ignore[override]
        self,
        destination: str | Path | IO[bytes] | None = None,
        *,
        format: str = "nquads",
        encoding: str | None = None,
        **kwargs: object,
    ) -> str | bytes | None:
        """Serialize the dataset (default N-Quads)."""
        return super().serialize(
            destination, format=format, encoding=encoding, **kwargs
        )


class ConjunctiveGraph(Dataset):
    """RDFLib ``ConjunctiveGraph`` alias over the dataset facade."""
