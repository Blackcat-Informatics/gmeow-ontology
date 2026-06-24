"""Pitch frame structural guards (issue #308).

Principles 4, 9, 11, 12, 15, 16.

Migrated to Rust (conformance_music_pitch.rs, #867):
  test_pitch_value_ratio_only_passes_shacl
  test_pitch_value_cents_only_passes_shacl
  test_pitch_value_missing_frame_fails_shacl
  test_pitch_value_ratio_and_cents_fails_shacl
  test_pitch_value_zero_denominator_fails_shacl
  test_pitch_interval_xor_ratio_cents (split into 3 Rust fns)
  test_tuning_system_shape_requires_kind_and_realm
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://example.org/test-music-pitch/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_tuning_system_is_reference_frame() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "TuningSystem"),
        RDFS.subClassOf,
        URIRef(GMEOW + "ReferenceFrame"),
    ) in graph


def test_pitch_anchor_is_functional() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "pitchAnchorOf"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph


def test_has_tuning_frame_subproperty() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasTuningFrame"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "hasReferenceFrame"),
    ) in graph


def test_tuning_kind_is_functional() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "tuningKind"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph


def test_tuning_frame_properties_are_not_functional() -> None:
    """tuningAnchor, hasTuningFrame, and derivedFromSpectrum may have many values."""
    graph = _graph()
    for prop in ("tuningAnchor", "hasTuningFrame", "derivedFromSpectrum"):
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) not in graph, f"{prop} must not be declared owl:FunctionalProperty"


def test_tuning_system_seeds_coexist() -> None:
    graph = _graph()
    for iri in (
        "tuningSystem12EDO",
        "tuningSystem19EDO",
        "tuningSystem24EDO",
        "tuningSystem31EDO",
        "tuningSystemJustIntonation",
        "tuningSystemPythagorean",
        "tuningSystemQuarterCommaMeantone",
        "tuningSystemPartch43",
        "tuningSystemBohlenPierce",
        "tuningSystemSlendro",
        "tuningSystemPelog",
    ):
        assert (
            URIRef(GMEOW + iri),
            RDF.type,
            URIRef(GMEOW + "TuningSystem"),
        ) in graph, f"missing {iri}"


def test_pitch_anchor_a440_and_a415_coexist() -> None:
    """Two anchors for the same tuning system coexist (Principle 9)."""
    graph = _graph()
    for iri in ("pitchAnchorA440", "pitchAnchorA415"):
        assert (
            URIRef(GMEOW + iri),
            RDF.type,
            URIRef(GMEOW + "PitchAnchor"),
        ) in graph
    assert (
        URIRef(GMEOW + "pitchAnchorA440"),
        URIRef(GMEOW + "pitchAnchorOf"),
        URIRef(GMEOW + "tuningSystem12EDO"),
    ) in graph
    assert (
        URIRef(GMEOW + "pitchAnchorA415"),
        URIRef(GMEOW + "pitchAnchorOf"),
        URIRef(GMEOW + "tuningSystem12EDO"),
    ) in graph


def test_slendro_requires_host_but_12edo_does_not() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "tuningSystemSlendro"),
        URIRef(GMEOW + "requiresHost"),
        Literal(True),
    ) in graph
    assert (
        URIRef(GMEOW + "tuningSystem12EDO"),
        URIRef(GMEOW + "requiresHost"),
        Literal(False),
    ) in graph


def test_no_direct_frequency_property_on_pitch_value() -> None:
    """Hz is reached only through fnPitchToFrequency + PitchAnchor (Principle 11)."""
    graph = _graph()
    for candidate in ("frequency", "pitchFrequency", "hz"):
        prop = URIRef(GMEOW + candidate)
        assert (prop, RDF.type, OWL.DatatypeProperty) not in graph
        assert (prop, RDF.type, OWL.ObjectProperty) not in graph
