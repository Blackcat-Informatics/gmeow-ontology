"""No-drift gate: the committed projection artifacts match a fresh compile.

This is the standing regression that replaces the bespoke cross-layer linters —
if anyone hand-edits a generated artifact (or changes a predicate's range without
recompiling), the committed files diverge from the DSL and this test fails.
"""

from __future__ import annotations

from gmeow_tools.mapping_compile import compile_all


def test_committed_artifacts_match_dsl() -> None:
    report = compile_all(check=True)
    assert report.drifted == [], (
        "committed artifacts are stale — run `gmeow compile-mappings`:\n  "
        + "\n  ".join(report.drifted)
    )
