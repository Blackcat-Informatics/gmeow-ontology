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
   "why does this lint exist?").

The build authority is the Rust ``gmeow-pipeline`` executor (the dogfooded DAG
in ``slices/core/pipeline/``); #861 P7 retired the Python generator registry, so
there is no longer a per-generator registry-equality check here.

A principle enforced ONLY by documented review practice is a warning, never
an error and never silent: the honor system is allowed but always visible.
"""

from __future__ import annotations

import ast
import re
from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path

import gmeow_validate
from gmeow_rdf.compat.rdflib import RDF, RDFS, Graph, URIRef
from gmeow_rdf.compat.rdflib.term import Literal

from gmeow_tools import diagnostics
from gmeow_tools.config import PROJECT_ROOT
from gmeow_tools.validate import ValidationResult

META = "https://blackcatinformatics.ca/gmeow/meta#"
MANIFEST_FILE = PROJECT_ROOT / "governance" / "constitution.ttl"
CONSTITUTION_FILE = PROJECT_ROOT / "CONSTITUTION.md"
MAKEFILE = PROJECT_ROOT / "Makefile"

_HEADING = re.compile(r"^## (\d+)\. (.+?)\s*$", re.MULTILINE)
_MAKE_TARGET = re.compile(r"^([A-Za-z][A-Za-z0-9_-]*):", re.MULTILINE)

#: Canonical bold supersession markers; the gate keeps these in sync with the TTL.
_SUPERSEDED_MARKER = "**Superseded in part by Principle"
_EXTENDS_MARKER = "**Extends Principle"
_PRINCIPLE_REF = re.compile(r"Principle (\d+)")

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


def _emit(
    report: diagnostics.DiagnosticsReport,
    *,
    severity: str,
    code: str,
    message: str,
    logical: str | None = None,
) -> None:
    """Add one granular ``constitution.<code>`` finding to the report (#809)."""
    report.add(
        diagnostics.finding(
            severity=severity,
            code=f"constitution.{code}",
            message=message,
            tool="constitution",
            logical=logical,
        )
    )


def _check_principle_sync(
    manifest: Manifest, headings: dict[int, str], report: diagnostics.DiagnosticsReport
) -> None:
    """Manifest principles and CONSTITUTION.md headings must agree exactly."""
    declared = {p.number: p for p in manifest.principles}
    for number in sorted(set(headings) - set(declared)):
        _emit(
            report,
            severity="error",
            code="missing-manifest-entry",
            message=f"principle {number} ({headings[number]!r}) has no manifest "
            f"entry in governance/constitution.ttl",
        )
    for number in sorted(set(declared) - set(headings)):
        _emit(
            report,
            severity="error",
            code="absent-from-constitution",
            message=f"manifest declares principle {number} "
            f"({declared[number].title!r}) absent from CONSTITUTION.md",
        )
    for number in sorted(set(declared) & set(headings)):
        if declared[number].title != headings[number]:
            _emit(
                report,
                severity="error",
                code="title-drift",
                message=f"principle {number} title drift: manifest says "
                f"{declared[number].title!r}, CONSTITUTION.md says "
                f"{headings[number]!r}",
            )


def _check_references(
    manifest: Manifest, root: Path, report: diagnostics.DiagnosticsReport
) -> None:
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
                _emit(
                    report,
                    severity="error",
                    code="stale-artifact",
                    message=f"{name}: cited artifact {artifact!r} does not exist",
                    logical=str(enforcement.iri),
                )
        for symbol in enforcement.symbols:
            if not _symbol_defined(symbol, enforcement.artifacts, root):
                _emit(
                    report,
                    severity="error",
                    code="stale-symbol",
                    message=f"{name}: symbol {symbol!r} not found in any "
                    f"cited artifact",
                    logical=str(enforcement.iri),
                )
        for target in enforcement.make_targets:
            if target not in make_targets:
                _emit(
                    report,
                    severity="error",
                    code="stale-make-target",
                    message=f"{name}: Makefile target {target!r} does not exist",
                    logical=str(enforcement.iri),
                )
        for command in enforcement.cli_commands:
            if command not in cli_commands:
                _emit(
                    report,
                    severity="error",
                    code="stale-cli-command",
                    message=f"{name}: gmeow CLI command {command!r} is not registered",
                    logical=str(enforcement.iri),
                )


def _markdown_relations(md_text: str, marker: str) -> dict[int, set[int]]:
    """Map each principle's heading number to the target numbers named in ``marker``.

    A relation is read from a bold marker line (e.g. ``**Superseded in part by
    Principle 17:**``) inside that principle's section; the ``from`` number is the
    enclosing ``## N. Title`` heading, the targets are every ``Principle N`` on the
    marker line.
    """
    headings = list(_HEADING.finditer(md_text))
    relations: dict[int, set[int]] = {}
    for index, heading in enumerate(headings):
        number = int(heading.group(1))
        end = headings[index + 1].start() if index + 1 < len(headings) else len(md_text)
        section = md_text[heading.end() : end]
        for line in section.splitlines():
            if line.lstrip().startswith(marker):
                targets = {int(n) for n in _PRINCIPLE_REF.findall(line)}
                if targets:
                    relations.setdefault(number, set()).update(targets)
    return relations


def _compare_relation(
    prop: str,
    md_relations: dict[int, set[int]],
    ttl_relations: dict[int, set[int]],
    report: diagnostics.DiagnosticsReport,
) -> None:
    """``meta:<prop>`` in the TTL must equal the markdown markers, both directions."""
    for number in sorted(set(md_relations) | set(ttl_relations)):
        md = md_relations.get(number, set())
        ttl = ttl_relations.get(number, set())
        if md != ttl:
            _emit(
                report,
                severity="error",
                code="relation-drift",
                message=f"principle {number} meta:{prop} drift: CONSTITUTION.md "
                f"marker names {sorted(md) or '∅'}, "
                f"governance/constitution.ttl names {sorted(ttl) or '∅'}",
            )


def _check_supersession(
    md_text: str, manifest: Manifest, report: diagnostics.DiagnosticsReport
) -> None:
    """The bold supersession markers in CONSTITUTION.md must match the TTL relations."""
    _compare_relation(
        "supersededInPartBy",
        _markdown_relations(md_text, _SUPERSEDED_MARKER),
        {
            p.number: set(p.superseded_in_part_by)
            for p in manifest.principles
            if p.superseded_in_part_by
        },
        report,
    )
    _compare_relation(
        "extends",
        _markdown_relations(md_text, _EXTENDS_MARKER),
        {p.number: set(p.extends) for p in manifest.principles if p.extends},
        report,
    )


def constitution_report(
    *,
    manifest_path: Path = MANIFEST_FILE,
    constitution_path: Path = CONSTITUTION_FILE,
    root: Path = PROJECT_ROOT,
) -> diagnostics.DiagnosticsReport:
    """Run every constitution-as-code check into one granular diagnostics report.

    RUST-FIRST/PYTHON-SURFACE (#809): the graph-resident **enforcement-coverage**
    check (principle-unenforced / honor-system / orphaned-enforcement /
    undeclared-enforcement) runs natively in Rust over a ``gmeow_rdf`` Store
    (``gmeow_validate.constitution_enforcement_report``). The other checks are
    inherently Python-introspection — they probe the filesystem, parse Python
    ASTs, introspect the Typer app, and read the live generator registry, none of
    which is RDF — so they stay in Python but emit *granular* ``constitution.*``
    findings through the same canonical (Rust-owned) ``Finding`` model.
    """
    report = diagnostics.report("constitution")
    try:
        manifest = load_manifest(manifest_path)
    except Exception as exc:
        _emit(
            report,
            severity="error",
            code="manifest-parse",
            message=f"{manifest_path}: does not parse: {exc}",
        )
        return report
    try:
        constitution_text = constitution_path.read_text(encoding="utf-8")
    except OSError as exc:
        _emit(
            report,
            severity="error",
            code="constitution-unreadable",
            message=f"{constitution_path}: cannot read: {exc}",
        )
        return report
    headings = {
        int(number): title for number, title in _HEADING.findall(constitution_text)
    }
    _check_principle_sync(manifest, headings, report)
    # Enforcement coverage — native Rust over the manifest graph (#809).
    report.extend(
        gmeow_validate.constitution_enforcement_report(
            manifest_path.read_text(encoding="utf-8")
        )
    )
    _check_references(manifest, root, report)
    _check_supersession(constitution_text, manifest, report)
    return report


def check_constitution(
    *,
    manifest_path: Path = MANIFEST_FILE,
    constitution_path: Path = CONSTITUTION_FILE,
    root: Path = PROJECT_ROOT,
) -> ValidationResult:
    """Run every constitution-as-code check; errors mean the build fails.

    A thin string view over :func:`constitution_report` (the granular canonical
    report) for the gate / ``ok`` check / legacy string consumers.
    """
    report = constitution_report(
        manifest_path=manifest_path,
        constitution_path=constitution_path,
        root=root,
    )
    return ValidationResult(
        errors=list(report.errors),
        warnings=list(report.warnings),
    )


def to_diagnostics_report(
    result: ValidationResult | None = None,
    *,
    tool: str = "constitution",
) -> diagnostics.DiagnosticsReport:
    """Return the granular constitution diagnostics report (#809).

    Supersedes the legacy ``constitution.error`` / ``constitution.warning`` roll-up:
    the canonical report now carries per-check codes (``principle-unenforced``,
    ``stale-artifact``, ``title-drift``, …). The ``result`` argument is ignored —
    the report is rebuilt from source so it never drifts from a stale snapshot.
    """
    return constitution_report()
