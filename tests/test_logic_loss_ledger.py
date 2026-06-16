# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for Task 5: materialization loss-ledger wiring into the projection report.

Verifies that :class:`~gmeow_tools.logic_materialize.LossEntry` records from
:func:`~gmeow_tools.logic_materialize.materialize_program` flow correctly into
:func:`~gmeow_tools.logic_projections.build_projection_report` as
``gmeow:lossyDrop`` triples on the ``nemo`` target, and that the overclaim gate
fires when the nemo target declares ``ExactPreservation`` but materialization
produced narrowed constructs.

Covers:
* Narrowing case: a non-POSITIVE_HORN profile triggers a loss entry in the
  materializer, which appears in the projection report with prefix
  ``"materialization: "`` on the ``nemo`` target node.
* Overclaim case: the same narrowing, with the nemo target synthetically
  declaring ``ExactPreservation``, raises :class:`OverclaimError` (red build).
* Clean case: a POSITIVE_HORN program with no narrowing produces zero
  materialization-sourced ``gmeow:lossyDrop`` triples and no overclaim error.
* Ordering: materialization drops are stable (sorted) so the report is
  deterministic across runs.
"""

from __future__ import annotations

import pytest
from rdflib import RDF, ConjunctiveGraph, Graph, Namespace, URIRef

from gmeow_tools.config import LOGIC_NAMESPACE, NAMESPACE
from gmeow_tools.logic_ir import (
    ComplexityClass,
    LogicAxiom,
    LogicProfile,
    LogicProgram,
    PreservationKind,
    SemanticProfileId,
)
from gmeow_tools.logic_materialize import (
    LossEntry,
    materialize_program,
    parse_nquads,
)
from gmeow_tools.logic_projections import (
    _TARGET_META,
    OverclaimError,
    ProjectionResult,
    build_projection_report,
    project_nemo,
)

LOGIC = Namespace(LOGIC_NAMESPACE)
GMEOW = Namespace(NAMESPACE)
EX = Namespace("https://example.org/loss-ledger-test/")

_RDF_TYPE = str(RDF.type)
_LOGIC_KIND = LOGIC_NAMESPACE + "Kind"
_LOGIC_SUBCLS = LOGIC_NAMESPACE + "subClassOf"
_LOGIC_POSITIVE_HORN_IRI = LOGIC_NAMESPACE + SemanticProfileId.POSITIVE_HORN


# --------------------------------------------------------------------------- #
# Fixture programs
# --------------------------------------------------------------------------- #


def _clean_program() -> LogicProgram:
    """A minimal PositiveHornProfile program that produces no loss entries."""
    axioms = (
        LogicAxiom(
            subject=str(EX.Animal),
            predicate=_RDF_TYPE,
            obj=_LOGIC_KIND,
        ),
        LogicAxiom(
            subject=str(EX.Dog),
            predicate=_LOGIC_SUBCLS,
            obj=str(EX.Animal),
        ),
    )
    profiles = (
        LogicProfile(
            profile_id=SemanticProfileId.POSITIVE_HORN,
            complexity=ComplexityClass("PTIME"),
        ),
    )
    return LogicProgram(axioms=axioms, rules=(), profiles=profiles)


def _narrowing_program() -> LogicProgram:
    """A program declaring a non-POSITIVE_HORN profile → loss entry in oracle.

    The v1 oracle supports only PositiveHornProfile; any other declared profile
    is narrowed (the semantics are not applied) and recorded as a LossEntry
    with PreservationKind.SOUND_UNDER.
    """
    axioms = (
        LogicAxiom(
            subject=str(EX.Animal),
            predicate=_RDF_TYPE,
            obj=_LOGIC_KIND,
        ),
    )
    profiles = (
        LogicProfile(
            profile_id=SemanticProfileId.PROBABILISTIC,  # not supported by v1 oracle
            complexity=ComplexityClass("probabilistic"),
        ),
    )
    return LogicProgram(axioms=axioms, rules=(), profiles=profiles)


def _empty_conjunctive_graph() -> ConjunctiveGraph:
    """Return an empty ConjunctiveGraph (no worlds → no chase needed)."""
    return parse_nquads("")


def _single_world_graph(
    world_iri: str = "https://example.org/world/w1",
) -> ConjunctiveGraph:
    """Return a ConjunctiveGraph with one named world containing one quad."""
    nq = f"<{EX.Animal!s}> <{RDF.type!s}> <{_LOGIC_KIND}> <{world_iri}> .\n"
    return parse_nquads(nq)


# --------------------------------------------------------------------------- #
# Helper
# --------------------------------------------------------------------------- #


def _nemo_proj(program: LogicProgram) -> ProjectionResult:
    """Run the nemo projection for *program* (no path, returns in-memory result)."""
    return project_nemo(program)


def _all_drops_on_nemo(report_graph: Graph) -> list[str]:
    """Return all gmeow:lossyDrop literals on the nemo target node."""
    nemo_iri = URIRef(LOGIC_NAMESPACE + "target/nemo")
    return [
        str(o) for _, p, o in report_graph.triples((nemo_iri, GMEOW.lossyDrop, None))
    ]


# --------------------------------------------------------------------------- #
# Clean case: no materialization narrowing
# --------------------------------------------------------------------------- #


def test_clean_program_no_mat_loss_entries() -> None:
    """A PositiveHornProfile program run through the oracle produces no loss entries."""
    prog = _clean_program()
    cg = _single_world_graph()
    result = materialize_program(prog, cg)
    assert result.loss_entries == (), (
        f"Expected no loss entries for clean program, got {result.loss_entries}"
    )


def test_clean_program_report_has_no_materialization_drops() -> None:
    """With empty materialization_loss_entries, no 'materialization: ' drops appear."""
    prog = _clean_program()
    cg = _single_world_graph()
    mat_result = materialize_program(prog, cg)

    nemo_proj = _nemo_proj(prog)
    report_g = build_projection_report(
        prog,
        [nemo_proj],
        materialization_loss_entries=list(mat_result.loss_entries),
    )

    nemo_drops = _all_drops_on_nemo(report_g)
    mat_prefixed = [d for d in nemo_drops if d.startswith("materialization: ")]
    assert not mat_prefixed, (
        f"Expected no materialization: drops in clean case; got {mat_prefixed}"
    )


def test_clean_program_no_overclaim_error() -> None:
    """ExactPreservation + empty materialization loss entries does NOT raise."""
    prog = _clean_program()
    cg = _single_world_graph()
    mat_result = materialize_program(prog, cg)

    nemo_proj = _nemo_proj(prog)
    # nemo declares ExactPreservation — with no loss entries this must not raise.
    assert nemo_proj.preservation == PreservationKind.EXACT
    build_projection_report(
        prog,
        [nemo_proj],
        materialization_loss_entries=list(mat_result.loss_entries),
    )


def test_none_materialization_loss_entries_is_clean() -> None:
    """Passing materialization_loss_entries=None treats the loss list as empty."""
    prog = _clean_program()
    nemo_proj = _nemo_proj(prog)
    report_g = build_projection_report(
        prog,
        [nemo_proj],
        materialization_loss_entries=None,
    )
    nemo_drops = _all_drops_on_nemo(report_g)
    mat_prefixed = [d for d in nemo_drops if d.startswith("materialization: ")]
    assert not mat_prefixed


def test_empty_list_materialization_loss_entries_is_clean() -> None:
    """Passing materialization_loss_entries=[] treats the loss list as empty."""
    prog = _clean_program()
    nemo_proj = _nemo_proj(prog)
    report_g = build_projection_report(
        prog,
        [nemo_proj],
        materialization_loss_entries=[],
    )
    nemo_drops = _all_drops_on_nemo(report_g)
    mat_prefixed = [d for d in nemo_drops if d.startswith("materialization: ")]
    assert not mat_prefixed


# --------------------------------------------------------------------------- #
# Narrowing case: materialization produces loss entries
# --------------------------------------------------------------------------- #


def test_narrowing_program_produces_loss_entries() -> None:
    """A non-POSITIVE_HORN program run through the oracle produces loss entries.

    The loss-entry recording happens inside _chase_world, which requires at
    least one named-graph world in the input.  An empty ConjunctiveGraph has
    no worlds so no chase runs and no entries are recorded.
    """
    prog = _narrowing_program()
    cg = _single_world_graph()  # one world → chase runs → loss entry recorded
    result = materialize_program(prog, cg)
    assert len(result.loss_entries) > 0, (
        "Expected loss entries for non-POSITIVE_HORN program; oracle did not record any"
    )


def test_narrowing_program_loss_entries_have_sound_under_preservation() -> None:
    """Loss entries for a non-POSITIVE_HORN profile carry SoundUnderApproximation."""
    prog = _narrowing_program()
    cg = _single_world_graph()  # one world needed for chase to run
    result = materialize_program(prog, cg)
    assert result.loss_entries, (
        "Expected at least one loss entry for narrowing program; loop would be vacuous"
    )
    for entry in result.loss_entries:
        assert entry.preservation_kind == PreservationKind.SOUND_UNDER, (
            f"Expected SOUND_UNDER on loss entry, got {entry.preservation_kind}"
        )


def test_narrowing_produces_materialization_drop_in_report() -> None:
    """Loss entries from the oracle appear as 'materialization: ' drops on nemo."""
    prog = _narrowing_program()
    cg = _single_world_graph()  # one world needed for chase to run
    mat_result = materialize_program(prog, cg)

    # Synthesise a nemo projection that does NOT claim ExactPreservation
    # (so the overclaim gate doesn't fire) — we patch it to SOUND_UNDER for this test.
    nemo_proj_exact = _nemo_proj(prog)
    nemo_proj_sound = ProjectionResult(
        target="nemo",
        content=nemo_proj_exact.content,
        graph=None,
        preservation=PreservationKind.SOUND_UNDER,  # downgrade to avoid overclaim
        complexity=nemo_proj_exact.complexity,
        lossy_drops=nemo_proj_exact.lossy_drops,
        actual_drops=nemo_proj_exact.actual_drops,
    )

    report_g = build_projection_report(
        prog,
        [nemo_proj_sound],
        materialization_loss_entries=list(mat_result.loss_entries),
    )

    nemo_drops = _all_drops_on_nemo(report_g)
    mat_drops = [d for d in nemo_drops if d.startswith("materialization: ")]
    assert mat_drops, (
        "Expected at least one 'materialization: ' gmeow:lossyDrop on the nemo "
        f"target; got zero.  All nemo drops: {nemo_drops}"
    )


def test_narrowing_drop_contains_construct_and_reason() -> None:
    """The 'materialization: ' drop text encodes the construct IRI and reason."""
    prog = _narrowing_program()
    cg = _single_world_graph()  # one world needed for chase to run
    mat_result = materialize_program(prog, cg)

    nemo_proj_sound = ProjectionResult(
        target="nemo",
        content="",
        graph=None,
        preservation=PreservationKind.SOUND_UNDER,
        complexity="PTIME/datalog",
        lossy_drops=(),
        actual_drops=[],
    )

    report_g = build_projection_report(
        prog,
        [nemo_proj_sound],
        materialization_loss_entries=list(mat_result.loss_entries),
    )

    nemo_drops = _all_drops_on_nemo(report_g)
    mat_drops = [d for d in nemo_drops if d.startswith("materialization: ")]

    # Each drop must encode the construct and reason from the LossEntry
    assert mat_result.loss_entries, (
        "Expected at least one loss entry for narrowing program; loop would be vacuous"
    )
    for entry in mat_result.loss_entries:
        found = any(entry.construct in d and entry.reason in d for d in mat_drops)
        assert found, (
            f"LossEntry construct={entry.construct!r}, reason={entry.reason!r} "
            f"not found in report drops:\n  " + "\n  ".join(mat_drops)
        )


def test_narrowing_drop_contains_preservation_kind() -> None:
    """The 'materialization: ' drop text encodes the preservationKind value."""
    prog = _narrowing_program()
    cg = _single_world_graph()  # one world needed for chase to run
    mat_result = materialize_program(prog, cg)

    nemo_proj_sound = ProjectionResult(
        target="nemo",
        content="",
        graph=None,
        preservation=PreservationKind.SOUND_UNDER,
        complexity="PTIME/datalog",
        lossy_drops=(),
        actual_drops=[],
    )

    report_g = build_projection_report(
        prog,
        [nemo_proj_sound],
        materialization_loss_entries=list(mat_result.loss_entries),
    )

    nemo_drops = _all_drops_on_nemo(report_g)
    mat_drops = [d for d in nemo_drops if d.startswith("materialization: ")]

    assert mat_result.loss_entries, (
        "Expected at least one loss entry for narrowing program; loop would be vacuous"
    )
    for entry in mat_result.loss_entries:
        found = any(entry.preservation_kind.value in d for d in mat_drops)
        assert found, (
            f"preservationKind={entry.preservation_kind.value!r} "
            f"not found in mat drops: {mat_drops}"
        )


# --------------------------------------------------------------------------- #
# Overclaim case: ExactPreservation + materialization drops → OverclaimError
# --------------------------------------------------------------------------- #


def test_overclaim_exact_with_mat_loss_raises_overclaim_error() -> None:
    """ExactPreservation + non-empty materialization loss entries → OverclaimError.

    This is the red-build gate: if the nemo target declares ExactPreservation
    but the oracle chase narrowed a construct, the build must fail.
    """
    # Craft a synthetic LossEntry (does not require running the oracle)
    synthetic_entry = LossEntry(
        construct=LOGIC_NAMESPACE + "ProbabilisticProfile",
        reason=(
            "v1 oracle supports only PositiveHornProfile; "
            "ProbabilisticProfile semantics not applied"
        ),
        preservation_kind=PreservationKind.SOUND_UNDER,
    )

    prog = _clean_program()
    # nemo naturally declares ExactPreservation (from _TARGET_META)
    nemo_proj = _nemo_proj(prog)
    assert nemo_proj.preservation == PreservationKind.EXACT

    with pytest.raises(OverclaimError, match="ExactPreservation"):
        build_projection_report(
            prog,
            [nemo_proj],
            materialization_loss_entries=[synthetic_entry],
        )


def test_overclaim_error_message_includes_target_name() -> None:
    """OverclaimError message identifies the 'nemo' target by name."""
    synthetic_entry = LossEntry(
        construct=LOGIC_NAMESPACE + "ProbabilisticProfile",
        reason="not applied by v1 oracle",
        preservation_kind=PreservationKind.SOUND_UNDER,
    )

    prog = _clean_program()
    nemo_proj = _nemo_proj(prog)

    with pytest.raises(OverclaimError, match="nemo"):
        build_projection_report(
            prog,
            [nemo_proj],
            materialization_loss_entries=[synthetic_entry],
        )


def test_overclaim_fires_only_for_exact_preservation() -> None:
    """OverclaimError is NOT raised when the nemo target declares SOUND_UNDER."""
    synthetic_entry = LossEntry(
        construct=LOGIC_NAMESPACE + "ProbabilisticProfile",
        reason="not applied by v1 oracle",
        preservation_kind=PreservationKind.SOUND_UNDER,
    )

    prog = _clean_program()
    nemo_proj_sound = ProjectionResult(
        target="nemo",
        content="",
        graph=None,
        preservation=PreservationKind.SOUND_UNDER,  # honest downgrade
        complexity="PTIME/datalog",
        lossy_drops=(),
        actual_drops=[],
    )

    # Must NOT raise
    build_projection_report(
        prog,
        [nemo_proj_sound],
        materialization_loss_entries=[synthetic_entry],
    )


def test_existing_overclaim_gate_still_fires_for_non_nemo_targets() -> None:
    """The pre-existing overclaim gate for non-materialization drops is unchanged.

    A projection target other than nemo that claims ExactPreservation but has
    actual_drops still raises OverclaimError (regression guard).
    """
    prog = _clean_program()
    bad_proj = ProjectionResult(
        target="owl-dl",
        content="",
        graph=None,
        preservation=PreservationKind.EXACT,  # overclaim!
        complexity="decidable/N2EXPTIME",
        lossy_drops=(),
        actual_drops=["dropped modal context on <ex:Foo>"],
    )

    with pytest.raises(OverclaimError, match="ExactPreservation"):
        build_projection_report(
            prog,
            [bad_proj],
            materialization_loss_entries=None,
        )


# --------------------------------------------------------------------------- #
# Determinism: materialization drops are stable
# --------------------------------------------------------------------------- #


def test_report_with_mat_drops_is_deterministic() -> None:
    """Two report calls with the same mat entries produce isomorphic graphs."""
    prog = _narrowing_program()
    cg = _single_world_graph()  # one world needed for chase to run
    mat_result = materialize_program(prog, cg)

    nemo_proj_sound = ProjectionResult(
        target="nemo",
        content="",
        graph=None,
        preservation=PreservationKind.SOUND_UNDER,
        complexity="PTIME/datalog",
        lossy_drops=(),
        actual_drops=[],
    )

    g1 = build_projection_report(
        prog,
        [nemo_proj_sound],
        materialization_loss_entries=list(mat_result.loss_entries),
    )
    g2 = build_projection_report(
        prog,
        [nemo_proj_sound],
        materialization_loss_entries=list(mat_result.loss_entries),
    )

    assert g1.isomorphic(g2), (
        "build_projection_report with same mat entries produced non-isomorphic graphs"
    )


# --------------------------------------------------------------------------- #
# End-to-end: full oracle run + projection report wiring
# --------------------------------------------------------------------------- #


def test_end_to_end_narrowing_to_report() -> None:
    """Full pipeline: oracle chase on narrowing program → mat drop on nemo target."""
    prog = _narrowing_program()
    cg = _single_world_graph()  # one world needed for chase to run

    # Oracle chase — captures loss entries
    mat_result = materialize_program(prog, cg)
    assert mat_result.loss_entries, "Narrowing program must produce loss entries"

    # Projection (downgraded to SOUND_UNDER for the nemo target)
    nemo_proj_sound = ProjectionResult(
        target="nemo",
        content=project_nemo(prog).content,
        graph=None,
        preservation=PreservationKind.SOUND_UNDER,
        complexity="PTIME/datalog",
        lossy_drops=_TARGET_META["nemo"][2],
        actual_drops=[],
    )

    report_g = build_projection_report(
        prog,
        [nemo_proj_sound],
        materialization_loss_entries=list(mat_result.loss_entries),
    )

    # The nemo target node must exist
    nemo_iri = URIRef(LOGIC_NAMESPACE + "target/nemo")
    type_triples = list(report_g.triples((nemo_iri, RDF.type, LOGIC.ProjectionTarget)))
    assert type_triples, "nemo target node must appear in the report"

    # At least one materialization: drop must be present
    nemo_drops = _all_drops_on_nemo(report_g)
    mat_drops = [d for d in nemo_drops if d.startswith("materialization: ")]
    assert mat_drops, (
        f"No 'materialization: ' lossyDrop triples found. All drops: {nemo_drops}"
    )
