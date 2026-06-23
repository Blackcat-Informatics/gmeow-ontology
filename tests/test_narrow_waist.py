# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The narrow-waist seal (#267, #12): GTS is the only exit for data.

The **static seal** — the surviving exporter modules must not import rdflib
or gmeow_rdf at all, and must not touch the canonical-source loaders. The
build authority itself is now the Rust pipeline (#861); the data-flow
ordering invariant is enforced there (``crates/pipeline``), not by a Python
registry.
"""

from __future__ import annotations

import ast

from gmeow_tools.config import PROJECT_ROOT

_SRC = PROJECT_ROOT / "src" / "gmeow_tools"

#: The data exporters: module name → modules that must NOT be imported.
_SEALED: dict[str, frozenset[str]] = {
    "export.py": frozenset({"rdflib", "gmeow_rdf"}),
    "schema_compile.py": frozenset({"rdflib", "gmeow_rdf"}),
}

#: Canonical-source readers no exporter may touch (metadata included).
_FORBIDDEN_LOADERS: frozenset[str] = frozenset(
    {
        "load_merged_graph",
        "shared_merged_graph",
        "load_mappings",
        "load_self_description",
        "load_tag_map",
    }
)


def _imported_modules(tree: ast.AST) -> set[str]:
    out: set[str] = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            out.update(alias.name.split(".")[0] for alias in node.names)
        elif isinstance(node, ast.ImportFrom) and node.module:
            out.add(node.module.split(".")[0])
    return out


def _referenced_names(tree: ast.AST) -> set[str]:
    """Every from-import, bare name, and attribute referenced in the module.

    Catching attribute access too means ``graph_mod.load_merged_graph()``
    cannot dodge the seal by avoiding a from-import (Gemini's review find).
    """
    out: set[str] = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.ImportFrom):
            out.update(alias.name for alias in node.names)
        elif isinstance(node, ast.Name):
            out.add(node.id)
        elif isinstance(node, ast.Attribute):
            out.add(node.attr)
    return out


def test_static_seal_no_rdf_parsers_in_exporters() -> None:
    """The exporter modules import neither rdflib nor gmeow_rdf."""
    for module, banned in _SEALED.items():
        tree = ast.parse((_SRC / module).read_text(encoding="utf-8"))
        offending = _imported_modules(tree) & banned
        assert not offending, f"{module} imports {sorted(offending)}"


def test_static_seal_no_canonical_source_loaders() -> None:
    """No exporter reads canonical sources — the snapshot is the only input."""
    for module in _SEALED:
        tree = ast.parse((_SRC / module).read_text(encoding="utf-8"))
        offending = _referenced_names(tree) & _FORBIDDEN_LOADERS
        assert not offending, f"{module} imports {sorted(offending)}"


_CLI = PROJECT_ROOT / "src" / "gmeow_tools" / "cli.py"
_GTS_APP = "gts_app"
_GTS_SUBCOMMANDS = frozenset(
    {
        "gts_info",
        "gts_verify",
        "gts_extract_key",
        "gts_to_nq",
        "gts_from_rdf",
        "gts_to_sqlite",
        "gts_to_duckdb",
    }
)


def test_public_cli_does_not_reimplement_gts_subcommands() -> None:
    """Static proof that the public ``gmeow`` CLI shells out to ``gts``.

    The GTS engine commands used to be reimplemented inside ``gmeow`` via a
    local ``gts_app`` Typer sub-application. After #617 they are delegated to
    the external ``gts`` binary, so the public CLI must contain no trace of
    the old sub-command functions or decorators.
    """
    tree = ast.parse(_CLI.read_text(encoding="utf-8"))

    assigned: set[str] = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name):
                    assigned.add(target.id)
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            assigned.add(node.target.id)
    assert _GTS_APP not in assigned, (
        f"public CLI still assigns to the legacy {_GTS_APP!r} Typer app"
    )

    defined = {
        node.name
        for node in ast.walk(tree)
        if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef)
    }
    offenders = defined & _GTS_SUBCOMMANDS
    assert not offenders, (
        f"public CLI still defines legacy GTS subcommand function(s): "
        f"{sorted(offenders)}"
    )

    for node in ast.walk(tree):
        if not isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
            continue
        for decorator in node.decorator_list:
            func = decorator.func if isinstance(decorator, ast.Call) else decorator
            if (
                isinstance(func, ast.Attribute)
                and func.attr == "command"
                and isinstance(func.value, ast.Name)
                and func.value.id == _GTS_APP
            ):
                raise AssertionError(
                    f"public CLI still uses @{_GTS_APP}.command on {node.name!r}"
                )
