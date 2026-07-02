"""Tests for the RDF-1.2-first statement-metadata pipeline.

The statement compiler is the native Rust `stage-statements` pipeline stage.
Compiler behavior is covered in the Rust test framework; pytest only
keeps Python wrapper/oracle scheduling checks here. The Apache Jena oracle
(no-drift / isomorphism cross-check) runs through a repo-local script so Make/CI
can schedule Docker outside pytest, in the non-required `classic-cross-check`
lane.
"""

from __future__ import annotations

from pathlib import Path

import purrdf
import pytest
from purrdf.compat.rdflib import Graph
from purrdf.compat.rdflib.compare import isomorphic

from gmeow_tools.config import (
    PROJECT_ROOT,
    STATEMENT_OWL_FILE,
    STATEMENT_RDF12_FILE,
)
from oracles import statements_docker_check
from rdf12 import project_owl_to_rdf12
from gmeow_tools.runner import ToolUnavailableError

# --------------------------------------------------------------------------- #
# Pure-Python: the no-preview-language gate
# --------------------------------------------------------------------------- #


def test_no_preview_language_remains() -> None:
    forbidden = (
        "preview",
        "experimental",
        "may change",
        "still finalizing",
        "canonical source of truth is the owl",
    )
    targets = [
        PROJECT_ROOT / "validations/classic-cross-check/rdf12.py",
        PROJECT_ROOT / "queries/codecs/rdf12-project.rq",
        PROJECT_ROOT / "queries/codecs/rdf12-to-owl.rq",
        PROJECT_ROOT / "README.md",
        PROJECT_ROOT / "docs/RATIONALE.md",
    ]
    offenders: list[str] = []
    for path in targets:
        text = path.read_text(encoding="utf-8").lower()
        for token in forbidden:
            if token in text:
                offenders.append(f"{path.name}: '{token}'")
    assert offenders == [], (
        "RDF 1.2 is canonical, not a preview — remove: " + "; ".join(offenders)
    )


# --------------------------------------------------------------------------- #
# Native lead codec: the RDF 1.2 writer runs with no Jena / no Docker
# --------------------------------------------------------------------------- #


def test_native_statement_codec_round_trips_without_jena() -> None:
    """The native gmeow-rdf lead codec projects + normalizes losslessly, no Docker.

    Proves the inversion: the committed OWL downcast projects to the RDF 1.2
    triple-term lead form and normalizes back isomorphic to the authored OWL — all
    via the native Rust codec, with neither Apache Jena nor Docker on the path.
    """
    owl_text = STATEMENT_OWL_FILE.read_text(encoding="utf-8")
    rdf12 = purrdf.project_statements_rdf12(owl_text)
    assert "<<(" in rdf12 and "#reifies>" in rdf12, "expected RDF 1.2 triple terms"

    owl_back = purrdf.normalize_rdf12_to_owl(rdf12)
    authored = Graph().parse(data=owl_text, format="turtle")
    round_trip = Graph().parse(data=owl_back, format="turtle")
    assert isomorphic(authored, round_trip), "native RDF 1.2 round-trip is lossy"


# --------------------------------------------------------------------------- #
# The Jena ORACLE codec stays hard-fail (no degraded mode) — classic-cross-check
# only. This is the cross-check engine, not the lead writer.
# --------------------------------------------------------------------------- #


def test_jena_oracle_codec_hard_fails_without_jena(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    import gmeow_tools.runner as runner

    monkeypatch.setattr(runner, "docker_available", lambda **_kw: True)
    monkeypatch.setattr(runner, "image_available", lambda _image, **_kw: False)

    with pytest.raises(ToolUnavailableError, match="Docker image not present locally"):
        project_owl_to_rdf12(STATEMENT_OWL_FILE, tmp_path / "gmeow.rdf12.ttl")


# --------------------------------------------------------------------------- #
# Jena-gated orchestration — mocked here, live in `scripts/statements_docker_check.py`
# --------------------------------------------------------------------------- #


def test_statement_docker_check_reports_drift(monkeypatch: pytest.MonkeyPatch) -> None:
    """The Docker lane fails on stale statement artifacts."""
    import gmeow_native.pipeline as _pipeline

    monkeypatch.setattr(
        _pipeline,
        "run_pipeline",
        lambda *_a, **_kw: {
            "drifted": ["generated/statements/gmeow.rdf12.ttl"],
            "orphans": [],
        },
    )

    with pytest.raises(AssertionError, match="stale"):
        statements_docker_check.assert_committed_artifacts_match_dsl()


def test_statement_docker_check_lossless_negative_control(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The Jena oracle lane keeps the dropped-annotation negative control."""
    monkeypatch.setattr(
        statements_docker_check,
        "assert_lossless_jena",
        lambda _owl, _path: ["OWL form has, RDF 1.2 lost: confidence"],
    )

    statements_docker_check.assert_lossless_gate_detects_a_dropped_annotation()


def test_statement_docker_check_run_all_order(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The CLI lane keeps the intended Jena/ROBOT checks in order."""
    calls: list[str] = []
    monkeypatch.setattr(
        statements_docker_check,
        "assert_committed_artifacts_match_dsl",
        lambda: calls.append("drift"),
    )
    monkeypatch.setattr(
        statements_docker_check,
        "assert_committed_rdf12_round_trips_to_owl",
        lambda: calls.append("roundtrip"),
    )
    monkeypatch.setattr(
        statements_docker_check,
        "assert_lossless_gate_detects_a_dropped_annotation",
        lambda: calls.append("negative"),
    )
    monkeypatch.setattr(
        statements_docker_check,
        "assert_committed_rdf12_uses_triple_term_syntax",
        lambda: calls.append("syntax"),
    )
    monkeypatch.setattr(
        statements_docker_check,
        "assert_reason_consumes_generated_owl_downcast",
        lambda: calls.append("reason"),
    )

    completed = statements_docker_check.run_all()

    assert calls == ["drift", "roundtrip", "negative", "syntax", "reason"]
    assert completed == [
        "statement artifact drift",
        "RDF 1.2 round-trip",
        "lossless gate negative control",
        "RDF 1.2 triple-term syntax",
        "OWL downcast reasoning",
    ]


def test_statement_rdf12_committed_under_repo_not_dist() -> None:
    """The lead artifact is committed (so --check has a target), not in dist/."""
    assert STATEMENT_RDF12_FILE.is_relative_to(
        PROJECT_ROOT / "generated" / "statements"
    )
    assert Path(STATEMENT_RDF12_FILE).exists()
