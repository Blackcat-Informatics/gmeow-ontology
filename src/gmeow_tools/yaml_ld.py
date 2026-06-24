# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""YAML-LD-star / JSON-LD-star codec and reverse parse lane (#699).

The Rust :class:`YamlLdStage` emits ``dist/gmeow.jsonld`` as RDF-1.2-star JSON-LD.
This module provides the Python side of that surface: deterministic conversion to
YAML-LD-star and a focused reverse parser that reconstructs RDF-1.2 triples,
quoted triples, and directional language strings on a native ``pyoxigraph.Store``.

The parser is intentionally narrow: it handles the node-object model produced by
the GMEOW JSON-LD-star emitter plus ordinary compact-IRI expansion against the
GMEOW prefix registry. It does not implement full JSON-LD 1.1 expansion and will
hard-fail on unsupported extended features.
"""

from __future__ import annotations

import json
from typing import Any

import pyoxigraph
import yaml
from gmeow_rdf.compat.rdflib import Graph

from gmeow_tools.bundle import bundled_schema
from gmeow_tools.config import NAMESPACE, PREFIXES, SCHEMAS_DIR

_RDF = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
_RDF_TYPE = _RDF + "type"
_RDF_REIFIES = _RDF + "reifies"


class YamlLdError(ValueError):
    """Raised when YAML-LD-star input cannot be parsed or uses unsupported features."""


class _NoAnchorSafeLoader(yaml.SafeLoader):  # type: ignore[misc]
    """SafeLoader that rejects YAML anchors, aliases, and therefore custom tags."""

    def compose_node(
        self, parent: yaml.nodes.Node | None, index: Any
    ) -> yaml.nodes.Node:
        """Compose a node, raising on aliases and anchored nodes."""
        if self.check_event(yaml.AliasEvent):
            self.get_event()
            raise YamlLdError("YAML-LD aliases are not supported")
        event = self.peek_event()
        if getattr(event, "anchor", None) is not None:
            raise YamlLdError("YAML-LD anchors are not supported")
        return super().compose_node(parent, index)


class _NoAliasSafeDumper(yaml.SafeDumper):  # type: ignore[misc]
    """SafeDumper that never emits alias/anchor references."""

    def ignore_aliases(self, data: Any) -> bool:
        """Disable alias generation for all repeated data."""
        return True


def _default_schema_url() -> str:
    """Return a bounded schema URL for the YAML language-server header.

    The default is tied to the bundled ``gmeow.schema.json`` (#700) so the emitted
    YAML-LD is self-describing even when no public schema URL has been finalized.
    """
    candidate = SCHEMAS_DIR / "gmeow.schema.json"
    if candidate.is_file():
        return candidate.as_uri()
    if bundled_schema() is not None:
        return "gmeow.schema.json"
    return "https://blackcatinformatics.ca/gmeow/schemas/gmeow.schema.json"


def jsonld_star_to_yamlld(json_bytes: bytes, *, schema_url: str | None = None) -> bytes:
    """Convert deterministic JSON-LD-star bytes to YAML-LD-star bytes.

    Args:
        json_bytes: JSON-LD-star document bytes.
        schema_url: Override the ``$schema`` URL in the YAML language-server header.

    Returns:
        UTF-8 encoded YAML-LD-star bytes with a deterministic, comment-safe dump.
    """
    url = schema_url if schema_url is not None else _default_schema_url()
    doc = json.loads(json_bytes)
    body: str = yaml.dump(
        doc,
        Dumper=_NoAliasSafeDumper,
        sort_keys=True,
        allow_unicode=True,
        default_flow_style=False,
    )
    header = (
        f"# yaml-language-server: $schema={url}\n"
        "# TODO(#700): default schema URL is bounded to the bundled"
        " gmeow.schema.json;\n"
        "# replace with the canonical public URL once issue #700 finalizes"
        " the schema surface.\n"
    )
    return (header + body).encode("utf-8")


def yamlld_to_jsonld(yaml_bytes: bytes) -> bytes:
    """Convert YAML-LD-star bytes to deterministic JSON-LD-star bytes.

    Args:
        yaml_bytes: YAML-LD-star document bytes.

    Returns:
        JSON bytes compacted with sorted keys and no insignificant whitespace.

    Raises:
        YamlLdError: If the YAML contains anchors, aliases, or unsupported tags.
    """
    doc = yaml.load(yaml_bytes, Loader=_NoAnchorSafeLoader)
    text: str = json.dumps(doc, sort_keys=True, separators=(",", ":"))
    return text.encode("utf-8")


class _Parser:
    """Focused JSON-LD-star node-object parser."""

    def __init__(
        self,
        term_map: dict[str, str],
        prefix_map: dict[str, str],
        vocab: str | None,
    ) -> None:
        self.store = pyoxigraph.Store()
        self.term_map = term_map
        self.prefix_map = prefix_map
        self.vocab = vocab
        self._blank_counter = 0

    def _fresh_blank(self) -> pyoxigraph.BlankNode:
        node = pyoxigraph.BlankNode(f"gmeow-fresh-{self._blank_counter}")
        self._blank_counter += 1
        return node

    def _expand_term(self, term: str) -> str:
        """Expand a JSON-LD term to an absolute IRI string."""
        if term in self.term_map:
            return self.term_map[term]
        if "://" in term or term.startswith("urn:"):
            return term
        if term.startswith("_:"):
            return term
        if ":" in term:
            prefix, local = term.split(":", 1)
            if prefix in self.prefix_map:
                return self.prefix_map[prefix] + local
            # Unknown prefix but looks like an absolute IRI scheme (e.g. mailto:).
            return term
        if self.vocab is not None:
            return self.vocab + term
        raise YamlLdError(f"Cannot expand term {term!r}: no @vocab and no prefix")

    def _node(self, term: str) -> pyoxigraph.NamedNode | pyoxigraph.BlankNode:
        """Return a NamedNode or BlankNode for an expanded or raw term."""
        if term.startswith("_:"):
            return pyoxigraph.BlankNode(term[2:])
        return pyoxigraph.NamedNode(self._expand_term(term))

    def _literal_from_value(self, value: dict[str, Any]) -> pyoxigraph.Literal:
        """Build a pyoxigraph Literal from a JSON-LD value object."""
        lexical = value["@value"]
        language = value.get("@language")
        direction = value.get("@direction")
        datatype = value.get("@type")
        if datatype is not None:
            if language is not None:
                raise YamlLdError("A value cannot have both @type and @language")
            return pyoxigraph.Literal(
                str(lexical),
                datatype=pyoxigraph.NamedNode(self._expand_term(datatype)),
            )
        if direction is not None and language is None:
            raise YamlLdError("@direction requires @language")
        if language is not None:
            if not isinstance(lexical, str):
                raise YamlLdError("Language-tagged value must be a string")
            bd = pyoxigraph.BaseDirection(direction) if direction is not None else None
            return pyoxigraph.Literal(lexical, language=language, direction=bd)
        if isinstance(lexical, bool):
            return pyoxigraph.Literal(lexical)
        if isinstance(lexical, int):
            return pyoxigraph.Literal(lexical)
        if isinstance(lexical, float):
            return pyoxigraph.Literal(lexical)
        return pyoxigraph.Literal(str(lexical))

    def _scalar_to_term(
        self, value: Any
    ) -> pyoxigraph.NamedNode | pyoxigraph.BlankNode | pyoxigraph.Literal:
        """Convert a JSON scalar to an RDF term (IRI or literal)."""
        if isinstance(value, bool):
            return pyoxigraph.Literal(value)
        if isinstance(value, int):
            return pyoxigraph.Literal(value)
        if isinstance(value, float):
            return pyoxigraph.Literal(value)
        if not isinstance(value, str):
            raise YamlLdError(f"Unsupported JSON scalar type {type(value).__name__}")
        if value.startswith("_:"):
            return pyoxigraph.BlankNode(value[2:])
        if "://" in value or value.startswith("urn:"):
            return pyoxigraph.NamedNode(value)
        if ":" in value:
            prefix, local = value.split(":", 1)
            if prefix in self.prefix_map:
                return pyoxigraph.NamedNode(self.prefix_map[prefix] + local)
            return pyoxigraph.NamedNode(value)
        if self.vocab is not None:
            return pyoxigraph.NamedNode(self.vocab + value)
        return pyoxigraph.Literal(value)

    def _parse_annotation(
        self,
        reifier: pyoxigraph.NamedNode | pyoxigraph.BlankNode,
        annotation: dict[str, Any] | list[Any],
    ) -> None:
        """Emit annotation triples about a reifier node."""
        annotations = annotation if isinstance(annotation, list) else [annotation]
        for ann_obj in annotations:
            if not isinstance(ann_obj, dict):
                raise YamlLdError(
                    "@annotation value must be an object or array of objects"
                )
            for key, val in ann_obj.items():
                pred = pyoxigraph.NamedNode(self._expand_term(key))
                self._parse_property(reifier, pred, val)

    def _parse_property(
        self,
        subj: pyoxigraph.NamedNode | pyoxigraph.BlankNode,
        pred: pyoxigraph.NamedNode,
        value: Any,
    ) -> None:
        """Emit triples for one property value, recursing into node objects."""
        if isinstance(value, list):
            for item in value:
                self._parse_property(subj, pred, item)
            return

        if isinstance(value, dict):
            if "@value" in value or "@language" in value or "@direction" in value:
                lit = self._literal_from_value(value)
                self.store.add(
                    pyoxigraph.Quad(subj, pred, lit, pyoxigraph.DefaultGraph())
                )
                if "@annotation" in value:
                    reifier = self._fresh_blank()
                    self.store.add(
                        pyoxigraph.Quad(
                            reifier,
                            pyoxigraph.NamedNode(_RDF_REIFIES),
                            pyoxigraph.Triple(subj, pred, lit),
                            pyoxigraph.DefaultGraph(),
                        )
                    )
                    self._parse_annotation(reifier, value["@annotation"])
                return

            annotation = value.get("@annotation")
            node_obj = self._parse_node_object(value)
            self.store.add(
                pyoxigraph.Quad(subj, pred, node_obj, pyoxigraph.DefaultGraph())
            )
            if annotation is not None:
                self.store.add(
                    pyoxigraph.Quad(
                        node_obj,
                        pyoxigraph.NamedNode(_RDF_REIFIES),
                        pyoxigraph.Triple(subj, pred, node_obj),
                        pyoxigraph.DefaultGraph(),
                    )
                )
                self._parse_annotation(node_obj, annotation)
            return

        scalar_obj = self._scalar_to_term(value)
        self.store.add(
            pyoxigraph.Quad(subj, pred, scalar_obj, pyoxigraph.DefaultGraph())
        )

    def _parse_node_object(
        self, obj: dict[str, Any]
    ) -> pyoxigraph.NamedNode | pyoxigraph.BlankNode:
        """Emit triples for a node object and return its node identifier.

        ``@annotation`` on the object is intentionally ignored here; the caller
        that holds the embedding subject/predicate handles annotation semantics.
        """
        node_id = obj.get("@id")
        if node_id is None:
            node: pyoxigraph.NamedNode | pyoxigraph.BlankNode = self._fresh_blank()
        else:
            node = self._node(str(node_id))

        for key, val in obj.items():
            if key in ("@id", "@annotation", "@context"):
                continue
            if key == "@type":
                for type_term in val if isinstance(val, list) else [val]:
                    type_node = self._node(str(type_term))
                    self.store.add(
                        pyoxigraph.Quad(
                            node,
                            pyoxigraph.NamedNode(_RDF_TYPE),
                            type_node,
                            pyoxigraph.DefaultGraph(),
                        )
                    )
                continue
            pred = pyoxigraph.NamedNode(self._expand_term(key))
            self._parse_property(node, pred, val)

        return node


def _load_context(
    context: Any,
) -> tuple[dict[str, str], dict[str, str], str | None]:
    """Build term map, prefix map, and optional @vocab from JSON-LD @context."""
    term_map: dict[str, str] = dict(PREFIXES)
    prefix_map: dict[str, str] = dict(PREFIXES)
    vocab: str | None = NAMESPACE

    if context is None:
        return term_map, prefix_map, vocab

    if isinstance(context, str):
        raise YamlLdError("Remote JSON-LD @context references are not supported")

    contexts = context if isinstance(context, list) else [context]
    for ctx in contexts:
        if not isinstance(ctx, dict):
            raise YamlLdError("JSON-LD @context must be an object or array of objects")
        for key, val in ctx.items():
            if key == "@vocab":
                vocab = str(val) if val is not None else None
            elif key == "@base":
                # Base resolution is not implemented; absolute IRIs are required.
                continue
            elif isinstance(val, str):
                term_map[key] = val
                if val.endswith(("/", "#")):
                    prefix_map[key] = val
            elif isinstance(val, dict) and "@id" in val:
                iri = str(val["@id"])
                term_map[key] = iri
                if iri.endswith(("/", "#")):
                    prefix_map[key] = iri
            else:
                raise YamlLdError(f"Unsupported @context term definition for {key!r}")

    return term_map, prefix_map, vocab


def parse_jsonld_star(json_bytes: bytes) -> pyoxigraph.Store:
    """Parse JSON-LD-star into an RDF-1.2 ``pyoxigraph.Store``.

    Args:
        json_bytes: JSON-LD-star document bytes.

    Returns:
        A ``pyoxigraph.Store`` containing the reconstructed default-graph triples,
        quoted triples (via ``rdf:reifies``), and directional language strings.

    Raises:
        YamlLdError: If the document uses unsupported JSON-LD features.
    """
    doc = json.loads(json_bytes)
    context = doc.get("@context") if isinstance(doc, dict) else None
    term_map, prefix_map, vocab = _load_context(context)
    parser = _Parser(term_map, prefix_map, vocab)

    if isinstance(doc, list):
        for item in doc:
            if isinstance(item, dict):
                parser._parse_node_object(item)
    elif isinstance(doc, dict):
        parser._parse_node_object(doc)
    else:
        raise YamlLdError("JSON-LD document must be an object or array of objects")

    return parser.store


def parse_yaml_ld(yaml_bytes: bytes) -> pyoxigraph.Store:
    """Parse YAML-LD-star into an RDF-1.2 ``pyoxigraph.Store``.

    Args:
        yaml_bytes: YAML-LD-star document bytes.

    Returns:
        A ``pyoxigraph.Store`` reconstructed from the intermediate JSON-LD-star form.
    """
    return parse_jsonld_star(yamlld_to_jsonld(yaml_bytes))


def yaml_ld_to_graph(yaml_bytes: bytes) -> Graph:
    """Parse YAML-LD-star into a ``gmeow_rdf.compat.rdflib.Graph``.

    This is a convenience wrapper for the existing up-projection lane: it parses
    YAML-LD-star to a ``pyoxigraph.Store``, exports canonical N-Quads, and loads
    them into the project's rdflib-compatible graph facade.

    Note:
        RDF 1.2 quoted triples are not representable through the compat ``Graph``
        facade. Use :func:`parse_yaml_ld` when the document contains annotations.
    """
    store = parse_yaml_ld(yaml_bytes)
    nquads = store.dump(format=pyoxigraph.RdfFormat.N_QUADS)
    assert nquads is not None
    graph = Graph()
    graph.parse(data=nquads.decode("utf-8"), format="nquads")
    return graph
