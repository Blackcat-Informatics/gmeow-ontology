# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The narrow-waist seal (#267, #12): GTS is the only exit for data.

Two complementary enforcement layers, so the waist cannot silently regress:

* **Behavioral seal** — with every canonical-source reader monkeypatched to
  raise, all five data exporters must still render, proving they need
  nothing but ``generated/dist/gmeow.gts``.
* **Static seal** — the exporter modules must not import rdflib or
  gmeow_rdf at all (``metadata.py`` keeps rdflib strictly as the OUTPUT
  serializer for its freshly built description graphs — the one agreed
  allowance — and must not touch the canonical-source loaders).

Plus the ordering invariant: the registry sequences ``gts`` before every
consumer, derived from declared inputs/outputs — never hand-maintained.
"""

from __future__ import annotations

import ast
from pathlib import Path

import pytest

from gmeow_tools import (  # noqa: F401  (@register side effects)
    apache,
    export,
    gts_gen,
    lpg,
    mapping_compile,
    matrix,
    metadata,
    parquet_gen,
    schema_compile,
    statement_compile,
)
from gmeow_tools.config import PROJECT_ROOT
from gmeow_tools.generator import regenerate_order, registry

_SRC = PROJECT_ROOT / "src" / "gmeow_tools"

#: The data exporters: module name → modules that must NOT be imported.
_SEALED: dict[str, frozenset[str]] = {
    "export.py": frozenset({"rdflib", "gmeow_rdf"}),
    "schema_compile.py": frozenset({"rdflib", "gmeow_rdf"}),
    "lpg.py": frozenset({"rdflib", "gmeow_rdf"}),
    "parquet_gen.py": frozenset({"rdflib", "gmeow_rdf"}),
    # rdflib allowed as the output serializer; gmeow_rdf not at all
    "metadata.py": frozenset({"gmeow_rdf"}),
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


def test_behavioral_seal_exporters_render_from_snapshot_alone(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """All five generators render with every source reader raising.

    The strongest form of the claim: not merely "they don't import the
    loaders" but "they cannot be USING them" — every canonical-source
    entry point detonates on touch, and the renders still succeed from
    ``generated/dist/gmeow.gts``.
    """
    import gmeow_tools.graph as graph_mod
    import gmeow_tools.mappings as mappings_mod
    import gmeow_tools.self_desc as self_desc_mod

    def boom(*_args: object, **_kwargs: object) -> object:
        msg = "narrow-waist violation: a canonical-source reader was called"
        raise AssertionError(msg)

    monkeypatch.setattr(graph_mod, "load_merged_graph", boom)
    monkeypatch.setattr(graph_mod, "shared_merged_graph", boom)
    monkeypatch.setattr(mappings_mod, "load_mappings", boom)
    monkeypatch.setattr(self_desc_mod, "load_self_description", boom)
    # gmeow_rdf (the native oxigraph binding) is always present, but the seal must
    # prove the narrow-waist generators never touch the engine at render time.
    import gmeow_rdf  # noqa: F401

    monkeypatch.setattr("gmeow_rdf.Store", boom)
    monkeypatch.setattr("gmeow_rdf.parse", boom)

    gens = registry()
    for name in ("exports", "metadata", "schemas", "lpg", "parquet"):
        staging = tmp_path / name
        staging.mkdir()
        gens[name].render(staging)  # must not raise
        rendered = list(staging.rglob("*"))
        assert any(p.is_file() for p in rendered), f"{name} rendered nothing"


def test_registry_orders_gts_before_every_consumer() -> None:
    """Topological order: statements → gts → the five exporters."""
    order = regenerate_order()
    gts_pos = order.index("gts")
    assert order.index("statements") < gts_pos
    for consumer in ("exports", "metadata", "schemas", "lpg", "parquet"):
        assert gts_pos < order.index(consumer), (
            f"{consumer} ordered before its input producer"
        )


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
