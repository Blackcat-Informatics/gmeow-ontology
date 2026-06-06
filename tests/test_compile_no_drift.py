"""No-drift gate: the committed projection artifacts match a fresh compile.

This is the standing regression that replaces the bespoke cross-layer linters —
if anyone hand-edits a generated artifact (or changes a predicate's range without
recompiling), the committed files diverge from the DSL and this test fails.
"""

from __future__ import annotations

import pytest

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.mapping_compile import compile_all, emit_edoal, emit_fno
from gmeow_tools.mapping_dsl import load_dsl

_PROFILES = ("schema-org", "foaf", "vcard", "geosparql", "ical", "owl-time")


def test_committed_artifacts_match_dsl() -> None:
    report = compile_all(check=True)
    assert report.drifted == [], (
        "committed artifacts are stale — run `gmeow compile-mappings`:\n  "
        + "\n  ".join(report.drifted)
    )


@pytest.mark.parametrize("profile", _PROFILES)
def test_edoal_serialization_is_deterministic(profile: str) -> None:
    """Issue #36: two compiles must emit byte-identical EDOAL. Emitting twice with
    fresh ``BNode()`` ids (the old behaviour) reshuffled cell order; the
    content-addressed blank-node ids (``_stable_bnode``) make it stable."""
    dsl = load_dsl()
    first = emit_edoal(dsl, profile).serialize(format="turtle")
    second = emit_edoal(dsl, profile).serialize(format="turtle")
    assert first == second, f"{profile} EDOAL serialization is non-deterministic"


def test_fno_serialization_is_deterministic() -> None:
    """Issue #36: the FnO transform catalog must serialize byte-identically too."""
    dsl = load_dsl()
    onto = load_merged_graph(include_imports=False)
    first = emit_fno(dsl, onto).serialize(format="turtle")
    second = emit_fno(dsl, onto).serialize(format="turtle")
    assert first == second, "FnO serialization is non-deterministic"
