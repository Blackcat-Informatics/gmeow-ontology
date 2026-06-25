# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The purrdf P0 self-host gate: gmeow's own code must not import ``rdflib``.

gmeow runs on the native ``gmeow_rdf.compat.rdflib`` facade (#834). The ONLY
first-party modules allowed to touch upstream ``rdflib`` are the dev-only
``gmeow_tools.oracles`` lane keepers, which use it as the *independent*
reasoner/engine oracle — and even those gate it behind ``pytest.importorskip``
so the default lanes never need it.

This guard is AST-based (not "uninstall rdflib and import"): rdflib is still present
transitively via ``sssom``/``linkml`` (tracked for native subsumption in #848), so a
"raise on any rdflib import" probe would fire on those external libraries, not on us.
What matters — and what this enforces — is that NO first-party module imports rdflib.
"""

from __future__ import annotations

import ast
from pathlib import Path

#: The classic_cross_check oracle lane — the sanctioned upstream-rdflib consumers.
_ALLOWED = {"oracles/engine_crosscheck.py", "oracles/rl_agreement.py"}

_SRC = Path(__file__).resolve().parent.parent / "src" / "gmeow_tools"


def _rdflib_imports(tree: ast.AST) -> list[str]:
    """Return every ``rdflib`` / ``rdflib.*`` import name found anywhere in ``tree``."""
    hits: list[str] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            hits += [a.name for a in node.names if a.name.split(".")[0] == "rdflib"]
        elif isinstance(node, ast.ImportFrom):
            mod = node.module or ""
            if mod.split(".")[0] == "rdflib":
                hits.append(mod)
    return hits


def test_no_first_party_module_imports_rdflib() -> None:
    """No ``src/gmeow_tools`` module imports ``rdflib`` except the cross-check lane."""
    offenders: dict[str, list[str]] = {}
    for path in sorted(_SRC.rglob("*.py")):
        rel = path.relative_to(_SRC).as_posix()
        if rel in _ALLOWED:
            continue
        imports = _rdflib_imports(ast.parse(path.read_text(encoding="utf-8")))
        if imports:
            offenders[rel] = imports
    assert not offenders, (
        "first-party modules must use gmeow_rdf.compat.rdflib, not upstream rdflib "
        f"(purrdf P0 #834): {offenders}"
    )


def test_keepers_are_the_only_rdflib_consumers() -> None:
    """The allow-list matches reality: exactly the cross-check keepers use rdflib."""
    actual = {
        path.relative_to(_SRC).as_posix()
        for path in _SRC.rglob("*.py")
        if _rdflib_imports(ast.parse(path.read_text(encoding="utf-8")))
    }
    assert actual == _ALLOWED, (
        f"the rdflib keeper allow-list is stale: expected {_ALLOWED}, found {actual}"
    )
