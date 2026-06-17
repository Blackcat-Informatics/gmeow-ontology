"""Closed-world SHACL data-shape tests (#39, epic #35).

The hybrid OWL+SHACL architecture's pure-Python, always-on negative lane: it
proves the relator/suppression/orthogonality shapes in shapes/gmeow-shapes.ttl
catch a malformed data graph and pass a well-formed one (CONSTITUTION P7/P9/P10).
The Docker ROBOT ``verify`` lane (reasoned-graph QC) and the HermiT inconsistency
lane live in ``tests/test_reasoning_entailments.py``; SHACL here is the
closed-world counterpart that needs no reasoner. See docs/reasoning.md.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import RDF, Graph, Literal, Namespace
from rdflib.namespace import RDFS, SH, SKOS, XSD
from rdflib.term import Node

from tests._graph_nt import run_shacl

SHAPES_FIXTURES = Path(__file__).parent / "fixtures" / "shapes"
GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://example.org/test/")


def _fixture(name: str) -> Graph:
    return Graph().parse(SHAPES_FIXTURES / f"{name}.ttl", format="turtle")


def test_wellformed_relator_fixture_conforms() -> None:
    """A well-formed data graph passes every closed-world shape (AC#1 positive)."""
    result = run_shacl(_fixture("relator-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_relator_fixture_is_flagged() -> None:
    """A malformed data graph is rejected, and each shape names its violation (AC#1)."""
    result = run_shacl(_fixture("relator-malformed"))
    assert not result.ok
    # Cardinality + orthogonality are Violations (errors); suppression is a Warning.
    errors = "\n".join(result.errors)
    report = "\n".join(result.errors + result.warnings)
    # Relator well-formedness (exactly-one cardinality), both min and max ends.
    assert "exactly one gmeow:Gender value" in errors
    assert "must use exactly one appellation" in errors
    # Orthogonality (Principle 9) is a Violation; suppression (P10) is a Warning.
    assert "may fill at most one identity axis" in errors
    assert "should set gmeow:displayable false" in report


def test_suppression_warning_does_not_fail_validation() -> None:
    """A superseded-but-unsuppressed facet warns, but does not hard-fail (Principle 10).

    The suppression contract is sh:Warning severity: a source may lag setting
    gmeow:displayable, so the graph still conforms (result.ok). This guards against
    drift in the severity bucketing in gmeow_tools.validate.run_shacl.
    """
    result = run_shacl(_fixture("suppression-warning-only"))
    assert result.ok, f"warning-only graph must pass; errors: {result.errors}"
    assert any("should set gmeow:displayable false" in w for w in result.warnings), (
        result.warnings
    )


def test_orthogonality_data_check_rejects_two_axes() -> None:
    """The closed-world dual of HermiT's two-axis inconsistency test.

    A single node typed in two disjoint identity axes is caught by SHACL without a
    reasoner — the counterpart of
    test_reasoning_entailments.test_two_axis_individual_is_inconsistent.
    """
    bad = Graph()
    bad.add((EX.x, RDF.type, GMEOW.GenderIdentity))
    bad.add((EX.x, RDF.type, GMEOW.SexualOrientation))
    result = run_shacl(bad)
    assert not result.ok
    assert "may fill at most one identity axis" in "\n".join(result.errors)


def test_wellformed_facet_cardinality_passes() -> None:
    """A lone facet with exactly one value conforms (cardinality-shape control)."""
    ok = Graph()
    ok.add((EX.f, RDF.type, GMEOW.GenderIdentity))
    ok.add((EX.f, GMEOW.facetSubject, EX.person))
    ok.add((EX.f, GMEOW.facetVantage, EX.person))
    ok.add((EX.f, GMEOW.genderValue, GMEOW.genderNonBinary))
    assert run_shacl(ok).ok


def test_internal_language_tag_shape_is_case_insensitive() -> None:
    """BCP-47 private-use tags are case-insensitive in SHACL too."""
    ok = Graph()
    ok.add((EX.name, GMEOW.fullName, Literal("Japanese", lang="x-GMEOW-Japanese")))
    assert run_shacl(ok).ok


def test_wellformed_reference_frame_passes() -> None:
    """A reference frame profile with all required properties passes SHACL."""
    ok = Graph()
    ok.add((EX.frame, RDF.type, GMEOW.ReferenceFrame))
    ok.add((EX.frame, GMEOW.frameRealm, GMEOW.frameRealmTerrestrial))
    ok.add((EX.frame, GMEOW.hasAxis, EX.axisX))
    ok.add(
        (EX.frame, GMEOW.dimensionCount, Literal(1, datatype=XSD.nonNegativeInteger))
    )
    ok.add((EX.frame, GMEOW.frameKind, GMEOW.frameKindCartesian))
    ok.add((EX.frame, GMEOW.requiresHost, Literal(False)))
    ok.add((EX.frame, GMEOW.determinacyModel, GMEOW.determinacyCrisp))

    ok.add((GMEOW.frameRealmTerrestrial, RDF.type, GMEOW.FrameRealm))
    ok.add((EX.axisX, RDF.type, GMEOW.Axis))
    ok.add((GMEOW.frameKindCartesian, RDF.type, GMEOW.FrameKind))
    ok.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))

    result = run_shacl(ok)
    assert result.ok, "\n".join(result.errors)


def test_reference_frame_axis_count_must_match_dimension_count() -> None:
    """Frame profiles reject mismatched axis cardinality and dimension count."""
    bad = Graph()
    bad.add((EX.frame, RDF.type, GMEOW.ReferenceFrame))
    bad.add((EX.frame, GMEOW.frameRealm, GMEOW.frameRealmTerrestrial))
    bad.add((EX.frame, GMEOW.hasAxis, EX.axisX))
    bad.add(
        (EX.frame, GMEOW.dimensionCount, Literal(3, datatype=XSD.nonNegativeInteger))
    )
    bad.add((EX.frame, GMEOW.frameKind, GMEOW.frameKindCartesian))
    bad.add((EX.frame, GMEOW.requiresHost, Literal(False)))
    bad.add((EX.frame, GMEOW.determinacyModel, GMEOW.determinacyCrisp))

    bad.add((GMEOW.frameRealmTerrestrial, RDF.type, GMEOW.FrameRealm))
    bad.add((EX.axisX, RDF.type, GMEOW.Axis))
    bad.add((GMEOW.frameKindCartesian, RDF.type, GMEOW.FrameKind))
    bad.add((GMEOW.determinacyCrisp, RDF.type, GMEOW.Determinacy))

    result = run_shacl(bad)
    assert not result.ok
    assert "dimension count must equal" in "\n".join(result.errors)


def test_malformed_reference_frame_fails() -> None:
    """A reference frame profile missing required descriptors fails SHACL validation."""
    bad = Graph()
    bad.add((EX.frame, RDF.type, GMEOW.ReferenceFrame))
    result = run_shacl(bad)
    assert not result.ok
    errors = "\n".join(result.errors)
    assert "declare its frame realm" in errors
    assert "have at least one coordinate axis" in errors


def test_profile_open_value_guard_warns_on_orphan() -> None:
    """A novel open-value individual with no profile descriptor triggers a warning."""
    bad = Graph()
    bad.add((GMEOW.profileReferenceFrame, RDF.type, GMEOW.Profile))
    bad.add(
        (GMEOW.profileReferenceFrame, RDFS.label, Literal("Reference Frame Profile"))
    )
    bad.add(
        (
            GMEOW.profileReferenceFrame,
            SKOS.definition,
            Literal("Closed descriptor schema for reference frames."),
        )
    )
    bad.add((GMEOW.profileReferenceFrame, GMEOW.profileDescriptor, GMEOW.frameRealm))
    bad.add((GMEOW.profileReferenceFrame, GMEOW.profileOpenValue, GMEOW.FrameRealm))
    bad.add((EX.customRealm, RDF.type, GMEOW.FrameRealm))
    result = run_shacl(bad)
    assert result.ok  # Warning only, so validation passes
    assert any(
        "Open value individuals must be referenced by at least one profile descriptor"
        in w
        for w in result.warnings
    )


def test_wellformed_proximity_fixture_conforms() -> None:
    """A well-formed ProximityMeasurement passes every shape (AC#1 positive, #95)."""
    result = run_shacl(_fixture("proximity-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_proximity_fixture_is_flagged() -> None:
    """A malformed ProximityMeasurement is rejected by SHACL (#95)."""
    result = run_shacl(_fixture("proximity-malformed"))
    assert not result.ok
    report = "\n".join(result.errors + result.warnings)
    assert "exactly one starting entity (gmeow:observedFeature)" in report
    assert "exactly one target entity (gmeow:proximityTo)" in report
    assert "exactly one scalar quantity result" in report


def test_wellformed_expertise_fixture_conforms() -> None:
    """Well-formed SkillProficiency + Credential graph passes expertise shapes."""
    result = run_shacl(_fixture("expertise-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_expertise_fixture_is_flagged() -> None:
    """A malformed expertise graph is rejected by the new SHACL shapes (#263)."""
    result = run_shacl(_fixture("expertise-malformed"))
    assert not result.ok
    report = "\n".join(result.errors + result.warnings)
    assert "must reference exactly one Skill" in report
    assert "levelScale should match" in report
    assert "must be an Organization" in report
    assert "should reference a gmeow:Attestation" in report


def test_no_nodeshape_iri_collision_across_shape_files() -> None:
    """Every sh:NodeShape IRI is owned by exactly one shape file (#478).

    ``_shapes_turtle`` merges hand-authored shapes, generated shapes, and slice
    shapes into a single document. If two files declare the same ``sh:NodeShape``
    subject, the definitions fuse, producing a shape whose meaning depends on
    which files happen to be parsed together. This guard fails CI mechanically
    if that ever happens.
    """
    from collections import defaultdict

    from gmeow_tools.config import GENERATED_SHAPES_DIR, SHAPES_DIR
    from gmeow_tools.slices import iter_slice_shape_files

    files = [
        *sorted(SHAPES_DIR.glob("*.ttl")),
        *sorted(GENERATED_SHAPES_DIR.glob("*.ttl")),
        *iter_slice_shape_files(),
    ]
    iri_to_files: dict[Node, list[Path]] = defaultdict(list)
    for path in files:
        graph = Graph().parse(path, format="turtle")
        for iri in graph.subjects(RDF.type, SH.NodeShape):
            iri_to_files[iri].append(path)

    collisions = {iri: paths for iri, paths in iri_to_files.items() if len(paths) > 1}
    assert not collisions, (
        "sh:NodeShape IRIs declared in more than one shape file: "
        + "; ".join(
            f"{iri} in {', '.join(str(p) for p in paths)}"
            for iri, paths in sorted(collisions.items(), key=lambda kv: str(kv[0]))
        )
    )
