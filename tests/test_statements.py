"""Tests for the RDF-1.2-first statement-metadata pipeline (issues #28, #29).

The pure-Python tests (DSL parse, reifier minting, the invariants, the OWL emit,
the no-preview-language gate, the hard-fail-without-Jena contract) run anywhere.
The round-trip / no-drift / lossless tests need Apache Jena and are marked
``docker`` + skipped when the pinned image is absent — but they are NOT silently
passed: CI's dedicated Jena job runs them for real on every change.
"""

from __future__ import annotations

from pathlib import Path

import pytest
from rdflib import RDF, XSD, Literal, URIRef
from rdflib.namespace import OWL
from typer.testing import CliRunner

from gmeow_tools.config import (
    JENA_IMAGE,
    PREFIXES,
    PROJECT_ROOT,
    STATEMENT_RDF12_FILE,
)
from gmeow_tools.generator import run
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.runner import image_available
from gmeow_tools.statement_compile import assert_lossless, emit_owl
from gmeow_tools.statement_dsl import (
    Annotation,
    QuotedTriple,
    StatementCell,
    StatementDsl,
    load_statement_dsl,
    mint_reifier,
)
from gmeow_tools.statement_lint import statement_invariants

GM = PREFIXES["gmeow"]
requires_jena = pytest.mark.skipif(
    not image_available(JENA_IMAGE), reason="pinned Jena image not present locally"
)


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


def test_invariants_clean_on_committed_dsl() -> None:
    dsl = load_statement_dsl()
    onto = load_merged_graph(include_imports=False)
    assert statement_invariants(dsl, onto) == []


def _cell(
    prop: URIRef, value: object, *, obj: URIRef | Literal | None = None
) -> StatementDsl:
    """A one-cell DSL with a single annotation, for negative invariant tests."""
    triple = QuotedTriple(
        subject=URIRef(GM + "examples/s"),
        predicate=URIRef(GM + "knowsLanguage"),
        obj=obj if obj is not None else URIRef(GM + "examples/o"),
    )
    cell = StatementCell(
        iri=URIRef(GM + "examples/c1"),
        label="t",
        reifier=mint_reifier(triple),
        triple=triple,
        annotations=(Annotation(prop=prop, value=value),),  # type: ignore[arg-type]
    )
    return StatementDsl(cells=(cell,))


def test_invariant_rejects_non_annotation_property() -> None:
    onto = load_merged_graph(include_imports=False)
    # gmeow:knowsLanguage is an object property, not an annotation property.
    dsl = _cell(URIRef(GM + "knowsLanguage"), Literal("x"))
    problems = statement_invariants(dsl, onto)
    assert any("owl:AnnotationProperty" in p for p in problems)


def test_invariant_rejects_confidence_out_of_range() -> None:
    onto = load_merged_graph(include_imports=False)
    dsl = _cell(URIRef(GM + "confidence"), Literal(1.5))
    problems = statement_invariants(dsl, onto)
    assert any("outside [0, 1]" in p for p in problems)


def test_invariant_rejects_non_owl2_datatype() -> None:
    onto = load_merged_graph(include_imports=False)
    # A base-triple literal typed xsd:date is not OWL 2 DL.
    bad = Literal("1990-05-01", datatype=XSD.date)
    dsl = _cell(URIRef(GM + "confidence"), Literal(0.9), obj=bad)
    problems = statement_invariants(dsl, onto)
    assert any("not an OWL 2 datatype" in p for p in problems)


def test_invariant_rejects_undeclared_predicate() -> None:
    onto = load_merged_graph(include_imports=False)
    triple = QuotedTriple(
        subject=URIRef(GM + "examples/s"),
        predicate=URIRef(GM + "totallyNotARealProperty"),
        obj=URIRef(GM + "examples/o"),
    )
    cell = StatementCell(
        iri=URIRef(GM + "examples/c1"),
        label="t",
        reifier=mint_reifier(triple),
        triple=triple,
        annotations=(Annotation(prop=URIRef(GM + "confidence"), value=Literal(0.5)),),
    )
    problems = statement_invariants(StatementDsl(cells=(cell,)), onto)
    assert any("not a declared GMEOW property" in p for p in problems)


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
# Pure-Python: the hard-fail-without-Jena contract (#28.3)
# --------------------------------------------------------------------------- #


def test_rdf12_hard_fails_without_jena(monkeypatch: pytest.MonkeyPatch) -> None:
    import gmeow_tools.runner as runner

    monkeypatch.setattr(runner, "image_available", lambda _image, **_kw: False)
    from gmeow_tools.cli import app

    result = CliRunner().invoke(app, ["regenerate", "statements"])
    assert result.exit_code != 0  # ToolUnavailableError → no degraded fallback
    assert result.exception is not None
    assert "ToolUnavailableError" in type(result.exception).__name__


# --------------------------------------------------------------------------- #
# Jena-gated: round-trip, no-drift, lossless (#28.1, #28.2, #29.2, #29.3)
# --------------------------------------------------------------------------- #


@pytest.mark.docker
@requires_jena
def test_committed_artifacts_match_dsl() -> None:
    report = run("statements", check=True)
    assert report.drifted == [], (
        "committed statement artifacts are stale — run `gmeow regenerate`:\n  "
        + "\n  ".join(report.drifted)
    )
    assert report.orphans == [], (
        "committed statement artifacts include orphaned generated files:\n  "
        + "\n  ".join(report.orphans)
    )


@pytest.mark.docker
@requires_jena
def test_committed_rdf12_round_trips_to_owl() -> None:
    """The committed RDF 1.2 lead artifact, normalized back, equals the OWL form."""
    owl = emit_owl(load_statement_dsl())
    assert assert_lossless(owl, STATEMENT_RDF12_FILE) == []


@pytest.mark.docker
@requires_jena
def test_lossless_gate_detects_a_dropped_annotation() -> None:
    """The round-trip gate actually bites: a missing annotation is reported."""
    owl = emit_owl(load_statement_dsl())
    dropped = next(t for t in owl if str(t[1]) == GM + "confidence")
    owl.remove(dropped)
    problems = assert_lossless(owl, STATEMENT_RDF12_FILE)
    assert problems and any("confidence" in p for p in problems)


@pytest.mark.docker
@requires_jena
def test_committed_rdf12_uses_triple_term_syntax() -> None:
    text = STATEMENT_RDF12_FILE.read_text(encoding="utf-8")
    assert "rdf:reifies" in text and "<<(" in text


@pytest.mark.docker
@requires_jena
def test_reason_consumes_generated_owl_downcast() -> None:
    """Reasoning runs on the generated OWL form and stays OWL 2 DL + consistent."""
    from gmeow_tools import reason as reasoning

    merged = reasoning.merge_release()
    text = merged.read_text(encoding="utf-8")
    assert "owl:Axiom" in text  # the generated downcast was merged in
    reasoning.validate_profile("DL", merged=merged)  # raises if not DL
    reasoning.reason("ELK", merged=merged)  # raises if incoherent


def test_statement_rdf12_committed_under_repo_not_dist() -> None:
    """The lead artifact is committed (so --check has a target), not in dist/."""
    assert STATEMENT_RDF12_FILE.is_relative_to(
        PROJECT_ROOT / "generated" / "statements"
    )
    assert Path(STATEMENT_RDF12_FILE).exists()
