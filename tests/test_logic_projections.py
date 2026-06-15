# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the logic: projection back-ends (issue #500, Task 3).

Module under test: ``logic_projections.py``.

Covers:
* Each back-end emits well-formed, deterministic output for a fixture LogicProgram.
* Two runs on the same program produce identical bytes.
* The report records the correct preservationKind + complexityClass per target.
* The report aggregates gmeow:lossyDrop from structural notes.
* An injected overclaim (ExactPreservation on a lossy projection) raises OverclaimError.
* AC4: project_canonical_rdf12 → re-parse via parse_logic_source → assert_ir_isomorphic.
* AC5: Write the EL projection of a small fixture to a temp file and run ELK over it;
  skip if ELK/Docker unavailable.
"""

from __future__ import annotations

import shutil
from pathlib import Path

import pytest
from rdflib import RDF, Graph, Namespace

from gmeow_tools.config import LOGIC_NAMESPACE, NAMESPACE
from gmeow_tools.logic_adapter import assert_ir_isomorphic
from gmeow_tools.logic_frontend import parse_logic_source
from gmeow_tools.logic_ir import (
    ComplexityClass,
    ContextualScope,
    LogicAxiom,
    LogicModality,
    LogicProfile,
    LogicProgram,
    LogicRule,
    PreservationKind,
    SemanticProfileId,
)
from gmeow_tools.logic_projections import (
    _TARGET_META,
    OverclaimError,
    ProjectionResult,
    assert_no_overclaim,
    build_projection_report,
    project_canonical_rdf12,
    project_datalog,
    project_gufo,
    project_n3,
    project_nemo,
    project_owl_dl,
    project_owl_el,
)

LOGIC = Namespace(LOGIC_NAMESPACE)
GMEOW = Namespace(NAMESPACE)
EX = Namespace("https://example.org/test/")

# --------------------------------------------------------------------------- #
# Fixture LogicProgram
# --------------------------------------------------------------------------- #

_RDF_TYPE = str(RDF.type)
_LOGIC_KIND = LOGIC_NAMESPACE + "Kind"
_LOGIC_ROLE = LOGIC_NAMESPACE + "Role"
_LOGIC_SUBCLS = LOGIC_NAMESPACE + "subClassOf"
_LOGIC_DISJ = LOGIC_NAMESPACE + "disjointWith"


def _minimal_program() -> LogicProgram:
    """A small but non-trivial LogicProgram for projection tests."""
    axioms = (
        LogicAxiom(
            subject=str(EX.Person),
            predicate=_RDF_TYPE,
            obj=_LOGIC_KIND,
        ),
        LogicAxiom(
            subject=str(EX.Employee),
            predicate=_RDF_TYPE,
            obj=_LOGIC_ROLE,
        ),
        LogicAxiom(
            subject=str(EX.Employee),
            predicate=_LOGIC_SUBCLS,
            obj=str(EX.Person),
        ),
        LogicAxiom(
            subject=str(EX.Manager),
            predicate=_LOGIC_DISJ,
            obj=str(EX.Intern),
        ),
    )
    profiles = (
        LogicProfile(
            profile_id=SemanticProfileId.POSITIVE_HORN,
            complexity=ComplexityClass("PTIME"),
        ),
    )
    return LogicProgram(
        axioms=axioms,
        rules=(),
        profiles=profiles,
    )


def _program_with_rule() -> LogicProgram:
    """A LogicProgram with one rule for rule-surface tests."""
    head = LogicAxiom(
        subject=str(EX.Employee),
        predicate=_LOGIC_SUBCLS,
        obj=str(EX.Person),
    )
    body1 = LogicAxiom(
        subject=str(EX.Employee),
        predicate=_RDF_TYPE,
        obj=_LOGIC_ROLE,
    )
    rule = LogicRule(head=head, body=(body1,))
    axioms = (
        LogicAxiom(
            subject=str(EX.Person),
            predicate=_RDF_TYPE,
            obj=_LOGIC_KIND,
        ),
    )
    return LogicProgram(axioms=axioms, rules=(rule,), profiles=())


def _program_with_scope() -> LogicProgram:
    """A LogicProgram with a scoped axiom (modal context)."""
    scoped = LogicAxiom(
        subject=str(EX.Person),
        predicate=_RDF_TYPE,
        obj=_LOGIC_KIND,
        scope=ContextualScope(
            modality=LogicModality.EPISTEMIC,
            confidence=0.9,
        ),
    )
    return LogicProgram(axioms=(scoped,), rules=(), profiles=())


# --------------------------------------------------------------------------- #
# Helper
# --------------------------------------------------------------------------- #


def _all_projections(program: LogicProgram) -> list[ProjectionResult]:
    return [
        project_owl_dl(program),
        project_owl_el(program),
        project_datalog(program),
        project_n3(program),
        project_gufo(program),
        project_canonical_rdf12(program),
        project_nemo(program),
    ]


# --------------------------------------------------------------------------- #
# Determinism: same program → identical bytes on two runs
# --------------------------------------------------------------------------- #


def test_owl_dl_deterministic() -> None:
    prog = _minimal_program()
    r1 = project_owl_dl(prog)
    r2 = project_owl_dl(prog)
    assert r1.content == r2.content


def test_owl_el_deterministic() -> None:
    prog = _minimal_program()
    r1 = project_owl_el(prog)
    r2 = project_owl_el(prog)
    assert r1.content == r2.content


def test_datalog_deterministic() -> None:
    prog = _minimal_program()
    r1 = project_datalog(prog)
    r2 = project_datalog(prog)
    assert r1.content == r2.content


def test_n3_deterministic() -> None:
    prog = _minimal_program()
    r1 = project_n3(prog)
    r2 = project_n3(prog)
    assert r1.content == r2.content


def test_gufo_deterministic() -> None:
    prog = _minimal_program()
    r1 = project_gufo(prog)
    r2 = project_gufo(prog)
    assert r1.content == r2.content


def test_canonical_rdf12_deterministic() -> None:
    prog = _minimal_program()
    r1 = project_canonical_rdf12(prog)
    r2 = project_canonical_rdf12(prog)
    assert r1.content == r2.content


# --------------------------------------------------------------------------- #
# Well-formedness: each RDF projection parses as valid Turtle
# --------------------------------------------------------------------------- #


def test_owl_dl_parses_as_turtle() -> None:
    prog = _minimal_program()
    result = project_owl_dl(prog)
    g = Graph()
    g.parse(data=result.content, format="turtle")
    assert len(g) > 0


def test_owl_el_parses_as_turtle() -> None:
    prog = _minimal_program()
    result = project_owl_el(prog)
    g = Graph()
    g.parse(data=result.content, format="turtle")
    assert len(g) > 0


def test_n3_graph_contains_axioms() -> None:
    prog = _minimal_program()
    result = project_n3(prog)
    assert result.graph is not None
    assert len(result.graph) > 0


def test_gufo_parses_as_turtle() -> None:
    prog = _minimal_program()
    result = project_gufo(prog)
    g = Graph()
    g.parse(data=result.content, format="turtle")
    assert len(g) > 0


def test_canonical_rdf12_parses_as_turtle() -> None:
    prog = _minimal_program()
    result = project_canonical_rdf12(prog)
    g = Graph()
    g.parse(data=result.content, format="turtle")
    assert len(g) > 0


# --------------------------------------------------------------------------- #
# Content smoke tests per back-end
# --------------------------------------------------------------------------- #


def test_owl_dl_contains_subclassof() -> None:
    """The OWL DL projection maps logic:subClassOf to rdfs:subClassOf."""
    prog = _minimal_program()
    result = project_owl_dl(prog)
    assert result.graph is not None
    from rdflib.namespace import RDFS

    subclassof_triples = list(result.graph.triples((None, RDFS.subClassOf, None)))
    assert subclassof_triples, "Expected rdfs:subClassOf triples in OWL DL output"


def test_owl_el_excludes_disjointwith() -> None:
    """disjointWith is not EL-safe and must not appear in the EL projection."""
    prog = _minimal_program()
    result = project_owl_el(prog)
    from rdflib.namespace import OWL

    disjoint_triples = list(result.graph.triples((None, OWL.disjointWith, None)))  # type: ignore[union-attr]
    assert not disjoint_triples, "OWL EL projection must not contain owl:disjointWith"


def test_owl_el_includes_subclassof() -> None:
    """subClassOf is EL-safe and must appear in the EL projection."""
    prog = _minimal_program()
    result = project_owl_el(prog)
    from rdflib.namespace import RDFS

    sc_triples = list(result.graph.triples((None, RDFS.subClassOf, None)))  # type: ignore[union-attr]
    assert sc_triples, "Expected rdfs:subClassOf triples in OWL EL output"


def test_datalog_contains_facts() -> None:
    """Datalog output contains ground fact lines."""
    prog = _minimal_program()
    result = project_datalog(prog)
    assert result.graph is None  # Datalog is text-only
    assert "type(" in result.content
    assert "subClassOf(" in result.content


def test_gufo_contains_gufo_type() -> None:
    """gUFO projection maps logic:Kind to gufo:Kind."""
    prog = _minimal_program()
    result = project_gufo(prog)
    assert result.graph is not None
    gufo_ns = "http://purl.org/nemo/gufo#"
    has_gufo = any(str(o).startswith(gufo_ns) for _, _, o in result.graph)
    assert has_gufo, "Expected gUFO-namespace objects in gufo projection"


def test_canonical_rdf12_contains_profiles() -> None:
    """Canonical projection emits logic:SemanticProfile declarations."""
    prog = _minimal_program()
    result = project_canonical_rdf12(prog)
    assert result.graph is not None
    semantic_profile = LOGIC.SemanticProfile
    profile_triples = list(result.graph.triples((None, RDF.type, semantic_profile)))
    assert profile_triples, (
        "Expected logic:SemanticProfile declarations in canonical RDF 1.2 output"
    )


def test_canonical_rdf12_contains_scoped_axioms() -> None:
    """Canonical projection emits reifier nodes for scoped axioms."""
    prog = _program_with_scope()
    result = project_canonical_rdf12(prog)
    assert result.graph is not None
    # Should have rdf:Statement nodes for the scoped axiom
    stmt_triples = list(result.graph.triples((None, RDF.type, RDF.Statement)))
    assert stmt_triples, "Expected rdf:Statement reifier nodes for scoped axioms"


def test_canonical_rdf12_contains_rules() -> None:
    """Canonical projection emits logic:Rule nodes."""
    prog = _program_with_rule()
    result = project_canonical_rdf12(prog)
    assert result.graph is not None
    rule_triples = list(result.graph.triples((None, RDF.type, LOGIC.Rule)))
    assert rule_triples, "Expected logic:Rule nodes in canonical RDF 1.2 output"


# --------------------------------------------------------------------------- #
# Preservation kind + complexity class per target
# --------------------------------------------------------------------------- #


@pytest.mark.parametrize(
    "target, expected_kind, cx_prefix",
    [
        ("owl-dl", PreservationKind.SOUND_UNDER, "decidable"),
        ("owl-el", PreservationKind.SOUND_UNDER, "PTIME"),
        ("datalog", PreservationKind.SOUND_UNDER, "terminating"),
        ("n3", PreservationKind.COMPLETE_OVER, "semi-decidable"),
        ("gufo", PreservationKind.VALIDATION_ONLY, "PTIME"),
        ("canonical-rdf12", PreservationKind.EXACT, "N/A"),
        ("nemo", PreservationKind.EXACT, "PTIME"),
    ],
)
def test_target_meta_preservation_kind(
    target: str, expected_kind: PreservationKind, cx_prefix: str
) -> None:
    """_TARGET_META declares the correct preservation kind and complexity."""
    kind, cx, _drops = _TARGET_META[target]
    assert kind == expected_kind, f"{target}: expected {expected_kind}, got {kind}"
    assert cx.startswith(cx_prefix), (
        f"{target}: expected complexity starting with {cx_prefix!r}, got {cx!r}"
    )


# --------------------------------------------------------------------------- #
# Projection report
# --------------------------------------------------------------------------- #


def test_report_contains_all_targets() -> None:
    """build_projection_report records a node for each projection."""
    prog = _minimal_program()
    projs = _all_projections(prog)
    report_g = build_projection_report(prog, projs)

    target_labels = {
        str(o)
        for _, p, o in report_g.triples((None, None, None))
        if str(p) == str(LOGIC.hasProjection)
        # hasProjection's object is the target IRI; label is on that node
    }
    # Seven targets should all appear (6 original + nemo)
    assert len(target_labels) == 7


def test_report_preservation_kind_in_graph() -> None:
    """Each target in the report has a logic:preservationKind triple."""
    prog = _minimal_program()
    projs = _all_projections(prog)
    report_g = build_projection_report(prog, projs)

    preservation_objs = {
        str(o) for _, p, o in report_g.triples((None, LOGIC.preservationKind, None))
    }
    expected = {
        LOGIC_NAMESPACE + PreservationKind.SOUND_UNDER,
        LOGIC_NAMESPACE + PreservationKind.COMPLETE_OVER,
        LOGIC_NAMESPACE + PreservationKind.VALIDATION_ONLY,
        LOGIC_NAMESPACE + PreservationKind.EXACT,
    }
    assert expected <= preservation_objs, (
        f"Expected preservation kinds missing: {expected - preservation_objs}"
    )


def test_report_complexity_class_in_graph() -> None:
    """Each target has a logic:complexityClass literal."""
    prog = _minimal_program()
    projs = _all_projections(prog)
    report_g = build_projection_report(prog, projs)

    cx_values = {
        str(o) for _, p, o in report_g.triples((None, LOGIC.complexityClass, None))
    }
    assert "PTIME" in cx_values


def test_report_lossy_drop_in_graph() -> None:
    """Lossy targets record gmeow:lossyDrop triples in the report."""
    prog = _minimal_program()
    projs = _all_projections(prog)
    report_g = build_projection_report(prog, projs)

    drops = list(report_g.triples((None, GMEOW.lossyDrop, None)))
    assert drops, "Expected gmeow:lossyDrop triples in the projection report"


def test_report_counts_match_program() -> None:
    """The report records axiom/rule/profile counts from the source program."""
    prog = _minimal_program()
    projs = _all_projections(prog)
    report_g = build_projection_report(prog, projs)

    report_iri = LOGIC.term("projection-report")
    axiom_counts = list(report_g.triples((report_iri, LOGIC.axiomCount, None)))
    assert axiom_counts, "Expected logic:axiomCount in report"
    assert int(str(axiom_counts[0][2])) == len(prog.axioms)


def test_report_writes_to_path(tmp_path: Path) -> None:
    """build_projection_report writes a valid Turtle file when path is given."""
    prog = _minimal_program()
    projs = _all_projections(prog)
    out_path = tmp_path / "projection-report.ttl"
    build_projection_report(prog, projs, path=out_path)
    assert out_path.exists()
    g = Graph()
    g.parse(str(out_path), format="turtle")
    assert len(g) > 0


# --------------------------------------------------------------------------- #
# Overclaim detection
# --------------------------------------------------------------------------- #


def test_assert_no_overclaim_passes_with_no_drops() -> None:
    """ExactPreservation with empty drops must not raise."""
    assert_no_overclaim("canonical-rdf12", PreservationKind.EXACT, [])


def test_assert_no_overclaim_passes_sound_under_with_drops() -> None:
    """SoundUnderApproximation with drops must not raise."""
    assert_no_overclaim(
        "owl-dl",
        PreservationKind.SOUND_UNDER,
        ["some predicate dropped"],
    )


def test_assert_no_overclaim_raises_on_exact_with_drops() -> None:
    """ExactPreservation with actual drops → OverclaimError (red build)."""
    with pytest.raises(OverclaimError, match="ExactPreservation"):
        assert_no_overclaim(
            "fake-target",
            PreservationKind.EXACT,
            ["dropped modal context on <ex:Foo>"],
        )


def test_report_raises_on_overclaim_projection() -> None:
    """build_projection_report raises OverclaimError if a projection overclaims."""
    prog = _minimal_program()
    # Inject a projection that claims ExactPreservation but has actual drops
    bad_proj = ProjectionResult(
        target="owl-dl",
        content="",
        graph=None,
        preservation=PreservationKind.EXACT,  # overclaim!
        complexity="N/A",
        lossy_drops=(),
        actual_drops=["dropped modal context on <ex:Foo>"],
    )
    with pytest.raises(OverclaimError):
        build_projection_report(prog, [bad_proj])


# --------------------------------------------------------------------------- #
# AC4: Canonical round-trip — project → re-parse → assert_ir_isomorphic
# --------------------------------------------------------------------------- #


def test_canonical_rdf12_round_trip(tmp_path: Path) -> None:
    """AC4: project_canonical_rdf12 → write → re-parse → assert_ir_isomorphic."""
    prog = _minimal_program()
    result = project_canonical_rdf12(prog)

    # Write to a temp file and re-parse via parse_logic_source
    ttl_path = tmp_path / "logic_canonical.ttl"
    ttl_path.write_text(result.content, encoding="utf-8")

    reparsed, diags = parse_logic_source(ttl_path)

    # Check no ERROR-level diagnostics
    errors = [d for d in diags if d.severity == "ERROR"]
    assert not errors, f"Re-parse produced errors: {errors}"

    # The axioms we authored must round-trip (re-parsed may have more from
    # the ontology header, but our axioms must be there).
    # Use assert_ir_isomorphic on programs restricted to the logic: axioms only.
    # We compare canonical() dicts for the axiom content.
    orig_axiom_keys = {f"{a.subject}\x00{a.predicate}\x00{a.obj}" for a in prog.axioms}
    reparsed_axiom_keys = {
        f"{a.subject}\x00{a.predicate}\x00{a.obj}" for a in reparsed.axioms
    }
    assert orig_axiom_keys <= reparsed_axiom_keys, (
        "Round-trip lost axioms:\n  "
        + "\n  ".join(sorted(orig_axiom_keys - reparsed_axiom_keys))
    )


def test_canonical_rdf12_round_trip_program_with_rule(tmp_path: Path) -> None:
    """AC4 variant: program with rules round-trips correctly."""
    prog = _program_with_rule()
    result = project_canonical_rdf12(prog)
    ttl_path = tmp_path / "logic_canonical_rules.ttl"
    ttl_path.write_text(result.content, encoding="utf-8")

    reparsed, diags = parse_logic_source(ttl_path)
    errors = [d for d in diags if d.severity == "ERROR"]
    assert not errors

    # Rules must round-trip
    assert len(reparsed.rules) >= len(prog.rules), (
        f"Expected at least {len(prog.rules)} rules; got {len(reparsed.rules)}"
    )


def test_canonical_rdf12_assert_ir_isomorphic(tmp_path: Path) -> None:
    """AC4: full assert_ir_isomorphic gate on the canonical round-trip."""
    prog = _minimal_program()
    result = project_canonical_rdf12(prog)
    ttl_path = tmp_path / "logic_canonical_iso.ttl"
    ttl_path.write_text(result.content, encoding="utf-8")

    reparsed, _ = parse_logic_source(ttl_path)

    # Build a restricted reparsed program with just the axioms/profiles from the
    # original (the canonical serialization may include extra ontology triples).
    orig_axiom_set = set(prog.axioms)
    filtered_axioms = tuple(a for a in reparsed.axioms if a in orig_axiom_set)
    filtered_profiles = tuple(p for p in reparsed.profiles if p in set(prog.profiles))
    reparsed_filtered = LogicProgram(
        axioms=filtered_axioms,
        rules=prog.rules,  # rules are an exact match for the simple fixture
        profiles=filtered_profiles,
        source_iri=None,
    )
    assert_ir_isomorphic(prog, reparsed_filtered)


# --------------------------------------------------------------------------- #
# AC5: Targeted ELK reasoning over the EL projection
# --------------------------------------------------------------------------- #


def _elk_available() -> bool:
    """Return True if Docker is on PATH and the ELK/ROBOT image is available."""
    if shutil.which("docker") is None:
        return False
    try:
        import subprocess

        result = subprocess.run(
            ["docker", "info"],
            capture_output=True,
            timeout=10,
        )
        return result.returncode == 0
    except Exception:
        return False


@pytest.mark.skipif(
    not _elk_available(),
    reason="ELK reasoner unavailable (Docker not running or not installed)",
)
def test_elk_certifies_owl_el_projection() -> None:
    """AC5: write EL projection inside PROJECT_ROOT temp dir, run ELK via runner.

    This test exercises the ELK/ROBOT reasoner path via run_container() over
    the OWL EL projection of the fixture program.  It is skipped when Docker is
    not available (CI environments without Docker, developer machines without the
    ROBOT image).

    The file must live inside PROJECT_ROOT because reason.py's _rel() helper
    requires a path below the repo root (Docker mounts it at /work).
    """
    from gmeow_tools.config import ROBOT_IMAGE, gmeow_temp_dir
    from gmeow_tools.runner import ToolUnavailableError, run_container

    prog = _minimal_program()
    el_result = project_owl_el(prog)

    # Use a GMEOW temp dir inside PROJECT_ROOT (Docker-accessible).
    with gmeow_temp_dir() as td:
        import uuid

        merged_name = f"merged-el-test-{uuid.uuid4().hex[:8]}.ttl"
        output_name = f"reasoned-el-{uuid.uuid4().hex[:8]}.ttl"
        merged_path = Path(td) / merged_name
        output_path = Path(td) / output_name

        merged_path.write_text(el_result.content, encoding="utf-8")

        from gmeow_tools.config import PROJECT_ROOT

        merged_rel = str(merged_path.relative_to(PROJECT_ROOT))
        output_rel = str(output_path.relative_to(PROJECT_ROOT))

        try:
            result = run_container(
                ROBOT_IMAGE,
                [
                    "robot",
                    "reason",
                    "--reasoner",
                    "ELK",
                    "--input",
                    merged_rel,
                    "--output",
                    output_rel,
                ],
            )
            # If ROBOT did not raise ToolExecutionError the ontology is consistent.
            combined = result.stdout + result.stderr
            # ELK reports "WARN" for unsatisfiable classes; ERROR means inconsistency.
            assert "ERROR" not in combined or "successful" in combined.lower(), (
                f"ELK reported an error:\n{combined}"
            )
        except ToolUnavailableError as e:
            pytest.skip(f"ELK reasoner unavailable: {e}")


# --------------------------------------------------------------------------- #
# Path-write integration
# --------------------------------------------------------------------------- #


def test_project_owl_dl_writes_path(tmp_path: Path) -> None:
    prog = _minimal_program()
    out = tmp_path / "gmeow-dl.ttl"
    project_owl_dl(prog, path=out)
    assert out.exists()
    assert out.stat().st_size > 0


def test_project_owl_el_writes_path(tmp_path: Path) -> None:
    prog = _minimal_program()
    out = tmp_path / "gmeow-el.ttl"
    project_owl_el(prog, path=out)
    assert out.exists()
    assert out.stat().st_size > 0


def test_project_datalog_writes_path(tmp_path: Path) -> None:
    prog = _minimal_program()
    out = tmp_path / "gmeow.dl"
    project_datalog(prog, path=out)
    assert out.exists()
    assert out.stat().st_size > 0


def test_project_n3_writes_path(tmp_path: Path) -> None:
    prog = _minimal_program()
    out = tmp_path / "gmeow.n3"
    project_n3(prog, path=out)
    assert out.exists()
    assert out.stat().st_size > 0


def test_project_gufo_writes_path(tmp_path: Path) -> None:
    prog = _minimal_program()
    out = tmp_path / "gufo.ttl"
    project_gufo(prog, path=out)
    assert out.exists()
    assert out.stat().st_size > 0


def test_project_canonical_rdf12_writes_path(tmp_path: Path) -> None:
    prog = _minimal_program()
    out = tmp_path / "gmeow.logic.rdf12.ttl"
    project_canonical_rdf12(prog, path=out)
    assert out.exists()


# --------------------------------------------------------------------------- #
# Regression: Datalog uniform arity (Gap 2 / issue #500)
# --------------------------------------------------------------------------- #


def _program_mixed_scope() -> LogicProgram:
    """A LogicProgram with BOTH scoped and unscoped axioms of the SAME predicate.

    The scoped ``subClassOf`` carries epistemic modality; the unscoped one has
    the default scope.  Before the fix, ``project_datalog`` emitted these at
    arity 2 and arity 3 respectively — a load-time error in Soufflé/DLV/Nemo.
    """
    scoped = LogicAxiom(
        subject=str(EX.Animal),
        predicate=_LOGIC_SUBCLS,
        obj=str(EX.Organism),
        scope=ContextualScope(modality=LogicModality.EPISTEMIC, confidence=0.9),
    )
    unscoped = LogicAxiom(
        subject=str(EX.Bird),
        predicate=_LOGIC_SUBCLS,
        obj=str(EX.Animal),
    )
    return LogicProgram(axioms=(scoped, unscoped), rules=(), profiles=())


def _parse_dl_facts(content: str) -> dict[str, set[int]]:
    """Parse a Datalog text and return {predicate_name: set_of_arities}.

    Only inspects ground fact lines of the form ``pred(...).`` — ignores
    comment lines (starting with ``%``) and rule lines (containing ``:-``).
    """
    import re

    fact_pattern = re.compile(r"^(\w+)\s*\(([^)]*)\)\s*\.$")
    result: dict[str, set[int]] = {}
    for line in content.splitlines():
        line = line.strip()
        if not line or line.startswith("%") or ":-" in line:
            continue
        m = fact_pattern.match(line)
        if not m:
            continue
        pred = m.group(1)
        args_str = m.group(2)
        # Count arguments (naive split by comma — adequate for IRI/string atoms).
        arity = len(args_str.split(","))
        result.setdefault(pred, set()).add(arity)
    return result


def test_datalog_uniform_arity_for_mixed_scope_program() -> None:
    """Regression: project_datalog emits uniform arity for every predicate.

    A program mixing scoped and unscoped axioms of the same predicate must
    produce Datalog where every predicate appears at exactly ONE arity.  Mixed
    arity (arity 2 for unscoped, arity 3 for scoped) was the bug reported in
    the Gap-2 Gemini review comment on ``project_datalog`` (~line 607) and is
    a syntax/load error in Soufflé, DLV, and Nemo.
    """
    prog = _program_mixed_scope()
    result = project_datalog(prog)

    arity_map = _parse_dl_facts(result.content)
    # Every predicate in the output must have exactly one arity.
    multi_arity = {
        pred: arities for pred, arities in arity_map.items() if len(arities) > 1
    }
    assert not multi_arity, (
        "project_datalog emitted mixed arity for the following predicate(s):\n"
        + "\n".join(
            f"  {pred}: arities {sorted(arities)}"
            for pred, arities in multi_arity.items()
        )
    )


def test_datalog_scoped_axiom_uses_modality_context() -> None:
    """Scoped axiom emits modality value as context, not 'default'."""
    prog = _program_mixed_scope()
    result = project_datalog(prog)
    # The scoped subClassOf(Animal, Organism) should carry "epistemic" as context.
    assert '"epistemic"' in result.content, (
        "Expected epistemic modality as context argument in scoped fact"
    )


def test_datalog_unscoped_axiom_uses_default_context() -> None:
    """Unscoped axiom emits 'default' as context argument."""
    prog = _program_mixed_scope()
    result = project_datalog(prog)
    # The unscoped subClassOf(Bird, Animal) should carry "default" as context.
    assert '"default"' in result.content, (
        "Expected 'default' as context argument for unscoped facts"
    )


# --------------------------------------------------------------------------- #
# project_nemo: focused unit tests (Task 2 — issue #501)
# --------------------------------------------------------------------------- #


def test_nemo_deterministic() -> None:
    """project_nemo: two runs on the same program produce identical bytes."""
    prog = _minimal_program()
    r1 = project_nemo(prog)
    r2 = project_nemo(prog)
    assert r1.content == r2.content


def test_nemo_target_name() -> None:
    """project_nemo: result.target is 'nemo'."""
    prog = _minimal_program()
    result = project_nemo(prog)
    assert result.target == "nemo"


def test_nemo_no_graph() -> None:
    """project_nemo: result.graph is None (text-only target)."""
    prog = _minimal_program()
    result = project_nemo(prog)
    assert result.graph is None


def test_nemo_preservation_exact() -> None:
    """project_nemo declares ExactPreservation (Horn/Datalog PTIME)."""
    prog = _minimal_program()
    result = project_nemo(prog)
    assert result.preservation == PreservationKind.EXACT


def test_nemo_complexity_ptime() -> None:
    """project_nemo declares PTIME/datalog complexity."""
    prog = _minimal_program()
    result = project_nemo(prog)
    assert result.complexity.startswith("PTIME")


def test_nemo_iri_syntax() -> None:
    """project_nemo encodes IRI arguments as <iri> (angle-bracket Nemo syntax)."""
    prog = _minimal_program()
    result = project_nemo(prog)
    # IRIs must appear as <...> not as "..."
    assert "<https://example.org/test/Person>" in result.content
    # Must NOT appear as double-quoted IRI strings (that is the Datalog encoding)
    assert '"https://example.org/test/Person"' not in result.content


def test_nemo_context_string_syntax() -> None:
    """project_nemo encodes context as a double-quoted Nemo string constant."""
    prog = _minimal_program()
    result = project_nemo(prog)
    assert '"default"' in result.content


def test_nemo_scoped_axiom_uses_modality_context() -> None:
    """Scoped axioms carry the modality value as the context string."""
    prog = _program_mixed_scope()
    result = project_nemo(prog)
    # The epistemic-scoped subClassOf must appear with "epistemic" context
    assert '"epistemic"' in result.content


def test_nemo_unscoped_axiom_uses_default_context() -> None:
    """Unscoped axioms carry the 'default' string as the context argument."""
    prog = _program_mixed_scope()
    result = project_nemo(prog)
    assert '"default"' in result.content


def test_nemo_uniform_arity_for_mixed_scope_program() -> None:
    """project_nemo: every predicate appears at exactly arity 3 (no mixed arity).

    Mirrors the Datalog arity-regression test but for Nemo IRI syntax.
    """
    import re

    prog = _program_mixed_scope()
    result = project_nemo(prog)

    fact_pattern = re.compile(r"^(\w+)\s*\(([^)]+)\)\s*\.$")
    arity_map: dict[str, set[int]] = {}
    for line in result.content.splitlines():
        line = line.strip()
        if not line or line.startswith("%") or ":-" in line:
            continue
        m = fact_pattern.match(line)
        if not m:
            continue
        pred = m.group(1)
        # Count top-level commas (IRIs may not contain commas, so naive split works)
        arity = m.group(2).count(",") + 1
        arity_map.setdefault(pred, set()).add(arity)

    multi_arity = {p: a for p, a in arity_map.items() if len(a) > 1}
    assert not multi_arity, (
        "project_nemo emitted mixed arity for predicates: "
        + ", ".join(f"{p}={sorted(a)}" for p, a in multi_arity.items())
    )


def test_nemo_rule_emitted_correctly() -> None:
    """project_nemo emits rules with :- and Nemo ?VarName variables."""
    prog = _program_with_rule()
    result = project_nemo(prog)
    # A rule must appear (contains ':-')
    assert ":-" in result.content
    # Variables must use the ?VarName Nemo convention
    assert "?C" in result.content


def test_nemo_fact_predicate_names_match_datalog() -> None:
    """project_nemo and project_datalog use identical predicate names.

    Principle 7 parity: the oracle (materialize) uses the same predicate
    names as both back-ends, so the Nemo engine and the Python oracle see
    the same symbolic vocabulary.
    """
    prog = _minimal_program()
    r_dl = project_datalog(prog)
    r_nemo = project_nemo(prog)

    import re

    def _extract_predicates(text: str) -> set[str]:
        pat = re.compile(r"^(\w+)\s*\(", re.MULTILINE)
        return {
            m.group(1) for m in pat.finditer(text) if not m.group(1).startswith("%")
        }

    dl_preds = _extract_predicates(r_dl.content)
    nemo_preds = _extract_predicates(r_nemo.content)
    assert dl_preds == nemo_preds, (
        f"Predicate names differ between Datalog and Nemo:\n"
        f"  Datalog only: {dl_preds - nemo_preds}\n"
        f"  Nemo only:    {nemo_preds - dl_preds}"
    )


def test_nemo_literal_in_output() -> None:
    """project_nemo: literal-valued axiom objects use double-quoted string."""
    scoped = LogicAxiom(
        subject=str(EX.Thing),
        predicate=_LOGIC_SUBCLS,
        obj="some literal value",
        obj_is_literal=True,
    )
    prog = LogicProgram(axioms=(scoped,), rules=(), profiles=())
    result = project_nemo(prog)
    assert '"some literal value"' in result.content


def test_nemo_writes_path(tmp_path: Path) -> None:
    """project_nemo writes the .rls text to path when given."""
    prog = _minimal_program()
    out = tmp_path / "gmeow.rls"
    project_nemo(prog, path=out)
    assert out.exists()
    assert out.stat().st_size > 0
    content = out.read_text(encoding="utf-8")
    assert content.startswith("% GENERATED")


def test_nemo_safety_violation_raises() -> None:
    """project_nemo raises ValueError on a rule with unbound head variable."""
    # Head contains ?Y but body only binds ?X — safety violation
    head = LogicAxiom(
        subject="?X",
        predicate=_LOGIC_SUBCLS,
        obj="?Y",  # unbound — not in body
    )
    body = LogicAxiom(
        subject="?X",
        predicate=_RDF_TYPE,
        obj=_LOGIC_KIND,
    )
    rule = LogicRule(head=head, body=(body,))
    prog = LogicProgram(axioms=(), rules=(rule,), profiles=())
    with pytest.raises(ValueError, match="safety violation"):
        project_nemo(prog)
