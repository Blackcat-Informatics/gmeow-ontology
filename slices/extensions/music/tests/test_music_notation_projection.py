# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Notation projection layer tests for the music extension (issue #318)."""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import RDF, RDFS, Graph, Namespace

GM = Namespace("https://blackcatinformatics.ca/gmeow/")


def _load_music_module() -> Graph:
    graph = Graph()
    module_path = Path(__file__).resolve().parents[1] / "module.ttl"
    graph.parse(module_path, format="turtle")
    return graph


def test_notation_projection_profile_completeness() -> None:
    """Every NotationProjectionProfile accounts for every MusicalParameter.

    A parameter is accounted for when the profile either lists it as
    representableParameter or declares a ProjectionLoss that accountsForParameter
    it. This is the machine-readable form of the issue-318 declared-loss manifest.
    """
    g = _load_music_module()

    params = set(g.subjects(RDF.type, GM.MusicalParameter))
    profiles = set(g.subjects(RDF.type, GM.NotationProjectionProfile))
    assert profiles, "No NotationProjectionProfile individuals found"
    assert params, "No MusicalParameter individuals found"

    missing: list[str] = []
    for profile in profiles:
        representable = set(g.objects(profile, GM.representableParameter))
        declared_losses = set(g.objects(profile, GM.declaredLoss))
        accounted_by_loss = set()
        for loss in declared_losses:
            accounted_by_loss.update(g.objects(loss, GM.accountsForParameter))
        accounted = representable | accounted_by_loss
        for param in params:
            if param not in accounted:
                missing.append(
                    f"{profile.n3(g.namespace_manager)} missing "
                    f"{param.n3(g.namespace_manager)}"
                )

    assert not missing, (
        "Silent omissions in NotationProjectionProfile(s):\n" + "\n".join(missing)
    )


def test_projection_losses_account_for_parameters() -> None:
    """Every ProjectionLoss individual must account for at least one
    MusicalParameter.
    """
    g = _load_music_module()

    losses = set(g.subjects(RDF.type, GM.ProjectionLoss))
    assert losses, "No ProjectionLoss individuals found"

    unaccounted: list[str] = []
    for loss in losses:
        if not list(g.objects(loss, GM.accountsForParameter)):
            unaccounted.append(loss.n3(g.namespace_manager))

    assert not unaccounted, (
        "ProjectionLoss individuals with no accountsForParameter:\n"
        + "\n".join(unaccounted)
    )


def test_projection_loss_no_subclasses() -> None:
    """ProjectionLoss is a value vocabulary and must not be subclassed
    (Principle 9).
    """
    g = _load_music_module()

    subclasses = set(g.subjects(RDFS.subClassOf, GM.ProjectionLoss)) - {
        GM.ProjectionLoss
    }
    assert not subclasses, f"ProjectionLoss must not be subclassed: {subclasses}"


def test_mensural_honesty_declares_own_time_semantics() -> None:
    """The mensural profile must not claim to quantize time to a rational grid,
    because mensural notation carries its own proportion/coloration semantics."""
    g = _load_music_module()

    profile = GM.profileMensural
    assert (profile, RDF.type, GM.NotationProjectionProfile) in g
    losses = set(g.objects(profile, GM.declaredLoss))
    assert GM.lossQuantizesTimeToRationalGrid not in losses, (
        "Mensural profile must not declare lossQuantizesTimeToRationalGrid; "
        "it carries its own time semantics."
    )


def test_graphic_honesty_near_total_symbolic_loss() -> None:
    """The graphic profile must declare near-total loss in the symbolic direction.

    Only soundContent and location are representable; all other parameters must be
    covered by declared losses.
    """
    g = _load_music_module()

    profile = GM.profileGraphic
    assert (profile, RDF.type, GM.NotationProjectionProfile) in g

    representable = set(g.objects(profile, GM.representableParameter))
    assert {
        GM.musicalParameterSoundContent,
        GM.musicalParameterLocation,
    } == representable


def test_every_music_notation_system_has_profile() -> None:
    """Every music-domain NotationSystem individual declared in the music module
    has a NotationProjectionProfile linked via notationSystemOf."""
    g = _load_music_module()

    notation_systems = {
        ns
        for ns in g.subjects(RDF.type, GM.NotationSystem)
        if str(ns).startswith(str(GM))
    }
    assert notation_systems, "No music-domain NotationSystem individuals found"

    profile_counts = dict.fromkeys(notation_systems, 0)
    for ns in g.objects(None, GM.notationSystemOf):
        if ns in profile_counts:
            profile_counts[ns] += 1

    unprofiled = {ns for ns, count in profile_counts.items() if count == 0}
    duplicated = {ns: count for ns, count in profile_counts.items() if count > 1}

    assert not unprofiled, f"NotationSystem(s) without projection profile: {unprofiled}"
    assert not duplicated, f"NotationSystem(s) with multiple profiles: {duplicated}"
