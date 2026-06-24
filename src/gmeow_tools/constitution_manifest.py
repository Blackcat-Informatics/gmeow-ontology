# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Constitution manifest data model.

This module owns ONLY the non-enforcement data model and parsing of
``governance/constitution.ttl``. It is intentionally free of gate/report logic
so that it can be imported by consumers that need the manifest structure
without pulling in diagnostics or check dependencies.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path

from gmeow_rdf.compat.rdflib import RDF, RDFS, Graph, URIRef
from gmeow_rdf.compat.rdflib.term import Literal

from gmeow_tools.config import PROJECT_ROOT

META = "https://blackcatinformatics.ca/gmeow/meta#"
MANIFEST_FILE = PROJECT_ROOT / "governance" / "constitution.ttl"
CONSTITUTION_FILE = PROJECT_ROOT / "CONSTITUTION.md"
MAKEFILE = PROJECT_ROOT / "Makefile"

_HEADING = re.compile(r"^## (\d+)\. (.+?)\s*$", re.MULTILINE)

#: Enforcement classes; Practice is the honor-system kind (warning when alone).
_ENFORCEMENT_KINDS = ("Lint", "TestSuite", "Shape", "Gate", "Practice")


@dataclass(frozen=True, slots=True)
class Enforcement:
    """One enforcement mechanism declared in the manifest."""

    iri: URIRef
    kind: str
    artifacts: tuple[str, ...]
    symbols: tuple[str, ...]
    make_targets: tuple[str, ...]
    cli_commands: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class Principle:
    """One constitutional principle declared in the manifest."""

    iri: URIRef
    number: int
    title: str
    enforced_by: tuple[URIRef, ...]
    #: Numbers of later principles that supersede part of this one (P17 over P2/P8/P12).
    superseded_in_part_by: tuple[int, ...] = ()
    #: Numbers of earlier principles this one extends (P18 over P17 / P13).
    extends: tuple[int, ...] = ()


@dataclass(slots=True)
class Manifest:
    """The parsed principle→enforcement manifest."""

    principles: list[Principle] = field(default_factory=list)
    enforcements: dict[URIRef, Enforcement] = field(default_factory=dict)


def _strings(graph: Graph, subject: URIRef, predicate: URIRef) -> tuple[str, ...]:
    return tuple(
        sorted(
            str(o) for o in graph.objects(subject, predicate) if isinstance(o, Literal)
        )
    )


def _principle_numbers(
    graph: Graph, subject: URIRef, predicate: URIRef
) -> tuple[int, ...]:
    """Resolve ``subject predicate meta:PrincipleN`` objects to sorted ``N`` ints."""
    numbers: set[int] = set()
    for obj in graph.objects(subject, predicate):
        if not isinstance(obj, URIRef):
            continue
        number = graph.value(obj, URIRef(META + "number"))
        if number is not None:
            numbers.add(int(str(number)))
    return tuple(sorted(numbers))


def load_manifest(path: Path = MANIFEST_FILE) -> Manifest:
    """Parse ``governance/constitution.ttl`` into typed records."""
    graph = Graph()
    graph.parse(path, format="turtle")
    manifest = Manifest()
    for kind in _ENFORCEMENT_KINDS:
        for node in graph.subjects(RDF.type, URIRef(META + kind)):
            if not isinstance(node, URIRef) or (node, RDF.type, RDFS.Class) in graph:
                continue
            manifest.enforcements[node] = Enforcement(
                iri=node,
                kind=kind,
                artifacts=_strings(graph, node, URIRef(META + "artifact")),
                symbols=_strings(graph, node, URIRef(META + "symbol")),
                make_targets=_strings(graph, node, URIRef(META + "makeTarget")),
                cli_commands=_strings(graph, node, URIRef(META + "cliCommand")),
            )
    for node in graph.subjects(RDF.type, URIRef(META + "Principle")):
        if not isinstance(node, URIRef):
            continue
        number = graph.value(node, URIRef(META + "number"))
        title = graph.value(node, URIRef(META + "title"))
        enforced = tuple(
            sorted(
                o
                for o in graph.objects(node, URIRef(META + "enforcedBy"))
                if isinstance(o, URIRef)
            )
        )
        manifest.principles.append(
            Principle(
                iri=node,
                number=int(str(number)) if number is not None else -1,
                title=str(title) if title is not None else "",
                enforced_by=enforced,
                superseded_in_part_by=_principle_numbers(
                    graph, node, URIRef(META + "supersededInPartBy")
                ),
                extends=_principle_numbers(graph, node, URIRef(META + "extends")),
            )
        )
    manifest.principles.sort(key=lambda p: p.number)
    return manifest


def constitution_headings(path: Path = CONSTITUTION_FILE) -> dict[int, str]:
    """``## N. Title`` headings of CONSTITUTION.md, as ``{number: title}``."""
    return {
        int(number): title
        for number, title in _HEADING.findall(path.read_text(encoding="utf-8"))
    }
