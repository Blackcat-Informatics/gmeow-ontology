# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""RDF term model for the purrdf rdflib compat shim (``gmeow_rdf.compat.rdflib``).

The terms are ``str`` subclasses — exactly as in RDFLib 7.6 — so existing call
sites that do ``str(uri)``, slicing, set/dict membership, and lexical comparison
keep working unchanged. Each term also owns the *value* needed to build its
native :mod:`gmeow_rdf` counterpart on demand (:func:`to_native`), and the inverse
(:func:`from_native`) reconstitutes a compat term from a native query/store term.

This is the **P0 subset** of the eventual P9 public shim: it presents the
constructor / namespace / accessor surface the internal toolchain uses. The full
RDFLib equality/ordering hardening (value-space ``eq``, ``xsd:string`` provenance)
is a P9 concern — here term equality follows RDFLib's *term* equality over
``(lexical, datatype, language)`` so differential tests against real RDFLib match.
"""

from __future__ import annotations

from decimal import Decimal
from typing import TYPE_CHECKING, Any
from uuid import uuid4

import gmeow_rdf

if TYPE_CHECKING:
    from gmeow_rdf import BlankNode, NamedNode
    from gmeow_rdf import Literal as _NativeLiteral

# XSD datatype IRIs used by the value-space coercion. Kept as bare strings here so
# this module has no import cycle with :mod:`.namespace` (which imports ``URIRef``).
_XSD = "http://www.w3.org/2001/XMLSchema#"
_XSD_STRING = _XSD + "string"
_XSD_BOOLEAN = _XSD + "boolean"
_XSD_DECIMAL = _XSD + "decimal"
_XSD_DOUBLE = _XSD + "double"
_XSD_FLOAT = _XSD + "float"
_XSD_INTEGERS = frozenset(
    _XSD + name
    for name in (
        "integer",
        "int",
        "long",
        "short",
        "byte",
        "nonNegativeInteger",
        "nonPositiveInteger",
        "negativeInteger",
        "positiveInteger",
        "unsignedInt",
        "unsignedLong",
        "unsignedShort",
        "unsignedByte",
    )
)


class Identifier(str):
    """A ``str``-subclass RDF term (mirrors ``rdflib.term.Identifier``).

    The compat surface collapses RDFLib's abstract ``Node`` base into this class
    (``Node`` below is an alias): every concrete term is a ``str`` subclass, so a
    separate non-``str`` base buys nothing and only introduces type-boundary noise.
    """

    __slots__ = ()

    def __new__(cls, value: str) -> Identifier:
        """Construct the term as its lexical/IRI string."""
        return str.__new__(cls, value)

    def n3(self, namespace_manager: object | None = None) -> str:
        """Return the N3/Turtle form (subclasses override; default = IRI form)."""
        return f"<{self}>"


#: RDFLib's abstract ``Node`` base — collapsed to :class:`Identifier` here.
Node = Identifier


class URIRef(Identifier):
    """An IRI term — RDFLib-shaped, backed by :class:`gmeow_rdf.NamedNode`."""

    __slots__ = ()

    def __new__(cls, value: str) -> URIRef:
        """Construct from the IRI string (no angle brackets)."""
        return str.__new__(cls, value)

    def n3(self, namespace_manager: object | None = None) -> str:
        """Return the N3/Turtle form ``<iri>``."""
        return f"<{self}>"

    def toPython(self) -> str:  # noqa: N802 - RDFLib API name
        """Return the IRI as a plain ``str`` (RDFLib parity)."""
        return str(self)

    def to_native(self) -> NamedNode:
        """Return the native :class:`gmeow_rdf.NamedNode` counterpart."""
        return gmeow_rdf.NamedNode(str(self))


class BNode(Identifier):
    """A blank-node term — RDFLib-shaped, backed by :class:`gmeow_rdf.BlankNode`."""

    __slots__ = ()

    def __new__(cls, value: str | None = None) -> BNode:
        """Construct from a label, or mint a fresh unique label when ``None``."""
        if value is None:
            value = f"N{uuid4().hex}"
        return str.__new__(cls, value)

    def n3(self, namespace_manager: object | None = None) -> str:
        """Return the N3/Turtle form ``_:label``."""
        return f"_:{self}"

    def toPython(self) -> str:  # noqa: N802 - RDFLib API name
        """Return the blank-node label as a plain ``str``."""
        return str(self)

    def to_native(self) -> BlankNode:
        """Return the native :class:`gmeow_rdf.BlankNode` counterpart."""
        return gmeow_rdf.BlankNode(str(self))


def _coerce_value(lexical: str, datatype: URIRef | None, language: str | None) -> Any:
    """Map a (lexical, datatype) pair to a Python value (RDFLib ``toPython``)."""
    if language is not None or datatype is None:
        return lexical
    dt = str(datatype)
    if dt == _XSD_STRING:
        return lexical
    if dt == _XSD_BOOLEAN:
        return lexical.strip() in ("true", "1")
    if dt in _XSD_INTEGERS:
        try:
            return int(lexical)
        except ValueError:
            return lexical
    if dt == _XSD_DECIMAL:
        try:
            return Decimal(lexical)
        except (ValueError, ArithmeticError):
            return lexical
    if dt in (_XSD_DOUBLE, _XSD_FLOAT):
        try:
            return float(lexical)
        except ValueError:
            return lexical
    return lexical


def _infer_typed(value: object) -> tuple[str, URIRef | None]:
    """Infer (lexical, datatype) for a non-``str`` Python value (RDFLib parity)."""
    if isinstance(value, bool):
        return ("true" if value else "false", URIRef(_XSD_BOOLEAN))
    if isinstance(value, int):
        return (str(value), URIRef(_XSD + "integer"))
    if isinstance(value, Decimal):
        return (str(value), URIRef(_XSD_DECIMAL))
    if isinstance(value, float):
        return (repr(value), URIRef(_XSD_DOUBLE))
    return (str(value), None)


class Literal(Identifier):
    """An RDF literal — ``str``-subclass over the lexical form (RDFLib-shaped).

    Carries ``.datatype`` / ``.language`` and the value-space ``.value`` /
    ``.toPython()``. Equality and hashing follow RDFLib *term* equality over
    ``(lexical, datatype, language)``.
    """

    __slots__ = ("_datatype", "_language", "_value")

    _datatype: URIRef | None
    _language: str | None
    _value: Any

    def __new__(
        cls,
        lexical_or_value: object,
        lang: str | None = None,
        datatype: URIRef | str | None = None,
        *,
        normalize: bool | None = None,
    ) -> Literal:
        """Construct from a lexical string (with optional lang/datatype) or value."""
        dt: URIRef | None
        if isinstance(datatype, str) and not isinstance(datatype, URIRef):
            dt = URIRef(datatype)
        else:
            dt = datatype
        if isinstance(lexical_or_value, str):
            lexical = str(lexical_or_value)
        else:
            inferred_lexical, inferred_dt = _infer_typed(lexical_or_value)
            lexical = inferred_lexical
            if dt is None and lang is None:
                dt = inferred_dt
        self = str.__new__(cls, lexical)
        self._language = lang
        self._datatype = dt
        self._value = _coerce_value(lexical, dt, lang)
        return self

    @property
    def language(self) -> str | None:
        """The language tag, or ``None``."""
        return self._language

    @property
    def datatype(self) -> URIRef | None:
        """The datatype IRI, or ``None`` for a plain literal (RDFLib parity)."""
        return self._datatype

    @property
    def value(self) -> Any:
        """The Python value-space form (``int``/``bool``/``Decimal``/… or ``str``)."""
        return self._value

    def toPython(self) -> Any:  # noqa: N802 - RDFLib API name
        """Return the Python value-space form (RDFLib ``toPython``)."""
        return self._value

    def n3(self, namespace_manager: object | None = None) -> str:
        """Return the N3/Turtle form (quoted lexical + lang or ``^^datatype``)."""
        escaped = (
            str(self)
            .replace("\\", "\\\\")
            .replace('"', '\\"')
            .replace("\n", "\\n")
            .replace("\r", "\\r")
            .replace("\t", "\\t")
        )
        body = f'"{escaped}"'
        if self._language is not None:
            return f"{body}@{self._language}"
        if self._datatype is not None:
            return f"{body}^^<{self._datatype}>"
        return body

    def to_native(self) -> _NativeLiteral:
        """Return the native :class:`gmeow_rdf.Literal` counterpart."""
        if self._language is not None:
            try:
                return gmeow_rdf.Literal(str(self), language=self._language)
            except ValueError:
                # The native Literal *constructor* validates language tags
                # strictly (RFC 5646: private-use subtags ≤ 8 chars), but the
                # lenient parser preserves the project's longer ``@x-gmeow-*`` tags
                # (e.g. ``x-gmeow-traditional``). Round-trip through N-Triples so
                # those tags survive construction, matching the parse path.
                nt = f"<urn:x> <urn:x> {self.n3()} .".encode()
                quad = gmeow_rdf.parse(nt, format=gmeow_rdf.RdfFormat.N_TRIPLES)[0]
                obj = quad.object
                assert isinstance(obj, gmeow_rdf.Literal)
                return obj
        if self._datatype is not None:
            return gmeow_rdf.Literal(
                str(self), datatype=gmeow_rdf.NamedNode(str(self._datatype))
            )
        return gmeow_rdf.Literal(str(self))

    def __eq__(self, other: object) -> bool:
        """RDFLib term equality over ``(lexical, datatype, language)``."""
        if self is other:
            return True
        if not isinstance(other, Literal):
            return NotImplemented
        return (
            str.__eq__(self, other) is True
            and self._datatype == other._datatype
            and self._language == other._language
        )

    def __ne__(self, other: object) -> bool:
        """Negate :meth:`__eq__` (``str`` provides its own ``__ne__`` otherwise)."""
        result = self.__eq__(other)
        if result is NotImplemented:
            return NotImplemented
        return not result

    def __hash__(self) -> int:
        """Hash over ``(lexical, datatype, language)`` — follows ``__eq__``."""
        return hash((str(self), self._datatype, self._language))


class Variable(Identifier):
    """A SPARQL variable term (mirrors ``rdflib.term.Variable``)."""

    __slots__ = ()

    def n3(self, namespace_manager: object | None = None) -> str:
        """Return the SPARQL form ``?name``."""
        return f"?{self}"

    def to_native(self) -> gmeow_rdf.Variable:
        """Return the native :class:`gmeow_rdf.Variable` counterpart."""
        return gmeow_rdf.Variable(str(self))


def to_native(
    term: Identifier,
) -> gmeow_rdf.NamedNode | gmeow_rdf.BlankNode | gmeow_rdf.Literal:
    """Convert a compat term to its native :mod:`gmeow_rdf` counterpart."""
    if isinstance(term, URIRef):
        return term.to_native()
    if isinstance(term, BNode):
        return term.to_native()
    if isinstance(term, Literal):
        return term.to_native()
    # A bare ``Identifier`` (or an unknown subclass): treat its string as an IRI,
    # matching how RDFLib widens to URIRef for raw identifiers in term position.
    return gmeow_rdf.NamedNode(str(term))


def from_native(
    value: gmeow_rdf.NamedNode
    | gmeow_rdf.BlankNode
    | gmeow_rdf.Literal
    | gmeow_rdf.Triple
    | None,
) -> URIRef | BNode | Literal | None:
    """Convert a native :mod:`gmeow_rdf` term back to a compat term.

    Returns ``None`` for an unbound value. RDF 1.2 quoted-triple terms have no
    RDFLib counterpart and are surfaced explicitly rather than mishandled.
    """
    if value is None:
        return None
    if isinstance(value, gmeow_rdf.NamedNode):
        return URIRef(value.value)
    if isinstance(value, gmeow_rdf.BlankNode):
        return BNode(value.value)
    if isinstance(value, gmeow_rdf.Literal):
        if value.language is not None:
            return Literal(value.value, lang=value.language)
        datatype = value.datatype.value
        # The native IR expands a plain literal to ``xsd:string``; RDFLib keeps a
        # plain literal datatype-less. Drop ``xsd:string`` on the way back so the
        # compat term matches a plain RDFLib literal (the documented asymmetry).
        if datatype == _XSD_STRING:
            return Literal(value.value)
        return Literal(value.value, datatype=URIRef(datatype))
    raise NotImplementedError(
        "RDF 1.2 quoted-triple term has no rdflib counterpart and is not "
        f"representable through the compat Graph facade: {value!r}"
    )
