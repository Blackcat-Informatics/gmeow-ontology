# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the GTS transport slice (GTS transport design workstream B)."""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
SLICE_DIR = Path(__file__).resolve().parent.parent


def _graph() -> Graph:
    return load_merged_graph(include_imports=True)


def test_artifact_classes_ground_in_existing_spine() -> None:
    """Document and segment are Manifestations; compaction is an Activity."""
    g = _graph()
    for cls in ("GTSDocument", "GTSSegment"):
        assert (
            URIRef(GMEOW + cls),
            RDFS.subClassOf,
            URIRef(GMEOW + "Manifestation"),
        ) in g
    assert (
        URIRef(GMEOW + "GTSCompaction"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Activity"),
    ) in g
    assert (
        URIRef(GMEOW + "OpaqueFrame"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in g


def test_head_id_is_a_version_fingerprint() -> None:
    """The chain head transitively commits to history — a fingerprint, not a
    byte digest; the subproperty axiom is the no-parallel-mechanisms seam."""
    g = _graph()
    assert (
        URIRef(GMEOW + "gtsHeadId"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "versionFingerprint"),
    ) in g


def test_structure_properties_are_part_of_spine() -> None:
    """Segment-of and frame-in walk as partOf, so disclosure coarsening
    (P10 generalizesVia default) traverses transport containment."""
    g = _graph()
    for prop in ("gtsSegmentOf", "opaqueFrameIn"):
        assert (
            URIRef(GMEOW + prop),
            RDFS.subPropertyOf,
            URIRef(GMEOW + "partOf"),
        ) in g


def test_value_vocabularies_are_seeded() -> None:
    """Open value vocabularies (P9): individuals, never subclasses."""
    g = _graph()
    profiles = set(g.subjects(RDF.type, URIRef(GMEOW + "GTSProfile")))
    assert len(profiles) >= 7
    for name in ("gtsProfileDist", "gtsProfileEvidence", "gtsProfileAiPackage"):
        assert URIRef(GMEOW + name) in profiles

    codecs = set(g.subjects(RDF.type, URIRef(GMEOW + "TransformCodec")))
    assert len(codecs) >= 7
    assert URIRef(GMEOW + "codecZstd") in codecs

    reasons = set(g.subjects(RDF.type, URIRef(GMEOW + "OpacityReason")))
    assert reasons == {
        URIRef(GMEOW + "opacityUnknownCodec"),
        URIRef(GMEOW + "opacityMissingKey"),
        URIRef(GMEOW + "opacityDamaged"),
    }

    classes = set(g.subjects(RDF.type, URIRef(GMEOW + "CodecClass")))
    assert len(classes) == 3


def test_every_codec_carries_a_codec_class() -> None:
    g = _graph()
    codec_class = URIRef(GMEOW + "codecClass")
    for codec in g.subjects(RDF.type, URIRef(GMEOW + "TransformCodec")):
        assert (codec, codec_class, None) in g, f"{codec} lacks a codecClass"


def test_no_parallel_signature_or_digest_mechanism() -> None:
    """The slice must reuse attestation/sources — never mint its own
    signature or byte-digest terms (GTS transport design acceptance)."""
    module = (SLICE_DIR / "module.ttl").read_text(encoding="utf-8")
    g = Graph()
    g.parse(data=module, format="turtle")
    terms = {
        str(node)
        for triple in g
        for node in triple
        if isinstance(node, URIRef) and str(node).startswith(GMEOW)
    }
    for forbidden in ("gtsSignature", "gtsDigest", "gtsContentDigest"):
        assert GMEOW + forbidden not in terms


def test_competency_queries_parse_and_run() -> None:
    g = _graph()
    for query_file in sorted((SLICE_DIR / "queries").glob("*.rq")):
        list(g.query(query_file.read_text(encoding="utf-8")))


def test_slice_terms_are_class_or_property_typed() -> None:
    """Every gmeow-namespaced subject in module.ttl is properly OWL-typed."""
    module = (SLICE_DIR / "module.ttl").read_text(encoding="utf-8")
    g = Graph()
    g.parse(data=module, format="turtle")
    allowed = {
        OWL.Class,
        OWL.ObjectProperty,
        OWL.DatatypeProperty,
        OWL.FunctionalProperty,
        OWL.Ontology,
    }
    for s in set(g.subjects()):
        if not str(s).startswith(GMEOW) or str(s).endswith("/slices/gts"):
            continue
        types = set(g.objects(s, RDF.type))
        assert types, f"{s} has no rdf:type"
        is_declared = bool(types & allowed)
        is_individual = any(str(t).startswith(GMEOW) for t in types)
        assert is_declared or is_individual, (
            f"{s} is neither declared nor an individual"
        )
