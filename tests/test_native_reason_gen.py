# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the ``native-reasoning`` generator (issue #665, Task 5).

The generator face of the native Docker-free EL/DL reasoning authority: it
renders 3 committed RDF 1.2 artifacts under ``generated/logic/`` from the single
``gmeow_logic.reason_native`` Rust result. Covers registration, output identity,
and a render → compare round-trip (no drift) using the star-aware pyoxigraph
comparator (the artifacts carry ``<< … >>`` triple terms rdflib cannot parse).
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from tests._required_native import require_gmeow_logic

if TYPE_CHECKING:
    from gmeow_tools.generator import Generator


def _get_native_reason_gen() -> Generator:
    """Return the registered ``native-reasoning`` generator instance."""
    from gmeow_tools.generator import registry
    from gmeow_tools.load_generators import load_all

    load_all()
    return registry()["native-reasoning"]


def test_native_reasoning_generator_registered() -> None:
    """The generator appears in the live registry after loading all generators."""
    from gmeow_tools.generator import registry
    from gmeow_tools.load_generators import load_all

    load_all()
    reg = registry()
    assert "native-reasoning" in reg, (
        f"'native-reasoning' not in registry: {sorted(reg)}"
    )


def test_native_reasoning_generator_name_and_outputs() -> None:
    """name == 'native-reasoning' and it owns the 3 generated/logic/ artifacts."""
    from gmeow_tools.native_reason_gen import (
        NATIVE_CLOSURE_FILE,
        NATIVE_EXPLANATIONS_FILE,
        NATIVE_LEDGER_FILE,
    )

    # ``@register`` instantiates the class on import, so the live registry holds
    # the instance under its declared name.
    gen = _get_native_reason_gen()
    assert gen.name == "native-reasoning"
    outputs = list(gen.outputs)
    assert len(outputs) == 3, f"Expected 3 outputs, got {len(outputs)}: {outputs}"
    assert set(outputs) == {
        NATIVE_CLOSURE_FILE,
        NATIVE_EXPLANATIONS_FILE,
        NATIVE_LEDGER_FILE,
    }
    # All 3 land under generated/logic/.
    for out in outputs:
        assert out.parent.name == "logic"
        assert out.parent.parent.name == "generated"


def test_native_reasoning_render_compare_round_trip(tmp_path: Path) -> None:
    """Rendering into a temp staging tree and comparing the committed artifacts
    produces no drift (RDF 1.2 graph-isomorphism, star-aware)."""
    require_gmeow_logic()
    from gmeow_tools.config import GTS_SNAPSHOT_FILE
    from gmeow_tools.generator import _staging_rel

    if not GTS_SNAPSHOT_FILE.exists():
        pytest.skip("GTS snapshot not present in this checkout")

    gen = _get_native_reason_gen()
    gen.render(tmp_path)

    drifts: list[str] = []
    for committed in gen.outputs:
        if not committed.exists():
            pytest.skip(f"committed artifact {committed} not in this checkout")
        fresh = tmp_path / _staging_rel(committed)
        assert fresh.exists(), f"render did not produce {fresh}"
        drifts.extend(gen.compare(fresh, committed))

    assert not drifts, "Round-trip produced drift:\n" + "\n".join(drifts)
