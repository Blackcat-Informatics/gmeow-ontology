# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Constitution-as-code (#280): the principle→enforcement gate.

CONSTITUTION.md already names "Embodied in / Tested by" artifacts for every
principle — but prose claims rot silently. ``governance/constitution.ttl``
restates the mapping as RDF, and this module makes it falsifiable. The gate
fails when:

1. a principle has **zero** registered enforcement, or the manifest and
   CONSTITUTION.md disagree on the principle set (numbers or verbatim titles);
2. a cited artifact, Python symbol, Makefile target, or CLI command no longer
   exists (stale "Tested by" reference);
3. an enforcement mechanism maps to no principle (orphaned enforcement —
   "why does this lint exist?");
4. the declared generator set differs from the live registry — a new
   generator must be constitutionally registered the moment it exists.

A principle enforced ONLY by documented review practice is a warning, never
an error and never silent: the honor system is allowed but always visible.
"""

from __future__ import annotations

import ast
import re
from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path

from rdflib import RDF, RDFS, Graph, URIRef
from rdflib.term import Literal

from gmeow_tools.config import PROJECT_ROOT
from gmeow_tools.validate import ValidationResult

META = "https://blackcatinformatics.ca/gmeow/meta#"
MANIFEST_FILE = PROJECT_ROOT / "governance" / "constitution.ttl"
CONSTITUTION_FILE = PROJECT_ROOT / "CONSTITUTION.md"
MAKEFILE = PROJECT_ROOT / "Makefile"

_HEADING = re.compile(r"^## (\d+)\. (.+?)\s*$", re.MULTILINE)
_MAKE_TARGET = re.compile(r"^([A-Za-z][A-Za-z0-9_-]*):", re.MULTILINE)

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
    generators: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class Principle:
    """One constitutional principle declared in the manifest."""

    iri: URIRef
    number: int
    title: str
    enforced_by: tuple[URIRef, ...]


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
                generators=_strings(graph, node, URIRef(META + "generator")),
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


@lru_cache(maxsize=8)
def _python_names(path: Path) -> frozenset[str]:
    """Top-level def / class / assignment names in a Python file."""
    tree = ast.parse(path.read_text(encoding="utf-8"))
    names: set[str] = set()
    for node in tree.body:
        if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef | ast.ClassDef):
            names.add(node.name)
        elif isinstance(node, ast.Assign):
            names.update(t.id for t in node.targets if isinstance(t, ast.Name))
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            names.add(node.target.id)
    return frozenset(names)


def _symbol_defined(symbol: str, artifacts: tuple[str, ...], root: Path) -> bool:
    """Whether ``symbol`` is defined in any artifact (AST for .py, verbatim else)."""
    for artifact in artifacts:
        path = root / artifact
        if not path.is_file():
            continue
        if path.suffix == ".py":
            if symbol in _python_names(path):
                return True
        elif symbol in path.read_text(encoding="utf-8"):
            return True
    return False


def _cli_command_names() -> frozenset[str]:
    """Every command registered on the gmeow-dev Typer app."""
    from gmeow_tools.cli_dev import app

    names: set[str] = set()
    for command in app.registered_commands:
        if command.name:
            names.add(command.name)
        elif command.callback is not None:
            names.add(command.callback.__name__.replace("_", "-"))
    return frozenset(names)


def _registered_generators() -> frozenset[str]:
    """Every generator name in the live registry (same imports as the CLI)."""
    from gmeow_tools.generator import registry
    from gmeow_tools.load_generators import load_all

    load_all()
    return frozenset(registry())


def _check_principle_sync(
    manifest: Manifest, headings: dict[int, str], result: ValidationResult
) -> None:
    """Manifest principles and CONSTITUTION.md headings must agree exactly."""
    declared = {p.number: p for p in manifest.principles}
    for number in sorted(set(headings) - set(declared)):
        result.errors.append(
            f"principle {number} ({headings[number]!r}) has no manifest entry "
            f"in governance/constitution.ttl"
        )
    for number in sorted(set(declared) - set(headings)):
        result.errors.append(
            f"manifest declares principle {number} "
            f"({declared[number].title!r}) absent from CONSTITUTION.md"
        )
    for number in sorted(set(declared) & set(headings)):
        if declared[number].title != headings[number]:
            result.errors.append(
                f"principle {number} title drift: manifest says "
                f"{declared[number].title!r}, CONSTITUTION.md says "
                f"{headings[number]!r}"
            )


def _check_enforcement_coverage(manifest: Manifest, result: ValidationResult) -> None:
    """Every principle enforced; honor-system-only is visible; no orphans."""
    cited: set[URIRef] = set()
    for principle in manifest.principles:
        known = [e for e in principle.enforced_by if e in manifest.enforcements]
        for missing in set(principle.enforced_by) - set(known):
            result.errors.append(
                f"principle {principle.number} cites undeclared enforcement {missing}"
            )
        cited.update(known)
        if not known:
            result.errors.append(
                f"principle {principle.number} ({principle.title!r}) has zero "
                f"registered enforcement"
            )
        elif all(manifest.enforcements[e].kind == "Practice" for e in known):
            result.warnings.append(
                f"principle {principle.number} ({principle.title!r}) is enforced "
                f"only by review practice (honor system)"
            )
    for orphan in sorted(set(manifest.enforcements) - cited):
        result.errors.append(
            f"orphaned enforcement {orphan} maps to no principle — why does it exist?"
        )


def _check_references(manifest: Manifest, root: Path, result: ValidationResult) -> None:
    """Every cited artifact / symbol / make target / CLI command must exist."""
    makefile = root / MAKEFILE.name
    make_targets = (
        frozenset(_MAKE_TARGET.findall(makefile.read_text(encoding="utf-8")))
        if makefile.is_file()
        else frozenset()
    )
    cli_commands = _cli_command_names()
    for enforcement in manifest.enforcements.values():
        name = enforcement.iri.removeprefix(META)
        for artifact in enforcement.artifacts:
            if not (root / artifact).exists():
                result.errors.append(
                    f"{name}: cited artifact {artifact!r} does not exist"
                )
        for symbol in enforcement.symbols:
            if not _symbol_defined(symbol, enforcement.artifacts, root):
                result.errors.append(
                    f"{name}: symbol {symbol!r} not found in any cited artifact"
                )
        for target in enforcement.make_targets:
            if target not in make_targets:
                result.errors.append(
                    f"{name}: Makefile target {target!r} does not exist"
                )
        for command in enforcement.cli_commands:
            if command not in cli_commands:
                result.errors.append(
                    f"{name}: gmeow CLI command {command!r} is not registered"
                )


def _check_generator_registry(manifest: Manifest, result: ValidationResult) -> None:
    """The declared generator set must equal the live registry exactly."""
    declared: set[str] = set()
    for enforcement in manifest.enforcements.values():
        declared.update(enforcement.generators)
    live = _registered_generators()
    for missing in sorted(live - declared):
        result.errors.append(
            f"generator {missing!r} is registered but not constitutionally "
            f"declared (add it to governance/constitution.ttl)"
        )
    for stale in sorted(declared - live):
        result.errors.append(
            f"manifest declares generator {stale!r} which is not in the live registry"
        )


def check_constitution(
    *,
    manifest_path: Path = MANIFEST_FILE,
    constitution_path: Path = CONSTITUTION_FILE,
    root: Path = PROJECT_ROOT,
) -> ValidationResult:
    """Run every constitution-as-code check; errors mean the build fails."""
    result = ValidationResult()
    try:
        manifest = load_manifest(manifest_path)
    except Exception as exc:
        result.errors.append(f"{manifest_path}: does not parse: {exc}")
        return result
    try:
        headings = constitution_headings(constitution_path)
    except Exception as exc:
        result.errors.append(f"{constitution_path}: cannot read: {exc}")
        return result
    _check_principle_sync(manifest, headings, result)
    _check_enforcement_coverage(manifest, result)
    _check_references(manifest, root, result)
    _check_generator_registry(manifest, result)
    return result
