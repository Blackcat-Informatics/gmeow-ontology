"""Tests for the RDF-1.2-first statement-metadata pipeline (issues #28, #29).

The pure-Python tests (DSL parse, reifier minting, the invariants, the OWL emit,
the no-preview-language gate, and the native lead-codec round-trip) run anywhere —
the RDF 1.2 lead writer is the native ``gmeow-rdf`` Rust codec (#667), so the
round-trip needs no Jena and no Docker. The Apache Jena oracle (no-drift /
isomorphism cross-check) runs through a repo-local script so Make/CI can schedule
Docker outside pytest, in the non-required ``classic-cross-check`` lane.
"""

from __future__ import annotations

from pathlib import Path

import gmeow_rdf
import pytest
from rdflib import RDF, Graph, URIRef
from rdflib.compare import isomorphic
from rdflib.namespace import OWL

from gmeow_tools import statements_docker_check
from gmeow_tools.config import (
    PREFIXES,
    PROJECT_ROOT,
    STATEMENT_OWL_FILE,
    STATEMENT_RDF12_FILE,
)
from gmeow_tools.rdf12 import project_owl_to_rdf12
from gmeow_tools.runner import ToolUnavailableError
from gmeow_tools.statement_compile import emit_owl
from gmeow_tools.statement_dsl import (
    QuotedTriple,
    load_statement_dsl,
    mint_reifier,
)

GM = PREFIXES["gmeow"]


# --------------------------------------------------------------------------- #
# Pure-Python: parsing, minting, emit, invariants
# --------------------------------------------------------------------------- #


def test_dsl_parses_and_is_sorted() -> None:
    dsl = load_statement_dsl()
    assert len(dsl.cells) >= 3
    assert list(dsl.cells) == sorted(dsl.cells, key=lambda c: str(c.iri))
    for cell in dsl.cells:
        assert cell.annotations  # every worked cell carries metadata
        assert list(cell.annotations) == sorted(
            cell.annotations, key=lambda a: (str(a.prop), a.value.n3())
        )


def test_reifier_minting_is_deterministic_and_content_addressed() -> None:
    triple = QuotedTriple(
        subject=URIRef("https://example.org/s"),
        predicate=URIRef(GM + "knowsLanguage"),
        obj=URIRef("https://example.org/o"),
    )
    assert mint_reifier(triple) == mint_reifier(triple)  # stable
    assert str(mint_reifier(triple)).startswith(GM + "reifier/")
    # The cell without an authored reifier got a minted, content-addressed one.
    minted = [
        c
        for c in load_statement_dsl().cells
        if str(c.reifier).startswith(GM + "reifier/")
    ]
    assert minted, "expected at least one minted reifier in the worked examples"


def test_duplicate_annotation_value_is_rejected(tmp_path: Path) -> None:
    """Two annValues on one annotation node is a malformed cell, not a silent pick."""
    from gmeow_tools.mapping_dsl import CompileError

    ttl = (
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n"
        "@prefix ex: <https://blackcatinformatics.ca/gmeow/examples/> .\n"
        "ex:c a gmeow:StatementMetadata ;\n"
        "    gmeow:qSubject ex:s ; gmeow:qPredicate gmeow:knowsLanguage ;\n"
        "    gmeow:qObject ex:o ;\n"
        "    gmeow:annotation [ gmeow:annProperty gmeow:confidence ;\n"
        "        gmeow:annValue 0.5 ; gmeow:annValue 0.6 ] .\n"
    )
    (tmp_path / "dup.ttl").write_text(ttl, encoding="utf-8")
    with pytest.raises(
        CompileError,
        match=r"statement DSL SHACL violations",
    ):
        load_statement_dsl(src=tmp_path)


def test_emit_owl_produces_axiom_annotation_form() -> None:
    dsl = load_statement_dsl()
    owl = emit_owl(dsl)
    axioms = set(owl.subjects(RDF.type, OWL.Axiom))
    assert len(axioms) == len(dsl.cells)
    for cell in dsl.cells:
        ax = cell.reifier
        assert (ax, OWL.annotatedSource, cell.triple.subject) in owl
        assert (ax, OWL.annotatedProperty, cell.triple.predicate) in owl
        assert (ax, OWL.annotatedTarget, cell.triple.obj) in owl
        assert (cell.triple.subject, cell.triple.predicate, cell.triple.obj) in owl
        for ann in cell.annotations:
            assert (ax, ann.prop, ann.value) in owl


# The statement-invariant checks (annotation-property soundness, confidence range,
# OWL 2 DL datatypes, predicate/term groundedness, no-preferred-rank) are now native
# Rust (gmeow_validate.check_statement_invariants); their unit tests live in
# crates/validate/src/statement.rs (issue #630).


# --------------------------------------------------------------------------- #
# Pure-Python: the no-preview-language gate (#28.4, #29.4)
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
        PROJECT_ROOT / "src/gmeow_tools/rdf12.py",
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
# Native lead codec: the RDF 1.2 writer runs with no Jena / no Docker (#667)
# --------------------------------------------------------------------------- #


def test_native_statement_codec_round_trips_without_jena() -> None:
    """The native gmeow-rdf lead codec projects + normalizes losslessly, no Docker.

    Proves the inversion (#667): the committed OWL downcast projects to the RDF 1.2
    triple-term lead form and normalizes back isomorphic to the authored OWL — all
    via the native Rust codec, with neither Apache Jena nor Docker on the path.
    """
    owl_text = STATEMENT_OWL_FILE.read_text(encoding="utf-8")
    rdf12 = gmeow_rdf.project_statements_rdf12(owl_text)
    assert "<<(" in rdf12 and "#reifies>" in rdf12, "expected RDF 1.2 triple terms"

    owl_back = gmeow_rdf.normalize_rdf12_to_owl(rdf12)
    authored = Graph().parse(data=owl_text, format="turtle")
    round_trip = Graph().parse(data=owl_back, format="turtle")
    assert isomorphic(authored, round_trip), "native RDF 1.2 round-trip is lossy"


# --------------------------------------------------------------------------- #
# The Jena ORACLE codec stays hard-fail (no degraded mode) — classic-cross-check
# only. After #667 this is the cross-check engine, not the lead writer.
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
    """The Docker lane fails on stale or orphaned statement artifacts."""

    class Report:
        def __init__(self) -> None:
            self.drifted = ["generated/statements/gmeow.rdf12.ttl"]
            self.orphans: list[str] = []

    monkeypatch.setattr(statements_docker_check, "run", lambda *_a, **_kw: Report())

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
