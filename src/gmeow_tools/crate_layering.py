# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Crate-layering gate (#820 S0): kernel purity + an acyclic crate DAG.

CONSTITUTION Principle 16 ("a small core; everything else a published
extension") has a Rust-side twin: ``gmeow-rdf`` is the generic RDF-1.2 narrow
waist (the kernel), and slice / domain semantics must layer *above* it, never
leak *into* it. This gate makes that falsifiable at the crate boundary, the way
the alignment / projection / i18n lints make their invariants falsifiable.

Two structural invariants over ``crates/*/Cargo.toml``:

* **Kernel purity** — ``gmeow-rdf`` has ZERO first-party (``gmeow-*`` path)
  dependencies. A ``SliceId`` or any other GMEOW-specific construct in the
  kernel is the failure #820 exists to prevent; if the kernel ever grows a
  first-party dependency it is no longer the generic waist.
* **Acyclic layering** — the first-party crate dependency graph is a DAG. A
  cycle between ``gmeow-slice`` / ``gmeow-rdf`` / ``gmeow-shacl`` /
  ``gmeow-slicetest`` / the mapping compiler is exactly the monolithic-compiler
  trap the RFC's §3 warns against, so it is a hard error.

First-party deps are identified by a ``path = "../…"`` entry under
``[dependencies]`` (or a ``cfg``-gated ``[target.*.dependencies]`` table) whose
package name starts with ``gmeow-``. Registry crates such as ``gmeow-gts`` (a
*published* dependency, no ``path``) are deliberately NOT first-party: they are
an external boundary, not an internal layering edge.

This is greenfield / no-optionality / hard-fail: a malformed ``Cargo.toml``, a
kernel impurity, or a cycle all FAIL — there is no degraded pass.
"""

from __future__ import annotations

import tomllib
from dataclasses import dataclass, field
from pathlib import Path

from gmeow_tools import diagnostics
from gmeow_tools.config import PROJECT_ROOT
from gmeow_tools.validate import ValidationResult

__all__ = [
    "KERNEL_CRATE",
    "CrateLayeringReport",
    "check_crate_layering",
    "findings_to_result",
    "to_diagnostics_report",
]

#: The generic RDF-1.2 narrow waist. It must remain free of any first-party
#: (``gmeow-*`` path) dependency — slice / domain semantics layer above it.
KERNEL_CRATE = "gmeow-rdf"

#: First-party crate prefix. Path-less registry crates with this prefix
#: (``gmeow-gts``) are an external boundary, not an internal layering edge.
_FIRST_PARTY_PREFIX = "gmeow-"

#: The dependency tables a Cargo manifest may carry. ``[target.*.dependencies]``
#: holds the ``cfg``-gated edges (e.g. gmeow-shacl's non-wasm ``pyo3``), so a
#: first-party edge hidden behind a ``cfg`` is still a real layering edge.
_DEP_TABLE_KEYS = ("dependencies", "build-dependencies", "dev-dependencies")


@dataclass
class CrateLayeringReport:
    """Outcome of the crate-layering gate.

    Attributes:
        errors: Hard violations (kernel impurity, dependency cycle, malformed
            or missing manifest) — any one fails the gate.
        warnings: Non-fatal observations (currently unused; kept for symmetry
            with the other lint reports and forward findings).
        edges: The discovered first-party dependency edges, ``{crate: {deps}}``,
            for downstream tooling / tests.
    """

    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    edges: dict[str, set[str]] = field(default_factory=dict)

    @property
    def ok(self) -> bool:
        """Whether the gate passed (no errors)."""
        return not self.errors


def _crate_name(manifest: dict[str, object]) -> str | None:
    """The ``[package] name`` of a parsed Cargo manifest, if present."""
    package = manifest.get("package")
    if isinstance(package, dict):
        name = package.get("name")
        if isinstance(name, str):
            return name
    return None


def _iter_dep_tables(manifest: dict[str, object]) -> list[dict[str, object]]:
    """Every ``[dependencies]``-shaped table in a manifest, cfg-gated ones included."""
    tables: list[dict[str, object]] = []
    for key in _DEP_TABLE_KEYS:
        table = manifest.get(key)
        if isinstance(table, dict):
            tables.append(table)
    target = manifest.get("target")
    if isinstance(target, dict):
        for cfg_table in target.values():
            if not isinstance(cfg_table, dict):
                continue
            for key in _DEP_TABLE_KEYS:
                table = cfg_table.get(key)
                if isinstance(table, dict):
                    tables.append(table)
    return tables


def _first_party_deps(manifest: dict[str, object]) -> set[str]:
    """First-party (``gmeow-*`` *with a* ``path``) dependency names of a manifest.

    A ``gmeow-*`` dependency declared *without* a ``path`` (i.e. resolved from a
    registry, like ``gmeow-gts``) is an external published boundary, not an
    internal layering edge, so it is excluded.
    """
    deps: set[str] = set()
    for table in _iter_dep_tables(manifest):
        for dep_name, spec in table.items():
            # A ``package = "gmeow-..."`` rename means the REAL crate is the
            # renamed package, not the table key; a first-party edge hidden
            # behind a rename is still a real layering edge (#820 S0). Resolve
            # the effective crate name before the first-party / path checks.
            effective = dep_name
            has_path = False
            if isinstance(spec, dict):
                package = spec.get("package")
                if isinstance(package, str):
                    effective = package
                has_path = isinstance(spec.get("path"), str)
            if not effective.startswith(_FIRST_PARTY_PREFIX):
                continue
            if has_path:
                deps.add(effective)
    return deps


#: DFS recursion-stack colours for cycle detection.
_WHITE, _GREY, _BLACK = 0, 1, 2


def _find_cycle(edges: dict[str, set[str]]) -> list[str] | None:
    """Return one dependency cycle as a node path, or ``None`` if the graph is acyclic.

    Iterative DFS with a recursion-stack colouring (white/grey/black). When a
    grey node is re-entered the grey stack slice is the cycle, rendered closed
    (``a -> b -> a``) so the diagnostic names every crate on the loop.
    """
    colour: dict[str, int] = dict.fromkeys(edges, _WHITE)

    for root in sorted(edges):
        if colour[root] != _WHITE:
            continue
        # (node, its remaining sorted out-edges) frames; ``stack_nodes`` mirrors
        # the grey recursion stack so a back-edge yields the exact cycle path.
        stack: list[tuple[str, list[str]]] = [(root, sorted(edges.get(root, ())))]
        stack_nodes: list[str] = [root]
        colour[root] = _GREY
        while stack:
            node, pending = stack[-1]
            if not pending:
                colour[node] = _BLACK
                stack.pop()
                stack_nodes.pop()
                continue
            nxt = pending.pop(0)
            state = colour.get(nxt, _WHITE)
            if state == _GREY:
                start = stack_nodes.index(nxt)
                return [*stack_nodes[start:], nxt]
            if state == _WHITE:
                colour[nxt] = _GREY
                stack.append((nxt, sorted(edges.get(nxt, ()))))
                stack_nodes.append(nxt)
    return None


def check_crate_layering(
    crates_dir: Path = PROJECT_ROOT / "crates",
) -> CrateLayeringReport:
    """Run the crate-layering gate over every ``crates/*/Cargo.toml``.

    Args:
        crates_dir: The workspace crate root (defaults to ``<repo>/crates``).

    Returns:
        A :class:`CrateLayeringReport`; ``ok`` is false when the kernel is
        impure, the first-party crate graph has a cycle, or any manifest is
        missing / malformed.
    """
    report = CrateLayeringReport()

    if not crates_dir.is_dir():
        report.errors.append(f"crates directory not found: {crates_dir}")
        return report

    manifests = sorted(crates_dir.glob("*/Cargo.toml"))
    if not manifests:
        report.errors.append(f"no crates/*/Cargo.toml under {crates_dir}")
        return report

    # name -> first-party deps. Keyed by the manifest's declared [package] name so
    # the edge graph speaks crate identity, not directory layout.
    names_seen: set[str] = set()
    for manifest_path in manifests:
        try:
            manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        except (OSError, tomllib.TOMLDecodeError) as exc:
            report.errors.append(f"{manifest_path}: cannot parse Cargo.toml: {exc}")
            continue
        name = _crate_name(manifest)
        if name is None:
            report.errors.append(f"{manifest_path}: no [package] name")
            continue
        if name in names_seen:
            report.errors.append(f"duplicate crate name {name!r} in {manifest_path}")
            continue
        names_seen.add(name)
        report.edges[name] = _first_party_deps(manifest)

    # Kernel purity (Principle 16 / #820): the narrow waist owns no first-party edge.
    kernel_deps = report.edges.get(KERNEL_CRATE)
    if kernel_deps is None:
        report.errors.append(
            f"kernel crate {KERNEL_CRATE!r} not found under {crates_dir}"
        )
    elif kernel_deps:
        report.errors.append(
            f"{KERNEL_CRATE} (the RDF-1.2 narrow waist) must have ZERO first-party "
            f"dependencies, but depends on "
            f"{', '.join(sorted(kernel_deps))} — slice/domain semantics must layer "
            f"ABOVE the kernel, never inside it (#820 S0 kernel purity)"
        )

    # Every referenced first-party dep must resolve to a known crate (a dangling
    # path edge is a malformed layering graph), and the graph must be acyclic.
    for crate, deps in sorted(report.edges.items()):
        for dep in sorted(deps):
            if dep not in report.edges:
                report.errors.append(
                    f"{crate}: first-party dependency {dep!r} is not a crates/* member"
                )

    cycle = _find_cycle(report.edges)
    if cycle is not None:
        report.errors.append(
            "first-party crate dependency cycle: " + " -> ".join(cycle)
        )

    return report


def findings_to_result(report: CrateLayeringReport) -> ValidationResult:
    """Collapse a crate-layering report into a :class:`ValidationResult`."""
    result = ValidationResult()
    result.errors.extend(report.errors)
    result.warnings.extend(report.warnings)
    return result


def to_diagnostics_report(
    report: CrateLayeringReport,
    *,
    tool: str = "crate-layering",
) -> diagnostics.DiagnosticsReport:
    """Project a crate-layering report into the canonical diagnostics report (#654)."""
    items = [
        diagnostics.finding(
            severity="error",
            code="crate-layering.violation",
            message=message,
            tool=tool,
        )
        for message in report.errors
    ]
    items.extend(
        diagnostics.finding(
            severity="warning",
            code="crate-layering.observation",
            message=message,
            tool=tool,
        )
        for message in report.warnings
    )
    return diagnostics.report_from_findings(tool=tool, findings=items)
