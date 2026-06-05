"""Parse the GMEOW statement DSL — the canonical RDF 1.2 / RDF* metadata source.

The statement DSL (``statement-dsl/*.ttl``, vocabulary in
``statement-dsl/vocabulary.ttl``) is the single authoring source for GMEOW's
statement-level metadata layer (provenance, confidence, temporal scope). Each
``gmeow:StatementMetadata`` cell is a 1:1 transcription of one RDF 1.2 reifying
statement — a quoted base triple plus the annotations hung off its reifier.

This module reads the DSL into typed, deterministically-ordered dataclasses; the
emitters (RDF 1.2 serialization + OWL axiom-annotation downcast) live in
:mod:`gmeow_tools.statement_compile`. The split mirrors the mapping DSL: *model*
here, *renderers* there.

Why a DSL and not literal RDF 1.2 Turtle? rdflib cannot yet parse RDF 1.2
triple-term syntax, so we author plain Turtle it reads today, shaped to mirror the
RDF 1.2 reifier 1:1 (CONSTITUTION Principles 2-3).
"""

from __future__ import annotations

from dataclasses import dataclass
from hashlib import sha1
from pathlib import Path

from rdflib import RDF, RDFS, Graph, Literal, URIRef
from rdflib.namespace import Namespace
from rdflib.term import Node

from gmeow_tools.config import NAMESPACE, PREFIXES, STATEMENT_DSL_DIR
from gmeow_tools.mapping_dsl import CompileError

GM = Namespace(PREFIXES["gmeow"])


# --------------------------------------------------------------------------- #
# Model
# --------------------------------------------------------------------------- #


@dataclass(frozen=True, slots=True)
class QuotedTriple:
    """The quoted base triple of a reified statement (``<<( s p o )>>``)."""

    subject: URIRef
    predicate: URIRef
    obj: URIRef | Literal


@dataclass(frozen=True, slots=True)
class Annotation:
    """One metadata annotation on the reifier (an ``annProperty``/``annValue``)."""

    prop: URIRef
    value: URIRef | Literal


@dataclass(frozen=True, slots=True)
class StatementCell:
    """One reified statement: a quoted triple + its reifier + its annotations."""

    iri: URIRef
    label: str
    reifier: URIRef
    triple: QuotedTriple
    annotations: tuple[Annotation, ...]


@dataclass(frozen=True, slots=True)
class StatementDsl:
    """The fully parsed statement DSL (deterministically ordered)."""

    cells: tuple[StatementCell, ...]


# --------------------------------------------------------------------------- #
# Parsing
# --------------------------------------------------------------------------- #


def _term_n3(term: Node) -> str:
    """A stable, prefix-free serialization of an RDF term (for content hashing)."""
    return term.n3()


def mint_reifier(triple: QuotedTriple) -> URIRef:
    """Mint a deterministic, content-addressed reifier IRI from a quoted triple.

    The same ``(s, p, o)`` always yields the same reifier, so an unauthored
    reifier never introduces spurious drift between compiles. The hash covers the
    object's full lexical form + datatype/language (via ``n3``), so a literal and
    an IRI object never collide.
    """
    canonical = " ".join(
        _term_n3(t) for t in (triple.subject, triple.predicate, triple.obj)
    )
    digest = sha1(canonical.encode("utf-8")).hexdigest()
    return URIRef(f"{NAMESPACE}reifier/{digest}")


def _value(node: object) -> URIRef | Literal:
    """Coerce an annotation/object node to a URIRef or Literal (else error)."""
    if isinstance(node, URIRef | Literal):
        return node
    raise CompileError(f"statement value must be an IRI or literal, got {node!r}")


def _annotations(graph: Graph, cell: Node) -> tuple[Annotation, ...]:
    """Parse and deterministically sort a cell's annotation nodes."""
    out: list[Annotation] = []
    for ann in graph.objects(cell, GM.annotation):
        prop = graph.value(ann, GM.annProperty)
        value = graph.value(ann, GM.annValue)
        if not isinstance(prop, URIRef):
            raise CompileError(f"annotation on {cell} missing an IRI annProperty")
        if value is None:
            raise CompileError(f"annotation {prop} on {cell} missing annValue")
        out.append(Annotation(prop=prop, value=_value(value)))
    # Deterministic order: by (property IRI, value lexical form).
    return tuple(sorted(out, key=lambda a: (str(a.prop), _term_n3(a.value))))


def _quoted_triple(graph: Graph, cell: Node) -> QuotedTriple:
    """Parse the quoted base triple (qSubject / qPredicate / qObject|Literal)."""
    subject = graph.value(cell, GM.qSubject)
    predicate = graph.value(cell, GM.qPredicate)
    obj_iri = graph.value(cell, GM.qObject)
    obj_lit = graph.value(cell, GM.qObjectLiteral)
    if not isinstance(subject, URIRef):
        raise CompileError(f"statement {cell} missing an IRI qSubject")
    if not isinstance(predicate, URIRef):
        raise CompileError(f"statement {cell} missing an IRI qPredicate")
    if (obj_iri is None) == (obj_lit is None):
        raise CompileError(
            f"statement {cell} needs exactly one of qObject / qObjectLiteral"
        )
    obj: URIRef | Literal
    if obj_iri is not None:
        if not isinstance(obj_iri, URIRef):
            raise CompileError(f"statement {cell} qObject must be an IRI")
        obj = obj_iri
    else:
        if not isinstance(obj_lit, Literal):
            raise CompileError(f"statement {cell} qObjectLiteral must be a literal")
        obj = obj_lit
    return QuotedTriple(subject=subject, predicate=predicate, obj=obj)


def _cells(graph: Graph) -> list[StatementCell]:
    cells: list[StatementCell] = []
    for cell in graph.subjects(RDF.type, GM.StatementMetadata):
        if not isinstance(cell, URIRef):
            raise CompileError("statement metadata cell must be a named IRI")
        triple = _quoted_triple(graph, cell)
        reifier = graph.value(cell, GM.reifier)
        if reifier is not None and not isinstance(reifier, URIRef):
            raise CompileError(f"statement {cell} reifier must be an IRI")
        cells.append(
            StatementCell(
                iri=cell,
                label=str(graph.value(cell, RDFS.label) or ""),
                reifier=reifier
                if isinstance(reifier, URIRef)
                else mint_reifier(triple),
                triple=triple,
                annotations=_annotations(graph, cell),
            )
        )
    return cells


def load_statement_dsl(src: Path = STATEMENT_DSL_DIR) -> StatementDsl:
    """Parse the whole statement DSL into deterministically-ordered cells."""
    graph = Graph()
    for path in sorted(src.rglob("*.ttl")):
        graph.parse(path, format="turtle")
    cells = sorted(_cells(graph), key=lambda c: str(c.iri))
    return StatementDsl(cells=tuple(cells))
