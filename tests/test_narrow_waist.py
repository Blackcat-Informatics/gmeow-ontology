# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The narrow-waist seal (#267, #12): GTS is the only exit for data.

Two complementary enforcement layers, so the waist cannot silently regress:

* **Behavioral seal** — with every canonical-source reader monkeypatched to
  raise, all four data exporters must still render, proving they need
  nothing but ``generated/dist/gmeow.gts``.
* **Static seal** — the exporter modules must not import rdflib or
  pyoxigraph at all (``metadata.py`` keeps rdflib strictly as the OUTPUT
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
    gts_vectors_gen,
    lpg,
    mapping_compile,
    matrix,
    metadata,
    schema_compile,
    statement_compile,
)
from gmeow_tools.config import PROJECT_ROOT
from gmeow_tools.generator import regenerate_order, registry

_SRC = PROJECT_ROOT / "src" / "gmeow_tools"

#: The four data exporters: module name → modules that must NOT be imported.
_SEALED: dict[str, frozenset[str]] = {
    "export.py": frozenset({"rdflib", "pyoxigraph"}),
    "schema_compile.py": frozenset({"rdflib", "pyoxigraph"}),
    "lpg.py": frozenset({"rdflib", "pyoxigraph"}),
    # rdflib allowed as the output serializer; pyoxigraph not at all
    "metadata.py": frozenset({"pyoxigraph"}),
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
    """The exporter modules import neither rdflib nor pyoxigraph."""
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
    """All four generators render with every source reader raising.

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
    try:  # pyoxigraph is a dev dependency here, but the seal must not
        import pyoxigraph  # noqa: F401  # depend on its presence to hold
    except ImportError:
        pass
    else:
        monkeypatch.setattr("pyoxigraph.Store", boom)
        monkeypatch.setattr("pyoxigraph.parse", boom)

    gens = registry()
    for name in ("exports", "metadata", "schemas", "lpg"):
        staging = tmp_path / name
        staging.mkdir()
        gens[name].render(staging)  # must not raise
        rendered = list(staging.rglob("*"))
        assert any(p.is_file() for p in rendered), f"{name} rendered nothing"


def test_registry_orders_gts_before_every_consumer() -> None:
    """Topological order: statements → gts → the four exporters."""
    order = regenerate_order()
    gts_pos = order.index("gts")
    assert order.index("statements") < gts_pos
    for consumer in ("exports", "metadata", "schemas", "lpg"):
        assert gts_pos < order.index(consumer), (
            f"{consumer} ordered before its input producer"
        )
