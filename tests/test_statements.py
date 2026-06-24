"""Tests for the RDF-1.2-first statement-metadata pipeline (issues #28, #29).

The statement compiler is the native Rust `stage-statements` pipeline stage
(#935). These tests inspect its output directly: deterministic DSL compilation,
content-addressed reifier minting, OWL axiom-annotation shape, and the native
lead-codec round-trip need no Jena and no Docker. The Apache Jena oracle
(no-drift / isomorphism cross-check) runs through a repo-local script so Make/CI
can schedule Docker outside pytest, in the non-required `classic-cross-check`
lane.
"""

from __future__ import annotations

from hashlib import sha1
from pathlib import Path
from typing import cast

import gmeow_native.pipeline as native_pipeline
import gmeow_rdf
import pytest
from gmeow_rdf.compat.rdflib import RDF, Graph, URIRef
from gmeow_rdf.compat.rdflib.compare import isomorphic
from gmeow_rdf.compat.rdflib.namespace import OWL

from gmeow_tools import statements_docker_check
from gmeow_tools.config import (
    PREFIXES,
    PROJECT_ROOT,
    STATEMENT_OWL_FILE,
    STATEMENT_RDF12_FILE,
)
from gmeow_tools.rdf12 import project_owl_to_rdf12
from gmeow_tools.runner import ToolUnavailableError

GM = PREFIXES["gmeow"]


def _p(local: str) -> URIRef:
    return URIRef(GM + local)


def _native_statement_artifacts(root: Path = PROJECT_ROOT) -> dict[str, str]:
    return cast("dict[str, str]", native_pipeline.compile_statements(str(root)))


def _native_owl_graph(root: Path = PROJECT_ROOT) -> Graph:
    graph = Graph()
    graph.parse(data=_native_statement_artifacts(root)["owl_ttl"], format="turtle")
    return graph


def _statement_source_graph(root: Path = PROJECT_ROOT) -> Graph:
    graph = Graph()
    for path in sorted((root / "dsl" / "statements").glob("*.ttl")):
        graph.parse(path, format="turtle")
    return graph


# --------------------------------------------------------------------------- #
# Native Rust statement compiler: parsing, minting, emit, invariants
# --------------------------------------------------------------------------- #


def test_native_statement_compiler_is_deterministic_and_complete() -> None:
    first = _native_statement_artifacts()
    second = _native_statement_artifacts()
    assert first == second

    source = _statement_source_graph()
    cells = set(source.subjects(RDF.type, _p("StatementMetadata")))
    assert len(cells) >= 3

    owl = Graph().parse(data=first["owl_ttl"], format="turtle")
    axioms = set(owl.subjects(RDF.type, OWL.Axiom))
    assert len(axioms) == len(cells)


def test_reifier_minting_is_deterministic_and_content_addressed() -> None:
    subject = URIRef(GM + "examples/alice")
    predicate = _p("knowsLanguage")
    obj = URIRef(GM + "examples/lang-toki-pona")
    canonical = " ".join(term.n3() for term in (subject, predicate, obj))
    minted = URIRef(GM + "reifier/" + sha1(canonical.encode("utf-8")).hexdigest())

    owl = _native_owl_graph()
    assert str(minted).startswith(GM + "reifier/")
    assert (minted, RDF.type, OWL.Axiom) in owl
    assert (minted, OWL.annotatedSource, subject) in owl
    assert (minted, OWL.annotatedProperty, predicate) in owl
    assert (minted, OWL.annotatedTarget, obj) in owl


def test_duplicate_annotation_value_is_rejected(tmp_path: Path) -> None:
    """Two annValues on one annotation node is a malformed cell, not a silent pick."""
    ttl = (
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n"
        "@prefix ex: <https://blackcatinformatics.ca/gmeow/examples/> .\n"
        "ex:c a gmeow:StatementMetadata ;\n"
        "    gmeow:qSubject ex:s ; gmeow:qPredicate gmeow:knowsLanguage ;\n"
        "    gmeow:qObject ex:o ;\n"
        "    gmeow:annotation [ gmeow:annProperty gmeow:confidence ;\n"
        "        gmeow:annValue 0.5 ; gmeow:annValue 0.6 ] .\n"
    )
    dsl_dir = tmp_path / "dsl" / "statements"
    dsl_dir.mkdir(parents=True)
    (dsl_dir / "dup.ttl").write_text(ttl, encoding="utf-8")
    with pytest.raises(
        ValueError,
        match=r"annValue|expected one",
    ):
        _native_statement_artifacts(tmp_path)


def test_native_owl_output_uses_axiom_annotation_form() -> None:
    source = _statement_source_graph()
    owl = _native_owl_graph()
    for cell in source.subjects(RDF.type, _p("StatementMetadata")):
        ax = source.value(cell, _p("reifier"))
        if ax is None:
            continue
        subject = source.value(cell, _p("qSubject"))
        predicate = source.value(cell, _p("qPredicate"))
        obj = source.value(cell, _p("qObject"))
        if obj is None:
            obj = source.value(cell, _p("qObjectLiteral"))
        assert isinstance(ax, URIRef)
        assert isinstance(subject, URIRef)
        assert isinstance(predicate, URIRef)
        assert obj is not None

        assert (ax, RDF.type, OWL.Axiom) in owl
        assert (ax, OWL.annotatedSource, subject) in owl
        assert (ax, OWL.annotatedProperty, predicate) in owl
        assert (ax, OWL.annotatedTarget, obj) in owl
        assert (subject, predicate, obj) in owl
        for ann in source.objects(cell, _p("annotation")):
            prop = source.value(ann, _p("annProperty"))
            value = source.value(ann, _p("annValue"))
            assert isinstance(prop, URIRef)
            assert value is not None
            assert (ax, prop, value) in owl


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
