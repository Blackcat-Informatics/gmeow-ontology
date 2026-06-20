# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Runtime import discipline (#667): the public CLI imports only declared deps.

The ``gmeow`` vs ``gmeow-dev`` razor: ``gmeow`` must run from the published wheel
without the repo, so every third-party module reachable from its entry points
(``gmeow`` → ``gmeow_tools.cli``, ``gmeow-music`` → ``gmeow_tools.ext.music.cli``)
must be a *declared* runtime dependency — a core dep, an optional extra, or one of
the project's own native PyO3 extensions. A dev/generator-only library (``linkml``,
``duckdb``, ``markdown``, ``owlrl``, ``pytest`` …) reaching the runtime path is a
razor violation: it would either bloat the install or ``ImportError`` in the wild.

This guard walks the first-party import graph from the entry points (following
lazy and ``TYPE_CHECKING`` imports too — a lazy import still needs the dependency
installed when its command runs) and asserts the third-party frontier stays within
the declared runtime surface. It is the regression backstop that lets the runtime
``[project.dependencies]`` stay slim: generator-only deps live in ``gmeow-dev``.

Pure-Python and offline — runs in the required CI ``python`` job.
"""

from __future__ import annotations

import ast
import sys
import tomllib
from pathlib import Path

from gmeow_tools.config import PROJECT_ROOT

SRC = PROJECT_ROOT / "src"
PYPROJECT = PROJECT_ROOT / "pyproject.toml"
PACKAGE = "gmeow_tools"

#: The two public console-script entry points (pyproject ``[project.scripts]``).
ENTRY_POINTS = ("gmeow_tools.cli", "gmeow_tools.ext.music.cli")

#: The project's own native extensions, built from ``crates/*`` via maturin — not
#: third-party PyPI distributions, so they are first-party for this guard.
NATIVE_EXTENSIONS = frozenset(
    {
        "gmeow_diagnostics",
        "gmeow_logic",
        "gmeow_rdf",
        "gmeow_shacl",
        "gmeow_validate",
    }
)

#: Distribution name → import name, where they differ.
_DIST_TO_IMPORT = {
    "pyyaml": "yaml",
    "gmeow-gts": "gts",
}


def _module_path(module: str) -> Path | None:
    rel = module.replace(".", "/")
    for candidate in (SRC / f"{rel}.py", SRC / rel / "__init__.py"):
        if candidate.is_file():
            return candidate
    return None


def _submodules(module: str, names: list[str]) -> list[str]:
    """``from pkg import x`` — keep the ``x`` that name a submodule, not a symbol."""
    return [f"{module}.{name}" for name in names if _module_path(f"{module}.{name}")]


def _reachable_third_party() -> set[str]:
    """Top-level third-party module names reachable from the entry points."""
    seen: set[str] = set()
    third_party: set[str] = set()
    queue: list[str] = list(ENTRY_POINTS)
    while queue:
        module = queue.pop()
        if module in seen:
            continue
        seen.add(module)
        path = _module_path(module)
        if path is None:
            continue
        package = module.rsplit(".", 1)[0] if "." in module else module
        tree = ast.parse(path.read_text(encoding="utf-8"))
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    top = alias.name.split(".")[0]
                    if top == PACKAGE:
                        queue.append(alias.name)
                    else:
                        third_party.add(top)
            elif isinstance(node, ast.ImportFrom):
                if node.level:  # relative import: resolve against the package
                    base = package
                    for _ in range(node.level - 1):
                        base = base.rsplit(".", 1)[0]
                    target = f"{base}.{node.module}" if node.module else base
                    queue.append(target)
                    queue.extend(_submodules(target, [a.name for a in node.names]))
                elif node.module:
                    top = node.module.split(".")[0]
                    if top == PACKAGE:
                        queue.append(node.module)
                        queue.extend(
                            _submodules(node.module, [a.name for a in node.names])
                        )
                    else:
                        third_party.add(top)
    return {name for name in third_party if name}


def _declared_runtime_imports() -> set[str]:
    """Import names of every declared runtime dependency + optional extra."""
    data = tomllib.loads(PYPROJECT.read_text(encoding="utf-8"))
    project = data["project"]
    specs: list[str] = list(project.get("dependencies", []))
    for extra in project.get("optional-dependencies", {}).values():
        specs.extend(extra)
    imports: set[str] = set()
    for spec in specs:
        dist = _dist_name(spec)
        imports.add(_DIST_TO_IMPORT.get(dist, dist.replace("-", "_")))
    return imports


def _dist_name(spec: str) -> str:
    """The distribution name from a PEP 508 requirement string, normalized."""
    head = spec.split(";", 1)[0]
    for sep in ("[", "<", ">", "=", "!", "~", " ", "("):
        head = head.split(sep, 1)[0]
    return head.strip().lower()


def test_runtime_cli_imports_only_declared_dependencies() -> None:
    """Every third-party module the public CLI can reach is a declared dependency.

    Catches a dev/generator-only library (e.g. ``linkml``, ``duckdb``, ``owlrl``)
    drifting onto the runtime path, which would break ``pip install gmeow`` usage.
    """
    allowed = (
        _declared_runtime_imports() | NATIVE_EXTENSIONS | set(sys.stdlib_module_names)
    )
    reachable = _reachable_third_party()
    leaks = sorted(reachable - allowed)
    assert not leaks, (
        "public CLI reaches undeclared third-party module(s) — declare them as a "
        f"gmeow runtime dependency or keep the importer in gmeow-dev: {leaks}"
    )


def test_generator_only_deps_are_absent_from_runtime() -> None:
    """The known generator-only libraries stay off the runtime import frontier.

    A positive pin for the #667 slimming: these power docs/schema/parquet/evals
    generation (gmeow-dev surface) and must never be pulled in by a ``gmeow``
    command. If one reappears here, a generator was wired into the runtime CLI.
    """
    generator_only = {"linkml", "duckdb", "markdown", "jsonschema", "yaml"}
    reachable = _reachable_third_party()
    intruders = sorted(generator_only & reachable)
    assert not intruders, (
        f"generator-only dependency reached the runtime CLI: {intruders}"
    )
